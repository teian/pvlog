//! Read-only system reporting endpoints used by the operational navigation pages.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{AccountId, Permission, SystemId};
use serde::Serialize;
use thiserror::Error;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemOverviewResponse {
    pub id: SystemId,
    pub name: String,
    pub timezone: String,
    pub lifecycle: String,
    pub inverter_count: u64,
    pub string_count: u64,
    pub capacity_watts: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsResponse {
    pub system_id: SystemId,
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub peak_generation_power_watts: Option<i64>,
    pub first_observation_at_epoch_millis: Option<i64>,
    pub last_observation_at_epoch_millis: Option<i64>,
    pub coverage_basis_points: u16,
    pub monthly: Vec<MonthlyProductionResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProductionResponse {
    pub bucket_start_epoch_millis: i64,
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub coverage_basis_points: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonalResponse {
    pub system_id: SystemId,
    pub seasons: Vec<SeasonProductionResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonProductionResponse {
    pub season: String,
    pub generation_energy_wh: i64,
    pub measured_days: u64,
    pub average_daily_energy_wh: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherForecastResponse {
    pub system_id: SystemId,
    pub issued_at_epoch_millis: Option<i64>,
    pub attribution: Option<String>,
    pub points: Vec<WeatherForecastPointResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherForecastPointResponse {
    pub interval_start_epoch_millis: i64,
    pub interval_end_epoch_millis: i64,
    pub irradiance_watts_per_square_metre: Option<i64>,
    pub ambient_temperature_millicelsius: Option<i64>,
    pub wind_speed_millimetres_per_second: Option<i64>,
    pub cloud_cover_basis_points: Option<u16>,
    pub predicted_energy_wh: Option<i64>,
}

#[async_trait]
pub trait ReportingApiUseCases: Send + Sync {
    async fn system_overview(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SystemOverviewResponse, ReportingApiError>;
    async fn statistics(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<StatisticsResponse, ReportingApiError>;
    async fn seasonal(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SeasonalResponse, ReportingApiError>;
    async fn weather_forecast(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<WeatherForecastResponse, ReportingApiError>;
}

#[derive(Clone)]
struct ReportingState {
    service: Arc<dyn ReportingApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

pub fn reporting_router(
    service: Arc<dyn ReportingApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/overview", get(system_overview))
        .route(
            "/api/v1/systems/{system_id}/reporting/statistics",
            get(statistics),
        )
        .route("/api/v1/systems/{system_id}/seasonal", get(seasonal))
        .route(
            "/api/v1/systems/{system_id}/weather-forecast",
            get(weather_forecast),
        )
        .with_state(ReportingState {
            service,
            authorizer,
        })
}

async fn system_overview(
    State(state): State<ReportingState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<SystemOverviewResponse>, ReportingApiError> {
    let account_id = authorize(
        &state,
        principal,
        system_id,
        Permission::SystemRead,
        "system.read",
    )
    .await?;
    Ok(Json(
        state.service.system_overview(account_id, system_id).await?,
    ))
}

async fn statistics(
    State(state): State<ReportingState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<StatisticsResponse>, ReportingApiError> {
    let account_id = authorize(
        &state,
        principal,
        system_id,
        Permission::TelemetryRead,
        "statistics.read",
    )
    .await?;
    Ok(Json(state.service.statistics(account_id, system_id).await?))
}

async fn seasonal(
    State(state): State<ReportingState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<SeasonalResponse>, ReportingApiError> {
    let account_id = authorize(
        &state,
        principal,
        system_id,
        Permission::TelemetryRead,
        "seasonal.read",
    )
    .await?;
    Ok(Json(state.service.seasonal(account_id, system_id).await?))
}

async fn weather_forecast(
    State(state): State<ReportingState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<WeatherForecastResponse>, ReportingApiError> {
    let account_id = authorize(
        &state,
        principal,
        system_id,
        Permission::TelemetryRead,
        "weather_forecast.read",
    )
    .await?;
    Ok(Json(
        state
            .service
            .weather_forecast(account_id, system_id)
            .await?,
    ))
}

async fn authorize(
    state: &ReportingState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    permission: Permission,
    action: &'static str,
) -> Result<AccountId, ReportingApiError> {
    let Extension(principal) = principal.ok_or(ReportingApiError::Forbidden)?;
    Ok(state
        .authorizer
        .authorize_system(
            principal_identity(&principal).map_err(|_| ReportingApiError::Forbidden)?,
            system_id,
            permission,
            action,
        )
        .await?
        .account_id)
}

impl From<RequestAuthorizationError> for ReportingApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReportingApiError {
    #[error("reporting operation is forbidden")]
    Forbidden,
    #[error("reporting resource was not found")]
    NotFound,
    #[error("reporting resource is unavailable")]
    Unavailable,
}

impl IntoResponse for ReportingApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
