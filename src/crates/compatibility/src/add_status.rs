//! `addstatus.jsp` compatibility adapter over canonical observation use cases.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, parse_legacy_auth, parse_legacy_bool, parse_legacy_date, parse_legacy_time,
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
use time::{Date, Duration, PrimitiveDateTime, Time};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyStatusEnergy {
    pub watt_hours: i64,
    pub cumulative: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyStatus {
    pub date: Date,
    pub time: Time,
    pub generation_energy: Option<LegacyStatusEnergy>,
    pub generation_power_watts: Option<i64>,
    pub consumption_energy: Option<LegacyStatusEnergy>,
    pub consumption_power_watts: Option<i64>,
    pub net_export_power_watts: Option<i64>,
    pub net_import_power_watts: Option<i64>,
    pub temperature_milli_celsius: Option<i64>,
    pub voltage_millivolts: Option<i64>,
    pub extended: BTreeMap<u8, String>,
    pub message: Option<String>,
    pub battery_power_watts: Option<i64>,
    pub battery_state_of_charge_basis_points: Option<u16>,
    pub battery_size_wh: Option<i64>,
    pub battery_lifetime_charge_wh: Option<i64>,
    pub battery_lifetime_discharge_wh: Option<i64>,
    pub battery_state: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddStatusPolicy {
    pub today: Date,
    pub effective_capacity_watts: u64,
    pub status_interval_minutes: u8,
    pub daylight_start: Time,
    pub daylight_end: Time,
    pub extended_enabled: bool,
    pub battery_enabled: bool,
}

#[async_trait]
pub trait AddStatusUseCases: Send + Sync {
    async fn policy(&self, auth: &LegacyAuth) -> Result<AddStatusPolicy, AddStatusServiceError>;
    async fn previous_status(
        &self,
        auth: &LegacyAuth,
        before: PrimitiveDateTime,
    ) -> Result<Option<LegacyStatus>, AddStatusServiceError>;
    async fn accept_status(
        &self,
        auth: &LegacyAuth,
        status: LegacyStatus,
    ) -> Result<(), AddStatusServiceError>;
}

#[derive(Clone)]
struct AddStatusState {
    service: Arc<dyn AddStatusUseCases>,
}

pub fn add_status_router(service: Arc<dyn AddStatusUseCases>) -> Router {
    Router::new()
        .route(
            "/service/r2/addstatus.jsp",
            get(add_status_get).post(add_status_post),
        )
        .with_state(AddStatusState { service })
}

async fn add_status_get(
    State(state): State<AddStatusState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, AddStatusApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    execute(state, LegacyMethod::Get, headers, parameters).await
}

async fn add_status_post(
    State(state): State<AddStatusState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, AddStatusApiError> {
    if query.is_some_and(|query| !query.is_empty()) {
        return Err(AddStatusApiError::bad("POST parameters must use form data"));
    }
    execute(
        state,
        LegacyMethod::Post,
        headers,
        LegacyParameters::parse(&body)?,
    )
    .await
}

async fn execute(
    state: AddStatusState,
    method: LegacyMethod,
    headers: HeaderMap,
    parameters: LegacyParameters,
) -> Result<Response, AddStatusApiError> {
    let auth = parse_legacy_auth(method, &headers, &parameters)?;
    let policy = state.service.policy(&auth).await?;
    let mut status = parse_status(&parameters, policy)?;
    let timestamp = PrimitiveDateTime::new(status.date, status.time);
    let previous = state.service.previous_status(&auth, timestamp).await?;
    derive_energy_and_power(&mut status, previous.as_ref())?;
    validate_status(&status, policy)?;
    state.service.accept_status(&auth, status).await?;
    Ok(text_response(
        StatusCode::OK,
        LegacySuccess::AddedStatus.body(),
    ))
}

#[allow(clippy::too_many_lines)]
fn parse_status(
    parameters: &LegacyParameters,
    policy: AddStatusPolicy,
) -> Result<LegacyStatus, AddStatusApiError> {
    let date_value = parameters
        .get("d")
        .ok_or_else(|| AddStatusApiError::bad("Missing date or time"))?;
    let date = parse_legacy_date(date_value)
        .map_err(|_| AddStatusApiError::bad(format!("Invalid Date {date_value}")))?;
    let time = parse_legacy_time(
        parameters
            .get("t")
            .ok_or_else(|| AddStatusApiError::bad("Missing date or time"))?,
    )
    .map_err(|_| AddStatusApiError::bad("Missing date or time"))?;
    let time = round_time(time, policy.status_interval_minutes)?;
    let cumulative = parse_cumulative(parameters.get("c1"))?;
    let net = parameters
        .get("n")
        .map(parse_legacy_bool)
        .transpose()?
        .unwrap_or(false);
    if net && cumulative != 0 {
        return Err(AddStatusApiError::bad("Invalid net and cumulative"));
    }
    let v1 = parse_nonnegative(parameters.get("v1"), "v1")?;
    let v2 = parse_integer(parameters.get("v2"), "v2")?;
    let v3 = parse_nonnegative(parameters.get("v3"), "v3")?;
    let v4 = parse_integer(parameters.get("v4"), "v4")?;
    if [v1, v2, v3, v4].iter().all(Option::is_none) {
        return Err(AddStatusApiError::bad("Missing energy and power values"));
    }
    let (generation_power_watts, consumption_power_watts, net_export, net_import) = if net {
        if v2.is_none() && v4.is_none() {
            return Err(AddStatusApiError::bad("Missing net power value"));
        }
        let (export, import) = net_flows(v2, v4)?;
        (None, None, Some(export), Some(import))
    } else {
        (
            nonnegative_power(v2, "v2")?,
            nonnegative_power(v4, "v4")?,
            None,
            None,
        )
    };
    let mut extended = BTreeMap::new();
    for index in 7_u8..=12 {
        if let Some(value) = parameters.get(&format!("v{index}")) {
            if !policy.extended_enabled {
                return Err(AddStatusApiError::forbidden(
                    "Extended values are disabled by administrator policy",
                ));
            }
            validate_decimal(value, "extended value")?;
            extended.insert(index, value.to_owned());
        }
    }
    let message = parameters.get("m1").map(ToOwned::to_owned);
    if message
        .as_ref()
        .is_some_and(|message| message.chars().count() > 30)
    {
        return Err(AddStatusApiError::bad("Text message exceeds 30 characters"));
    }
    let battery_power = parse_integer(parameters.get("b1"), "b1")?;
    if battery_power.is_some() && !policy.battery_enabled {
        return Err(AddStatusApiError::forbidden(
            "Battery data is disabled by administrator policy",
        ));
    }
    let battery_state_of_charge_basis_points = if battery_power.is_some() {
        parameters.get("b2").map(parse_percentage).transpose()?
    } else {
        None
    };
    let battery_state = if battery_power.is_some() {
        match parse_nonnegative(parameters.get("b6"), "b6")? {
            Some(state) => Some(
                u8::try_from(state)
                    .ok()
                    .filter(|state| *state <= 9)
                    .ok_or_else(|| AddStatusApiError::bad("Battery state invalid"))?,
            ),
            None => None,
        }
    } else {
        None
    };
    Ok(LegacyStatus {
        date,
        time,
        generation_energy: v1.map(|watt_hours| LegacyStatusEnergy {
            watt_hours,
            cumulative: matches!(cumulative, 1 | 2),
        }),
        generation_power_watts,
        consumption_energy: v3.map(|watt_hours| LegacyStatusEnergy {
            watt_hours,
            cumulative: matches!(cumulative, 1 | 3),
        }),
        consumption_power_watts,
        net_export_power_watts: net_export,
        net_import_power_watts: net_import,
        temperature_milli_celsius: parameters.get("v5").map(parse_milli).transpose()?,
        voltage_millivolts: parameters.get("v6").map(parse_milli).transpose()?,
        extended,
        message,
        battery_power_watts: battery_power,
        battery_state_of_charge_basis_points,
        battery_size_wh: battery_power
            .and(parameters.get("b3"))
            .map(|_| parse_nonnegative(parameters.get("b3"), "b3"))
            .transpose()?
            .flatten(),
        battery_lifetime_charge_wh: battery_power
            .and(parameters.get("b4"))
            .map(|_| parse_nonnegative(parameters.get("b4"), "b4"))
            .transpose()?
            .flatten(),
        battery_lifetime_discharge_wh: battery_power
            .and(parameters.get("b5"))
            .map(|_| parse_nonnegative(parameters.get("b5"), "b5"))
            .transpose()?
            .flatten(),
        battery_state,
    })
}

fn parse_cumulative(value: Option<&str>) -> Result<u8, AddStatusApiError> {
    value
        .unwrap_or("0")
        .parse::<u8>()
        .ok()
        .filter(|value| *value <= 3)
        .ok_or_else(|| AddStatusApiError::bad("Cumulative flag invalid"))
}

fn round_time(time: Time, interval_minutes: u8) -> Result<Time, AddStatusApiError> {
    if !(5..=15).contains(&interval_minutes) {
        return Err(AddStatusApiError::bad("Status interval invalid"));
    }
    let minutes = u16::from(time.hour()) * 60 + u16::from(time.minute());
    let interval = u16::from(interval_minutes);
    let rounded = ((minutes + interval / 2) / interval) * interval;
    let bounded = rounded.min(23 * 60 + 59);
    Time::from_hms(
        u8::try_from(bounded / 60).unwrap_or_default(),
        u8::try_from(bounded % 60).unwrap_or_default(),
        0,
    )
    .map_err(|_| AddStatusApiError::bad("Status time invalid"))
}

fn derive_energy_and_power(
    status: &mut LegacyStatus,
    previous: Option<&LegacyStatus>,
) -> Result<(), AddStatusApiError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let elapsed = PrimitiveDateTime::new(status.date, status.time)
        - PrimitiveDateTime::new(previous.date, previous.time);
    let elapsed_seconds = elapsed.whole_seconds();
    if elapsed_seconds <= 0 {
        return Err(AddStatusApiError::bad(
            "Status time is not after previous status",
        ));
    }
    derive_pair(
        &mut status.generation_energy,
        &mut status.generation_power_watts,
        previous.generation_energy,
        elapsed_seconds,
    )?;
    derive_pair(
        &mut status.consumption_energy,
        &mut status.consumption_power_watts,
        previous.consumption_energy,
        elapsed_seconds,
    )
}

fn derive_pair(
    energy: &mut Option<LegacyStatusEnergy>,
    power: &mut Option<i64>,
    previous_energy: Option<LegacyStatusEnergy>,
    elapsed_seconds: i64,
) -> Result<(), AddStatusApiError> {
    match (*energy, *power, previous_energy) {
        (Some(current), None, Some(previous)) if current.cumulative == previous.cumulative => {
            let delta = current
                .watt_hours
                .checked_sub(previous.watt_hours)
                .filter(|delta| *delta >= 0)
                .ok_or_else(|| AddStatusApiError::bad("Energy value is lower than previous"))?;
            *power = Some(
                delta
                    .checked_mul(3_600)
                    .and_then(|value| value.checked_div(elapsed_seconds))
                    .ok_or_else(|| AddStatusApiError::bad("Energy calculation failed"))?,
            );
        }
        (None, Some(current_power), Some(previous)) if !previous.cumulative => {
            let increment = current_power
                .checked_mul(elapsed_seconds)
                .and_then(|value| value.checked_div(3_600))
                .ok_or_else(|| AddStatusApiError::bad("Power calculation failed"))?;
            *energy = Some(LegacyStatusEnergy {
                watt_hours: previous
                    .watt_hours
                    .checked_add(increment)
                    .ok_or_else(|| AddStatusApiError::bad("Power calculation failed"))?,
                cumulative: false,
            });
        }
        _ => {}
    }
    Ok(())
}

fn net_flows(v2: Option<i64>, v4: Option<i64>) -> Result<(i64, i64), AddStatusApiError> {
    let (export, import) = match (v2, v4) {
        (None, Some(value)) if value >= 0 => (0, value),
        (None, Some(value)) => (value.saturating_abs(), 0),
        (Some(value), None) if value >= 0 => (value, 0),
        (Some(value), None) => (0, value.saturating_abs()),
        (Some(left), Some(right)) if left >= 0 && right < 0 => {
            (left.saturating_add(right.saturating_abs()), 0)
        }
        (Some(left), Some(right)) if left < 0 && right >= 0 => {
            (0, right.saturating_add(left.saturating_abs()))
        }
        (Some(left), Some(right)) if left >= 0 && right >= 0 => (left, right),
        (Some(left), Some(right)) => (right.saturating_abs(), left.saturating_abs()),
        (None, None) => return Err(AddStatusApiError::bad("Missing net power value")),
    };
    Ok((export, import))
}

fn validate_status(
    status: &LegacyStatus,
    policy: AddStatusPolicy,
) -> Result<(), AddStatusApiError> {
    let age = policy.today - status.date;
    if status.date > policy.today {
        return Err(AddStatusApiError::bad("Invalid future date"));
    }
    if age > Duration::days(14) {
        return Err(AddStatusApiError::bad("Date is older than 14 days"));
    }
    let capacity = i64::try_from(policy.effective_capacity_watts)
        .map_err(|_| AddStatusApiError::bad("System size invalid"))?;
    let maximum_power = capacity
        .checked_mul(3)
        .and_then(|value| value.checked_div(2))
        .ok_or_else(|| AddStatusApiError::bad("System size invalid"))?;
    if status
        .generation_power_watts
        .is_some_and(|power| power > maximum_power)
    {
        return Err(AddStatusApiError::bad(
            "Power value too high for system size",
        ));
    }
    if status
        .consumption_power_watts
        .is_some_and(|power| power > 100_000)
    {
        return Err(AddStatusApiError::bad("Power consumption too high"));
    }
    if status
        .consumption_energy
        .is_some_and(|energy| energy.watt_hours > 200_000)
    {
        return Err(AddStatusApiError::bad("Energy consumption too high"));
    }
    if status
        .generation_energy
        .is_some_and(|energy| energy.watt_hours > capacity.saturating_mul(12))
    {
        return Err(AddStatusApiError::bad(
            "Energy value too high for system size",
        ));
    }
    if status.generation_power_watts.is_some_and(|power| power > 0)
        && (status.time < policy.daylight_start || status.time > policy.daylight_end)
    {
        return Err(AddStatusApiError::bad("Moon Powered"));
    }
    if (6..8).contains(&status.time.hour())
        && let Some(energy) = status.generation_energy
    {
        let elapsed_minutes =
            i64::from(status.time.hour() - 6) * 60 + i64::from(status.time.minute());
        let threshold_wh_per_kw = 1_000 + elapsed_minutes * 3_000 / 120;
        let threshold = capacity
            .checked_mul(threshold_wh_per_kw)
            .and_then(|value| value.checked_div(1_000))
            .ok_or_else(|| AddStatusApiError::bad("Energy threshold invalid"))?;
        if energy.watt_hours > threshold {
            return Err(AddStatusApiError::bad("Energy value too high for time"));
        }
    }
    Ok(())
}

fn parse_integer(value: Option<&str>, field: &str) -> Result<Option<i64>, AddStatusApiError> {
    value
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| AddStatusApiError::bad(format!("{field} invalid")))
        })
        .transpose()
}

fn parse_nonnegative(value: Option<&str>, field: &str) -> Result<Option<i64>, AddStatusApiError> {
    parse_integer(value, field).and_then(|value| {
        if value.is_some_and(|value| value < 0) {
            Err(AddStatusApiError::bad(format!("{field} invalid")))
        } else {
            Ok(value)
        }
    })
}

fn nonnegative_power(value: Option<i64>, field: &str) -> Result<Option<i64>, AddStatusApiError> {
    if value.is_some_and(|value| value < 0) {
        Err(AddStatusApiError::bad(format!("{field} invalid")))
    } else {
        Ok(value)
    }
}

fn validate_decimal(value: &str, field: &str) -> Result<(), AddStatusApiError> {
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction
            .is_some_and(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        || parts.next().is_some()
    {
        Err(AddStatusApiError::bad(format!("{field} invalid")))
    } else {
        Ok(())
    }
}

fn parse_milli(value: &str) -> Result<i64, AddStatusApiError> {
    validate_decimal(value, "decimal")?;
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let mut parts = unsigned.split('.');
    let whole = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .ok_or_else(|| AddStatusApiError::bad("Decimal invalid"))?;
    let fraction = parts.next().unwrap_or("");
    if fraction.len() > 3 {
        return Err(AddStatusApiError::bad("Decimal invalid"));
    }
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| AddStatusApiError::bad("Decimal invalid"))?
            * 10_i64.pow(u32::try_from(3 - fraction.len()).unwrap_or_default())
    };
    let scaled = whole
        .checked_mul(1_000)
        .and_then(|whole| whole.checked_add(fraction_value))
        .ok_or_else(|| AddStatusApiError::bad("Decimal invalid"))?;
    Ok(if negative { -scaled } else { scaled })
}

fn parse_percentage(value: &str) -> Result<u16, AddStatusApiError> {
    let milli = parse_milli(value)?;
    if !(0..=100_000).contains(&milli) {
        return Err(AddStatusApiError::bad("Battery state of charge invalid"));
    }
    u16::try_from(milli / 10).map_err(|_| AddStatusApiError::bad("Battery state of charge invalid"))
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AddStatusServiceError {
    #[error("legacy credentials are invalid")]
    Unauthorized,
    #[error("legacy credential is read-only")]
    Forbidden,
    #[error("status storage is unavailable")]
    Unavailable,
}

enum AddStatusApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(AddStatusServiceError),
}

impl AddStatusApiError {
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

impl From<LegacyProtocolError> for AddStatusApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<AddStatusServiceError> for AddStatusApiError {
    fn from(value: AddStatusServiceError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for AddStatusApiError {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Legacy(error) => error,
            Self::Protocol(error) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: error.to_string(),
            },
            Self::Service(AddStatusServiceError::Unauthorized) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Invalid API Key".to_owned(),
            },
            Self::Service(AddStatusServiceError::Forbidden) => LegacyError {
                kind: LegacyErrorKind::Forbidden,
                detail: "Read only key".to_owned(),
            },
            Self::Service(AddStatusServiceError::Unavailable) => {
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
