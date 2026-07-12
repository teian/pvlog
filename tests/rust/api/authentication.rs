use std::{collections::BTreeSet, convert::Infallible, error::Error, sync::Arc};

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
use pvlog_domain::{ApiScope, UserId};
use secrecy::{ExposeSecret as _, SecretString};
use tower::ServiceExt as _;

#[tokio::test]
async fn bearer_and_session_credentials_are_extracted_and_invalid_credentials_fail_closed()
-> Result<(), Box<dyn Error>> {
    let app = with_request_authentication(
        Router::new().route("/protected", get(protected).post(protected)),
        Arc::new(Authenticator),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("authorization", "Bearer valid-bearer")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/protected")
                .header("cookie", "other=value; __Host-pvlog_session=valid-session")
                .header("x-csrf-token", "valid-csrf")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("cookie", "pvlog_session=valid-session")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("authorization", "Bearer invalid")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("authorization", "Basic legacy")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn stale_session_cookie_recovers_on_session_bootstrap_and_login() -> Result<(), Box<dyn Error>>
{
    let app = with_request_authentication(
        Router::new()
            .route("/api/v1/session", get(protected))
            .route("/api/v1/auth/local/login", axum::routing::post(protected)),
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

async fn protected(
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<&'static str, Infallible> {
    let present = principal.is_some();
    Ok(if present {
        "authenticated"
    } else {
        "anonymous"
    })
}

struct Authenticator;

#[async_trait]
impl RequestAuthenticator for Authenticator {
    async fn authenticate_bearer(
        &self,
        token: SecretString,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        if token.expose_secret() == "valid-bearer" {
            Ok(RequestPrincipal::ApiCredential {
                id: pvlog_domain::ApiCredentialId::new(),
                owner_user_id: UserId::new(),
                account_id: pvlog_domain::AccountId::new(),
                system_id: None,
                scopes: BTreeSet::from([ApiScope::SystemsRead]),
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
