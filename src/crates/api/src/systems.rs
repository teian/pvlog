use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{post, put},
};
use pvlog_application::{
    CreateSystem, SystemLifecycleError, SystemLifecycleRecord, SystemLifecycleUseCases,
    UpdateSystem,
};
use pvlog_domain::{AccountId, SystemId, UserId, Visibility};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
struct SystemState {
    service: Arc<dyn SystemLifecycleUseCases>,
}
pub fn systems_router(service: Arc<dyn SystemLifecycleUseCases>) -> Router {
    Router::new()
        .route("/api/v1/systems", post(create))
        .route("/api/v1/systems/{id}", put(update).delete(remove))
        .route("/api/v1/systems/{id}/archive", post(archive))
        .route("/api/v1/systems/{id}/restore", post(restore))
        .with_state(SystemState { service })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBody {
    account_id: AccountId,
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
    actor: Option<Extension<UserId>>,
    Json(body): Json<CreateBody>,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .create_system(CreateSystem {
            account_id: body.account_id,
            actor: actor_id(actor)?,
            name: body.name,
            timezone: body.timezone,
        })
        .await?;
    Ok(with_etag(StatusCode::CREATED, record))
}
async fn update(
    State(state): State<SystemState>,
    actor: Option<Extension<UserId>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .update_system(UpdateSystem {
            id,
            actor: actor_id(actor)?,
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
    actor: Option<Extension<UserId>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .archive_system(id, actor_id(actor)?, expected_version(&headers)?)
        .await?;
    Ok(with_etag(StatusCode::OK, record))
}
async fn restore(
    State(state): State<SystemState>,
    actor: Option<Extension<UserId>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
) -> Result<Response, SystemApiError> {
    let record = state
        .service
        .restore_system(id, actor_id(actor)?, expected_version(&headers)?)
        .await?;
    Ok(with_etag(StatusCode::OK, record))
}
async fn remove(
    State(state): State<SystemState>,
    actor: Option<Extension<UserId>>,
    Path(id): Path<SystemId>,
    headers: HeaderMap,
) -> Result<StatusCode, SystemApiError> {
    let confirmed = headers
        .get("x-confirm-delete")
        .is_some_and(|value| value == "true");
    state
        .service
        .delete_system(id, actor_id(actor)?, expected_version(&headers)?, confirmed)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn actor_id(actor: Option<Extension<UserId>>) -> Result<UserId, SystemApiError> {
    actor
        .map(|Extension(id)| id)
        .ok_or(SystemApiError::Forbidden)
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
}
impl From<SystemLifecycleError> for SystemApiError {
    fn from(value: SystemLifecycleError) -> Self {
        Self::Domain(value)
    }
}
impl IntoResponse for SystemApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::Domain(SystemLifecycleError::NotFound) => StatusCode::NOT_FOUND,
            Self::Domain(SystemLifecycleError::Conflict) => StatusCode::PRECONDITION_FAILED,
            Self::Domain(
                SystemLifecycleError::InvalidInput | SystemLifecycleError::ConfirmationRequired,
            ) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Domain(SystemLifecycleError::Time | SystemLifecycleError::Repository(_)) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        };
        status.into_response()
    }
}
