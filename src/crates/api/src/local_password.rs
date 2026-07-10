//! Local password change, initialization, and enumeration-resistant recovery endpoints.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{post, put},
};
use pvlog_application::{
    AdminUserActor, ChangePassword, LocalPasswordUseCases, PasswordServiceError, SetInitialPassword,
};
use pvlog_domain::UserId;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct PasswordApiState {
    service: Arc<dyn LocalPasswordUseCases>,
}

pub fn local_password_router(service: Arc<dyn LocalPasswordUseCases>) -> Router {
    Router::new()
        .route(
            "/api/v1/admin/users/{id}/password",
            post(set_initial_password),
        )
        .route("/api/v1/auth/password", put(change_password))
        .route("/api/v1/auth/password-recovery", post(request_recovery))
        .route(
            "/api/v1/auth/password-recovery/complete",
            post(complete_recovery),
        )
        .with_state(PasswordApiState { service })
}

#[derive(Debug, Deserialize)]
struct PasswordBody {
    password: SecretString,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordBody {
    current_password: SecretString,
    new_password: SecretString,
}

#[derive(Debug, Deserialize)]
struct RecoveryRequestBody {
    email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryCompleteBody {
    token: SecretString,
    new_password: SecretString,
}

async fn set_initial_password(
    State(state): State<PasswordApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Path(id): Path<UserId>,
    Json(body): Json<PasswordBody>,
) -> Result<Response, PasswordApiError> {
    state
        .service
        .set_initial_password(
            admin(actor)?,
            SetInitialPassword {
                user_id: id,
                password: body.password,
            },
        )
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn change_password(
    State(state): State<PasswordApiState>,
    user: Option<Extension<UserId>>,
    Json(body): Json<ChangePasswordBody>,
) -> Result<Response, PasswordApiError> {
    let user_id = user
        .map(|Extension(user_id)| user_id)
        .ok_or(PasswordServiceError::Forbidden)?;
    state
        .service
        .change_password(ChangePassword {
            user_id,
            current_password: body.current_password,
            new_password: body.new_password,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn request_recovery(
    State(state): State<PasswordApiState>,
    Json(body): Json<RecoveryRequestBody>,
) -> Result<Response, PasswordApiError> {
    state.service.request_recovery(body.email).await?;
    Ok(accepted())
}

async fn complete_recovery(
    State(state): State<PasswordApiState>,
    Json(body): Json<RecoveryCompleteBody>,
) -> Result<Response, PasswordApiError> {
    state
        .service
        .complete_recovery(body.token, body.new_password)
        .await?;
    Ok(accepted())
}

fn admin(actor: Option<Extension<AdminUserActor>>) -> Result<AdminUserActor, PasswordApiError> {
    actor
        .map(|Extension(actor)| actor)
        .ok_or(PasswordServiceError::Forbidden.into())
}

fn accepted() -> Response {
    (
        StatusCode::ACCEPTED,
        Json(AcceptedResponse { status: "accepted" }),
    )
        .into_response()
}

struct PasswordApiError(PasswordServiceError);

impl From<PasswordServiceError> for PasswordApiError {
    fn from(value: PasswordServiceError) -> Self {
        Self(value)
    }
}

impl IntoResponse for PasswordApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            PasswordServiceError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            PasswordServiceError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            PasswordServiceError::CurrentCredentialRejected => {
                (StatusCode::UNAUTHORIZED, "credential_rejected")
            }
            PasswordServiceError::Policy(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "password_policy_rejected")
            }
            PasswordServiceError::Persistence => {
                (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable")
            }
            PasswordServiceError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        (status, Json(ErrorResponse { code })).into_response()
    }
}

#[derive(Debug, Serialize)]
struct AcceptedResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
}
