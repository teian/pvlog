//! Legacy insolation and regional supply adapters.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    csv_record, format_legacy_time, parse_legacy_auth, parse_legacy_date,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono_tz::Tz;
use std::{str::FromStr as _, sync::Arc};
use thiserror::Error;
use time::{Date, Time};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyInsolationQuery {
    pub date: Option<Date>,
    pub timezone: String,
    pub coordinates: Option<(String, String)>,
    pub target_system_id: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyInsolationPoint {
    pub time: Time,
    pub power_watts: i64,
    pub energy_wh: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySupplyQuery {
    pub timezone: String,
    pub region_key: Option<String>,
    pub include_history: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySupplyStatus {
    pub timestamp: String,
    pub region_name: String,
    pub utilisation_milli_percent: i64,
    pub total_output_watts: i64,
    pub total_input_watts: i64,
    pub average_output_watts: i64,
    pub average_input_watts: i64,
    pub average_net_watts: i64,
    pub systems_out: u64,
    pub systems_in: u64,
    pub total_size_watts: i64,
    pub average_size_watts: i64,
}

#[async_trait]
pub trait LegacyProviderUseCases: Send + Sync {
    async fn insolation(
        &self,
        auth: &LegacyAuth,
        query: &LegacyInsolationQuery,
    ) -> Result<Vec<LegacyInsolationPoint>, LegacyProviderError>;
    async fn supply(
        &self,
        auth: &LegacyAuth,
        query: &LegacySupplyQuery,
    ) -> Result<Vec<LegacySupplyStatus>, LegacyProviderError>;
}

#[derive(Clone)]
struct ProviderState {
    service: Arc<dyn LegacyProviderUseCases>,
}
pub fn legacy_provider_router(service: Arc<dyn LegacyProviderUseCases>) -> Router {
    Router::new()
        .route("/service/r2/getinsolation.jsp", get(get_insolation))
        .route("/service/r2/getsupply.jsp", get(get_supply))
        .with_state(ProviderState { service })
}

async fn get_insolation(
    State(state): State<ProviderState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, ProviderApiError> {
    let parameters = parameters(query)?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let points = state
        .service
        .insolation(&auth, &parse_insolation(&parameters)?)
        .await?;
    let body = points
        .iter()
        .map(|point| {
            let fields = [
                format_legacy_time(point.time),
                point.power_watts.to_string(),
                point.energy_wh.to_string(),
            ];
            csv_record(fields.iter().map(|field| Some(field.as_str())))
        })
        .collect::<Vec<_>>()
        .join(";");
    Ok(text_response(StatusCode::OK, &body))
}

async fn get_supply(
    State(state): State<ProviderState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, ProviderApiError> {
    let parameters = parameters(query)?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let statuses = state
        .service
        .supply(&auth, &parse_supply(&parameters)?)
        .await?;
    Ok(text_response(
        StatusCode::OK,
        &statuses
            .iter()
            .map(format_supply)
            .collect::<Vec<_>>()
            .join(";"),
    ))
}

fn parameters(query: Option<String>) -> Result<LegacyParameters, ProviderApiError> {
    LegacyParameters::parse(query.unwrap_or_default().as_bytes()).map_err(ProviderApiError::from)
}
fn parse_insolation(
    parameters: &LegacyParameters,
) -> Result<LegacyInsolationQuery, ProviderApiError> {
    let coordinates = parameters
        .get("ll")
        .map(|value| -> Result<(String, String), ProviderApiError> {
            let (latitude, longitude) = value
                .split_once(',')
                .ok_or_else(|| ProviderApiError::bad("Latitude/Longitude invalid"))?;
            validate_decimal(latitude)?;
            validate_decimal(longitude)?;
            Ok((latitude.to_owned(), longitude.to_owned()))
        })
        .transpose()?;
    Ok(LegacyInsolationQuery {
        date: parameters.get("d").map(parse_legacy_date).transpose()?,
        timezone: timezone(parameters.get("tz"))?,
        coordinates,
        target_system_id: parse_id(parameters.get("sid1"), "sid1")?,
    })
}
fn parse_supply(parameters: &LegacyParameters) -> Result<LegacySupplyQuery, ProviderApiError> {
    let region_key = parameters
        .get("r")
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(LegacySupplyQuery {
        timezone: timezone(parameters.get("tz"))?,
        include_history: region_key.is_some(),
        region_key,
    })
}
fn timezone(value: Option<&str>) -> Result<String, ProviderApiError> {
    let value = value.filter(|value| !value.is_empty()).unwrap_or("UTC");
    Tz::from_str(value).map_err(|_| ProviderApiError::bad("Timezone invalid"))?;
    Ok(value.to_owned())
}
fn validate_decimal(value: &str) -> Result<(), ProviderApiError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| ProviderApiError::bad("Coordinate invalid"))?;
    if parsed.is_finite() {
        Ok(())
    } else {
        Err(ProviderApiError::bad("Coordinate invalid"))
    }
}
fn parse_id(value: Option<&str>, field: &str) -> Result<Option<u64>, ProviderApiError> {
    value
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| ProviderApiError::bad(format!("{field} invalid")))
}
fn format_supply(status: &LegacySupplyStatus) -> String {
    let fields = [
        status.timestamp.clone(),
        status.region_name.clone(),
        scaled(status.utilisation_milli_percent),
        status.total_output_watts.to_string(),
        status.total_input_watts.to_string(),
        status.average_output_watts.to_string(),
        status.average_input_watts.to_string(),
        status.average_net_watts.to_string(),
        status.systems_out.to_string(),
        status.systems_in.to_string(),
        status.total_size_watts.to_string(),
        status.average_size_watts.to_string(),
    ];
    csv_record(fields.iter().map(|field| Some(field.as_str())))
}
fn scaled(value: i64) -> String {
    let negative = value < 0;
    let absolute = value.unsigned_abs();
    let whole = absolute / 1_000;
    let fraction = absolute % 1_000;
    let mut result = if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:03}")
            .trim_end_matches('0')
            .to_owned()
    };
    if negative {
        result.insert(0, '-');
    }
    result
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LegacyProviderError {
    #[error("provider credentials are invalid")]
    Unauthorized,
    #[error("provider data is unavailable")]
    Unavailable,
    #[error("requested location or region is unsupported")]
    Unsupported,
}
enum ProviderApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(LegacyProviderError),
}
impl ProviderApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}
impl From<LegacyProtocolError> for ProviderApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}
impl From<LegacyProviderError> for ProviderApiError {
    fn from(value: LegacyProviderError) -> Self {
        Self::Service(value)
    }
}
impl IntoResponse for ProviderApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Legacy(error) => text_response(
                StatusCode::from_u16(error.kind.status()).unwrap_or(StatusCode::BAD_REQUEST),
                &error.body(),
            ),
            Self::Protocol(error) => text_response(
                StatusCode::BAD_REQUEST,
                &format!("Bad request 400: {error}"),
            ),
            Self::Service(LegacyProviderError::Unauthorized) => {
                text_response(StatusCode::FORBIDDEN, "Forbidden 403: Invalid API Key")
            }
            Self::Service(LegacyProviderError::Unavailable) => text_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Service unavailable 503: Provider data is unavailable",
            ),
            Self::Service(LegacyProviderError::Unsupported) => text_response(
                StatusCode::BAD_REQUEST,
                "Bad request 400: Location or region is unsupported",
            ),
        }
    }
}
fn text_response(status: StatusCode, body: &str) -> Response {
    let mut response = Response::new(Body::from(body.to_owned()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
