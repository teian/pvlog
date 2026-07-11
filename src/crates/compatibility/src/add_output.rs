//! `addoutput.jsp` compatibility adapter over canonical daily-output use cases.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, parse_csv_record, parse_legacy_auth, parse_legacy_date, parse_legacy_time,
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
use std::sync::Arc;
use thiserror::Error;
use time::{Date, Time, macros::date};

const MAX_DAILY_CONSUMPTION_WH: i64 = 999_999_999;
const VALID_CONDITIONS: &[&str] = &[
    "Fine",
    "Partly Cloudy",
    "Mostly Cloudy",
    "Cloudy",
    "Showers",
    "Snow",
    "Hazy",
    "Fog",
    "Dusty",
    "Frost",
    "Storm",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyOutput {
    pub date: Date,
    pub generated_wh: Option<i64>,
    pub exported_wh: Option<i64>,
    pub peak_power_watts: Option<i64>,
    pub peak_time: Option<Time>,
    pub condition: Option<String>,
    pub minimum_temperature_milli_celsius: Option<i64>,
    pub maximum_temperature_milli_celsius: Option<i64>,
    pub comments: Option<String>,
    pub import_peak_wh: Option<i64>,
    pub import_off_peak_wh: Option<i64>,
    pub import_shoulder_wh: Option<i64>,
    pub import_high_shoulder_wh: Option<i64>,
    pub consumption_wh: Option<i64>,
    pub export_peak_wh: Option<i64>,
    pub export_off_peak_wh: Option<i64>,
    pub export_shoulder_wh: Option<i64>,
    pub export_high_shoulder_wh: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddOutputPolicy {
    pub today: Date,
    pub effective_capacity_watts: u64,
    pub batching_enabled: bool,
    pub maximum_batch_size: usize,
}

#[async_trait]
pub trait AddOutputUseCases: Send + Sync {
    async fn policy(&self, auth: &LegacyAuth) -> Result<AddOutputPolicy, AddOutputServiceError>;
    async fn output_exists(
        &self,
        auth: &LegacyAuth,
        date: Date,
    ) -> Result<bool, AddOutputServiceError>;
    async fn upsert_outputs(
        &self,
        auth: &LegacyAuth,
        outputs: Vec<DailyOutput>,
    ) -> Result<(), AddOutputServiceError>;
}

#[derive(Clone)]
struct AddOutputState {
    service: Arc<dyn AddOutputUseCases>,
}

pub fn add_output_router(service: Arc<dyn AddOutputUseCases>) -> Router {
    Router::new()
        .route(
            "/service/r2/addoutput.jsp",
            get(add_output_get).post(add_output_post),
        )
        .with_state(AddOutputState { service })
}

async fn add_output_get(
    State(state): State<AddOutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, AddOutputApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    execute(state, LegacyMethod::Get, headers, parameters).await
}

async fn add_output_post(
    State(state): State<AddOutputState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, AddOutputApiError> {
    if query.is_some_and(|query| !query.is_empty()) {
        return Err(AddOutputApiError::bad("POST parameters must use form data"));
    }
    let parameters = LegacyParameters::parse(&body)?;
    execute(state, LegacyMethod::Post, headers, parameters).await
}

async fn execute(
    state: AddOutputState,
    method: LegacyMethod,
    headers: HeaderMap,
    parameters: LegacyParameters,
) -> Result<Response, AddOutputApiError> {
    let auth = parse_legacy_auth(method, &headers, &parameters)?;
    let policy = state.service.policy(&auth).await?;
    let mut outputs = parse_outputs(&parameters)?;
    validate_batch(&outputs, policy)?;
    for output in &mut outputs {
        validate_output(output, policy)?;
        if !state.service.output_exists(&auth, output.date).await?
            && output.generated_wh.is_none()
            && output.consumption_wh.is_none()
        {
            return Err(AddOutputApiError::bad(
                "Generated or consumption must be provided for a new output",
            ));
        }
    }
    state.service.upsert_outputs(&auth, outputs).await?;
    Ok(text_response(
        StatusCode::OK,
        LegacySuccess::AddedOutput.body(),
    ))
}

fn parse_outputs(parameters: &LegacyParameters) -> Result<Vec<DailyOutput>, AddOutputApiError> {
    if let Some(data) = parameters.get("data") {
        if [
            "d", "g", "e", "pp", "pt", "cd", "tm", "tx", "cm", "ip", "io", "is", "ih", "c", "ep",
            "eo", "es", "eh",
        ]
        .iter()
        .any(|name| parameters.get(name).is_some())
        {
            return Err(AddOutputApiError::bad(
                "The data parameter cannot be combined with individual fields",
            ));
        }
        return split_output_records(data)
            .into_iter()
            .map(|record| parse_csv_record(&record).map_err(AddOutputApiError::from))
            .map(|fields| fields.and_then(|fields| output_from_fields(&fields)))
            .collect();
    }
    output_from_parameters(parameters).map(|output| vec![output])
}

fn split_output_records(data: &str) -> Vec<String> {
    let mut records = Vec::new();
    let mut record = String::new();
    let mut quoted = false;
    let mut characters = data.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '"' {
            record.push(character);
            if quoted && characters.peek() == Some(&'"') {
                if let Some(escaped) = characters.next() {
                    record.push(escaped);
                }
            } else {
                quoted = !quoted;
            }
        } else if character == ';' && !quoted {
            records.push(std::mem::take(&mut record));
        } else {
            record.push(character);
        }
    }
    records.push(record);
    records
}

fn output_from_fields(fields: &[String]) -> Result<DailyOutput, AddOutputApiError> {
    if fields.is_empty() || fields.len() > 18 {
        return Err(AddOutputApiError::bad("Output CSV data is invalid"));
    }
    let get = |index: usize| {
        fields
            .get(index)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    };
    build_output(
        get(0).ok_or_else(|| AddOutputApiError::bad("Date is required"))?,
        get(1),
        get(2),
        get(3),
        get(4),
        get(5),
        get(6),
        get(7),
        get(8),
        get(9),
        get(10),
        get(11),
        get(12),
        get(13),
        get(14),
        get(15),
        get(16),
        get(17),
    )
}

fn output_from_parameters(parameters: &LegacyParameters) -> Result<DailyOutput, AddOutputApiError> {
    build_output(
        parameters
            .get("d")
            .ok_or_else(|| AddOutputApiError::bad("Date is required"))?,
        parameters.get("g"),
        parameters.get("e"),
        parameters.get("pp"),
        parameters.get("pt"),
        parameters.get("cd"),
        parameters.get("tm"),
        parameters.get("tx"),
        parameters.get("cm"),
        parameters.get("ip"),
        parameters.get("io"),
        parameters.get("is"),
        parameters.get("ih"),
        parameters.get("c"),
        parameters.get("ep"),
        parameters.get("eo"),
        parameters.get("es"),
        parameters.get("eh"),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_output(
    date_value: &str,
    generated: Option<&str>,
    exported: Option<&str>,
    peak_power: Option<&str>,
    peak_time: Option<&str>,
    condition: Option<&str>,
    minimum_temperature: Option<&str>,
    maximum_temperature: Option<&str>,
    comments: Option<&str>,
    import_peak: Option<&str>,
    import_off_peak: Option<&str>,
    import_shoulder: Option<&str>,
    import_high_shoulder: Option<&str>,
    consumption: Option<&str>,
    export_peak: Option<&str>,
    export_off_peak: Option<&str>,
    export_shoulder: Option<&str>,
    export_high_shoulder: Option<&str>,
) -> Result<DailyOutput, AddOutputApiError> {
    let export_periods = [
        parse_nonnegative(export_peak, "Export Peak")?,
        parse_nonnegative(export_off_peak, "Export Off-Peak")?,
        parse_nonnegative(export_shoulder, "Export Shoulder")?,
        parse_nonnegative(export_high_shoulder, "Export High Shoulder")?,
    ];
    let exported_wh = if export_periods.iter().any(Option::is_some) {
        Some(checked_sum(export_periods.into_iter().flatten())?)
    } else {
        parse_nonnegative(exported, "Exported")?
    };
    Ok(DailyOutput {
        date: parse_legacy_date(date_value)
            .map_err(|_| AddOutputApiError::bad(format!("Date {date_value} invalid")))?,
        generated_wh: parse_nonnegative(generated, "Generated")?,
        exported_wh,
        peak_power_watts: parse_nonnegative(peak_power, "Peak power")?,
        peak_time: peak_time
            .map(parse_legacy_time)
            .transpose()
            .map_err(|_| AddOutputApiError::bad("Peak time invalid"))?,
        condition: condition.map(ToOwned::to_owned),
        minimum_temperature_milli_celsius: minimum_temperature
            .map(parse_milli_celsius)
            .transpose()?,
        maximum_temperature_milli_celsius: maximum_temperature
            .map(parse_milli_celsius)
            .transpose()?,
        comments: comments.map(ToOwned::to_owned),
        import_peak_wh: parse_nonnegative(import_peak, "Import Peak")?,
        import_off_peak_wh: parse_nonnegative(import_off_peak, "Import Off Peak")?,
        import_shoulder_wh: parse_nonnegative(import_shoulder, "Import Shoulder")?,
        import_high_shoulder_wh: parse_nonnegative(import_high_shoulder, "Import High Shoulder")?,
        consumption_wh: parse_nonnegative(consumption, "Consumption")?,
        export_peak_wh: export_periods[0],
        export_off_peak_wh: export_periods[1],
        export_shoulder_wh: export_periods[2],
        export_high_shoulder_wh: export_periods[3],
    })
}

fn parse_nonnegative(value: Option<&str>, field: &str) -> Result<Option<i64>, AddOutputApiError> {
    value
        .map(|value| {
            value
                .parse::<i64>()
                .ok()
                .filter(|value| *value >= 0)
                .ok_or_else(|| AddOutputApiError::bad(format!("{field} invalid")))
        })
        .transpose()
}

fn parse_milli_celsius(value: &str) -> Result<i64, AddOutputApiError> {
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let mut parts = unsigned.split('.');
    let whole = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .ok_or_else(|| AddOutputApiError::bad("Temperature invalid"))?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() || fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(AddOutputApiError::bad("Temperature invalid"));
    }
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| AddOutputApiError::bad("Temperature invalid"))?
            * 10_i64.pow(u32::try_from(3 - fraction.len()).unwrap_or_default())
    };
    let scaled = whole
        .checked_mul(1_000)
        .and_then(|whole| whole.checked_add(fraction_value))
        .ok_or_else(|| AddOutputApiError::bad("Temperature invalid"))?;
    Ok(if negative { -scaled } else { scaled })
}

fn checked_sum(mut values: impl Iterator<Item = i64>) -> Result<i64, AddOutputApiError> {
    values.try_fold(0_i64, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| AddOutputApiError::bad("Output value too high"))
    })
}

fn validate_batch(
    outputs: &[DailyOutput],
    policy: AddOutputPolicy,
) -> Result<(), AddOutputApiError> {
    if outputs.is_empty() || outputs.len() > policy.maximum_batch_size.min(100) {
        return Err(AddOutputApiError::bad("Maximum batch size is 100"));
    }
    if outputs.len() > 1 && !policy.batching_enabled {
        return Err(AddOutputApiError::forbidden(
            "Batching is disabled by administrator policy",
        ));
    }
    Ok(())
}

fn validate_output(output: &DailyOutput, policy: AddOutputPolicy) -> Result<(), AddOutputApiError> {
    if output.date <= date!(2000 - 01 - 01) {
        return Err(AddOutputApiError::bad(format!(
            "Date {} too old",
            crate::format_legacy_date(output.date)
        )));
    }
    if output.date > policy.today {
        return Err(AddOutputApiError::bad(format!(
            "Date {} too new",
            crate::format_legacy_date(output.date)
        )));
    }
    if output
        .condition
        .as_deref()
        .is_some_and(|condition| !VALID_CONDITIONS.contains(&condition))
    {
        return Err(AddOutputApiError::bad("Condition invalid"));
    }
    match (
        output.minimum_temperature_milli_celsius,
        output.maximum_temperature_milli_celsius,
    ) {
        (Some(minimum), Some(maximum))
            if (-100_000..=100_000).contains(&minimum)
                && (-100_000..=100_000).contains(&maximum)
                && minimum <= maximum => {}
        (None, None) => {}
        _ => return Err(AddOutputApiError::bad("Min/Max temp missing or invalid")),
    }
    if output
        .consumption_wh
        .is_some_and(|value| value > MAX_DAILY_CONSUMPTION_WH)
    {
        return Err(AddOutputApiError::bad("Consumption too high"));
    }
    let capacity = i64::try_from(policy.effective_capacity_watts)
        .map_err(|_| AddOutputApiError::bad("System size invalid"))?;
    let maximum_daily = capacity
        .checked_mul(24)
        .ok_or_else(|| AddOutputApiError::bad("System size invalid"))?;
    if output
        .generated_wh
        .is_some_and(|value| value > maximum_daily)
    {
        return Err(AddOutputApiError::bad(
            "Generation too high for system size",
        ));
    }
    if output
        .exported_wh
        .is_some_and(|value| value > maximum_daily)
    {
        return Err(AddOutputApiError::bad("Export too high for system size"));
    }
    if output
        .generated_wh
        .zip(output.exported_wh)
        .is_some_and(|(generation, export)| i128::from(export) * 100 > i128::from(generation) * 115)
    {
        return Err(AddOutputApiError::bad(
            "Export cannot exceed generation by 15%",
        ));
    }
    if output
        .peak_power_watts
        .is_some_and(|peak| i128::from(peak) * 100 > i128::from(capacity) * 150)
    {
        return Err(AddOutputApiError::bad(
            "Peak power too high for system size",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AddOutputServiceError {
    #[error("legacy credentials are invalid")]
    Unauthorized,
    #[error("legacy credential is read-only")]
    Forbidden,
    #[error("daily output storage is unavailable")]
    Unavailable,
}

enum AddOutputApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(AddOutputServiceError),
}

impl AddOutputApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }

    fn forbidden(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::Forbidden,
            detail: detail.into(),
        })
    }
}

impl From<LegacyProtocolError> for AddOutputApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<AddOutputServiceError> for AddOutputApiError {
    fn from(value: AddOutputServiceError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for AddOutputApiError {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Legacy(error) => error,
            Self::Protocol(error) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: error.to_string(),
            },
            Self::Service(AddOutputServiceError::Unauthorized) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Invalid API Key".to_owned(),
            },
            Self::Service(AddOutputServiceError::Forbidden) => LegacyError {
                kind: LegacyErrorKind::Forbidden,
                detail: "Read only key".to_owned(),
            },
            Self::Service(AddOutputServiceError::Unavailable) => {
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
