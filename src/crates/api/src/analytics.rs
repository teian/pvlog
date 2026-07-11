use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{
    AnalysisExportFormat, AnalysisExportRequest, AnalysisExportResult, ModernAnalyticsError,
    ModernAnalyticsUseCases, QueryPlanRequest, QueryResolution, RequestedResolution, SeriesField,
    StatisticsPeriod,
};
use pvlog_domain::{SystemId, UserId};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
struct AnalyticsState {
    service: Arc<dyn ModernAnalyticsUseCases>,
}

pub fn analytics_router(service: Arc<dyn ModernAnalyticsUseCases>) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/series", get(time_series))
        .route("/api/v1/systems/{system_id}/statistics", get(statistics))
        .route(
            "/api/v1/systems/{system_id}/data-quality",
            get(data_quality),
        )
        .route("/api/v1/systems/{system_id}/analysis-exports", post(export))
        .with_state(AnalyticsState { service })
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeriesParameters {
    start_epoch_millis: i64,
    end_epoch_millis: i64,
    fields: String,
    resolution: Option<String>,
    timezone: Option<String>,
    maximum_points: Option<u32>,
    expected_interval_millis: Option<u64>,
    hot_data_start_epoch_millis: Option<i64>,
}

#[derive(Deserialize)]
struct StatisticsParameters {
    period: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeParameters {
    start_epoch_millis: i64,
    end_epoch_millis: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportBody {
    start_epoch_millis: i64,
    end_epoch_millis: i64,
    fields: Vec<String>,
    resolution: Option<String>,
    timezone: Option<String>,
    maximum_points: Option<u32>,
    format: String,
    asynchronous: Option<bool>,
}

async fn time_series(
    State(state): State<AnalyticsState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
    Query(parameters): Query<SeriesParameters>,
) -> Result<Response, AnalyticsApiError> {
    let request = query_request(parameters)?;
    Ok(Json(
        state
            .service
            .time_series(actor_id(actor)?, system_id, request)
            .await?,
    )
    .into_response())
}

async fn statistics(
    State(state): State<AnalyticsState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
    Query(parameters): Query<StatisticsParameters>,
) -> Result<Response, AnalyticsApiError> {
    let period = parse_period(&parameters.period)?;
    Ok(Json(
        state
            .service
            .statistics(actor_id(actor)?, system_id, period)
            .await?,
    )
    .into_response())
}

async fn data_quality(
    State(state): State<AnalyticsState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
    Query(parameters): Query<RangeParameters>,
) -> Result<Response, AnalyticsApiError> {
    if parameters.end_epoch_millis <= parameters.start_epoch_millis {
        return Err(AnalyticsApiError::Invalid);
    }
    Ok(Json(
        state
            .service
            .data_quality(
                actor_id(actor)?,
                system_id,
                parameters.start_epoch_millis,
                parameters.end_epoch_millis,
            )
            .await?,
    )
    .into_response())
}

async fn export(
    State(state): State<AnalyticsState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
    Json(body): Json<ExportBody>,
) -> Result<Response, AnalyticsApiError> {
    let format = match body.format.as_str() {
        "csv" => AnalysisExportFormat::Csv,
        "json" => AnalysisExportFormat::Json,
        _ => return Err(AnalyticsApiError::Invalid),
    };
    let parameters = SeriesParameters {
        start_epoch_millis: body.start_epoch_millis,
        end_epoch_millis: body.end_epoch_millis,
        fields: body.fields.join(","),
        resolution: body.resolution,
        timezone: body.timezone,
        maximum_points: body.maximum_points,
        expected_interval_millis: None,
        hot_data_start_epoch_millis: None,
    };
    match state
        .service
        .export(AnalysisExportRequest {
            system_id,
            actor: actor_id(actor)?,
            query: query_request(parameters)?,
            format,
            asynchronous: body.asynchronous.unwrap_or(false),
        })
        .await?
    {
        AnalysisExportResult::Ready {
            content_type,
            filename,
            bytes,
        } => download_response(&content_type, &filename, bytes),
        AnalysisExportResult::Queued { job_id } => Ok((
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "jobId": job_id })),
        )
            .into_response()),
    }
}

fn query_request(parameters: SeriesParameters) -> Result<QueryPlanRequest, AnalyticsApiError> {
    let fields = parameters
        .fields
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(parse_field)
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(QueryPlanRequest {
        start_epoch_millis: parameters.start_epoch_millis,
        end_epoch_millis: parameters.end_epoch_millis,
        requested_resolution: parameters
            .resolution
            .as_deref()
            .map_or(Ok(RequestedResolution::Auto), parse_resolution)?,
        fields,
        timezone: parameters.timezone.unwrap_or_else(|| "UTC".to_owned()),
        maximum_points: parameters.maximum_points.unwrap_or(2_000).min(10_000),
        expected_raw_interval_millis: parameters.expected_interval_millis.unwrap_or(300_000),
        hot_data_start_epoch_millis: parameters.hot_data_start_epoch_millis.unwrap_or(i64::MIN),
        available_rollups: BTreeSet::from([
            QueryResolution::FifteenMinutes,
            QueryResolution::Hourly,
            QueryResolution::Daily,
            QueryResolution::Monthly,
            QueryResolution::Yearly,
        ]),
    })
}

fn parse_field(value: &str) -> Result<SeriesField, AnalyticsApiError> {
    match value {
        "generation_power" => Ok(SeriesField::GenerationPower),
        "generation_energy" => Ok(SeriesField::GenerationEnergy),
        "consumption_power" => Ok(SeriesField::ConsumptionPower),
        "consumption_energy" => Ok(SeriesField::ConsumptionEnergy),
        "grid_power" => Ok(SeriesField::GridPower),
        "battery_power" => Ok(SeriesField::BatteryPower),
        "battery_state_of_charge" => Ok(SeriesField::BatteryStateOfCharge),
        "temperature" => Ok(SeriesField::Temperature),
        "extended" => Ok(SeriesField::Extended),
        "provenance" => Ok(SeriesField::Provenance),
        _ => Err(AnalyticsApiError::Invalid),
    }
}

fn parse_resolution(value: &str) -> Result<RequestedResolution, AnalyticsApiError> {
    match value {
        "auto" => Ok(RequestedResolution::Auto),
        "raw" => Ok(RequestedResolution::Raw),
        "15m" => Ok(RequestedResolution::FifteenMinutes),
        "hour" => Ok(RequestedResolution::Hourly),
        "day" => Ok(RequestedResolution::Daily),
        "month" => Ok(RequestedResolution::Monthly),
        "year" => Ok(RequestedResolution::Yearly),
        _ => Err(AnalyticsApiError::Invalid),
    }
}

fn parse_period(value: &str) -> Result<StatisticsPeriod, AnalyticsApiError> {
    match value {
        "day" => Ok(StatisticsPeriod::Daily),
        "month" => Ok(StatisticsPeriod::Monthly),
        "year" => Ok(StatisticsPeriod::Yearly),
        "lifetime" => Ok(StatisticsPeriod::Lifetime),
        _ => Err(AnalyticsApiError::Invalid),
    }
}

fn actor_id(actor: Option<Extension<UserId>>) -> Result<UserId, AnalyticsApiError> {
    actor
        .map(|Extension(actor)| actor)
        .ok_or(AnalyticsApiError::Forbidden)
}

fn download_response(
    content_type: &str,
    filename: &str,
    bytes: Vec<u8>,
) -> Result<Response, AnalyticsApiError> {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).map_err(|_| AnalyticsApiError::Invalid)?,
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| AnalyticsApiError::Invalid)?,
    );
    Ok(response)
}

enum AnalyticsApiError {
    Invalid,
    Forbidden,
    Domain(ModernAnalyticsError),
}

impl From<ModernAnalyticsError> for AnalyticsApiError {
    fn from(value: ModernAnalyticsError) -> Self {
        Self::Domain(value)
    }
}

impl IntoResponse for AnalyticsApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Invalid | Self::Domain(ModernAnalyticsError::Invalid) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::Forbidden | Self::Domain(ModernAnalyticsError::Forbidden) => {
                StatusCode::FORBIDDEN
            }
            Self::Domain(ModernAnalyticsError::NotFound) => StatusCode::NOT_FOUND,
            Self::Domain(ModernAnalyticsError::RequiresAsync) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Domain(ModernAnalyticsError::Unavailable) => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
