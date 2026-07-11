//! Legacy daily-output, extended, missing-date, and status-deletion adapters.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, csv_record, format_legacy_date, format_legacy_time, parse_legacy_auth,
    parse_legacy_bool, parse_legacy_date, parse_legacy_time,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;
use time::{Date, Duration, Time};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyAggregate {
    Month,
    Year,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyOutputQuery {
    pub date_from: Option<Date>,
    pub date_to: Option<Date>,
    pub aggregate: Option<LegacyAggregate>,
    pub limit: u16,
    pub team_id: Option<u64>,
    pub target_system_id: Option<u64>,
    pub include_insolation: bool,
    pub include_time_of_export: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyDailyOutputRecord {
    pub date: Date,
    pub generated_wh: Option<i64>,
    pub efficiency_milli_kwh_per_kw: Option<i64>,
    pub exported_wh: Option<i64>,
    pub used_wh: Option<i64>,
    pub peak_power_watts: Option<i64>,
    pub peak_time: Option<Time>,
    pub condition: Option<String>,
    pub minimum_temperature_milli_celsius: Option<i64>,
    pub maximum_temperature_milli_celsius: Option<i64>,
    pub import_peak_wh: Option<i64>,
    pub import_off_peak_wh: Option<i64>,
    pub import_shoulder_wh: Option<i64>,
    pub import_high_shoulder_wh: Option<i64>,
    pub export_peak_wh: Option<i64>,
    pub export_off_peak_wh: Option<i64>,
    pub export_shoulder_wh: Option<i64>,
    pub export_high_shoulder_wh: Option<i64>,
    pub insolation_wh: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyDailyExtended {
    pub date: Date,
    pub values_milli: BTreeMap<u8, i64>,
}

#[async_trait]
pub trait LegacyOutputUseCases: Send + Sync {
    async fn outputs(
        &self,
        auth: &LegacyAuth,
        query: &LegacyOutputQuery,
    ) -> Result<Vec<LegacyDailyOutputRecord>, LegacyOutputsError>;
    async fn extended(
        &self,
        auth: &LegacyAuth,
        date_from: Option<Date>,
        date_to: Option<Date>,
        limit: u16,
    ) -> Result<Vec<LegacyDailyExtended>, LegacyOutputsError>;
    async fn missing_dates(
        &self,
        auth: &LegacyAuth,
        date_from: Option<Date>,
        date_to: Option<Date>,
    ) -> Result<Vec<Date>, LegacyOutputsError>;
    async fn deletion_today(&self, auth: &LegacyAuth) -> Result<Date, LegacyOutputsError>;
    async fn delete_status(
        &self,
        auth: &LegacyAuth,
        date: Date,
        time: Option<Time>,
    ) -> Result<bool, LegacyOutputsError>;
}

#[derive(Clone)]
struct OutputState {
    service: Arc<dyn LegacyOutputUseCases>,
}

pub fn legacy_outputs_router(service: Arc<dyn LegacyOutputUseCases>) -> Router {
    Router::new()
        .route("/service/r2/getoutput.jsp", get(get_output))
        .route("/service/r2/getextended.jsp", get(get_extended))
        .route("/service/r2/getmissing.jsp", get(get_missing))
        .route(
            "/service/r2/deletestatus.jsp",
            get(delete_get).post(delete_post),
        )
        .with_state(OutputState { service })
}

async fn get_output(
    State(state): State<OutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, OutputApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let query = parse_output_query(&parameters)?;
    let body = state
        .service
        .outputs(&auth, &query)
        .await?
        .iter()
        .map(|record| format_output(record, &query))
        .collect::<Vec<_>>()
        .join(";");
    Ok(text_response(StatusCode::OK, &body))
}

async fn get_extended(
    State(state): State<OutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, OutputApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let (date_from, date_to) = date_range(&parameters)?;
    let limit = parse_limit(parameters.get("limit"), 50, 50)?;
    let body = state
        .service
        .extended(&auth, date_from, date_to, limit)
        .await?
        .iter()
        .map(format_extended)
        .collect::<Vec<_>>()
        .join(";");
    Ok(text_response(StatusCode::OK, &body))
}

async fn get_missing(
    State(state): State<OutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, OutputApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let (date_from, date_to) = date_range(&parameters)?;
    let mut dates = state
        .service
        .missing_dates(&auth, date_from, date_to)
        .await?;
    dates.sort_unstable();
    dates.truncate(50);
    Ok(text_response(
        StatusCode::OK,
        &dates
            .into_iter()
            .map(format_legacy_date)
            .collect::<Vec<_>>()
            .join(","),
    ))
}

async fn delete_get(
    State(state): State<OutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, OutputApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    delete(state, LegacyMethod::Get, headers, parameters).await
}

async fn delete_post(
    State(state): State<OutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, OutputApiError> {
    if query.is_some_and(|query| !query.is_empty()) {
        return Err(OutputApiError::bad("POST parameters must use form data"));
    }
    delete(
        state,
        LegacyMethod::Post,
        headers,
        LegacyParameters::parse(&body)?,
    )
    .await
}

async fn delete(
    state: OutputState,
    method: LegacyMethod,
    headers: HeaderMap,
    parameters: LegacyParameters,
) -> Result<Response, OutputApiError> {
    let auth = parse_legacy_auth(method, &headers, &parameters)?;
    let date_value = parameters
        .get("d")
        .ok_or_else(|| OutputApiError::bad("Date is required"))?;
    let date = parse_legacy_date(date_value)
        .map_err(|_| OutputApiError::bad(format!("Invalid date: {date_value}")))?;
    let time = parameters
        .get("t")
        .map(parse_legacy_time)
        .transpose()
        .map_err(|_| {
            OutputApiError::bad(format!(
                "Invalid time: {}",
                parameters.get("t").unwrap_or_default()
            ))
        })?;
    let today = state.service.deletion_today(&auth).await?;
    if date > today {
        return Err(OutputApiError::bad("Date is in the future"));
    }
    if today - date > Duration::days(14) {
        return Err(OutputApiError::bad("Date is older than 14 days"));
    }
    if !state.service.delete_status(&auth, date, time).await? {
        return Err(OutputApiError::bad("Status not found"));
    }
    Ok(text_response(
        StatusCode::OK,
        LegacySuccess::DeletedStatus.body(),
    ))
}

fn parse_output_query(parameters: &LegacyParameters) -> Result<LegacyOutputQuery, OutputApiError> {
    let (date_from, date_to) = date_range(parameters)?;
    let aggregate = match parameters.get("a") {
        None => None,
        Some("m") => Some(LegacyAggregate::Month),
        Some("y") => Some(LegacyAggregate::Year),
        Some(_) => return Err(OutputApiError::bad("Aggregate invalid")),
    };
    let team_id = parse_id(parameters.get("tid"), "tid")?;
    if aggregate.is_some() && team_id.is_some() {
        return Err(OutputApiError::bad(
            "Aggregated team output is not supported",
        ));
    }
    Ok(LegacyOutputQuery {
        date_from,
        date_to,
        aggregate,
        limit: parse_limit(parameters.get("limit"), 30, 150)?,
        team_id,
        target_system_id: parse_id(parameters.get("sid1"), "sid1")?,
        include_insolation: flag(parameters.get("insolation"))?,
        include_time_of_export: flag(parameters.get("timeofexport"))?,
    })
}

fn date_range(
    parameters: &LegacyParameters,
) -> Result<(Option<Date>, Option<Date>), OutputApiError> {
    let from = parameters.get("df").map(parse_legacy_date).transpose()?;
    let to = parameters.get("dt").map(parse_legacy_date).transpose()?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err(OutputApiError::bad("Date range is invalid"));
    }
    Ok((from, to))
}

fn parse_limit(value: Option<&str>, default: u16, maximum: u16) -> Result<u16, OutputApiError> {
    value.map_or(Ok(default), |value| {
        value
            .parse::<u16>()
            .ok()
            .filter(|limit| *limit > 0)
            .map(|limit| limit.min(maximum))
            .ok_or_else(|| OutputApiError::bad("Limit invalid"))
    })
}

fn parse_id(value: Option<&str>, field: &str) -> Result<Option<u64>, OutputApiError> {
    value
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| OutputApiError::bad(format!("{field} invalid")))
        })
        .transpose()
}

fn flag(value: Option<&str>) -> Result<bool, OutputApiError> {
    value
        .map(parse_legacy_bool)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(OutputApiError::from)
}

fn format_output(record: &LegacyDailyOutputRecord, query: &LegacyOutputQuery) -> String {
    let mut fields = vec![
        format_legacy_date(record.date),
        number(record.generated_wh),
        decimal(record.efficiency_milli_kwh_per_kw),
        number(record.exported_wh),
        number(record.used_wh),
        number(record.peak_power_watts),
        record
            .peak_time
            .map_or_else(String::new, format_legacy_time),
        record.condition.clone().unwrap_or_default(),
        decimal(record.minimum_temperature_milli_celsius),
        decimal(record.maximum_temperature_milli_celsius),
        number(record.import_peak_wh),
        number(record.import_off_peak_wh),
        number(record.import_shoulder_wh),
        number(record.import_high_shoulder_wh),
    ];
    if query.include_time_of_export {
        fields.extend([
            number(record.export_peak_wh),
            number(record.export_off_peak_wh),
            number(record.export_shoulder_wh),
            number(record.export_high_shoulder_wh),
        ]);
    }
    if query.include_insolation {
        fields.push(number(record.insolation_wh));
    }
    csv_record(fields.iter().map(|field| Some(field.as_str())))
}

fn format_extended(record: &LegacyDailyExtended) -> String {
    let mut fields = vec![format_legacy_date(record.date)];
    fields.extend((7_u8..=12).map(|index| decimal(record.values_milli.get(&index).copied())));
    csv_record(fields.iter().map(|field| Some(field.as_str())))
}

fn number(value: Option<i64>) -> String {
    value.map_or_else(|| "NaN".to_owned(), |value| value.to_string())
}

fn decimal(value: Option<i64>) -> String {
    value.map_or_else(
        || "NaN".to_owned(),
        |value| {
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
        },
    )
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LegacyOutputsError {
    #[error("legacy output credentials are invalid")]
    Unauthorized,
    #[error("requested output data is inaccessible")]
    Inaccessible,
    #[error("legacy output storage is unavailable")]
    Unavailable,
}

enum OutputApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(LegacyOutputsError),
}

impl OutputApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}

impl From<LegacyProtocolError> for OutputApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LegacyOutputsError> for OutputApiError {
    fn from(value: LegacyOutputsError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for OutputApiError {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Legacy(error) => error,
            Self::Protocol(error) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: error.to_string(),
            },
            Self::Service(LegacyOutputsError::Unauthorized) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Invalid API Key".to_owned(),
            },
            Self::Service(LegacyOutputsError::Inaccessible) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Inaccessible System ID".to_owned(),
            },
            Self::Service(LegacyOutputsError::Unavailable) => {
                return text_response(StatusCode::SERVICE_UNAVAILABLE, "Service unavailable");
            }
        };
        text_response(
            StatusCode::from_u16(error.kind.status()).unwrap_or(StatusCode::BAD_REQUEST),
            &error.body(),
        )
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
