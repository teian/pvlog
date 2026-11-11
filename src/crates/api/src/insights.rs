//! Range summaries and tariff periods behind the dashboard tiles.
//!
//! The dashboard shows four figures per range and a per-string breakdown. Both
//! are derived from the same stored buckets, so they are served here rather than
//! recomputed by every client — which would also mean every client repeating the
//! tariff walk that turns energy into revenue.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{ApiScope, Permission, SystemId, UserId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// Range the dashboard asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightRange {
    Live,
    Day,
    Week,
    Month,
    Year,
}

impl InsightRange {
    /// Parses a range name (`live`, `day`, `week`, `month`, `year`), returning `None` for anything else.
    fn parse(value: &str) -> Option<Self> {
        match value {
            "live" => Some(Self::Live),
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
}

/// What a range produced, in canonical units, without presentation choices.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeSummary {
    pub range: InsightRange,
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub generation_energy_wh: i64,
    pub consumption_energy_wh: Option<i64>,
    pub peak_power_watts: Option<i64>,
    /// Newest reading in the range; the live tile shows this.
    pub current_power_watts: Option<i64>,
    /// Mean energy per bucket, for the "per day" and "per month" tiles.
    pub average_bucket_energy_wh: Option<i64>,
    pub best_bucket: Option<BestBucket>,
    pub previous_period: Option<PreviousPeriod>,
    pub revenue: RevenueSummary,
    pub capacity_watts: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BestBucket {
    pub start_epoch_millis: i64,
    pub energy_wh: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousPeriod {
    pub generation_energy_wh: i64,
    /// Signed change against the previous period; 500 means five percent up.
    pub change_basis_points: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevenueSummary {
    pub minor_units: i64,
    pub currency_code: Option<String>,
    pub periods_applied: u32,
    pub unpriced_energy_wh: i64,
}

/// One string's contribution to the range.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringContribution {
    pub string_id: Uuid,
    pub name: String,
    pub inverter_id: Uuid,
    pub inverter_name: String,
    pub rated_power_watts: i64,
    pub generation_energy_wh: i64,
    /// `online`, `idle`, `fault`, or `offline`, derived from recent readings.
    pub status: String,
    /// Absent while no channel is bound to the string.
    pub metered: bool,
}

/// How a month's production split across sky conditions.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherMonth {
    pub month_start_epoch_millis: i64,
    pub generation_energy_wh: i64,
    pub classes: Vec<WeatherClassShare>,
}

/// One sky condition's share of a month.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherClassShare {
    /// `clear`, `partly_cloudy`, `mostly_cloudy`, or `overcast`.
    pub class: String,
    pub share_basis_points: u16,
    pub generation_energy_wh: i64,
}

#[async_trait::async_trait]
pub trait InsightApiUseCases: Send + Sync {
    /// Returns monthly weather-class generation shares for the last `months` months.
    async fn weather_composition(
        &self,
        actor: UserId,
        system_id: SystemId,
        months: u32,
    ) -> Result<Vec<WeatherMonth>, InsightApiError>;
    /// Summarizes generation for a scope over the requested range, compared with the previous window.
    async fn range_summary(
        &self,
        actor: UserId,
        system_id: SystemId,
        scope: Option<String>,
        scope_id: Option<Uuid>,
        range: InsightRange,
    ) -> Result<RangeSummary, InsightApiError>;
    /// Breaks the system's generation down per PV string for the requested range.
    async fn string_breakdown(
        &self,
        actor: UserId,
        system_id: SystemId,
        range: InsightRange,
    ) -> Result<Vec<StringContribution>, InsightApiError>;
    /// Lists the system's tariff periods as JSON values.
    async fn tariffs(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<Vec<serde_json::Value>, InsightApiError>;
    /// Creates a tariff period, or updates the one named by `tariff_id`, from JSON input.
    async fn upsert_tariff(
        &self,
        actor: UserId,
        system_id: SystemId,
        tariff_id: Option<Uuid>,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, InsightApiError>;
    /// Deletes a tariff period; an unknown identifier yields `NotFound`.
    async fn delete_tariff(
        &self,
        actor: UserId,
        system_id: SystemId,
        tariff_id: Uuid,
    ) -> Result<(), InsightApiError>;
}

#[derive(Clone)]
struct InsightState {
    service: Arc<dyn InsightApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryParameters {
    range: String,
    scope: Option<String>,
    scope_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BreakdownParameters {
    range: String,
}

/// Creates the range-summary, string-breakdown, and tariff routes.
pub fn insights_router(
    service: Arc<dyn InsightApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/range-summary", get(summary))
        .route(
            "/api/v1/systems/{system_id}/string-breakdown",
            get(breakdown),
        )
        .route(
            "/api/v1/systems/{system_id}/weather-composition",
            get(weather_composition),
        )
        .route(
            "/api/v1/systems/{system_id}/tariffs",
            get(list_tariffs).post(create_tariff),
        )
        .route(
            "/api/v1/systems/{system_id}/tariffs/{tariff_id}",
            axum::routing::put(update_tariff).delete(remove_tariff),
        )
        .with_state(InsightState {
            service,
            authorizer,
        })
}

/// Handles the range-summary route; an unrecognized `range` is rejected as invalid input.
async fn summary(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(parameters): Query<SummaryParameters>,
) -> Result<Json<RangeSummary>, InsightApiError> {
    let actor = authorize(&state, principal, system_id, false, "system.range.summary").await?;
    let range =
        InsightRange::parse(&parameters.range).ok_or(InsightApiError::InvalidInput("range"))?;
    Ok(Json(
        state
            .service
            .range_summary(
                actor,
                system_id,
                parameters.scope,
                parameters.scope_id,
                range,
            )
            .await?,
    ))
}

/// Handles the string-breakdown route; an unrecognized `range` is rejected as invalid input.
async fn breakdown(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(parameters): Query<BreakdownParameters>,
) -> Result<Json<Vec<StringContribution>>, InsightApiError> {
    let actor = authorize(
        &state,
        principal,
        system_id,
        false,
        "system.string.breakdown",
    )
    .await?;
    let range =
        InsightRange::parse(&parameters.range).ok_or(InsightApiError::InvalidInput("range"))?;
    Ok(Json(
        state
            .service
            .string_breakdown(actor, system_id, range)
            .await?,
    ))
}

#[derive(Deserialize)]
struct CompositionParameters {
    months: Option<u32>,
}

/// Handles the weather-composition route, defaulting to 12 months and clamping the request to 1..=60.
async fn weather_composition(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(parameters): Query<CompositionParameters>,
) -> Result<Json<Vec<WeatherMonth>>, InsightApiError> {
    let actor = authorize(
        &state,
        principal,
        system_id,
        false,
        "system.weather.composition",
    )
    .await?;
    let months = parameters.months.unwrap_or(12).clamp(1, 60);
    Ok(Json(
        state
            .service
            .weather_composition(actor, system_id, months)
            .await?,
    ))
}

/// Handles the tariff listing route.
async fn list_tariffs(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<Vec<serde_json::Value>>, InsightApiError> {
    let actor = authorize(&state, principal, system_id, false, "system.tariff.list").await?;
    Ok(Json(state.service.tariffs(actor, system_id).await?))
}

/// Handles tariff creation and answers `201 Created` with the stored tariff.
async fn create_tariff(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Json(input): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), InsightApiError> {
    let actor = authorize(&state, principal, system_id, true, "system.tariff.create").await?;
    let created = state
        .service
        .upsert_tariff(actor, system_id, None, input)
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Handles tariff replacement for an existing tariff identifier.
async fn update_tariff(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, tariff_id)): Path<(SystemId, Uuid)>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, InsightApiError> {
    let actor = authorize(&state, principal, system_id, true, "system.tariff.update").await?;
    Ok(Json(
        state
            .service
            .upsert_tariff(actor, system_id, Some(tariff_id), input)
            .await?,
    ))
}

/// Handles tariff deletion and answers `204 No Content` on success.
async fn remove_tariff(
    State(state): State<InsightState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, tariff_id)): Path<(SystemId, Uuid)>,
) -> Result<StatusCode, InsightApiError> {
    let actor = authorize(&state, principal, system_id, true, "system.tariff.delete").await?;
    state
        .service
        .delete_tariff(actor, system_id, tariff_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Checks the principal's API scope and system permission, returning the acting user; a missing principal or scope is `Forbidden`.
async fn authorize(
    state: &InsightState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<UserId, InsightApiError> {
    let Extension(principal) = principal.ok_or(InsightApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::TelemetryRead
        };
        if !scopes.contains(&scope) {
            return Err(InsightApiError::Forbidden);
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
                Permission::TelemetryRead
            },
            action,
        )
        .await?;
    Ok(authorized.actor_user_id)
}

#[derive(Clone, Copy, Debug)]
pub enum InsightApiError {
    Forbidden,
    NotFound,
    Conflict(&'static str),
    InvalidInput(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for InsightApiError {
    /// Maps authorization failures onto the insight API errors, hiding `NotFound` behind `Forbidden`.
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for InsightApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response with a matching status code.
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource_not_found"),
            Self::Conflict(code) => (StatusCode::CONFLICT, "conflict", code),
            Self::InvalidInput(code) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", code),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "insight_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/insights",
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
