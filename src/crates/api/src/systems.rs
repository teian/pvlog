use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{
    CreateSystem, SystemDetails, SystemLifecycleError, SystemLifecycleRecord,
    SystemLifecycleUseCases, UpdateSystem,
};
use pvlog_domain::{AccountId, ApiScope, Permission, SystemId, UserId, Visibility};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    InverterApiError, InverterApiUseCases, InverterInput, ModernRequestAuthorizer, Problem,
    RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone)]
struct SystemState {
    service: Arc<dyn SystemLifecycleUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
    /// Stores inverters submitted together with a new system.
    equipment: Option<Arc<dyn InverterApiUseCases>>,
}
pub fn systems_router(
    service: Arc<dyn SystemLifecycleUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    router(service, authorizer, None)
}

/// Creates the system routes with creation of a system and its inverters in one request.
pub fn systems_router_with_equipment(
    service: Arc<dyn SystemLifecycleUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
    equipment: Arc<dyn InverterApiUseCases>,
) -> Router {
    router(service, authorizer, Some(equipment))
}

fn router(
    service: Arc<dyn SystemLifecycleUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
    equipment: Option<Arc<dyn InverterApiUseCases>>,
) -> Router {
    Router::new()
        .route("/api/v1/systems", post(create))
        .route("/api/v1/systems/{id}", get(read).put(update).delete(remove))
        .route("/api/v1/systems/{id}/archive", post(archive))
        .route("/api/v1/systems/{id}/restore", post(restore))
        .with_state(SystemState {
            service,
            authorizer,
            equipment,
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
    /// Timezone the owner chose; omitted to derive it from the coordinates.
    #[serde(default)]
    timezone: Option<String>,
    /// Timezone of the client, used when nothing else decides it.
    #[serde(default)]
    client_timezone: Option<String>,
    #[serde(flatten)]
    details: DetailsBody,
    /// Inverters with their strings to create together with the system.
    #[serde(default)]
    inverters: Vec<InverterInput>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBody {
    name: String,
    timezone: String,
    visibility: Visibility,
    #[serde(flatten)]
    details: DetailsBody,
}

/// Optional system fields shared by creation and update.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsBody {
    /// Free-text placement shown beside the name.
    #[serde(default)]
    description: String,
    /// Local commissioning date as `YYYY-MM-DD`.
    #[serde(default)]
    commissioning_date: Option<String>,
    #[serde(default)]
    location_label: Option<String>,
    /// ISO 3166-1 alpha-2 code of the country the location lies in.
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    latitude_microdegrees: Option<i32>,
    #[serde(default)]
    longitude_microdegrees: Option<i32>,
    #[serde(default)]
    status_interval_seconds: Option<u32>,
    #[serde(default)]
    declared_capacity_watts: Option<i64>,
}

impl From<DetailsBody> for SystemDetails {
    fn from(body: DetailsBody) -> Self {
        Self {
            description: body.description,
            commissioning_date: body.commissioning_date.filter(|date| !date.is_empty()),
            location_label: body.location_label,
            country_code: body.country_code.filter(|code| !code.is_empty()),
            latitude_microdegrees: body.latitude_microdegrees,
            longitude_microdegrees: body.longitude_microdegrees,
            status_interval_seconds: body.status_interval_seconds,
            declared_capacity_watts: body.declared_capacity_watts,
        }
    }
}

async fn create(
    State(state): State<SystemState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(body): Json<CreateBody>,
) -> Result<Response, SystemApiError> {
    let (actor, account_id) = authorize_create(&state, principal).await?;
    // Everything is checked before the first record exists, so invalid
    // equipment cannot leave a system behind.
    let equipment = match (&state.equipment, body.inverters.is_empty()) {
        (_, true) => None,
        (Some(equipment), false) => {
            for inverter in &body.inverters {
                equipment.validate(inverter)?;
            }
            Some(equipment)
        }
        (None, false) => return Err(SystemApiError::InvalidEquipment("inverters")),
    };
    let record = state
        .service
        .create_system(CreateSystem {
            account_id,
            actor,
            name: body.name,
            timezone: body.timezone.filter(|timezone| !timezone.is_empty()),
            client_timezone: body.client_timezone.filter(|timezone| !timezone.is_empty()),
            details: body.details.into(),
        })
        .await?;
    if let Some(equipment) = equipment {
        for inverter in body.inverters {
            if let Err(error) = equipment
                .create(actor, account_id, record.id, inverter)
                .await
            {
                // Storage failed after validation passed; remove the system again
                // so the request either creates everything or nothing.
                let _ = state
                    .service
                    .delete_system(record.id, actor, record.version, true)
                    .await;
                return Err(error.into());
            }
        }
    }
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
            details: body.details.into(),
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
    /// Equipment submitted with a new system failed validation; names the field.
    InvalidEquipment(&'static str),
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
impl From<InverterApiError> for SystemApiError {
    fn from(value: InverterApiError) -> Self {
        match value {
            InverterApiError::InvalidInput(field) => Self::InvalidEquipment(field),
            InverterApiError::Forbidden => Self::Forbidden,
            InverterApiError::NotFound | InverterApiError::Unavailable => Self::Unavailable,
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
            Self::Domain(SystemLifecycleError::InvalidField(code)) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "invalid_system", code)
            }
            Self::InvalidEquipment(field) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "invalid_equipment", field)
            }
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
