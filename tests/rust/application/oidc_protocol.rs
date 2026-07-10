//! Fake-provider OIDC discovery, PKCE, state, nonce, and signed-token validation.

use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{Duration, Utc};
use openidconnect::{
    AccessToken, Audience, EmptyAdditionalClaims, IssuerUrl, Nonce, StandardClaims,
    SubjectIdentifier,
    core::{CoreHmacKey, CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm},
};
use pvlog_application::{
    Clock, OidcConnectorSettings, OidcProtocolClient, OidcProtocolError, PortError, SecretResolver,
};
use pvlog_domain::UtcTimestamp;
use secrecy::SecretString;
use serde_json::json;
use url::Url;

const CLIENT_ID: &str = "pvlog-oidc-test";
const CLIENT_SECRET: &str = "provider-neutral-client-secret-32";
const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
async fn oidc_authorization_code_flow_validates_every_correlation_boundary()
-> Result<(), Box<dyn Error>> {
    let provider = FakeProvider::start().await?;
    let clock = Arc::new(TestClock::new(UtcTimestamp::from_epoch_millis(NOW)?));
    let client = OidcProtocolClient::discover(
        OidcConnectorSettings {
            issuer: provider.issuer.clone(),
            client_id: CLIENT_ID.to_owned(),
            client_secret_ref: "secret://oidc/test".to_owned(),
            redirect_uri: Url::parse("https://pvlog.example/api/v1/auth/connectors/callback")?,
            scopes: vec![
                "openid".to_owned(),
                "profile".to_owned(),
                "email".to_owned(),
            ],
            flow_lifetime_seconds: 300,
        },
        clock.clone(),
        Arc::new(TestSecretResolver),
    )
    .await?;

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
    assert!(
        parameters
            .get("code_challenge")
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        parameters
            .get("nonce")
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(client.pending_flow_count(), 1);
    assert_eq!(
        client
            .complete_authorization("unknown-state", SecretString::from("code"))
            .await,
        Err(OidcProtocolError::FlowRejected)
    );

    provider.set_token(valid_token(
        &provider.issuer,
        parameters.get("nonce").ok_or("nonce missing")?,
        CLIENT_ID,
    )?)?;
    let claims = client
        .complete_authorization(
            &authorization.state_handle,
            SecretString::from("valid-code"),
        )
        .await?;
    assert_eq!(claims.subject, "stable-provider-subject");
    assert_eq!(claims.email.as_deref(), Some("solar@example.test"));
    assert_eq!(claims.email_verified, Some(true));
    assert_eq!(client.pending_flow_count(), 0);
    assert_eq!(
        client
            .complete_authorization(&authorization.state_handle, SecretString::from("replay"))
            .await,
        Err(OidcProtocolError::FlowRejected)
    );

    let invalid = client.begin_authorization()?;
    provider.set_token(valid_token(&provider.issuer, "wrong-nonce", CLIENT_ID)?)?;
    assert_eq!(
        client
            .complete_authorization(&invalid.state_handle, SecretString::from("invalid-code"))
            .await,
        Err(OidcProtocolError::TokenRejected)
    );

    let expired = client.begin_authorization()?;
    clock.set(UtcTimestamp::from_epoch_millis(NOW + 301_000)?)?;
    assert_eq!(
        client
            .complete_authorization(&expired.state_handle, SecretString::from("expired-code"))
            .await,
        Err(OidcProtocolError::FlowRejected)
    );
    Ok(())
}

struct TestClock(Mutex<UtcTimestamp>);

struct TestSecretResolver;

#[async_trait::async_trait]
impl SecretResolver for TestSecretResolver {
    async fn resolve(&self, secret_reference: &str) -> Result<SecretString, PortError> {
        if secret_reference == "secret://oidc/test" {
            Ok(SecretString::from(CLIENT_SECRET))
        } else {
            Err(PortError::NotFound)
        }
    }
}

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

fn valid_token(issuer: &Url, nonce: &str, audience: &str) -> Result<String, Box<dyn Error>> {
    let key = CoreHmacKey::new(CLIENT_SECRET);
    let issued_at = Utc::now();
    let token = CoreIdToken::new(
        CoreIdTokenClaims::new(
            IssuerUrl::new(issuer.to_string())?,
            vec![Audience::new(audience.to_owned())],
            issued_at + Duration::minutes(5),
            issued_at,
            StandardClaims::new(SubjectIdentifier::new("stable-provider-subject".to_owned()))
                .set_email(Some(openidconnect::EndUserEmail::new(
                    "solar@example.test".to_owned(),
                )))
                .set_email_verified(Some(true)),
            EmptyAdditionalClaims {},
        )
        .set_nonce(Some(Nonce::new(nonce.to_owned()))),
        &key,
        CoreJwsSigningAlgorithm::HmacSha256,
        None::<&AccessToken>,
        None,
    )?;
    Ok(token.to_string())
}

#[derive(Clone)]
struct ProviderState {
    issuer: Url,
    token: Arc<Mutex<String>>,
}

struct FakeProvider {
    issuer: Url,
    token: Arc<Mutex<String>>,
}

impl FakeProvider {
    async fn start() -> Result<Self, Box<dyn Error>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let issuer = Url::parse(&format!("http://{}", listener.local_addr()?))?;
        let token = Arc::new(Mutex::new(String::new()));
        let state = ProviderState {
            issuer: issuer.clone(),
            token: token.clone(),
        };
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/authorize", get(|| async { "authorization" }))
            .route("/token", post(token_response))
            .route("/jwks", get(|| async { axum::Json(json!({"keys": []})) }))
            .with_state(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self { issuer, token })
    }

    fn set_token(&self, token: String) -> Result<(), Box<dyn Error>> {
        *self.token.lock().map_err(|_| "token mutex poisoned")? = token;
        Ok(())
    }
}

async fn discovery(State(state): State<ProviderState>) -> impl IntoResponse {
    let base = state.issuer.as_str().trim_end_matches('/');
    axum::Json(json!({
        "issuer": state.issuer,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "jwks_uri": format!("{base}/jwks"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["HS256"],
        "code_challenge_methods_supported": ["S256"]
    }))
}

async fn token_response(State(state): State<ProviderState>, _body: Bytes) -> impl IntoResponse {
    let token = state
        .token
        .lock()
        .map_or_else(|_| String::new(), |token| token.clone());
    axum::Json(json!({
        "access_token": "provider-access-token",
        "token_type": "Bearer",
        "expires_in": 300,
        "id_token": token
    }))
}
