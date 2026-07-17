//! Interactive local-login, browser-session bootstrap, and logout endpoints.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{
    AuthenticatePassword, AuthenticationOutcome, BrowserSession, BrowserSessionError,
    BrowserSessionUseCases, LocalPasswordUseCases, PasswordServiceError,
};
use pvlog_domain::UserId;
use secrecy::ExposeSecret as _;
use serde::Serialize;

use crate::{RequestPrincipal, session_cookie_token};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBootstrap {
    pub authenticated: bool,
    pub user: Option<SessionUser>,
    pub account_id: Option<pvlog_domain::AccountId>,
    pub system_ids: Vec<pvlog_domain::SystemId>,
    pub permissions: Vec<String>,
    pub connectors: Vec<SessionConnector>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUser {
    pub id: UserId,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConnector {
    pub id: String,
    pub name: String,
    pub authorization_url: String,
}

#[async_trait]
pub trait SessionBootstrapUseCases: Send + Sync {
    async fn bootstrap(&self, user_id: UserId) -> Result<SessionBootstrap, SessionApiError>;
}

#[derive(Clone)]
struct SessionState {
    passwords: Arc<dyn LocalPasswordUseCases>,
    sessions: Arc<dyn BrowserSessionUseCases>,
    bootstrap: Arc<dyn SessionBootstrapUseCases>,
}

pub fn sessions_router(
    passwords: Arc<dyn LocalPasswordUseCases>,
    sessions: Arc<dyn BrowserSessionUseCases>,
    bootstrap: Arc<dyn SessionBootstrapUseCases>,
) -> Router {
    Router::new()
        .route("/api/v1/session", get(session).post(logout))
        .route("/api/v1/auth/local/login", post(login))
        .with_state(SessionState {
            passwords,
            sessions,
            bootstrap,
        })
}

#[derive(serde::Deserialize)]
struct LoginBody {
    email: String,
    password: secrecy::SecretString,
}

async fn session(
    State(state): State<SessionState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<SessionBootstrap>, SessionApiError> {
    let Some(Extension(RequestPrincipal::User(user_id))) = principal else {
        return Ok(Json(anonymous_bootstrap()));
    };
    Ok(Json(state.bootstrap.bootstrap(user_id).await?))
}

async fn login(
    State(state): State<SessionState>,
    Json(body): Json<LoginBody>,
) -> Result<Response, SessionApiError> {
    let AuthenticationOutcome::Authenticated(user_id) = state
        .passwords
        .authenticate(AuthenticatePassword {
            email: body.email,
            password: body.password,
        })
        .await?
    else {
        return Err(SessionApiError::Rejected);
    };
    let browser_session = state.sessions.issue(user_id).await?;
    let bootstrap = state.bootstrap.bootstrap(user_id).await?;
    Ok(login_response(&browser_session, bootstrap))
}

async fn logout(
    State(state): State<SessionState>,
    headers: HeaderMap,
) -> Result<StatusCode, SessionApiError> {
    let token = session_cookie_token(&headers).ok_or(SessionApiError::Rejected)?;
    state.sessions.logout(&token).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn anonymous_bootstrap() -> SessionBootstrap {
    SessionBootstrap {
        authenticated: false,
        user: None,
        account_id: None,
        system_ids: Vec::new(),
        permissions: Vec::new(),
        connectors: Vec::new(),
    }
}

fn login_response(browser_session: &BrowserSession, bootstrap: SessionBootstrap) -> Response {
    let mut response = (StatusCode::OK, Json(bootstrap)).into_response();
    let secure = if browser_session.session_cookie.secure {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}={}; Path={}; Max-Age={}; HttpOnly; SameSite={}{}",
        browser_session.session_cookie.name,
        browser_session.session_cookie.value.expose_secret(),
        browser_session.session_cookie.path,
        browser_session.session_cookie.max_age_seconds,
        browser_session.session_cookie.same_site,
        secure,
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    if let Ok(value) = HeaderValue::from_str(browser_session.csrf_token.expose_secret()) {
        response.headers_mut().insert("x-csrf-token", value);
    }
    response
}

#[derive(Debug)]
pub enum SessionApiError {
    Rejected,
    Password(PasswordServiceError),
    Session(BrowserSessionError),
    Bootstrap,
}

impl From<PasswordServiceError> for SessionApiError {
    fn from(value: PasswordServiceError) -> Self {
        Self::Password(value)
    }
}
impl From<BrowserSessionError> for SessionApiError {
    fn from(value: BrowserSessionError) -> Self {
        Self::Session(value)
    }
}
impl IntoResponse for SessionApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Rejected | Self::Password(PasswordServiceError::CurrentCredentialRejected) => {
                StatusCode::UNAUTHORIZED
            }
            Self::Password(PasswordServiceError::Persistence)
            | Self::Session(BrowserSessionError::Repository(_) | BrowserSessionError::Time)
            | Self::Bootstrap => StatusCode::SERVICE_UNAVAILABLE,
            Self::Password(_) | Self::Session(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
        .into_response()
    }
}
