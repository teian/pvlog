//! Nested system → inverter → PV-string modern resources.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_application::PortError;
use pvlog_domain::{
    AccountId, ApiScope, EquipmentValueProvenance, InverterId, InverterSpecificationSnapshot,
    Permission, SolarModuleSpecificationSnapshot, StringId, SystemId, UserId,
};
use serde::{Deserialize, Serialize};

use crate::{
    AuthorizedRequest, ModernRequestAuthorizer, Problem, RequestAuthorizationError,
    RequestPrincipal, principal_identity,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvStringInput {
    pub name: String,
    pub panel_count: u32,
    pub panel_manufacturer: Option<String>,
    pub panel_model: Option<String>,
    pub value_provenance: Option<EquipmentValueProvenance>,
    pub module_specification_snapshot: Option<SolarModuleSpecificationSnapshot>,
    pub module_peak_power_watts: i64,
    pub orientation_degrees: Option<u16>,
    pub tilt_degrees: Option<u8>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterInput {
    pub name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_reference: Option<String>,
    pub rated_power_watts: Option<i64>,
    pub value_provenance: Option<EquipmentValueProvenance>,
    pub specification_snapshot: Option<InverterSpecificationSnapshot>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub strings: Vec<PvStringInput>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvStringResponse {
    pub id: StringId,
    pub inverter_id: InverterId,
    pub name: String,
    pub panel_count: u32,
    pub panel_manufacturer: Option<String>,
    pub panel_model: Option<String>,
    pub rated_power_watts: i64,
    pub module_catalog_entry_id: Option<String>,
    pub module_catalog_revision: Option<String>,
    pub value_provenance: EquipmentValueProvenance,
    pub module_specification_snapshot: Option<SolarModuleSpecificationSnapshot>,
    pub module_peak_power_watts: Option<i64>,
    pub total_peak_power_watts: Option<i64>,
    pub orientation_degrees: Option<u16>,
    pub tilt_degrees: Option<u8>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterResponse {
    pub id: InverterId,
    pub system_id: SystemId,
    pub name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_reference: Option<String>,
    pub rated_power_watts: Option<i64>,
    pub catalog_entry_id: Option<String>,
    pub catalog_revision: Option<String>,
    pub value_provenance: EquipmentValueProvenance,
    pub specification_snapshot: Option<InverterSpecificationSnapshot>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub version: u64,
    pub strings: Vec<PvStringResponse>,
}

#[async_trait]
pub trait InverterApiUseCases: Send + Sync {
    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<InverterResponse>, InverterApiError>;
    async fn create(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        input: InverterInput,
    ) -> Result<InverterResponse, InverterApiError>;
    async fn update(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
        input: InverterInput,
    ) -> Result<InverterResponse, InverterApiError>;
    async fn delete(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
    ) -> Result<(), InverterApiError>;
}

#[derive(Clone)]
struct InverterState {
    service: Arc<dyn InverterApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

pub fn inverters_router(
    service: Arc<dyn InverterApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/systems/{system_id}/inverters",
            get(list).post(create),
        )
        .route(
            "/api/v1/systems/{system_id}/inverters/{inverter_id}",
            axum::routing::put(update).delete(remove),
        )
        .with_state(InverterState {
            service,
            authorizer,
        })
}

async fn update(
    State(state): State<InverterState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, inverter_id)): Path<(SystemId, InverterId)>,
    Json(input): Json<InverterInput>,
) -> Result<Response, InverterApiError> {
    let authorized =
        authorize(&state, principal, system_id, true, "system.inverter.update").await?;
    let record = state
        .service
        .update(
            authorized.actor_user_id,
            authorized.account_id,
            system_id,
            inverter_id,
            input,
        )
        .await?;
    Ok(versioned_response(StatusCode::OK, record))
}

async fn list(
    State(state): State<InverterState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<Vec<InverterResponse>>, InverterApiError> {
    let authorized = authorize(&state, principal, system_id, false, "system.inverter.list").await?;
    Ok(Json(
        state
            .service
            .list(authorized.account_id, system_id, now())
            .await?,
    ))
}

async fn create(
    State(state): State<InverterState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Json(input): Json<InverterInput>,
) -> Result<Response, InverterApiError> {
    let authorized =
        authorize(&state, principal, system_id, true, "system.inverter.create").await?;
    let record = state
        .service
        .create(
            authorized.actor_user_id,
            authorized.account_id,
            system_id,
            input,
        )
        .await?;
    Ok(versioned_response(StatusCode::CREATED, record))
}

async fn remove(
    State(state): State<InverterState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, inverter_id)): Path<(SystemId, InverterId)>,
) -> Result<StatusCode, InverterApiError> {
    let authorized =
        authorize(&state, principal, system_id, true, "system.inverter.delete").await?;
    state
        .service
        .delete(
            authorized.actor_user_id,
            authorized.account_id,
            system_id,
            inverter_id,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn versioned_response(status: StatusCode, record: InverterResponse) -> Response {
    let version = record.version;
    let mut response = (status, Json(record)).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{version}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("\"1\"")),
    );
    response
}

async fn authorize(
    state: &InverterState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<AuthorizedRequest, InverterApiError> {
    let Extension(principal) = principal.ok_or(InverterApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::SystemsRead
        };
        if !scopes.contains(&scope) {
            return Err(InverterApiError::Forbidden);
        }
    }
    let authorized = state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            if write {
                Permission::SystemManage
            } else {
                Permission::SystemRead
            },
            action,
        )
        .await?;
    Ok(authorized)
}

fn now() -> i64 {
    let value = time::OffsetDateTime::now_utc();
    value.unix_timestamp() * 1_000 + i64::from(value.nanosecond() / 1_000_000)
}

#[derive(Debug)]
pub enum InverterApiError {
    Forbidden,
    NotFound,
    InvalidInput(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for InverterApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}
impl From<PortError> for InverterApiError {
    fn from(_: PortError) -> Self {
        Self::Unavailable
    }
}
impl IntoResponse for InverterApiError {
    fn into_response(self) -> Response {
        if let Self::InvalidInput(field) = self {
            let mut response = (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(Problem {
                    problem_type: "https://pvlog.example/problems/equipment-validation",
                    title: "invalid_equipment_value",
                    status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                    detail: field,
                    request_id: None,
                }),
            )
                .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/problem+json"),
            );
            return response;
        }
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "inverter_not_found"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "inverter_service_unavailable",
            ),
            Self::InvalidInput(_) => unreachable!(),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/inverter-management",
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
