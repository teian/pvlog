//! Legacy status and statistic query adapters with fixed CSV field order.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    csv_record, format_legacy_date, format_legacy_time, parse_legacy_auth, parse_legacy_bool,
    parse_legacy_date, parse_legacy_time,
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
use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;
use time::{Date, Time};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyStatusRecord {
    pub date: Date,
    pub time: Time,
    pub generation_energy_wh: Option<i64>,
    pub generation_power_watts: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub consumption_power_watts: Option<i64>,
    pub normalized_output_milli_kw_per_kw: Option<i64>,
    pub temperature_milli_celsius: Option<i64>,
    pub voltage_millivolts: Option<i64>,
    pub extended_milli: BTreeMap<u8, i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyHistoryStatus {
    pub status: LegacyStatusRecord,
    pub efficiency_milli_kwh_per_kw: Option<i64>,
    pub average_power_watts: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyDayStatistics {
    pub generation_energy_wh: Option<i64>,
    pub generation_power_watts: Option<i64>,
    pub peak_power_watts: Option<i64>,
    pub peak_power_time: Option<Time>,
    pub consumption_energy_wh: Option<i64>,
    pub consumption_power_watts: Option<i64>,
    pub standby_power_watts: Option<i64>,
    pub standby_power_time: Option<Time>,
    pub minimum_temperature_milli_celsius: Option<i64>,
    pub maximum_temperature_milli_celsius: Option<i64>,
    pub average_temperature_milli_celsius: Option<i64>,
    pub include_consumption: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyRangeStatistics {
    pub generated_wh: Option<i64>,
    pub exported_wh: Option<i64>,
    pub average_generation_wh: Option<i64>,
    pub minimum_generation_wh: Option<i64>,
    pub maximum_generation_wh: Option<i64>,
    pub average_efficiency_milli_kwh_per_kw: Option<i64>,
    pub outputs: u32,
    pub actual_date_from: Date,
    pub actual_date_to: Date,
    pub record_efficiency_milli_kwh_per_kw: Option<i64>,
    pub record_date: Option<Date>,
    pub consumed_wh: Option<i64>,
    pub import_peak_wh: Option<i64>,
    pub import_off_peak_wh: Option<i64>,
    pub import_shoulder_wh: Option<i64>,
    pub import_high_shoulder_wh: Option<i64>,
    pub average_consumption_wh: Option<i64>,
    pub minimum_consumption_wh: Option<i64>,
    pub maximum_consumption_wh: Option<i64>,
    pub credit_milli_currency: Option<i64>,
    pub debit_milli_currency: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyStatusQuery {
    Latest {
        date: Option<Date>,
        time: Option<Time>,
        target_system_id: Option<u64>,
        include_extended: bool,
    },
    History {
        date: Option<Date>,
        after: Option<Time>,
        from: Option<Time>,
        to: Option<Time>,
        ascending: bool,
        limit: u16,
        target_system_id: Option<u64>,
        include_extended: bool,
    },
    DayStatistics {
        date: Option<Date>,
        from: Option<Time>,
        to: Option<Time>,
        target_system_id: Option<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyStatisticQuery {
    pub date_from: Option<Date>,
    pub date_to: Option<Date>,
    pub include_consumption: bool,
    pub include_credit_debit: bool,
    pub target_system_id: Option<u64>,
}

#[async_trait]
pub trait LegacyQueryUseCases: Send + Sync {
    async fn latest_status(
        &self,
        auth: &LegacyAuth,
        query: &LegacyStatusQuery,
    ) -> Result<LegacyStatusRecord, LegacyQueryError>;
    async fn status_history(
        &self,
        auth: &LegacyAuth,
        query: &LegacyStatusQuery,
    ) -> Result<Vec<LegacyHistoryStatus>, LegacyQueryError>;
    async fn day_statistics(
        &self,
        auth: &LegacyAuth,
        query: &LegacyStatusQuery,
    ) -> Result<LegacyDayStatistics, LegacyQueryError>;
    async fn range_statistics(
        &self,
        auth: &LegacyAuth,
        query: &LegacyStatisticQuery,
    ) -> Result<LegacyRangeStatistics, LegacyQueryError>;
}

#[derive(Clone)]
struct QueryState {
    service: Arc<dyn LegacyQueryUseCases>,
}

pub fn legacy_query_router(service: Arc<dyn LegacyQueryUseCases>) -> Router {
    Router::new()
        .route("/service/r2/getstatus.jsp", get(get_status))
        .route("/service/r2/getstatistic.jsp", get(get_statistic))
        .with_state(QueryState { service })
}

async fn get_status(
    State(state): State<QueryState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, QueryApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let query = parse_status_query(&parameters)?;
    let body = match &query {
        LegacyStatusQuery::Latest {
            include_extended, ..
        } => format_latest(
            &state.service.latest_status(&auth, &query).await?,
            *include_extended,
        ),
        LegacyStatusQuery::History {
            include_extended, ..
        } => state
            .service
            .status_history(&auth, &query)
            .await?
            .iter()
            .map(|status| format_history(status, *include_extended))
            .collect::<Vec<_>>()
            .join(";"),
        LegacyStatusQuery::DayStatistics { .. } => {
            format_day_statistics(&state.service.day_statistics(&auth, &query).await?)
        }
    };
    Ok(text_response(StatusCode::OK, &body))
}

async fn get_statistic(
    State(state): State<QueryState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, QueryApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let query = parse_statistic_query(&parameters)?;
    let statistics = state.service.range_statistics(&auth, &query).await?;
    Ok(text_response(
        StatusCode::OK,
        &format_range_statistics(&statistics, &query),
    ))
}

fn parse_status_query(parameters: &LegacyParameters) -> Result<LegacyStatusQuery, QueryApiError> {
    let date = parameters.get("d").map(parse_legacy_date).transpose()?;
    let time = parameters.get("t").map(parse_legacy_time).transpose()?;
    let from = parameters.get("from").map(parse_legacy_time).transpose()?;
    let to = parameters.get("to").map(parse_legacy_time).transpose()?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err(QueryApiError::bad("Time range is invalid"));
    }
    let target_system_id = parse_u64(parameters.get("sid1"), "sid1")?;
    let include_extended = flag(parameters.get("ext"))?;
    let history = flag(parameters.get("h"))?;
    let statistics = flag(parameters.get("stats"))?;
    if history && statistics {
        return Err(QueryApiError::bad(
            "History and day statistics are exclusive",
        ));
    }
    if statistics {
        return Ok(LegacyStatusQuery::DayStatistics {
            date,
            from,
            to,
            target_system_id,
        });
    }
    if history {
        let limit = parse_u64(parameters.get("limit"), "limit")?
            .unwrap_or(30)
            .min(288);
        return Ok(LegacyStatusQuery::History {
            date,
            after: time,
            from,
            to,
            ascending: flag(parameters.get("asc"))?,
            limit: u16::try_from(limit).map_err(|_| QueryApiError::bad("Limit invalid"))?,
            target_system_id,
            include_extended,
        });
    }
    Ok(LegacyStatusQuery::Latest {
        date,
        time,
        target_system_id,
        include_extended,
    })
}

fn parse_statistic_query(
    parameters: &LegacyParameters,
) -> Result<LegacyStatisticQuery, QueryApiError> {
    let date_from = parameters.get("df").map(parse_legacy_date).transpose()?;
    let date_to = parameters.get("dt").map(parse_legacy_date).transpose()?;
    if date_from.zip(date_to).is_some_and(|(from, to)| from > to) {
        return Err(QueryApiError::bad("Date range is invalid"));
    }
    Ok(LegacyStatisticQuery {
        date_from,
        date_to,
        include_consumption: flag(parameters.get("c"))?,
        include_credit_debit: flag(parameters.get("crdr"))?,
        target_system_id: parse_u64(parameters.get("sid1"), "sid1")?,
    })
}

fn flag(value: Option<&str>) -> Result<bool, QueryApiError> {
    value
        .map(parse_legacy_bool)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(QueryApiError::from)
}

fn parse_u64(value: Option<&str>, field: &str) -> Result<Option<u64>, QueryApiError> {
    value
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| QueryApiError::bad(format!("{field} invalid")))
        })
        .transpose()
}

fn format_latest(status: &LegacyStatusRecord, extended: bool) -> String {
    let mut fields = vec![
        format_legacy_date(status.date),
        format_legacy_time(status.time),
        number(status.generation_energy_wh),
        number(status.generation_power_watts),
        number(status.consumption_energy_wh),
        number(status.consumption_power_watts),
        decimal(status.normalized_output_milli_kw_per_kw),
        decimal(status.temperature_milli_celsius),
        decimal(status.voltage_millivolts),
    ];
    append_extended(&mut fields, status, extended);
    csv_record(fields.iter().map(|field| Some(field.as_str())))
}

fn format_history(history: &LegacyHistoryStatus, extended: bool) -> String {
    let status = &history.status;
    let mut fields = vec![
        format_legacy_date(status.date),
        format_legacy_time(status.time),
        number(status.generation_energy_wh),
        decimal(history.efficiency_milli_kwh_per_kw),
        number(status.generation_power_watts),
        number(history.average_power_watts),
        decimal(status.normalized_output_milli_kw_per_kw),
        number(status.consumption_energy_wh),
        number(status.consumption_power_watts),
        decimal(status.temperature_milli_celsius),
        decimal(status.voltage_millivolts),
    ];
    append_extended(&mut fields, status, extended);
    csv_record(fields.iter().map(|field| Some(field.as_str())))
}

fn append_extended(fields: &mut Vec<String>, status: &LegacyStatusRecord, include: bool) {
    if include {
        fields.extend((7_u8..=12).map(|index| decimal(status.extended_milli.get(&index).copied())));
    }
}

fn format_day_statistics(statistics: &LegacyDayStatistics) -> String {
    let generation = csv_record([
        Some(number(statistics.generation_energy_wh).as_str()),
        Some(number(statistics.generation_power_watts).as_str()),
        Some(number(statistics.peak_power_watts).as_str()),
        Some(optional_time(statistics.peak_power_time).as_str()),
    ]);
    let mut sections = vec![generation];
    if statistics.include_consumption {
        sections.push(csv_record([
            Some(number(statistics.consumption_energy_wh).as_str()),
            Some(number(statistics.consumption_power_watts).as_str()),
            Some(number(statistics.standby_power_watts).as_str()),
            Some(optional_time(statistics.standby_power_time).as_str()),
        ]));
    }
    if statistics.minimum_temperature_milli_celsius.is_some()
        || statistics.maximum_temperature_milli_celsius.is_some()
        || statistics.average_temperature_milli_celsius.is_some()
    {
        sections.push(csv_record([
            Some(decimal(statistics.minimum_temperature_milli_celsius).as_str()),
            Some(decimal(statistics.maximum_temperature_milli_celsius).as_str()),
            Some(decimal(statistics.average_temperature_milli_celsius).as_str()),
        ]));
    }
    sections.join(";")
}

fn format_range_statistics(
    statistics: &LegacyRangeStatistics,
    query: &LegacyStatisticQuery,
) -> String {
    let mut fields = vec![
        number(statistics.generated_wh),
        number(statistics.exported_wh),
        number(statistics.average_generation_wh),
        number(statistics.minimum_generation_wh),
        number(statistics.maximum_generation_wh),
        decimal(statistics.average_efficiency_milli_kwh_per_kw),
        statistics.outputs.to_string(),
        format_legacy_date(statistics.actual_date_from),
        format_legacy_date(statistics.actual_date_to),
        decimal(statistics.record_efficiency_milli_kwh_per_kw),
        statistics
            .record_date
            .map_or_else(|| "NaN".to_owned(), format_legacy_date),
    ];
    if query.include_consumption {
        fields.extend([
            number(statistics.consumed_wh),
            number(statistics.import_peak_wh),
            number(statistics.import_off_peak_wh),
            number(statistics.import_shoulder_wh),
            number(statistics.import_high_shoulder_wh),
            number(statistics.average_consumption_wh),
            number(statistics.minimum_consumption_wh),
            number(statistics.maximum_consumption_wh),
        ]);
    }
    if query.include_credit_debit {
        fields.extend([
            decimal(statistics.credit_milli_currency),
            decimal(statistics.debit_milli_currency),
        ]);
    }
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

fn optional_time(value: Option<Time>) -> String {
    value.map_or_else(|| "NaN".to_owned(), format_legacy_time)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LegacyQueryError {
    #[error("legacy query credentials are invalid")]
    Unauthorized,
    #[error("requested system is inaccessible")]
    InaccessibleSystem,
    #[error("no matching status was found")]
    NoStatus,
    #[error("legacy query storage is unavailable")]
    Unavailable,
}

enum QueryApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(LegacyQueryError),
}

impl QueryApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}

impl From<LegacyProtocolError> for QueryApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LegacyQueryError> for QueryApiError {
    fn from(value: LegacyQueryError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for QueryApiError {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Legacy(error) => error,
            Self::Protocol(error) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: error.to_string(),
            },
            Self::Service(LegacyQueryError::Unauthorized) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Invalid API Key".to_owned(),
            },
            Self::Service(LegacyQueryError::InaccessibleSystem) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Inaccessible System ID".to_owned(),
            },
            Self::Service(LegacyQueryError::NoStatus) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: "No status found".to_owned(),
            },
            Self::Service(LegacyQueryError::Unavailable) => {
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
