//! HTTP credential extraction for bearer tokens and browser session cookies.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use axum::{
    Router,
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, SystemId, UserId};
use secrecy::SecretString;

const SESSION_COOKIE: &str = "__Host-pvlog_session";
const CSRF_HEADER: &str = "x-csrf-token";

/// Principal authenticated from an HTTP credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestPrincipal {
    User(UserId),
    ApiCredential {
        id: ApiCredentialId,
        owner_user_id: UserId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        scopes: BTreeSet<ApiScope>,
    },
}

/// Backend-neutral credential verification required by the HTTP adapter.
#[async_trait]
pub trait RequestAuthenticator: Send + Sync {
    async fn authenticate_bearer(
        &self,
        token: SecretString,
    ) -> Result<RequestPrincipal, RequestAuthenticationError>;
    async fn authenticate_session(
        &self,
        session_token: SecretString,
        csrf_token: Option<SecretString>,
        state_changing: bool,
    ) -> Result<RequestPrincipal, RequestAuthenticationError>;
}

/// Applies optional HTTP authentication globally.
///
/// Protected handlers must require [`RequestPrincipal`] explicitly. Invalid presented
/// credentials fail closed with `401`; unauthenticated requests remain available to public
/// endpoints.
pub fn with_request_authentication(
    router: Router,
    service: Arc<dyn RequestAuthenticator>,
) -> Router {
    router.layer(middleware::from_fn_with_state(
        AuthenticationState { service },
        authenticate,
    ))
}

#[derive(Clone)]
struct AuthenticationState {
    service: Arc<dyn RequestAuthenticator>,
}

async fn authenticate(
    State(state): State<AuthenticationState>,
    mut request: Request,
    next: Next,
) -> Response {
    let state_changing = !matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    );
    let bearer = match bearer_token(request.headers()) {
        Ok(token) => token,
        Err(RequestAuthenticationError::Invalid) => {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Err(RequestAuthenticationError::Unavailable) => {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let result = if let Some(token) = bearer {
        state.service.authenticate_bearer(token).await.map(Some)
    } else if let Some(session_token) = session_cookie_token(request.headers()) {
        state
            .service
            .authenticate_session(session_token, csrf_token(request.headers()), state_changing)
            .await
            .map(Some)
    } else {
        Ok(None)
    };

    match result {
        Ok(Some(principal)) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Ok(None) => next.run(request).await,
        Err(RequestAuthenticationError::Invalid) => StatusCode::UNAUTHORIZED.into_response(),
        Err(RequestAuthenticationError::Unavailable) => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

fn bearer_token(
    headers: &axum::http::HeaderMap,
) -> Result<Option<SecretString>, RequestAuthenticationError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| RequestAuthenticationError::Invalid)?;
    let token = value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty() && !token.chars().any(char::is_whitespace))
        .ok_or(RequestAuthenticationError::Invalid)?;
    Ok(Some(SecretString::from(token.to_owned())))
}

/// Extracts the browser session cookie without exposing its value in logs or responses.
#[must_use]
pub fn session_cookie_token(headers: &axum::http::HeaderMap) -> Option<SecretString> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{SESSION_COOKIE}=")))
        .filter(|value| !value.is_empty())
        .map(|value| SecretString::from(value.to_owned()))
}

fn csrf_token(headers: &axum::http::HeaderMap) -> Option<SecretString> {
    headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(|value| SecretString::from(value.to_owned()))
}

/// Failure category intentionally safe to return at the HTTP boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestAuthenticationError {
    Invalid,
    Unavailable,
}
