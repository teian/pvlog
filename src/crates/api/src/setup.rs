//! Setup status and first-data verification of a system.
//!
//! Both are derived when asked for, so the checklist a client shows always
//! matches what the system really has.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_application::{STALE_AFTER_INTERVALS, SetupItem};
use pvlog_domain::{AccountId, ApiScope, DataSourceId, Permission, SystemId};
use serde::Serialize;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// Whether a system currently has data, judged by its own status interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataState {
    /// No real reading was ever stored.
    None,
    /// The latest reading arrived within three status intervals.
    Current,
    /// Readings exist, but none arrived within three status intervals.
    Stale,
}

impl DataState {
    /// Judges a system by the time of its latest reading and its status interval.
    ///
    /// A system without any reading has no data; it is never reported as
    /// current or as having data of age zero. Times are epoch milliseconds.
    #[must_use]
    pub fn at(last_reading_at: Option<i64>, status_interval_seconds: u32, now: i64) -> Self {
        let Some(last_reading_at) = last_reading_at else {
            return Self::None;
        };
        let allowed = i64::from(status_interval_seconds)
            .saturating_mul(1_000)
            .saturating_mul(STALE_AFTER_INTERVALS);
        if now.saturating_sub(last_reading_at) > allowed {
            Self::Stale
        } else {
            Self::Current
        }
    }
}

/// Ordered setup status of one system.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusResponse {
    pub system_id: SystemId,
    pub items: Vec<SetupItem>,
    pub data_state: DataState,
    pub last_received_at_epoch_millis: Option<i64>,
}

/// A reading as the first-data check shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedReading {
    pub id: uuid::Uuid,
    pub measured_at_epoch_millis: i64,
    pub received_at_epoch_millis: i64,
    pub generation_power_watts: Option<i64>,
    pub generation_energy_wh: Option<i64>,
    pub consumption_power_watts: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub data_source_id: Option<DataSourceId>,
    pub data_source_name: Option<String>,
    /// True for a reading that only proves the connection.
    pub test: bool,
}

/// Whether a system has received real data, with the newest reading to show.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstDataResponse {
    pub system_id: SystemId,
    /// True once a reading that is not a test reading is stored.
    pub received: bool,
    /// Newest real reading, or the newest test reading while there is none.
    pub reading: Option<VerifiedReading>,
}

#[async_trait::async_trait]
pub trait SetupApiUseCases: Send + Sync {
    async fn status(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SetupStatusResponse, SetupApiError>;
    async fn first_data(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<FirstDataResponse, SetupApiError>;
}

#[derive(Clone)]
struct SetupState {
    service: Arc<dyn SetupApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the setup-status and first-data routes.
pub fn setup_router(
    service: Arc<dyn SetupApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/setup-status", get(status))
        .route("/api/v1/systems/{system_id}/first-data", get(first_data))
        .with_state(SetupState {
            service,
            authorizer,
        })
}

async fn status(
    State(state): State<SetupState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<SetupStatusResponse>, SetupApiError> {
    let account_id = authorize(&state, principal, system_id, "system.setup_status.read").await?;
    Ok(Json(state.service.status(account_id, system_id).await?))
}

async fn first_data(
    State(state): State<SetupState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<FirstDataResponse>, SetupApiError> {
    let account_id = authorize(&state, principal, system_id, "system.first_data.read").await?;
    Ok(Json(state.service.first_data(account_id, system_id).await?))
}

async fn authorize(
    state: &SetupState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    action: &'static str,
) -> Result<AccountId, SetupApiError> {
    let Extension(principal) = principal.ok_or(SetupApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal
        && !scopes.contains(&ApiScope::SystemsRead)
    {
        return Err(SetupApiError::Forbidden);
    }
    Ok(state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            Permission::SystemRead,
            action,
        )
        .await?
        .account_id)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupApiError {
    Forbidden,
    NotFound,
    Unavailable,
}

impl From<RequestAuthorizationError> for SetupApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for SetupApiError {
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "system_not_found"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "setup_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/system-setup",
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
