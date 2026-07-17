//! HTTP credential extraction for bearer tokens and browser session cookies.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

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

const SECURE_SESSION_COOKIE: &str = "__Host-pvlog_session";
const DEVELOPMENT_SESSION_COOKIE: &str = "pvlog_session";
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

impl RequestPrincipal {
    /// Returns a stable non-secret identity for quotas, idempotency, and audit correlation.
    #[must_use]
    pub fn safe_ingestion_identity(&self) -> String {
        match self {
            Self::User(id) => format!("user:{id}"),
            Self::ApiCredential { id, .. } => format!("api_credential:{id}"),
        }
    }
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
        AuthenticationState {
            service,
            ingestion_quota: Arc::new(IngestionQuota::default()),
        },
        authenticate,
    ))
}

#[derive(Clone)]
struct AuthenticationState {
    service: Arc<dyn RequestAuthenticator>,
    ingestion_quota: Arc<IngestionQuota>,
}

async fn authenticate(
    State(state): State<AuthenticationState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let state_changing = is_state_changing(request.method());
    let ingestion_route = is_ingestion_route(&path);
    if ingestion_route && has_credential_query(request.uri().query()) {
        return authentication_error(&request, RequestAuthenticationError::Invalid);
    }
    let bearer = match bearer_token(request.headers()) {
        Ok(token) => token,
        Err(error) => {
            return authentication_error(&request, error);
        }
    };
    let used_session_cookie = bearer.is_none() && session_cookie_token(request.headers()).is_some();
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
            if ingestion_route && let Err(error) = state.ingestion_quota.admit(&principal) {
                return error.into_response();
            }
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Ok(None) => next.run(request).await,
        Err(RequestAuthenticationError::Invalid)
            if used_session_cookie && allows_stale_session_cookie(&method, &path) =>
        {
            let mut response = next.run(request).await;
            if method == Method::GET && path == "/api/v1/session" {
                expire_session_cookies(response.headers_mut());
            }
            response
        }
        Err(error) => authentication_error(&request, error),
    }
}

const INGESTION_REQUESTS_PER_MINUTE: u32 = 600;

#[derive(Default)]
struct IngestionQuota {
    windows: Mutex<HashMap<String, (u64, u32)>>,
}

impl IngestionQuota {
    fn admit(&self, principal: &RequestPrincipal) -> Result<(), IngestionQuotaError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| IngestionQuotaError::Unavailable)?
            .as_secs();
        let window = now / 60;
        let mut windows = self
            .windows
            .lock()
            .map_err(|_| IngestionQuotaError::Unavailable)?;
        let entry = windows
            .entry(principal.safe_ingestion_identity())
            .or_insert((window, 0));
        if entry.0 != window {
            *entry = (window, 0);
        }
        entry.1 = entry.1.saturating_add(1);
        if entry.1 <= INGESTION_REQUESTS_PER_MINUTE {
            return Ok(());
        }
        Err(IngestionQuotaError::Limited {
            retry_after: 60 - now % 60,
        })
    }
}

enum IngestionQuotaError {
    Unavailable,
    Limited { retry_after: u64 },
}

impl IngestionQuotaError {
    fn into_response(self) -> Response {
        let Self::Limited { retry_after } = self else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
        if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response.headers_mut().insert(
            "ratelimit-limit",
            axum::http::HeaderValue::from_static("600"),
        );
        response.headers_mut().insert(
            "ratelimit-remaining",
            axum::http::HeaderValue::from_static("0"),
        );
        response
    }
}

fn is_state_changing(method: &Method) -> bool {
    !matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

fn authentication_error(request: &Request, error: RequestAuthenticationError) -> Response {
    let (status, title) = match error {
        RequestAuthenticationError::Invalid => (StatusCode::UNAUTHORIZED, "authentication_failed"),
        RequestAuthenticationError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication_unavailable",
        ),
    };
    crate::problem::problem(
        request,
        status,
        title,
        "The request could not be authenticated.",
    )
}

fn is_ingestion_route(path: &str) -> bool {
    let Some(path) = path.strip_prefix("/api/v1/systems/") else {
        return false;
    };
    let mut segments = path.split('/');
    segments.next().is_some()
        && segments.next() == Some("observations")
        && matches!(
            (segments.next(), segments.next()),
            (None | Some("batch"), None)
        )
}

fn has_credential_query(query: Option<&str>) -> bool {
    query.is_some_and(|query| {
        query.split('&').any(|pair| {
            let name = pair.split_once('=').map_or(pair, |(name, _)| name);
            matches!(
                name.to_ascii_lowercase().as_str(),
                "api_key" | "api-key" | "ingestion_key" | "ingestion-key"
            )
        })
    })
}

fn allows_stale_session_cookie(method: &Method, path: &str) -> bool {
    (method == Method::GET && path == "/api/v1/session")
        || (method == Method::POST
            && matches!(
                path,
                "/api/v1/auth/local/login"
                    | "/api/v1/auth/register"
                    | "/api/v1/auth/invitations/accept"
                    | "/api/v1/auth/password-recovery"
            ))
}

fn expire_session_cookies(headers: &mut axum::http::HeaderMap) {
    for cookie in [
        "pvlog_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax",
        "__Host-pvlog_session=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax",
    ] {
        if let Ok(value) = axum::http::HeaderValue::from_str(cookie) {
            headers.append(header::SET_COOKIE, value);
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
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    [SECURE_SESSION_COOKIE, DEVELOPMENT_SESSION_COOKIE]
        .into_iter()
        .find_map(|name| {
            cookie
                .split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix(&format!("{name}=")))
        })
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
