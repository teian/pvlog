use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use pvlog_api::{
    SessionApiError, SessionBootstrap, SessionBootstrapUseCases, SessionConnector, SessionUser,
    sessions_router,
};
use pvlog_application::{
    AuthenticatePassword, AuthenticationOutcome, BrowserSession, BrowserSessionError,
    BrowserSessionRecord, BrowserSessionUseCases, ChangePassword, LocalPasswordUseCases,
    PasswordServiceError, PublicLifecycleOutcome, SetInitialPassword,
};
use pvlog_domain::UserId;
use secrecy::SecretString;
use tower::ServiceExt as _;

#[tokio::test]
async fn local_login_issues_host_cookie_and_session_bootstrap() -> Result<(), Box<dyn Error>> {
    let user_id = UserId::new();
    let app = Router::new().merge(sessions_router(
        Arc::new(Passwords { user_id }),
        Arc::new(Sessions { user_id }),
        Arc::new(Bootstrap),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"user@example.test","password":"secret"}"#,
                ))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("__Host-pvlog_session=session-token;"))
    );
    assert_eq!(
        response
            .headers()
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok()),
        Some("csrf-token")
    );
    Ok(())
}

#[tokio::test]
async fn logout_revokes_the_cookie_addressed_browser_session() -> Result<(), Box<dyn Error>> {
    let user_id = UserId::new();
    let app = Router::new().merge(sessions_router(
        Arc::new(Passwords { user_id }),
        Arc::new(Sessions { user_id }),
        Arc::new(Bootstrap),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/session")
                .header("cookie", "__Host-pvlog_session=session-token")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    Ok(())
}

struct Passwords {
    user_id: UserId,
}

#[async_trait]
impl LocalPasswordUseCases for Passwords {
    async fn set_initial_password(
        &self,
        _actor: pvlog_application::AdminUserActor,
        _command: SetInitialPassword,
    ) -> Result<(), PasswordServiceError> {
        Ok(())
    }
    async fn authenticate(
        &self,
        _command: AuthenticatePassword,
    ) -> Result<AuthenticationOutcome, PasswordServiceError> {
        Ok(AuthenticationOutcome::Authenticated(self.user_id))
    }
    async fn change_password(&self, _command: ChangePassword) -> Result<(), PasswordServiceError> {
        Ok(())
    }
    async fn request_recovery(
        &self,
        _email: String,
    ) -> Result<PublicLifecycleOutcome, PasswordServiceError> {
        Ok(PublicLifecycleOutcome::Accepted)
    }
    async fn complete_recovery(
        &self,
        _token: SecretString,
        _new_password: SecretString,
    ) -> Result<PublicLifecycleOutcome, PasswordServiceError> {
        Ok(PublicLifecycleOutcome::Accepted)
    }
}

struct Sessions {
    user_id: UserId,
}

#[async_trait]
impl BrowserSessionUseCases for Sessions {
    async fn issue(&self, _user_id: UserId) -> Result<BrowserSession, BrowserSessionError> {
        Ok(BrowserSession {
            user_id: self.user_id,
            session_cookie: pvlog_application::SessionCookie {
                name: "__Host-pvlog_session",
                value: SecretString::from("session-token"),
                http_only: true,
                secure: true,
                same_site: "Lax",
                path: "/",
            },
            csrf_token: SecretString::from("csrf-token"),
            idle_expires_at: 0,
            absolute_expires_at: 0,
        })
    }
    async fn authenticate(
        &self,
        _session_token: &SecretString,
        _csrf_token: Option<&SecretString>,
        _state_changing: bool,
    ) -> Result<BrowserSessionRecord, BrowserSessionError> {
        Err(BrowserSessionError::InvalidSession)
    }
    async fn rotate(
        &self,
        _session_token: &SecretString,
    ) -> Result<BrowserSession, BrowserSessionError> {
        Err(BrowserSessionError::InvalidSession)
    }
    async fn logout(&self, _session_token: &SecretString) -> Result<(), BrowserSessionError> {
        Ok(())
    }
}

struct Bootstrap;
#[async_trait]
impl SessionBootstrapUseCases for Bootstrap {
    async fn bootstrap(&self, user_id: UserId) -> Result<SessionBootstrap, SessionApiError> {
        Ok(SessionBootstrap {
            authenticated: true,
            user: Some(SessionUser {
                id: user_id,
                display_name: "User".to_owned(),
            }),
            account_id: None,
            system_ids: Vec::new(),
            permissions: Vec::new(),
            connectors: Vec::<SessionConnector>::new(),
        })
    }
}
