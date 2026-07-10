//! Provider-neutral OAuth 2.0 Authorization Code client with PKCE and normalized user info.

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, OsRng, Payload, rand_core::RngCore as _},
};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse as _, TokenUrl, basic::BasicClient,
    reqwest,
};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use thiserror::Error;
use url::Url;

use crate::{Clock, IdentityClaims, PortError, SecretResolver};

const MAX_PENDING_FLOWS: usize = 10_000;
const MAX_USER_INFO_BYTES: usize = 64 * 1024;

/// Configurable JSON paths used to normalize an `OAuth2` user-info response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2ClaimMappings {
    pub subject: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<String>,
    pub avatar_url: Option<String>,
}

/// Provider-neutral `OAuth2` connector configuration.
#[derive(Clone, Debug)]
pub struct OAuth2ConnectorSettings {
    pub authorization_endpoint: Url,
    pub token_endpoint: Url,
    pub user_info_endpoint: Url,
    pub client_id: String,
    pub client_secret_ref: String,
    pub client_auth_method: OAuth2ClientAuthMethod,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub claim_mappings: OAuth2ClaimMappings,
    pub flow_lifetime_seconds: u32,
}

/// Standard client authentication methods supported by a configured token endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuth2ClientAuthMethod {
    BasicAuth,
    RequestBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2AuthorizationRequest {
    pub redirect_url: Url,
    pub state_handle: String,
}

/// Token purpose supplied to encryption so adapters can bind ciphertext to its use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderTokenKind {
    Access,
    Refresh,
}

/// Opaque encrypted token state suitable for server-side persistence.
#[derive(Clone, Eq, PartialEq)]
pub struct EncryptedProviderToken {
    pub key_id: String,
    pub ciphertext: Vec<u8>,
}

impl fmt::Debug for EncryptedProviderToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncryptedProviderToken")
            .field("key_id", &self.key_id)
            .field("ciphertext", &"[REDACTED]")
            .finish()
    }
}

/// Encryption boundary for provider tokens retained by the backend.
#[async_trait]
pub trait TokenCipher: Send + Sync {
    async fn encrypt(
        &self,
        kind: ProviderTokenKind,
        plaintext: &SecretString,
    ) -> Result<EncryptedProviderToken, PortError>;
}

/// RustCrypto-backed token protection using XChaCha20-Poly1305 AEAD.
pub struct XChaCha20Poly1305TokenCipher {
    key_id: String,
    cipher: XChaCha20Poly1305,
}

impl XChaCha20Poly1305TokenCipher {
    /// Creates a token cipher for a versioned 256-bit key supplied by secret management.
    ///
    /// # Errors
    ///
    /// Returns an error when the persistence key identifier is empty.
    pub fn new(key_id: String, key: &[u8; 32]) -> Result<Self, TokenCipherConfigError> {
        if key_id.trim().is_empty() {
            return Err(TokenCipherConfigError::EmptyKeyId);
        }
        Ok(Self {
            key_id,
            cipher: XChaCha20Poly1305::new(key.into()),
        })
    }
}

#[async_trait]
impl TokenCipher for XChaCha20Poly1305TokenCipher {
    async fn encrypt(
        &self,
        kind: ProviderTokenKind,
        plaintext: &SecretString,
    ) -> Result<EncryptedProviderToken, PortError> {
        let mut nonce_bytes = [0_u8; 24];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext.expose_secret().as_bytes(),
                    aad: token_aad(kind),
                },
            )
            .map_err(|_| PortError::Unavailable)?;
        let mut sealed = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        Ok(EncryptedProviderToken {
            key_id: self.key_id.clone(),
            ciphertext: sealed,
        })
    }
}

fn token_aad(kind: ProviderTokenKind) -> &'static [u8] {
    match kind {
        ProviderTokenKind::Access => b"pvlog/oauth2/access-token/v1",
        ProviderTokenKind::Refresh => b"pvlog/oauth2/refresh-token/v1",
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenCipherConfigError {
    #[error("token encryption key identifier must not be empty")]
    EmptyKeyId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedOAuth2Tokens {
    pub access_token: EncryptedProviderToken,
    pub refresh_token: Option<EncryptedProviderToken>,
    pub expires_at_epoch_millis: Option<i64>,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2UserInfo {
    pub identity: IdentityClaims,
    pub tokens: ProtectedOAuth2Tokens,
}

struct PendingFlow {
    pkce_verifier: String,
    expires_at: i64,
}

pub struct OAuth2ProtocolClient {
    settings: OAuth2ConnectorSettings,
    client_secret: SecretString,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    token_cipher: Arc<dyn TokenCipher>,
    flows: Mutex<HashMap<String, PendingFlow>>,
}

impl OAuth2ProtocolClient {
    /// Builds a connector after resolving its secret reference.
    ///
    /// # Errors
    ///
    /// Returns a safe configuration error for invalid endpoints, mappings, secret references, or
    /// HTTP client configuration.
    pub async fn new(
        settings: OAuth2ConnectorSettings,
        clock: Arc<dyn Clock>,
        secrets: Arc<dyn SecretResolver>,
        token_cipher: Arc<dyn TokenCipher>,
    ) -> Result<Self, OAuth2ProtocolError> {
        validate_settings(&settings)?;
        let client_secret = secrets
            .resolve(&settings.client_secret_ref)
            .await
            .map_err(|_| OAuth2ProtocolError::SecretResolution)?;
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| OAuth2ProtocolError::HttpClient)?;
        let _ = build_client(&settings, &client_secret)?;
        Ok(Self {
            settings,
            client_secret,
            http,
            clock,
            token_cipher,
            flows: Mutex::new(HashMap::new()),
        })
    }

    /// Starts a one-time Authorization Code flow using random state and S256 PKCE.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration, time, or bounded flow storage is unavailable.
    pub fn begin_authorization(&self) -> Result<OAuth2AuthorizationRequest, OAuth2ProtocolError> {
        let client = build_client(&self.settings, &self.client_secret)?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(challenge);
        for scope in &self.settings.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let (redirect_url, state) = request.url();
        let now = self.now()?;
        let expires_at = now
            .checked_add(i64::from(self.settings.flow_lifetime_seconds) * 1_000)
            .ok_or(OAuth2ProtocolError::Time)?;
        let state_handle = state.secret().to_owned();
        let mut flows = self
            .flows
            .lock()
            .map_err(|_| OAuth2ProtocolError::FlowStore)?;
        flows.retain(|_, flow| flow.expires_at > now);
        if flows.len() >= MAX_PENDING_FLOWS {
            return Err(OAuth2ProtocolError::FlowCapacity);
        }
        flows.insert(
            state_handle.clone(),
            PendingFlow {
                pkce_verifier: verifier.secret().to_owned(),
                expires_at,
            },
        );
        Ok(OAuth2AuthorizationRequest {
            redirect_url,
            state_handle,
        })
    }

    /// Consumes a flow, exchanges its code, fetches normalized user info, and protects all tokens.
    ///
    /// # Errors
    ///
    /// Returns a uniform protocol error for invalid/replayed state, exchange or user-info failure,
    /// invalid mapped claims, or token encryption failure.
    pub async fn complete_authorization(
        &self,
        state: &str,
        code: SecretString,
    ) -> Result<OAuth2UserInfo, OAuth2ProtocolError> {
        let flow = self
            .flows
            .lock()
            .map_err(|_| OAuth2ProtocolError::FlowStore)?
            .remove(state)
            .ok_or(OAuth2ProtocolError::FlowRejected)?;
        if flow.expires_at <= self.now()? {
            return Err(OAuth2ProtocolError::FlowRejected);
        }
        let client = build_client(&self.settings, &self.client_secret)?;
        let token_response = client
            .exchange_code(AuthorizationCode::new(code.expose_secret().to_owned()))
            .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
            .request_async(&self.http)
            .await
            .map_err(|_| OAuth2ProtocolError::Exchange)?;

        let access_plaintext =
            SecretString::from(token_response.access_token().secret().to_owned());
        let identity = self.fetch_user_info(&access_plaintext).await?;
        let access_token = self
            .token_cipher
            .encrypt(ProviderTokenKind::Access, &access_plaintext)
            .await
            .map_err(|_| OAuth2ProtocolError::TokenProtection)?;
        let refresh_token = if let Some(refresh) = token_response.refresh_token() {
            let plaintext = SecretString::from(refresh.secret().to_owned());
            Some(
                self.token_cipher
                    .encrypt(ProviderTokenKind::Refresh, &plaintext)
                    .await
                    .map_err(|_| OAuth2ProtocolError::TokenProtection)?,
            )
        } else {
            None
        };
        let now = self.now()?;
        let expires_at_epoch_millis = token_response
            .expires_in()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .and_then(|duration| now.checked_add(duration));
        let scopes = token_response.scopes().map_or_else(
            || self.settings.scopes.clone(),
            |scopes| {
                scopes
                    .iter()
                    .map(|scope| scope.as_ref().to_owned())
                    .collect()
            },
        );

        Ok(OAuth2UserInfo {
            identity,
            tokens: ProtectedOAuth2Tokens {
                access_token,
                refresh_token,
                expires_at_epoch_millis,
                scopes,
            },
        })
    }

    #[must_use]
    pub fn pending_flow_count(&self) -> usize {
        self.flows.lock().map_or(0, |flows| flows.len())
    }

    async fn fetch_user_info(
        &self,
        access_token: &SecretString,
    ) -> Result<IdentityClaims, OAuth2ProtocolError> {
        let response = self
            .http
            .get(self.settings.user_info_endpoint.clone())
            .bearer_auth(access_token.expose_secret())
            .send()
            .await
            .map_err(|_| OAuth2ProtocolError::UserInfo)?;
        if !response.status().is_success() {
            return Err(OAuth2ProtocolError::UserInfo);
        }
        if response.content_length().is_some_and(|size| {
            usize::try_from(size).map_or(true, |size| size > MAX_USER_INFO_BYTES)
        }) {
            return Err(OAuth2ProtocolError::UserInfo);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| OAuth2ProtocolError::UserInfo)?;
        if bytes.len() > MAX_USER_INFO_BYTES {
            return Err(OAuth2ProtocolError::UserInfo);
        }
        let document: Value =
            serde_json::from_slice(&bytes).map_err(|_| OAuth2ProtocolError::UserInfoClaims)?;
        normalize_claims(&document, &self.settings.claim_mappings)
    }

    fn now(&self) -> Result<i64, OAuth2ProtocolError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| OAuth2ProtocolError::Time)
    }
}

fn validate_settings(settings: &OAuth2ConnectorSettings) -> Result<(), OAuth2ProtocolError> {
    let mappings = &settings.claim_mappings;
    if settings.client_id.trim().is_empty()
        || settings.client_secret_ref.trim().is_empty()
        || settings.flow_lifetime_seconds == 0
        || mappings.subject.trim().is_empty()
        || !mapping_paths(mappings).all(valid_mapping_path)
    {
        return Err(OAuth2ProtocolError::InvalidConfiguration);
    }
    Ok(())
}

fn mapping_paths(mappings: &OAuth2ClaimMappings) -> impl Iterator<Item = &str> {
    std::iter::once(mappings.subject.as_str()).chain(
        [
            mappings.display_name.as_deref(),
            mappings.email.as_deref(),
            mappings.email_verified.as_deref(),
            mappings.avatar_url.as_deref(),
        ]
        .into_iter()
        .flatten(),
    )
}

fn valid_mapping_path(path: &str) -> bool {
    !path.is_empty() && path.split('.').all(|segment| !segment.trim().is_empty())
}

fn build_client(
    settings: &OAuth2ConnectorSettings,
    client_secret: &SecretString,
) -> Result<
    BasicClient<
        oauth2::EndpointSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointSet,
    >,
    OAuth2ProtocolError,
> {
    Ok(BasicClient::new(ClientId::new(settings.client_id.clone()))
        .set_client_secret(ClientSecret::new(client_secret.expose_secret().to_owned()))
        .set_auth_type(match settings.client_auth_method {
            OAuth2ClientAuthMethod::BasicAuth => AuthType::BasicAuth,
            OAuth2ClientAuthMethod::RequestBody => AuthType::RequestBody,
        })
        .set_auth_uri(
            AuthUrl::new(settings.authorization_endpoint.to_string())
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
        )
        .set_token_uri(
            TokenUrl::new(settings.token_endpoint.to_string())
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
        )
        .set_redirect_uri(
            RedirectUrl::new(settings.redirect_uri.to_string())
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
        ))
}

fn normalize_claims(
    document: &Value,
    mappings: &OAuth2ClaimMappings,
) -> Result<IdentityClaims, OAuth2ProtocolError> {
    let subject = mapped_value(document, &mappings.subject)
        .and_then(mapped_subject)
        .ok_or(OAuth2ProtocolError::UserInfoClaims)?;
    Ok(IdentityClaims {
        subject,
        display_name: mapped_optional_string(document, mappings.display_name.as_deref())?,
        email: mapped_optional_string(document, mappings.email.as_deref())?,
        email_verified: mapped_optional_bool(document, mappings.email_verified.as_deref())?,
        avatar_url: mapped_optional_string(document, mappings.avatar_url.as_deref())?
            .map(|value| Url::parse(&value).map_err(|_| OAuth2ProtocolError::UserInfoClaims))
            .transpose()?,
    })
}

fn mapped_subject(value: &Value) -> Option<String> {
    match value {
        Value::String(subject) if !subject.trim().is_empty() => Some(subject.clone()),
        Value::Number(subject) => Some(subject.to_string()),
        _ => None,
    }
}

fn mapped_value<'a>(document: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(document, |value, key| value.get(key))
}

fn mapped_optional_string(
    document: &Value,
    path: Option<&str>,
) -> Result<Option<String>, OAuth2ProtocolError> {
    path.map(|path| {
        mapped_value(document, path)
            .map(|value| {
                if value.is_null() {
                    return Ok(None);
                }
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .map(Some)
                    .ok_or(OAuth2ProtocolError::UserInfoClaims)
            })
            .transpose()
    })
    .transpose()
    .map(Option::flatten)
    .map(Option::flatten)
}

fn mapped_optional_bool(
    document: &Value,
    path: Option<&str>,
) -> Result<Option<bool>, OAuth2ProtocolError> {
    path.map(|path| {
        mapped_value(document, path)
            .map(|value| {
                if value.is_null() {
                    return Ok(None);
                }
                value
                    .as_bool()
                    .map(Some)
                    .ok_or(OAuth2ProtocolError::UserInfoClaims)
            })
            .transpose()
    })
    .transpose()
    .map(Option::flatten)
    .map(Option::flatten)
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OAuth2ProtocolError {
    #[error("OAuth2 configuration is invalid")]
    InvalidConfiguration,
    #[error("OAuth2 HTTP client could not be created")]
    HttpClient,
    #[error("OAuth2 client secret reference could not be resolved")]
    SecretResolution,
    #[error("OAuth2 authorization flow was rejected")]
    FlowRejected,
    #[error("OAuth2 authorization flow storage is unavailable")]
    FlowStore,
    #[error("OAuth2 authorization flow capacity was reached")]
    FlowCapacity,
    #[error("OAuth2 authorization code exchange failed")]
    Exchange,
    #[error("OAuth2 user-info request failed")]
    UserInfo,
    #[error("OAuth2 user-info claims are invalid")]
    UserInfoClaims,
    #[error("OAuth2 provider token protection failed")]
    TokenProtection,
    #[error("OAuth2 time value is invalid")]
    Time,
}
