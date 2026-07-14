//! Authorized forecast settings and modeled-yield query resources.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{
    AccountId, ApiScope, CalculationBasis, ForecastCompleteness, ForecastCompletenessReason,
    InverterId, Permission, StringId, SystemId, UserId, WeatherDataKind, WeatherDataRunId,
    YieldCalculationRunId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForecastResourceScope {
    Account {
        account_id: AccountId,
    },
    System {
        account_id: AccountId,
        system_id: SystemId,
    },
    Inverter {
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
    },
    String {
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
        string_id: StringId,
    },
}

impl ForecastResourceScope {
    const fn account_id(self) -> AccountId {
        match self {
            Self::Account { account_id }
            | Self::System { account_id, .. }
            | Self::Inverter { account_id, .. }
            | Self::String { account_id, .. } => account_id,
        }
    }

    const fn system_id(self) -> Option<SystemId> {
        match self {
            Self::Account { .. } => None,
            Self::System { system_id, .. }
            | Self::Inverter { system_id, .. }
            | Self::String { system_id, .. } => Some(system_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastLossInput {
    pub soiling_basis_points: u16,
    pub shading_basis_points: u16,
    pub mismatch_basis_points: u16,
    pub wiring_basis_points: u16,
    pub unavailability_basis_points: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastSettingsInput {
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub model_identifier: String,
    pub model_revision: u16,
    pub losses: ForecastLossInput,
    pub calibration_basis_points: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastSettingsResponse {
    pub scope: ForecastResourceScope,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub model_identifier: String,
    pub model_revision: u16,
    pub losses: ForecastLossInput,
    pub calibration_basis_points: i32,
    pub version: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastInputCompletenessResponse {
    pub scope: ForecastResourceScope,
    pub effective_at: i64,
    pub included_capacity_watts: i64,
    pub total_effective_capacity_watts: i64,
    pub complete: bool,
    pub reasons: Vec<ForecastCompletenessReason>,
    pub version: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastFreshness {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastProvenanceResponse {
    pub provider_id: String,
    pub adapter: String,
    pub source_url: String,
    pub license_identifier: String,
    pub attribution: String,
    pub fetched_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastRunResponse {
    pub id: WeatherDataRunId,
    pub system_id: SystemId,
    pub kind: WeatherDataKind,
    pub issued_at: Option<i64>,
    pub valid_from: i64,
    pub valid_to: i64,
    pub resolution_seconds: u32,
    pub freshness: ForecastFreshness,
    pub provenance: ForecastProvenanceResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldSeriesResolution {
    FifteenMinutes,
    Hour,
    Day,
}

impl YieldSeriesResolution {
    const fn seconds(self) -> u32 {
        match self {
            Self::FifteenMinutes => 900,
            Self::Hour => 3_600,
            Self::Day => 86_400,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForecastRunQuery {
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub issued_before_epoch_millis: Option<i64>,
    pub kind: WeatherDataKind,
    pub limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldSeriesQuery {
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub basis: CalculationBasis,
    pub resolution: YieldSeriesResolution,
    pub weather_run_id: Option<WeatherDataRunId>,
    pub include_partial: bool,
    pub maximum_points: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YieldSeriesPointResponse {
    pub interval_start: i64,
    pub interval_end: i64,
    pub central_power_watts: Option<i64>,
    pub lower_power_watts: Option<i64>,
    pub upper_power_watts: Option<i64>,
    pub central_energy_watt_hours: Option<i64>,
    pub lower_energy_watt_hours: Option<i64>,
    pub upper_energy_watt_hours: Option<i64>,
    pub coverage_basis_points: u16,
    pub completeness: ForecastCompleteness,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YieldSeriesResponse {
    pub scope: ForecastResourceScope,
    pub basis: CalculationBasis,
    pub resolution: YieldSeriesResolution,
    pub issue_time: Option<i64>,
    pub weather_run_id: WeatherDataRunId,
    pub calculation_run_id: YieldCalculationRunId,
    pub model_identifier: String,
    pub model_revision: u16,
    pub configuration_digest: String,
    pub freshness: ForecastFreshness,
    pub provenance: ForecastProvenanceResponse,
    pub included_capacity_watts: i64,
    pub total_effective_capacity_watts: i64,
    pub completeness: ForecastCompleteness,
    pub unavailable_reasons: Vec<ForecastCompletenessReason>,
    pub points: Vec<YieldSeriesPointResponse>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMetric {
    GenerationPerformance,
    ForecastRealization,
}

impl PerformanceMetric {
    #[must_use]
    pub const fn basis(self) -> CalculationBasis {
        match self {
            Self::GenerationPerformance => CalculationBasis::Expected,
            Self::ForecastRealization => CalculationBasis::Forecast,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerformanceQuery {
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub metric: PerformanceMetric,
    pub resolution: YieldSeriesResolution,
    pub weather_run_id: Option<WeatherDataRunId>,
    pub maximum_points: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformancePointResponse {
    pub interval_start: i64,
    pub interval_end: i64,
    pub actual_energy_watt_hours: Option<i64>,
    pub modeled_energy_watt_hours: Option<i64>,
    pub ratio_basis_points: Option<u32>,
    pub actual_coverage_basis_points: u16,
    pub modeled_coverage_basis_points: u16,
    pub unavailable_reason: Option<ForecastCompletenessReason>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSeriesResponse {
    pub scope: ForecastResourceScope,
    pub metric: PerformanceMetric,
    pub basis: CalculationBasis,
    pub resolution: YieldSeriesResolution,
    pub issue_time: Option<i64>,
    pub weather_run_id: WeatherDataRunId,
    pub calculation_run_id: YieldCalculationRunId,
    pub model_identifier: String,
    pub model_revision: u16,
    pub configuration_digest: String,
    pub freshness: ForecastFreshness,
    pub provenance: ForecastProvenanceResponse,
    pub points: Vec<PerformancePointResponse>,
}

#[async_trait]
pub trait ForecastApiUseCases: Send + Sync {
    async fn settings(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastSettingsResponse, ForecastApiError>;

    async fn update_settings(
        &self,
        actor: UserId,
        scope: ForecastResourceScope,
        expected_version: u64,
        input: ForecastSettingsInput,
    ) -> Result<ForecastSettingsResponse, ForecastApiError>;

    async fn input_completeness(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastInputCompletenessResponse, ForecastApiError>;

    async fn forecast_runs(
        &self,
        scope: ForecastResourceScope,
        query: ForecastRunQuery,
    ) -> Result<Vec<ForecastRunResponse>, ForecastApiError>;

    async fn yield_series(
        &self,
        scope: ForecastResourceScope,
        query: YieldSeriesQuery,
    ) -> Result<YieldSeriesResponse, ForecastApiError>;

    async fn performance_series(
        &self,
        scope: ForecastResourceScope,
        query: PerformanceQuery,
    ) -> Result<PerformanceSeriesResponse, ForecastApiError>;
}

#[derive(Clone)]
struct ForecastState {
    service: Arc<dyn ForecastApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates account, system, inverter, and string forecast administration routes.
pub fn forecasting_router(
    service: Arc<dyn ForecastApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    const SETTINGS_ROUTES: [&str; 4] = [
        "/api/v1/accounts/{account_id}/forecast-settings",
        "/api/v1/accounts/{account_id}/systems/{system_id}/forecast-settings",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/forecast-settings",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/strings/{string_id}/forecast-settings",
    ];
    const COMPLETENESS_ROUTES: [&str; 4] = [
        "/api/v1/accounts/{account_id}/forecast-input-completeness",
        "/api/v1/accounts/{account_id}/systems/{system_id}/forecast-input-completeness",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/forecast-input-completeness",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/strings/{string_id}/forecast-input-completeness",
    ];
    let mut router = Router::new();
    for route in SETTINGS_ROUTES {
        router = router.route(route, get(settings).put(update_settings));
    }
    for route in COMPLETENESS_ROUTES {
        router = router.route(route, get(input_completeness));
    }
    router = router
        .route(
            "/api/v1/accounts/{account_id}/systems/{system_id}/forecast-runs",
            get(forecast_runs),
        )
        .route(
            "/api/v1/accounts/{account_id}/systems/{system_id}/yield-series",
            get(yield_series),
        )
        .route(
            "/api/v1/accounts/{account_id}/systems/{system_id}/yield-performance",
            get(performance_series),
        );
    router.with_state(ForecastState {
        service,
        authorizer,
    })
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct ForecastPath {
    #[serde(rename = "account_id")]
    account: AccountId,
    #[serde(rename = "system_id")]
    system: Option<SystemId>,
    #[serde(rename = "inverter_id")]
    inverter: Option<InverterId>,
    #[serde(rename = "string_id")]
    string: Option<StringId>,
}

impl ForecastPath {
    fn scope(self) -> Result<ForecastResourceScope, ForecastApiError> {
        match (self.system, self.inverter, self.string) {
            (None, None, None) => Ok(ForecastResourceScope::Account {
                account_id: self.account,
            }),
            (Some(system_id), None, None) => Ok(ForecastResourceScope::System {
                account_id: self.account,
                system_id,
            }),
            (Some(system_id), Some(inverter_id), None) => Ok(ForecastResourceScope::Inverter {
                account_id: self.account,
                system_id,
                inverter_id,
            }),
            (Some(system_id), Some(inverter_id), Some(string_id)) => {
                Ok(ForecastResourceScope::String {
                    account_id: self.account,
                    system_id,
                    inverter_id,
                    string_id,
                })
            }
            _ => Err(ForecastApiError::InvalidPath),
        }
    }
}

async fn settings(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
) -> Result<Response, ForecastApiError> {
    let scope = path.scope()?;
    authorize(&state, principal, scope, false).await?;
    Ok(with_etag(
        StatusCode::OK,
        state.service.settings(scope).await?,
    ))
}

async fn update_settings(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
    headers: HeaderMap,
    Json(input): Json<ForecastSettingsInput>,
) -> Result<Response, ForecastApiError> {
    validate_settings(&input)?;
    let scope = path.scope()?;
    let actor = authorize(&state, principal, scope, true).await?;
    let expected_version = expected_version(&headers)?;
    let response = state
        .service
        .update_settings(actor, scope, expected_version, input)
        .await?;
    Ok(with_etag(StatusCode::OK, response))
}

async fn input_completeness(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
) -> Result<Response, ForecastApiError> {
    let scope = path.scope()?;
    authorize(&state, principal, scope, false).await?;
    let response = state.service.input_completeness(scope).await?;
    let version = response.version;
    Ok(json_with_etag(StatusCode::OK, response, version))
}

#[derive(Clone, Debug, Deserialize)]
struct RunParameters {
    #[serde(rename = "startEpochMillis")]
    start: i64,
    #[serde(rename = "endEpochMillis")]
    end: i64,
    #[serde(rename = "issuedBeforeEpochMillis")]
    issued_before: Option<i64>,
    kind: Option<String>,
    limit: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
struct SeriesParameters {
    #[serde(rename = "startEpochMillis")]
    start: i64,
    #[serde(rename = "endEpochMillis")]
    end: i64,
    basis: Option<String>,
    metric: Option<String>,
    resolution: Option<String>,
    #[serde(rename = "weatherRunId")]
    weather_run_id: Option<WeatherDataRunId>,
    #[serde(rename = "includePartial")]
    include_partial: Option<bool>,
    #[serde(rename = "maximumPoints")]
    maximum_points: Option<u32>,
    #[serde(rename = "inverterId")]
    inverter_id: Option<InverterId>,
    #[serde(rename = "stringId")]
    string_id: Option<StringId>,
}

async fn forecast_runs(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
    Query(parameters): Query<RunParameters>,
) -> Result<Json<Vec<ForecastRunResponse>>, ForecastApiError> {
    let scope = path.scope()?;
    if !matches!(scope, ForecastResourceScope::System { .. }) {
        return Err(ForecastApiError::InvalidPath);
    }
    authorize(&state, principal, scope, false).await?;
    let query = run_query(&parameters)?;
    Ok(Json(state.service.forecast_runs(scope, query).await?))
}

async fn yield_series(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
    Query(parameters): Query<SeriesParameters>,
) -> Result<Json<YieldSeriesResponse>, ForecastApiError> {
    let system_scope = path.scope()?;
    let ForecastResourceScope::System {
        account_id,
        system_id,
    } = system_scope
    else {
        return Err(ForecastApiError::InvalidPath);
    };
    authorize(&state, principal, system_scope, false).await?;
    let (scope, query) = series_query(account_id, system_id, &parameters)?;
    Ok(Json(state.service.yield_series(scope, query).await?))
}

async fn performance_series(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
    Query(parameters): Query<SeriesParameters>,
) -> Result<Json<PerformanceSeriesResponse>, ForecastApiError> {
    let system_scope = path.scope()?;
    let ForecastResourceScope::System {
        account_id,
        system_id,
    } = system_scope
    else {
        return Err(ForecastApiError::InvalidPath);
    };
    authorize(&state, principal, system_scope, false).await?;
    let (scope, series) = series_query(account_id, system_id, &parameters)?;
    let metric = match parameters
        .metric
        .as_deref()
        .unwrap_or("generation_performance")
    {
        "generation_performance" => PerformanceMetric::GenerationPerformance,
        "forecast_realization" => PerformanceMetric::ForecastRealization,
        _ => return Err(ForecastApiError::InvalidQuery("metric")),
    };
    let query = PerformanceQuery {
        start_epoch_millis: series.start_epoch_millis,
        end_epoch_millis: series.end_epoch_millis,
        metric,
        resolution: series.resolution,
        weather_run_id: series.weather_run_id,
        maximum_points: series.maximum_points,
    };
    Ok(Json(state.service.performance_series(scope, query).await?))
}

fn run_query(parameters: &RunParameters) -> Result<ForecastRunQuery, ForecastApiError> {
    validate_range(parameters.start, parameters.end)?;
    let kind = match parameters.kind.as_deref().unwrap_or("forecast") {
        "forecast" => WeatherDataKind::Forecast,
        "observed" => WeatherDataKind::Observed,
        "reanalysis" => WeatherDataKind::Reanalysis,
        _ => return Err(ForecastApiError::InvalidQuery("kind")),
    };
    let limit = parameters.limit.unwrap_or(50);
    if limit == 0 || limit > 100 {
        return Err(ForecastApiError::InvalidQuery("limit"));
    }
    Ok(ForecastRunQuery {
        start_epoch_millis: parameters.start,
        end_epoch_millis: parameters.end,
        issued_before_epoch_millis: parameters.issued_before,
        kind,
        limit,
    })
}

fn series_query(
    account_id: AccountId,
    system_id: SystemId,
    parameters: &SeriesParameters,
) -> Result<(ForecastResourceScope, YieldSeriesQuery), ForecastApiError> {
    validate_range(parameters.start, parameters.end)?;
    let basis = match parameters.basis.as_deref().unwrap_or("forecast") {
        "forecast" => CalculationBasis::Forecast,
        "expected" => CalculationBasis::Expected,
        _ => return Err(ForecastApiError::InvalidQuery("basis")),
    };
    let resolution = match parameters.resolution.as_deref().unwrap_or("hour") {
        "15m" => YieldSeriesResolution::FifteenMinutes,
        "hour" => YieldSeriesResolution::Hour,
        "day" => YieldSeriesResolution::Day,
        _ => return Err(ForecastApiError::InvalidQuery("resolution")),
    };
    let maximum_points = parameters.maximum_points.unwrap_or(2_000);
    if maximum_points == 0 || maximum_points > 10_000 {
        return Err(ForecastApiError::InvalidQuery("maximumPoints"));
    }
    let interval_count = (i128::from(parameters.end) - i128::from(parameters.start))
        / (i128::from(resolution.seconds()) * 1_000);
    if interval_count > i128::from(maximum_points) {
        return Err(ForecastApiError::QueryTooLarge);
    }
    let scope = match (parameters.inverter_id, parameters.string_id) {
        (None, None) => ForecastResourceScope::System {
            account_id,
            system_id,
        },
        (Some(inverter_id), None) => ForecastResourceScope::Inverter {
            account_id,
            system_id,
            inverter_id,
        },
        (Some(inverter_id), Some(string_id)) => ForecastResourceScope::String {
            account_id,
            system_id,
            inverter_id,
            string_id,
        },
        (None, Some(_)) => return Err(ForecastApiError::InvalidQuery("inverterId")),
    };
    Ok((
        scope,
        YieldSeriesQuery {
            start_epoch_millis: parameters.start,
            end_epoch_millis: parameters.end,
            basis,
            resolution,
            weather_run_id: parameters.weather_run_id,
            include_partial: parameters.include_partial.unwrap_or(false),
            maximum_points,
        },
    ))
}

fn validate_range(start: i64, end: i64) -> Result<(), ForecastApiError> {
    const MAXIMUM_RANGE_MILLISECONDS: i64 = 366 * 86_400 * 1_000;
    if end <= start {
        return Err(ForecastApiError::InvalidQuery("endEpochMillis"));
    }
    if end.saturating_sub(start) > MAXIMUM_RANGE_MILLISECONDS {
        return Err(ForecastApiError::QueryTooLarge);
    }
    Ok(())
}

async fn authorize(
    state: &ForecastState,
    principal: Option<Extension<RequestPrincipal>>,
    scope: ForecastResourceScope,
    write: bool,
) -> Result<UserId, ForecastApiError> {
    let Extension(principal) = principal.ok_or(ForecastApiError::Forbidden)?;
    require_scope(&principal, write)?;
    let identity = principal_identity(&principal)?;
    let authorized = if let Some(system_id) = scope.system_id() {
        state
            .authorizer
            .authorize_system(
                identity,
                system_id,
                if write {
                    Permission::SystemManage
                } else {
                    Permission::SystemRead
                },
                if write {
                    "forecast.settings.update"
                } else {
                    "forecast.settings.read"
                },
            )
            .await?
    } else {
        state
            .authorizer
            .authorize_account(
                identity,
                scope.account_id(),
                if write {
                    Permission::AccountManage
                } else {
                    Permission::AccountRead
                },
                if write {
                    "forecast.settings.update"
                } else {
                    "forecast.settings.read"
                },
            )
            .await?
    };
    if authorized.account_id != scope.account_id() {
        return Err(ForecastApiError::Forbidden);
    }
    Ok(authorized.actor_user_id)
}

fn require_scope(principal: &RequestPrincipal, write: bool) -> Result<(), ForecastApiError> {
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
        RequestPrincipal::ApiCredential { .. } | RequestPrincipal::SystemIngestion(_) => {
            Err(ForecastApiError::Forbidden)
        }
    }
}

fn validate_settings(input: &ForecastSettingsInput) -> Result<(), ForecastApiError> {
    if input.model_identifier.trim().is_empty() || input.model_identifier.len() > 64 {
        return Err(ForecastApiError::Validation(
            "modelIdentifier",
            "invalid_model_identifier",
        ));
    }
    if input.model_revision == 0 {
        return Err(ForecastApiError::Validation(
            "modelRevision",
            "invalid_model_revision",
        ));
    }
    if input
        .effective_to
        .is_some_and(|effective_to| effective_to <= input.effective_from)
    {
        return Err(ForecastApiError::Validation(
            "effectiveTo",
            "invalid_effective_period",
        ));
    }
    for (field, value) in [
        (
            "losses.soilingBasisPoints",
            input.losses.soiling_basis_points,
        ),
        (
            "losses.shadingBasisPoints",
            input.losses.shading_basis_points,
        ),
        (
            "losses.mismatchBasisPoints",
            input.losses.mismatch_basis_points,
        ),
        ("losses.wiringBasisPoints", input.losses.wiring_basis_points),
        (
            "losses.unavailabilityBasisPoints",
            input.losses.unavailability_basis_points,
        ),
    ] {
        if value > 10_000 {
            return Err(ForecastApiError::Validation(field, "loss_out_of_range"));
        }
    }
    if !(-5_000..=5_000).contains(&input.calibration_basis_points) {
        return Err(ForecastApiError::Validation(
            "calibrationBasisPoints",
            "calibration_out_of_range",
        ));
    }
    Ok(())
}

fn expected_version(headers: &HeaderMap) -> Result<u64, ForecastApiError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse().ok())
        .ok_or(ForecastApiError::PreconditionRequired)
}

fn with_etag(status: StatusCode, response: ForecastSettingsResponse) -> Response {
    let version = response.version;
    json_with_etag(status, response, version)
}

fn json_with_etag<T: Serialize>(status: StatusCode, body: T, version: u64) -> Response {
    let mut response = (status, Json(body)).into_response();
    if let Ok(value) = HeaderValue::from_str(&format!("\"{version}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ForecastApiError {
    #[error("forecast resource path is invalid")]
    InvalidPath,
    #[error("forecast query field is invalid")]
    InvalidQuery(&'static str),
    #[error("forecast query exceeds documented bounds")]
    QueryTooLarge,
    #[error("actual telemetry does not support the requested child scope")]
    UnsupportedScope,
    #[error("forecast access is forbidden")]
    Forbidden,
    #[error("forecast resource was not found")]
    NotFound,
    #[error("forecast resource version precondition is required")]
    PreconditionRequired,
    #[error("forecast resource version conflicts")]
    Conflict,
    #[error("forecast setting is invalid")]
    Validation(&'static str, &'static str),
    #[error("forecast service is unavailable")]
    Unavailable,
}

impl From<RequestAuthorizationError> for ForecastApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForecastValidationProblem {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: &'static str,
    field: &'static str,
}

impl IntoResponse for ForecastApiError {
    fn into_response(self) -> Response {
        let validation = match self {
            Self::Validation(field, detail) => Some((field, detail)),
            Self::InvalidQuery(field) => Some((field, "invalid_query_parameter")),
            Self::UnsupportedScope => Some(("scope", "unsupported_actual_scope")),
            _ => None,
        };
        if let Some((field, detail)) = validation {
            let mut response = (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ForecastValidationProblem {
                    problem_type: "https://pvlog.example/problems/forecast-validation",
                    title: "invalid_forecast_request",
                    status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                    detail,
                    field,
                }),
            )
                .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/problem+json"),
            );
            return response;
        }
        match self {
            Self::InvalidPath | Self::NotFound => StatusCode::NOT_FOUND,
            Self::QueryTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::Conflict => StatusCode::PRECONDITION_FAILED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Validation(_, _) | Self::InvalidQuery(_) | Self::UnsupportedScope => {
                unreachable!()
            }
        }
        .into_response()
    }
}
