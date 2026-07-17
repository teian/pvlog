use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode},
    routing::get,
};
use pvlog_api::{
    RequestAuthenticationError, RequestAuthenticator, RequestPrincipal, with_request_authentication,
};
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, UserId};
use secrecy::{ExposeSecret as _, SecretString};
use tower::ServiceExt as _;

#[tokio::test]
async fn bearer_and_session_credentials_are_extracted_and_invalid_credentials_fail_closed()
-> Result<(), Box<dyn Error>> {
    let app = with_request_authentication(
        Router::new().route("/protected", get(protected).post(protected)),
        Arc::new(Authenticator),
    );

    for request in [
        Request::builder()
            .uri("/protected")
            .header("authorization", "Bearer valid-bearer")
            .body(Body::empty())?,
        Request::builder()
            .method(Method::POST)
            .uri("/protected")
            .header("cookie", "other=value; __Host-pvlog_session=valid-session")
            .header("x-csrf-token", "valid-csrf")
            .body(Body::empty())?,
        Request::builder()
            .uri("/protected")
            .header("cookie", "pvlog_session=valid-session")
            .body(Body::empty())?,
    ] {
        assert_eq!(app.clone().oneshot(request).await?.status(), StatusCode::OK);
    }

    for request in [
        Request::builder()
            .uri("/protected")
            .header("authorization", "Bearer invalid")
            .body(Body::empty())?,
        Request::builder()
            .uri("/protected")
            .header("authorization", "Basic legacy")
            .body(Body::empty())?,
    ] {
        assert_eq!(
            app.clone().oneshot(request).await?.status(),
            StatusCode::UNAUTHORIZED
        );
    }
    Ok(())
}

#[tokio::test]
async fn stale_session_cookie_recovers_on_public_session_bootstrap_and_login()
-> Result<(), Box<dyn Error>> {
    let app = with_request_authentication(
        Router::new()
            .route("/api/v1/session", get(public))
            .route("/api/v1/auth/local/login", axum::routing::post(public)),
        Arc::new(Authenticator),
    );
    let bootstrap = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/session")
                .header("cookie", "pvlog_session=expired-session")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(bootstrap.status(), StatusCode::OK);
    assert_eq!(bootstrap.headers().get_all("set-cookie").iter().count(), 2);

    let login = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/local/login")
                .header("cookie", "pvlog_session=expired-session")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(login.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn telemetry_authentication_accepts_bearer_only_and_rejects_query_credentials()
-> Result<(), Box<dyn Error>> {
    let system = pvlog_domain::SystemId::new();
    let app = with_request_authentication(
        Router::new().route(
            "/api/v1/systems/{system_id}/observations",
            axum::routing::post(protected),
        ),
        Arc::new(Authenticator),
    );
    let canonical = format!("/api/v1/systems/{system}/observations");

    let accepted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(&canonical)
                .header("authorization", "Bearer valid-bearer")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(accepted.status(), StatusCode::OK);

    let legacy_header = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(&canonical)
                .header("x-pvlog-api-key", "retired-key")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(legacy_header.status(), StatusCode::UNAUTHORIZED);

    for name in ["api_key", "ingestion_key"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("{canonical}?{name}=retired-key"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
    }
    Ok(())
}

#[tokio::test]
async fn ingestion_quota_is_keyed_by_safe_api_credential_id() -> Result<(), Box<dyn Error>> {
    let system = pvlog_domain::SystemId::new();
    let app = with_request_authentication(
        Router::new().route(
            "/api/v1/systems/{system_id}/observations",
            axum::routing::post(protected),
        ),
        Arc::new(QuotaAuthenticator {
            credential_id: ApiCredentialId::new(),
        }),
    );
    let uri = format!("/api/v1/systems/{system}/observations");
    for _ in 0..600 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(&uri)
                    .header("authorization", "Bearer upload-key")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let limited = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(&uri)
                .header("authorization", "Bearer upload-key")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key("retry-after"));
    assert_eq!(limited.headers()["ratelimit-limit"], "600");
    Ok(())
}

async fn protected(principal: Option<Extension<RequestPrincipal>>) -> StatusCode {
    if principal.is_some() {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

async fn public() -> StatusCode {
    StatusCode::OK
}

struct Authenticator;

struct QuotaAuthenticator {
    credential_id: ApiCredentialId,
}

#[async_trait]
impl RequestAuthenticator for QuotaAuthenticator {
    async fn authenticate_bearer(
        &self,
        _token: SecretString,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        Ok(RequestPrincipal::ApiCredential {
            id: self.credential_id,
            owner_user_id: UserId::new(),
            account_id: AccountId::new(),
            system_id: None,
            scopes: BTreeSet::from([ApiScope::TelemetryWrite]),
        })
    }

    async fn authenticate_session(
        &self,
        _session_token: SecretString,
        _csrf_token: Option<SecretString>,
        _state_changing: bool,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        Err(RequestAuthenticationError::Invalid)
    }
}

#[async_trait]
impl RequestAuthenticator for Authenticator {
    async fn authenticate_bearer(
        &self,
        token: SecretString,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        if token.expose_secret() == "valid-bearer" {
            Ok(RequestPrincipal::ApiCredential {
                id: ApiCredentialId::new(),
                owner_user_id: UserId::new(),
                account_id: AccountId::new(),
                system_id: None,
                scopes: BTreeSet::from([ApiScope::TelemetryWrite]),
            })
        } else {
            Err(RequestAuthenticationError::Invalid)
        }
    }

    async fn authenticate_session(
        &self,
        session_token: SecretString,
        csrf_token: Option<SecretString>,
        state_changing: bool,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        if session_token.expose_secret() == "valid-session"
            && (!state_changing
                || csrf_token
                    .as_ref()
                    .is_some_and(|token| token.expose_secret() == "valid-csrf"))
        {
            Ok(RequestPrincipal::User(UserId::new()))
        } else {
            Err(RequestAuthenticationError::Invalid)
        }
    }
}
