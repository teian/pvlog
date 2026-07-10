//! Provider-neutral `OpenID Connect` Authorization Code client with PKCE and one-time flows.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    OAuth2TokenResponse as _, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse as _,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use secrecy::{ExposeSecret as _, SecretString};
use thiserror::Error;
use url::Url;

use crate::{Clock, IdentityClaims, SecretResolver};

#[derive(Debug)]
pub struct OidcConnectorSettings {
    pub issuer: Url,
    pub client_id: String,
    pub client_secret_ref: String,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub flow_lifetime_seconds: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OidcAuthorizationRequest {
    pub redirect_url: Url,
    pub state_handle: String,
}

struct PendingFlow {
    nonce: String,
    pkce_verifier: String,
    expires_at: i64,
}

pub struct OidcProtocolClient {
    settings: OidcConnectorSettings,
    client_secret: SecretString,
    metadata: CoreProviderMetadata,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    flows: Mutex<HashMap<String, PendingFlow>>,
}

impl OidcProtocolClient {
    /// Discovers and validates provider metadata without following HTTP redirects.
    ///
    /// # Errors
    ///
    /// Returns a safe protocol error when the issuer, discovery document, endpoints, or HTTP
    /// client configuration is invalid.
    pub async fn discover(
        settings: OidcConnectorSettings,
        clock: Arc<dyn Clock>,
        secrets: Arc<dyn SecretResolver>,
    ) -> Result<Self, OidcProtocolError> {
        if settings.client_secret_ref.trim().is_empty() {
            return Err(OidcProtocolError::InvalidConfiguration);
        }
        let client_secret = secrets
            .resolve(&settings.client_secret_ref)
            .await
            .map_err(|_| OidcProtocolError::SecretResolution)?;
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| OidcProtocolError::HttpClient)?;
        let issuer = IssuerUrl::new(settings.issuer.to_string())
            .map_err(|_| OidcProtocolError::InvalidConfiguration)?;
        let metadata = CoreProviderMetadata::discover_async(issuer, &http)
            .await
            .map_err(|_| OidcProtocolError::Discovery)?;
        // Building the typed client validates required authorization and token endpoints.
        let _ = build_client(&settings, &client_secret, metadata.clone())?;
        Ok(Self {
            settings,
            client_secret,
            metadata,
            http,
            clock,
            flows: Mutex::new(HashMap::new()),
        })
    }

    /// Starts a one-time Authorization Code flow with state, nonce, and S256 PKCE.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration, time, or bounded flow storage is unavailable.
    pub fn begin_authorization(&self) -> Result<OidcAuthorizationRequest, OidcProtocolError> {
        let client = build_client(&self.settings, &self.client_secret, self.metadata.clone())?;
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);
        for scope in &self.settings.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let (redirect_url, state, nonce) = request.url();
        let now = self.now()?;
        let expires_at = now
            .checked_add(i64::from(self.settings.flow_lifetime_seconds) * 1_000)
            .ok_or(OidcProtocolError::Time)?;
        let state_handle = state.secret().to_owned();
        let mut flows = self
            .flows
            .lock()
            .map_err(|_| OidcProtocolError::FlowStore)?;
        flows.retain(|_, flow| flow.expires_at > now);
        if flows.len() >= 10_000 {
            return Err(OidcProtocolError::FlowCapacity);
        }
        flows.insert(
            state_handle.clone(),
            PendingFlow {
                nonce: nonce.secret().to_owned(),
                pkce_verifier: pkce_verifier.secret().to_owned(),
                expires_at,
            },
        );
        Ok(OidcAuthorizationRequest {
            redirect_url,
            state_handle,
        })
    }

    /// Completes and consumes a flow, exchanges the code, and validates the signed ID token.
    ///
    /// Signature, issuer, audience, expiration, nonce, and optional access-token hash checks are
    /// delegated to the `OpenID Connect` verifier built from discovered metadata.
    ///
    /// # Errors
    ///
    /// Returns a uniform protocol error for unknown/replayed state, expired flow, exchange
    /// failure, missing ID token, or any token validation failure.
    pub async fn complete_authorization(
        &self,
        state: &str,
        code: SecretString,
    ) -> Result<IdentityClaims, OidcProtocolError> {
        let flow = self
            .flows
            .lock()
            .map_err(|_| OidcProtocolError::FlowStore)?
            .remove(state)
            .ok_or(OidcProtocolError::FlowRejected)?;
        if flow.expires_at <= self.now()? {
            return Err(OidcProtocolError::FlowRejected);
        }
        let client = build_client(&self.settings, &self.client_secret, self.metadata.clone())?;
        let token_response = client
            .exchange_code(AuthorizationCode::new(code.expose_secret().to_owned()))
            .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
            .request_async(&self.http)
            .await
            .map_err(|_| OidcProtocolError::Exchange)?;
        let id_token = token_response
            .id_token()
            .ok_or(OidcProtocolError::MissingIdToken)?;
        let verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &Nonce::new(flow.nonce))
            .map_err(|_| OidcProtocolError::TokenRejected)?;
        if let Some(expected_hash) = claims.access_token_hash() {
            let actual_hash = AccessTokenHash::from_token(
                token_response.access_token(),
                id_token
                    .signing_alg()
                    .map_err(|_| OidcProtocolError::TokenRejected)?,
                id_token
                    .signing_key(&verifier)
                    .map_err(|_| OidcProtocolError::TokenRejected)?,
            )
            .map_err(|_| OidcProtocolError::TokenRejected)?;
            if actual_hash != *expected_hash {
                return Err(OidcProtocolError::TokenRejected);
            }
        }
        Ok(IdentityClaims {
            subject: claims.subject().as_str().to_owned(),
            display_name: None,
            email: claims.email().map(|email| email.as_str().to_owned()),
            email_verified: claims.email_verified(),
            avatar_url: None,
        })
    }

    #[must_use]
    pub fn pending_flow_count(&self) -> usize {
        self.flows.lock().map_or(0, |flows| flows.len())
    }

    fn now(&self) -> Result<i64, OidcProtocolError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| OidcProtocolError::Time)
    }
}

fn build_client(
    settings: &OidcConnectorSettings,
    client_secret: &SecretString,
    metadata: CoreProviderMetadata,
) -> Result<
    CoreClient<
        openidconnect::EndpointSet,
        openidconnect::EndpointNotSet,
        openidconnect::EndpointNotSet,
        openidconnect::EndpointNotSet,
        openidconnect::EndpointSet,
        openidconnect::EndpointMaybeSet,
    >,
    OidcProtocolError,
> {
    let token_endpoint = metadata
        .token_endpoint()
        .cloned()
        .ok_or(OidcProtocolError::InvalidConfiguration)?;
    Ok(CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(settings.client_id.clone()),
        Some(ClientSecret::new(client_secret.expose_secret().to_owned())),
    )
    .set_redirect_uri(
        RedirectUrl::new(settings.redirect_uri.to_string())
            .map_err(|_| OidcProtocolError::InvalidConfiguration)?,
    )
    .set_token_uri(token_endpoint))
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OidcProtocolError {
    #[error("OIDC configuration is invalid")]
    InvalidConfiguration,
    #[error("OIDC HTTP client could not be created")]
    HttpClient,
    #[error("OIDC client secret reference could not be resolved")]
    SecretResolution,
    #[error("OIDC discovery or provider metadata validation failed")]
    Discovery,
    #[error("OIDC authorization flow was rejected")]
    FlowRejected,
    #[error("OIDC authorization flow storage is unavailable")]
    FlowStore,
    #[error("OIDC authorization flow capacity was reached")]
    FlowCapacity,
    #[error("OIDC authorization code exchange failed")]
    Exchange,
    #[error("OIDC response did not contain an ID token")]
    MissingIdToken,
    #[error("OIDC ID token validation failed")]
    TokenRejected,
    #[error("OIDC time value is invalid")]
    Time,
}
