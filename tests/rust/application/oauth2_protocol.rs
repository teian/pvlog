//! Fake-provider coverage for generic `OAuth2` state, PKCE, user info, and token protection.

use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use oauth2::{PkceCodeChallenge, PkceCodeVerifier};
use pvlog_application::{
    Clock, EncryptedProviderToken, OAuth2ClaimMappings, OAuth2ClientAuthMethod,
    OAuth2ConnectorSettings, OAuth2ProtocolClient, OAuth2ProtocolError, PortError,
    ProviderTokenKind, SecretResolver, TokenCipher, XChaCha20Poly1305TokenCipher,
};
use pvlog_domain::UtcTimestamp;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use url::Url;

const CLIENT_SECRET: &str = "provider-neutral-oauth2-secret";
const ACCESS_TOKEN: &str = "provider-access-token-plain";
const REFRESH_TOKEN: &str = "provider-refresh-token-plain";
const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
async fn production_token_cipher_uses_random_authenticated_encryption() -> Result<(), Box<dyn Error>>
{
    let cipher = XChaCha20Poly1305TokenCipher::new("provider-key-v1".to_owned(), &[7_u8; 32])?;
    let plaintext = SecretString::from(ACCESS_TOKEN);
    let first = cipher
        .encrypt(ProviderTokenKind::Access, &plaintext)
        .await?;
    let second = cipher
        .encrypt(ProviderTokenKind::Access, &plaintext)
        .await?;

    assert_eq!(first.key_id, "provider-key-v1");
    assert_ne!(first.ciphertext, second.ciphertext);
    assert!(
        !first
            .ciphertext
            .windows(ACCESS_TOKEN.len())
            .any(|window| window == ACCESS_TOKEN.as_bytes())
    );
    assert_eq!(
        format!("{first:?}"),
        "EncryptedProviderToken { key_id: \"provider-key-v1\", ciphertext: \"[REDACTED]\" }"
    );
    Ok(())
}

#[tokio::test]
async fn oauth2_flow_normalizes_user_info_and_only_returns_protected_tokens()
-> Result<(), Box<dyn Error>> {
    let provider = FakeProvider::start().await?;
    let clock = Arc::new(TestClock::new(UtcTimestamp::from_epoch_millis(NOW)?));
    let cipher = Arc::new(RecordingCipher::default());
    let client = test_client(&provider, clock.clone(), cipher.clone()).await?;

    let authorization = client.begin_authorization()?;
    let parameters: std::collections::HashMap<_, _> = authorization
        .redirect_url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(parameters.get("state"), Some(&authorization.state_handle));
    assert_eq!(
        parameters.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    provider.expect_challenge(
        parameters
            .get("code_challenge")
            .ok_or("PKCE challenge missing")?,
    )?;
    assert_eq!(client.pending_flow_count(), 1);
    assert_eq!(
        client
            .complete_authorization("unknown-state", SecretString::from("code"))
            .await,
        Err(OAuth2ProtocolError::FlowRejected)
    );

    let result = client
        .complete_authorization(
            &authorization.state_handle,
            SecretString::from("valid-code"),
        )
        .await?;
    assert_eq!(result.identity.subject, "stable-subject");
    assert_eq!(result.identity.display_name.as_deref(), Some("Solar Owner"));
    assert_eq!(result.identity.email.as_deref(), Some("solar@example.test"));
    assert_eq!(result.identity.email_verified, Some(true));
    assert_eq!(
        result.identity.avatar_url,
        Some(Url::parse("https://cdn.example.test/avatar.png")?)
    );
    assert_eq!(result.tokens.access_token.key_id, "test-key-v1");
    assert!(result.tokens.refresh_token.is_some());
    let debug = format!("{result:?}");
    assert!(!debug.contains(ACCESS_TOKEN));
    assert!(!debug.contains(REFRESH_TOKEN));
    assert!(debug.contains("[REDACTED]"));
    assert_eq!(
        cipher.plaintexts()?,
        vec![
            (ProviderTokenKind::Access, ACCESS_TOKEN.to_owned()),
            (ProviderTokenKind::Refresh, REFRESH_TOKEN.to_owned()),
        ]
    );
    assert_eq!(
        client
            .complete_authorization(&authorization.state_handle, SecretString::from("replay"))
            .await,
        Err(OAuth2ProtocolError::FlowRejected)
    );

    let expired = client.begin_authorization()?;
    clock.set(UtcTimestamp::from_epoch_millis(NOW + 301_000)?)?;
    assert_eq!(
        client
            .complete_authorization(&expired.state_handle, SecretString::from("expired"))
            .await,
        Err(OAuth2ProtocolError::FlowRejected)
    );
    Ok(())
}

#[tokio::test]
async fn oauth2_flow_rejects_missing_required_subject() -> Result<(), Box<dyn Error>> {
    let provider = FakeProvider::start().await?;
    provider.set_user_info(json!({"profile": {"name": "No stable subject"}}))?;
    let client = test_client(
        &provider,
        Arc::new(TestClock::new(UtcTimestamp::from_epoch_millis(NOW)?)),
        Arc::new(RecordingCipher::default()),
    )
    .await?;
    let authorization = client.begin_authorization()?;
    let challenge = authorization
        .redirect_url
        .query_pairs()
        .find_map(|(key, value)| (key == "code_challenge").then(|| value.into_owned()))
        .ok_or("PKCE challenge missing")?;
    provider.expect_challenge(&challenge)?;
    assert_eq!(
        client
            .complete_authorization(&authorization.state_handle, SecretString::from("code"))
            .await,
        Err(OAuth2ProtocolError::UserInfoClaims)
    );
    Ok(())
}

#[tokio::test]
async fn oauth2_flow_normalizes_numeric_subjects_and_nullable_optional_claims()
-> Result<(), Box<dyn Error>> {
    let provider = FakeProvider::start().await?;
    provider.set_user_info(json!({
        "profile": {
            "id": 12_345_678,
            "name": null,
            "email": null,
            "email_verified": null,
            "avatar": null
        }
    }))?;
    let client = test_client(
        &provider,
        Arc::new(TestClock::new(UtcTimestamp::from_epoch_millis(NOW)?)),
        Arc::new(RecordingCipher::default()),
    )
    .await?;
    let authorization = client.begin_authorization()?;
    let challenge = authorization
        .redirect_url
        .query_pairs()
        .find_map(|(key, value)| (key == "code_challenge").then(|| value.into_owned()))
        .ok_or("PKCE challenge missing")?;
    provider.expect_challenge(&challenge)?;

    let result = client
        .complete_authorization(&authorization.state_handle, SecretString::from("code"))
        .await?;
    assert_eq!(result.identity.subject, "12345678");
    assert_eq!(result.identity.display_name, None);
    assert_eq!(result.identity.email, None);
    assert_eq!(result.identity.email_verified, None);
    assert_eq!(result.identity.avatar_url, None);
    Ok(())
}

async fn test_client(
    provider: &FakeProvider,
    clock: Arc<TestClock>,
    cipher: Arc<RecordingCipher>,
) -> Result<OAuth2ProtocolClient, OAuth2ProtocolError> {
    let base = provider.base.as_str().trim_end_matches('/');
    OAuth2ProtocolClient::new(
        OAuth2ConnectorSettings {
            authorization_endpoint: Url::parse(&format!("{base}/authorize"))
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
            token_endpoint: Url::parse(&format!("{base}/token"))
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
            user_info_endpoint: Url::parse(&format!("{base}/userinfo"))
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
            client_id: "pvlog-oauth2-test".to_owned(),
            client_secret_ref: "secret://oauth2/test".to_owned(),
            client_auth_method: OAuth2ClientAuthMethod::RequestBody,
            redirect_uri: Url::parse("https://pvlog.example/api/v1/auth/connectors/callback")
                .map_err(|_| OAuth2ProtocolError::InvalidConfiguration)?,
            scopes: vec!["profile".to_owned(), "email".to_owned()],
            claim_mappings: OAuth2ClaimMappings {
                subject: "profile.id".to_owned(),
                display_name: Some("profile.name".to_owned()),
                email: Some("profile.email".to_owned()),
                email_verified: Some("profile.email_verified".to_owned()),
                avatar_url: Some("profile.avatar".to_owned()),
            },
            flow_lifetime_seconds: 300,
        },
        clock,
        Arc::new(TestSecretResolver),
        cipher,
    )
    .await
}

struct TestSecretResolver;

#[async_trait]
impl SecretResolver for TestSecretResolver {
    async fn resolve(&self, secret_reference: &str) -> Result<SecretString, PortError> {
        if secret_reference == "secret://oauth2/test" {
            Ok(SecretString::from(CLIENT_SECRET))
        } else {
            Err(PortError::NotFound)
        }
    }
}

#[derive(Default)]
struct RecordingCipher(Mutex<Vec<(ProviderTokenKind, String)>>);

impl RecordingCipher {
    fn plaintexts(&self) -> Result<Vec<(ProviderTokenKind, String)>, Box<dyn Error>> {
        Ok(self.0.lock().map_err(|_| "cipher mutex poisoned")?.clone())
    }
}

#[async_trait]
impl TokenCipher for RecordingCipher {
    async fn encrypt(
        &self,
        kind: ProviderTokenKind,
        plaintext: &SecretString,
    ) -> Result<EncryptedProviderToken, PortError> {
        self.0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push((kind, plaintext.expose_secret().to_owned()));
        Ok(EncryptedProviderToken {
            key_id: "test-key-v1".to_owned(),
            ciphertext: format!("protected:{kind:?}:{}", plaintext.expose_secret()).into_bytes(),
        })
    }
}

struct TestClock(Mutex<UtcTimestamp>);

impl TestClock {
    fn new(now: UtcTimestamp) -> Self {
        Self(Mutex::new(now))
    }

    fn set(&self, now: UtcTimestamp) -> Result<(), Box<dyn Error>> {
        *self.0.lock().map_err(|_| "clock mutex poisoned")? = now;
        Ok(())
    }
}

impl Clock for TestClock {
    fn now(&self) -> UtcTimestamp {
        self.0
            .lock()
            .map_or_else(|poisoned| **poisoned.get_ref(), |now| *now)
    }
}

#[derive(Clone)]
struct ProviderState {
    expected_challenge: Arc<Mutex<String>>,
    user_info: Arc<Mutex<Value>>,
}

struct FakeProvider {
    base: Url,
    expected_challenge: Arc<Mutex<String>>,
    user_info: Arc<Mutex<Value>>,
}

impl FakeProvider {
    async fn start() -> Result<Self, Box<dyn Error>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let base = Url::parse(&format!("http://{}", listener.local_addr()?))?;
        let expected_challenge = Arc::new(Mutex::new(String::new()));
        let user_info = Arc::new(Mutex::new(json!({
            "profile": {
                "id": "stable-subject",
                "name": "Solar Owner",
                "email": "solar@example.test",
                "email_verified": true,
                "avatar": "https://cdn.example.test/avatar.png"
            }
        })));
        let router = Router::new()
            .route("/authorize", get(|| async { "authorization" }))
            .route("/token", post(token_response))
            .route("/userinfo", get(user_info_response))
            .with_state(ProviderState {
                expected_challenge: expected_challenge.clone(),
                user_info: user_info.clone(),
            });
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            base,
            expected_challenge,
            user_info,
        })
    }

    fn expect_challenge(&self, challenge: &str) -> Result<(), Box<dyn Error>> {
        let mut expected = self
            .expected_challenge
            .lock()
            .map_err(|_| "challenge mutex poisoned")?;
        challenge.clone_into(&mut *expected);
        Ok(())
    }

    fn set_user_info(&self, value: Value) -> Result<(), Box<dyn Error>> {
        *self
            .user_info
            .lock()
            .map_err(|_| "user-info mutex poisoned")? = value;
        Ok(())
    }
}

async fn token_response(
    State(state): State<ProviderState>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    let parameters: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(&body).into_owned().collect();
    let valid = parameters.get("grant_type").map(String::as_str) == Some("authorization_code")
        && parameters.get("client_id").map(String::as_str) == Some("pvlog-oauth2-test")
        && parameters.get("client_secret").map(String::as_str) == Some(CLIENT_SECRET)
        && parameters.get("code").is_some_and(|code| !code.is_empty())
        && parameters.get("code_verifier").is_some_and(|verifier| {
            let challenge = PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(
                verifier.to_owned(),
            ));
            state
                .expected_challenge
                .lock()
                .is_ok_and(|expected| challenge.as_str() == expected.as_str())
        });
    if !valid {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "access_token": ACCESS_TOKEN,
            "refresh_token": REFRESH_TOKEN,
            "token_type": "Bearer",
            "expires_in": 300,
            "scope": "profile email"
        })),
    )
}

async fn user_info_response(
    State(state): State<ProviderState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some("Bearer provider-access-token-plain")
    {
        return (StatusCode::UNAUTHORIZED, Json(json!({})));
    }
    (
        StatusCode::OK,
        Json(
            state
                .user_info
                .lock()
                .map_or_else(|_| json!({}), |value| value.clone()),
        ),
    )
}
