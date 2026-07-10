//! Local-user lifecycle HTTP adapter.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, post},
};
use pvlog_application::{
    AdminUserActor, CreateLocalUser, InviteLocalUser, RegisterLocalUser, UserLifecycleError,
    UserLifecycleUseCases,
};
use pvlog_domain::UserId;
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct LifecycleApiState {
    service: Arc<dyn UserLifecycleUseCases>,
}

/// Creates the local-user administration and public activation routes.
pub fn user_lifecycle_router(service: Arc<dyn UserLifecycleUseCases>) -> Router {
    Router::new()
        .route("/api/v1/admin/users", post(create_user))
        .route("/api/v1/admin/user-invitations", post(invite_user))
        .route("/api/v1/admin/users/{id}/activate", post(activate_user))
        .route("/api/v1/admin/users/{id}/disable", post(disable_user))
        .route("/api/v1/admin/users/{id}/unlock", post(unlock_user))
        .route("/api/v1/admin/users/{id}", delete(delete_user))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/invitations/accept", post(accept_invitation))
        .with_state(LifecycleApiState { service })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserBody {
    email: String,
    display_name: String,
    #[serde(default)]
    email_verified: bool,
}

#[derive(Debug, Deserialize)]
struct InviteUserBody {
    email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterBody {
    email: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptInvitationBody {
    token: SecretString,
    display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationBody {
    #[serde(default)]
    email_verified: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationResponse {
    invitation_id: pvlog_domain::UserInvitationId,
    activation_token: String,
    expires_at: i64,
}

#[derive(Debug, Serialize)]
struct AcceptedResponse {
    status: &'static str,
}

async fn create_user(
    State(state): State<LifecycleApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Json(body): Json<CreateUserBody>,
) -> Result<Response, LifecycleApiError> {
    let user = state
        .service
        .create_user(
            admin(actor)?,
            CreateLocalUser {
                email: body.email,
                display_name: body.display_name,
                email_verified: body.email_verified,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(user)).into_response())
}

async fn invite_user(
    State(state): State<LifecycleApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Json(body): Json<InviteUserBody>,
) -> Result<Response, LifecycleApiError> {
    let result = state
        .service
        .invite_user(admin(actor)?, InviteLocalUser { email: body.email })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(InvitationResponse {
            invitation_id: result.invitation_id,
            activation_token: result.token.expose_secret().to_owned(),
            expires_at: result.expires_at,
        }),
    )
        .into_response())
}

async fn register(
    State(state): State<LifecycleApiState>,
    Json(body): Json<RegisterBody>,
) -> Result<Response, LifecycleApiError> {
    state
        .service
        .register(RegisterLocalUser {
            email: body.email,
            display_name: body.display_name,
        })
        .await?;
    Ok(accepted())
}

async fn accept_invitation(
    State(state): State<LifecycleApiState>,
    Json(body): Json<AcceptInvitationBody>,
) -> Result<Response, LifecycleApiError> {
    state
        .service
        .accept_invitation(body.token, body.display_name)
        .await?;
    Ok(accepted())
}

async fn activate_user(
    State(state): State<LifecycleApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Path(id): Path<UserId>,
    Json(body): Json<ActivationBody>,
) -> Result<Response, LifecycleApiError> {
    let user = state
        .service
        .activate(admin(actor)?, id, body.email_verified)
        .await?;
    Ok(Json(user).into_response())
}

async fn disable_user(
    State(state): State<LifecycleApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Path(id): Path<UserId>,
) -> Result<Response, LifecycleApiError> {
    let user = state.service.disable(admin(actor)?, id).await?;
    Ok(Json(user).into_response())
}

async fn unlock_user(
    State(state): State<LifecycleApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Path(id): Path<UserId>,
) -> Result<Response, LifecycleApiError> {
    let user = state.service.unlock(admin(actor)?, id).await?;
    Ok(Json(user).into_response())
}

async fn delete_user(
    State(state): State<LifecycleApiState>,
    actor: Option<Extension<AdminUserActor>>,
    Path(id): Path<UserId>,
) -> Result<Response, LifecycleApiError> {
    state.service.delete(admin(actor)?, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn admin(actor: Option<Extension<AdminUserActor>>) -> Result<AdminUserActor, LifecycleApiError> {
    actor
        .map(|Extension(actor)| actor)
        .ok_or(UserLifecycleError::Forbidden.into())
}

fn accepted() -> Response {
    (
        StatusCode::ACCEPTED,
        Json(AcceptedResponse { status: "accepted" }),
    )
        .into_response()
}

struct LifecycleApiError(UserLifecycleError);

impl From<UserLifecycleError> for LifecycleApiError {
    fn from(value: UserLifecycleError) -> Self {
        Self(value)
    }
}

impl IntoResponse for LifecycleApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            UserLifecycleError::Forbidden | UserLifecycleError::SelfAdministrationDenied => {
                (StatusCode::FORBIDDEN, "forbidden")
            }
            UserLifecycleError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            UserLifecycleError::Conflict => (StatusCode::CONFLICT, "conflict"),
            UserLifecycleError::RegistrationDisabled => {
                (StatusCode::FORBIDDEN, "registration_disabled")
            }
            UserLifecycleError::EmailVerificationRequired => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "email_verification_required",
            ),
            UserLifecycleError::InvalidInput(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "invalid_request")
            }
            UserLifecycleError::Persistence => {
                (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable")
            }
            UserLifecycleError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        (status, Json(ErrorResponse { code })).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
}
