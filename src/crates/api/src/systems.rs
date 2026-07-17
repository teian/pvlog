use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{
    CreateSystem, SystemLifecycleError, SystemLifecycleRecord, SystemLifecycleUseCases,
    UpdateSystem,
};
use pvlog_domain::{AccountId, ApiScope, Permission, SystemId, UserId, Visibility};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    ModernRequestAuthorizer, Problem, RequestAuthorizationError, RequestPrincipal,
    principal_identity,
};

#[derive(Clone)]
struct SystemState {
    service: Arc<dyn SystemLifecycleUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}
pub fn systems_router(
    service: Arc<dyn SystemLifecycleUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems", post(create))
        .route("/api/v1/systems/{id}", get(read).put(update).delete(remove))
        .route("/api/v1/systems/{id}/archive", post(archive))
        .route("/api/v1/systems/{id}/restore", post(restore))
        .with_state(SystemState {
            service,
            authorizer,
        })
}

async fn read(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(id): Path<SystemId>,
) -> Result<Response, SystemApiError> {
    authorize_system(&state, principal, id, Permission::SystemRead, "system.read").await?;
    Ok(with_etag(StatusCode::OK, state.service.system(id).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBody {
    name: String,
    timezone: String,
}
#[derive(Deserialize)]
struct UpdateBody {
    name: String,
    timezone: String,
    visibility: Visibility,
}

async fn create(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(body): Json<CreateBody>,
) -> Result<Response, SystemApiError> {
    let (actor, account_id) = authorize_create(&state, principal).await?;
    let record = state
        .service
        .create_system(CreateSystem {
            account_id,
            actor,
            name: body.name,
            timezone: body.timezone,
        })
        .await?;
    Ok(with_etag(StatusCode::CREATED, record))
}
async fn update(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .update_system(UpdateSystem {
            id,
            actor: authorize_system(
                &state,
                principal,
                id,
                Permission::SystemManage,
                "system.update",
            )
            .await?,
            expected_version: expected_version(&headers)?,
            name: body.name,
            timezone: body.timezone,
            visibility: body.visibility,
        })
        .await?;
    Ok(with_etag(StatusCode::OK, record))
}
async fn archive(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .archive_system(
            id,
            authorize_system(
                &state,
                principal,
                id,
                Permission::SystemManage,
                "system.archive",
            )
            .await?,
            expected_version(&headers)?,
        )
        .await?;
    Ok(with_etag(StatusCode::OK, record))
}
async fn restore(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .restore_system(
            id,
            authorize_system(
                &state,
                principal,
                id,
                Permission::SystemManage,
                "system.restore",
            )
            .await?,
            expected_version(&headers)?,
        )
        .await?;
    Ok(with_etag(StatusCode::OK, record))
}
async fn remove(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
) -> Result<StatusCode, SystemApiError> {
    let confirmed = headers
        .get("x-confirm-delete")
        .is_some_and(|value| value == "true");
    state
        .service
        .delete_system(
            id,
            authorize_system(
                &state,
                principal,
                id,
                Permission::SystemManage,
                "system.delete",
            )
            .await?,
            expected_version(&headers)?,
            confirmed,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn authorize_create(
    state: &SystemState,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<(UserId, AccountId), SystemApiError> {
    let Extension(principal) = principal.ok_or(SystemApiError::Forbidden)?;
    require_system_scope(&principal, true)?;
    let account_id = match &principal {
        RequestPrincipal::User(user_id) => {
            AccountId::from_uuid(user_id.as_uuid()).map_err(|_| SystemApiError::Unavailable)?
        }
        RequestPrincipal::ApiCredential { account_id, .. } => *account_id,
    };
    let authorized = state
        .authorizer
        .authorize_account(
            principal_identity(&principal)?,
            account_id,
            Permission::SystemManage,
            "system.create",
        )
        .await?;
    Ok((authorized.actor_user_id, authorized.account_id))
}

async fn authorize_system(
    state: &SystemState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    permission: Permission,
    action: &'static str,
) -> Result<UserId, SystemApiError> {
    let Extension(principal) = principal.ok_or(SystemApiError::Forbidden)?;
    require_system_scope(&principal, permission == Permission::SystemManage)?;
    let authorized = state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            permission,
            action,
        )
        .await?;
    Ok(authorized.actor_user_id)
}

fn require_system_scope(principal: &RequestPrincipal, write: bool) -> Result<(), SystemApiError> {
    match principal {
        RequestPrincipal::User(_) => Ok(()),
        RequestPrincipal::ApiCredential { scopes, .. }
            if scopes.contains(if write {
                &ApiScope::SystemsWrite
            } else {
                &ApiScope::SystemsRead
            }) =>
        {
            Ok(())
        }
        RequestPrincipal::ApiCredential { .. } => Err(SystemApiError::Forbidden),
    }
}
fn expected_version(headers: &HeaderMap) -> Result<u64, SystemApiError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse().ok())
        .ok_or(SystemApiError::PreconditionRequired)
}
fn with_etag(status: StatusCode, record: SystemLifecycleRecord) -> Response {
    let version = record.version;
    let mut response = (status, Json(record)).into_response();
    if let Ok(value) = HeaderValue::from_str(&format!("\"{version}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

enum SystemApiError {
    Domain(SystemLifecycleError),
    Forbidden,
    PreconditionRequired,
    Unavailable,
}
impl From<RequestAuthorizationError> for SystemApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::Domain(SystemLifecycleError::NotFound),
            RequestAuthorizationError::Unavailable => Self::Domain(
                SystemLifecycleError::Repository(pvlog_application::PortError::Unavailable),
            ),
        }
    }
}
impl From<SystemLifecycleError> for SystemApiError {
    fn from(value: SystemLifecycleError) -> Self {
        Self::Domain(value)
    }
}
impl IntoResponse for SystemApiError {
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::PreconditionRequired => (
                StatusCode::PRECONDITION_REQUIRED,
                "precondition_required",
                "if_match_header_required",
            ),
            Self::Domain(SystemLifecycleError::NotFound) => {
                (StatusCode::NOT_FOUND, "not_found", "system_not_found")
            }
            Self::Domain(SystemLifecycleError::Conflict) => (
                StatusCode::PRECONDITION_FAILED,
                "version_conflict",
                "system_version_is_stale",
            ),
            Self::Domain(
                SystemLifecycleError::InvalidInput | SystemLifecycleError::ConfirmationRequired,
            ) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_system",
                "system_input_is_invalid",
            ),
            Self::Unavailable
            | Self::Domain(SystemLifecycleError::Time | SystemLifecycleError::Repository(_)) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "system_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/system-management",
                title,
                status: status.as_u16(),
                detail,
                request_id: None,
            }),
        )
            .into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
