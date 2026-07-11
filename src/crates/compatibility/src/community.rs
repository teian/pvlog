//! Legacy system, search, favourite, and ladder community adapters.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    csv_record, format_legacy_date, parse_legacy_auth, parse_legacy_bool,
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
use time::Date;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct LegacySystemOptions {
    pub include_array_two: bool,
    pub include_array_three: bool,
    pub include_timezone: bool,
    pub include_tariffs: bool,
    pub include_teams: bool,
    pub include_estimates: bool,
    pub include_donations: bool,
    pub include_extended: bool,
    pub target_system_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyArrayDetails {
    pub panels: Option<i64>,
    pub panel_power_watts: Option<i64>,
    pub orientation: Option<String>,
    pub tilt_milli_degrees: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySystemDetails {
    pub system_id: u64,
    pub name: String,
    pub size_watts: Option<i64>,
    pub postcode: String,
    pub panels: Option<i64>,
    pub panel_power_watts: Option<i64>,
    pub panel_brand: String,
    pub inverters: Option<i64>,
    pub inverter_power_watts: Option<i64>,
    pub inverter_brand: String,
    pub orientation: String,
    pub tilt_milli_degrees: Option<i64>,
    pub shade: String,
    pub install_date: Option<Date>,
    pub latitude_microdegrees: Option<i64>,
    pub longitude_microdegrees: Option<i64>,
    pub status_interval_minutes: Option<i64>,
    pub array_two: Option<LegacyArrayDetails>,
    pub array_three: Option<LegacyArrayDetails>,
    pub timezone: Option<String>,
    pub export_tariff_milli_cents: Option<i64>,
    pub import_peak_tariff_milli_cents: Option<i64>,
    pub import_off_peak_tariff_milli_cents: Option<i64>,
    pub import_shoulder_tariff_milli_cents: Option<i64>,
    pub import_high_shoulder_tariff_milli_cents: Option<i64>,
    pub import_daily_charge_milli_cents: Option<i64>,
    pub team_ids: Vec<u64>,
    pub donations: u64,
    pub extended_config_fields: Vec<String>,
    pub monthly_estimates_kwh: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtendedConfigUpdate {
    pub label: String,
    pub unit: String,
    pub colour: Option<String>,
    pub axis: Option<u8>,
    pub graph: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySystemUpdate {
    pub name: Option<String>,
    pub extended: BTreeMap<u8, ExtendedConfigUpdate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySearchQuery {
    pub query: String,
    pub origin_microdegrees: Option<(i64, i64)>,
    pub country_only: bool,
    pub country_code: Option<String>,
    pub seen_days: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySearchSystem {
    pub name: String,
    pub size_watts: Option<i64>,
    pub postcode: String,
    pub orientation: String,
    pub outputs: u64,
    pub last_output: String,
    pub system_id: u64,
    pub panel: String,
    pub inverter: String,
    pub distance_kilometres: Option<i64>,
    pub latitude_microdegrees: Option<i64>,
    pub longitude_microdegrees: Option<i64>,
}

pub type LegacyFavouriteSystem = LegacySystemDetails;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyLadderSummary {
    pub ranking_date: Date,
    pub generation_rank: Option<i64>,
    pub efficiency_rank: Option<i64>,
    pub average_efficiency_milli_kwh_per_kw: Option<i64>,
    pub total_outputs: u64,
    pub last_output: Option<Date>,
    pub total_generation_wh: Option<i64>,
    pub total_consumption_wh: Option<i64>,
    pub average_generation_wh: Option<i64>,
    pub average_consumption_wh: Option<i64>,
    pub maximum_generation_wh: Option<i64>,
    pub maximum_consumption_wh: Option<i64>,
    pub system_age_days: u64,
}

#[async_trait]
pub trait LegacyCommunityUseCases: Send + Sync {
    async fn system(
        &self,
        auth: &LegacyAuth,
        options: &LegacySystemOptions,
    ) -> Result<LegacySystemDetails, LegacyCommunityError>;
    async fn update_system(
        &self,
        auth: &LegacyAuth,
        update: LegacySystemUpdate,
    ) -> Result<(), LegacyCommunityError>;
    async fn search(
        &self,
        auth: &LegacyAuth,
        query: &LegacySearchQuery,
    ) -> Result<Vec<LegacySearchSystem>, LegacyCommunityError>;
    async fn favourites(
        &self,
        auth: &LegacyAuth,
        target_system_id: Option<u64>,
    ) -> Result<Vec<LegacyFavouriteSystem>, LegacyCommunityError>;
    async fn ladder(
        &self,
        auth: &LegacyAuth,
        target_system_id: Option<u64>,
    ) -> Result<LegacyLadderSummary, LegacyCommunityError>;
}

#[derive(Clone)]
struct CommunityState {
    service: Arc<dyn LegacyCommunityUseCases>,
}

pub fn legacy_community_router(service: Arc<dyn LegacyCommunityUseCases>) -> Router {
    Router::new()
        .route("/service/r2/getsystem.jsp", get(get_system))
        .route(
            "/service/r2/postsystem.jsp",
            axum::routing::post(post_system),
        )
        .route("/service/r2/search.jsp", get(search_get).post(search_post))
        .route("/service/r2/getfavourite.jsp", get(get_favourite))
        .route("/service/r2/getladder.jsp", get(get_ladder))
        .with_state(CommunityState { service })
}

async fn get_system(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, CommunityApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let options = parse_system_options(&parameters)?;
    let system = state.service.system(&auth, &options).await?;
    Ok(text_response(
        StatusCode::OK,
        &format_system(&system, &options),
    ))
}

async fn post_system(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CommunityApiError> {
    let parameters = LegacyParameters::parse(&body)?;
    let auth = parse_legacy_auth(LegacyMethod::Post, &headers, &parameters)?;
    state
        .service
        .update_system(&auth, parse_system_update(&parameters)?)
        .await?;
    Ok(text_response(StatusCode::OK, "Updated System"))
}

async fn search_get(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, CommunityApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    search(state, LegacyMethod::Get, headers, parameters).await
}

async fn search_post(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CommunityApiError> {
    search(
        state,
        LegacyMethod::Post,
        headers,
        LegacyParameters::parse(&body)?,
    )
    .await
}

async fn search(
    state: CommunityState,
    method: LegacyMethod,
    headers: HeaderMap,
    parameters: LegacyParameters,
) -> Result<Response, CommunityApiError> {
    let auth = parse_legacy_auth(method, &headers, &parameters)?;
    let query = parse_search(&parameters)?;
    let mut systems = state.service.search(&auth, &query).await?;
    systems.truncate(30);
    Ok(text_response(
        StatusCode::OK,
        &systems
            .iter()
            .map(format_search)
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

async fn get_favourite(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, CommunityApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let mut systems = state
        .service
        .favourites(&auth, parse_id(parameters.get("sid1"), "sid1")?)
        .await?;
    systems.truncate(50);
    Ok(text_response(
        StatusCode::OK,
        &systems
            .iter()
            .map(format_favourite)
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

async fn get_ladder(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, CommunityApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let ladder = state
        .service
        .ladder(&auth, parse_id(parameters.get("sid1"), "sid1")?)
        .await?;
    Ok(text_response(StatusCode::OK, &format_ladder(&ladder)))
}

fn parse_system_options(
    parameters: &LegacyParameters,
) -> Result<LegacySystemOptions, CommunityApiError> {
    Ok(LegacySystemOptions {
        include_array_two: flag(parameters.get("array2"))?,
        include_array_three: flag(parameters.get("array3"))?,
        include_timezone: flag(parameters.get("tz"))?,
        include_tariffs: flag(parameters.get("tariffs"))?,
        include_teams: flag(parameters.get("teams"))?,
        include_estimates: flag(parameters.get("est"))?,
        include_donations: flag(parameters.get("donations"))?,
        include_extended: flag(parameters.get("ext"))?,
        target_system_id: parse_id(parameters.get("sid1"), "sid1")?,
    })
}

fn parse_system_update(
    parameters: &LegacyParameters,
) -> Result<LegacySystemUpdate, CommunityApiError> {
    let name = parameters.get("name").map(ToOwned::to_owned);
    if name.as_ref().is_some_and(|name| name.chars().count() > 30) {
        return Err(CommunityApiError::bad("System Name exceeds 30 characters"));
    }
    let mut extended = BTreeMap::new();
    for index in 7_u8..=12 {
        let label = parameters.get(&format!("v{index}l"));
        let unit = parameters.get(&format!("v{index}u"));
        if label.is_none() && unit.is_none() {
            continue;
        }
        let (Some(label), Some(unit)) = (label, unit) else {
            return Err(CommunityApiError::bad(
                "Both extended label and unit are required",
            ));
        };
        if label.is_empty() || unit.is_empty() {
            continue;
        }
        if label.chars().count() > 20 || unit.chars().count() > 10 {
            return Err(CommunityApiError::bad(
                "Extended configuration exceeds limit",
            ));
        }
        let colour = parameters.get(&format!("v{index}c")).map(ToOwned::to_owned);
        if colour.as_ref().is_some_and(|value| {
            value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(CommunityApiError::bad("Extended colour invalid"));
        }
        let axis = parameters
            .get(&format!("v{index}a"))
            .map(str::parse::<u8>)
            .transpose()
            .map_err(|_| CommunityApiError::bad("Extended axis invalid"))?;
        if axis.is_some_and(|axis| axis > 5) {
            return Err(CommunityApiError::bad("Extended axis invalid"));
        }
        let graph = parameters.get(&format!("v{index}g")).map(ToOwned::to_owned);
        if graph
            .as_deref()
            .is_some_and(|graph| !matches!(graph, "area" | "line"))
        {
            return Err(CommunityApiError::bad("Extended graph invalid"));
        }
        extended.insert(
            index,
            ExtendedConfigUpdate {
                label: label.to_owned(),
                unit: unit.to_owned(),
                colour,
                axis,
                graph,
            },
        );
    }
    Ok(LegacySystemUpdate { name, extended })
}

fn parse_search(parameters: &LegacyParameters) -> Result<LegacySearchQuery, CommunityApiError> {
    let query = parameters
        .get("q")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CommunityApiError::bad("Query is required"))?
        .to_owned();
    let origin_microdegrees = parameters.get("ll").map(parse_coordinates).transpose()?;
    let country_code = parameters.get("country_code").map(str::to_uppercase);
    if country_code
        .as_ref()
        .is_some_and(|country| country.len() != 2)
    {
        return Err(CommunityApiError::bad("Country code invalid"));
    }
    Ok(LegacySearchQuery {
        query,
        origin_microdegrees,
        country_only: flag(parameters.get("country"))?,
        country_code,
        seen_days: parameters
            .get("seen")
            .map(str::parse::<u16>)
            .transpose()
            .map_err(|_| CommunityApiError::bad("Last Seen invalid"))?,
    })
}

fn parse_coordinates(value: &str) -> Result<(i64, i64), CommunityApiError> {
    let (latitude, longitude) = value
        .split_once(',')
        .ok_or_else(|| CommunityApiError::bad("Latitude/Longitude invalid"))?;
    Ok((decimal_to_micro(latitude)?, decimal_to_micro(longitude)?))
}

fn decimal_to_micro(value: &str) -> Result<i64, CommunityApiError> {
    let parsed = value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| CommunityApiError::bad("Coordinate invalid"))?;
    #[allow(clippy::cast_possible_truncation)]
    Ok((parsed * 1_000_000.0).round() as i64)
}

fn format_system(system: &LegacySystemDetails, options: &LegacySystemOptions) -> String {
    let mut base = base_system_fields(system);
    if options.include_array_two {
        append_array(&mut base, system.array_two.as_ref());
    }
    if options.include_array_three {
        append_array(&mut base, system.array_three.as_ref());
    }
    if options.include_timezone {
        base.push(system.timezone.clone().unwrap_or_default());
    }
    let mut sections = vec![csv_record(base.iter().map(|field| Some(field.as_str())))];
    if options.include_tariffs {
        let tariffs = [
            system.export_tariff_milli_cents,
            system.import_peak_tariff_milli_cents,
            system.import_off_peak_tariff_milli_cents,
            system.import_shoulder_tariff_milli_cents,
            system.import_high_shoulder_tariff_milli_cents,
            system.import_daily_charge_milli_cents,
        ]
        .into_iter()
        .map(decimal)
        .collect::<Vec<_>>();
        sections.push(csv_record(tariffs.iter().map(|value| Some(value.as_str()))));
    }
    if options.include_teams {
        sections.push(
            system
                .team_ids
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if options.include_donations {
        sections.push(system.donations.to_string());
    }
    if options.include_extended {
        sections.push(csv_record(
            system
                .extended_config_fields
                .iter()
                .map(|field| Some(field.as_str())),
        ));
    }
    if options.include_estimates {
        sections.push(csv_record(
            system
                .monthly_estimates_kwh
                .iter()
                .map(|field| Some(field.as_str())),
        ));
    }
    sections.join(";")
}

fn base_system_fields(system: &LegacySystemDetails) -> Vec<String> {
    vec![
        system.name.clone(),
        number(system.size_watts),
        system.postcode.clone(),
        number(system.panels),
        number(system.panel_power_watts),
        system.panel_brand.clone(),
        number(system.inverters),
        number(system.inverter_power_watts),
        system.inverter_brand.clone(),
        system.orientation.clone(),
        decimal(system.tilt_milli_degrees),
        system.shade.clone(),
        system
            .install_date
            .map_or_else(String::new, format_legacy_date),
        microdegrees(system.latitude_microdegrees),
        microdegrees(system.longitude_microdegrees),
        number(system.status_interval_minutes),
    ]
}

fn append_array(fields: &mut Vec<String>, array: Option<&LegacyArrayDetails>) {
    fields.extend([
        number(array.and_then(|array| array.panels)),
        number(array.and_then(|array| array.panel_power_watts)),
        array
            .and_then(|array| array.orientation.clone())
            .unwrap_or_default(),
        decimal(array.and_then(|array| array.tilt_milli_degrees)),
    ]);
}

fn format_search(system: &LegacySearchSystem) -> String {
    csv_record(
        [
            system.name.clone(),
            number(system.size_watts),
            system.postcode.clone(),
            system.orientation.clone(),
            system.outputs.to_string(),
            system.last_output.clone(),
            system.system_id.to_string(),
            system.panel.clone(),
            system.inverter.clone(),
            number(system.distance_kilometres),
            microdegrees(system.latitude_microdegrees),
            microdegrees(system.longitude_microdegrees),
        ]
        .iter()
        .map(|field| Some(field.as_str())),
    )
}

fn format_favourite(system: &LegacyFavouriteSystem) -> String {
    let mut fields = vec![system.system_id.to_string()];
    fields.extend(base_system_fields(system));
    csv_record(fields.iter().map(|field| Some(field.as_str())))
}

fn format_ladder(ladder: &LegacyLadderSummary) -> String {
    csv_record(
        [
            format_legacy_date(ladder.ranking_date),
            number(ladder.generation_rank),
            number(ladder.efficiency_rank),
            decimal(ladder.average_efficiency_milli_kwh_per_kw),
            ladder.total_outputs.to_string(),
            ladder
                .last_output
                .map_or_else(String::new, format_legacy_date),
            number(ladder.total_generation_wh),
            number(ladder.total_consumption_wh),
            number(ladder.average_generation_wh),
            number(ladder.average_consumption_wh),
            number(ladder.maximum_generation_wh),
            number(ladder.maximum_consumption_wh),
            ladder.system_age_days.to_string(),
        ]
        .iter()
        .map(|field| Some(field.as_str())),
    )
}

fn flag(value: Option<&str>) -> Result<bool, CommunityApiError> {
    value
        .map(parse_legacy_bool)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(CommunityApiError::from)
}

fn parse_id(value: Option<&str>, field: &str) -> Result<Option<u64>, CommunityApiError> {
    value
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| CommunityApiError::bad(format!("{field} invalid")))
        })
        .transpose()
}

fn number(value: Option<i64>) -> String {
    value.map_or_else(|| "NaN".to_owned(), |value| value.to_string())
}

fn decimal(value: Option<i64>) -> String {
    scaled(value, 1_000)
}

fn scaled(value: Option<i64>, scale: u64) -> String {
    value.map_or_else(
        || "NaN".to_owned(),
        |value| {
            let negative = value < 0;
            let absolute = value.unsigned_abs();
            let whole = absolute / scale;
            let fraction = absolute % scale;
            let width = usize::try_from(scale.ilog10()).unwrap_or_default();
            let mut result = if fraction == 0 {
                whole.to_string()
            } else {
                format!("{whole}.{fraction:0width$}")
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

fn microdegrees(value: Option<i64>) -> String {
    scaled(value, 1_000_000)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LegacyCommunityError {
    #[error("legacy community credentials are invalid")]
    Unauthorized,
    #[error("legacy community resource is inaccessible")]
    Inaccessible,
    #[error("legacy community resource was not found")]
    NotFound,
    #[error("legacy community storage is unavailable")]
    Unavailable,
}

enum CommunityApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(LegacyCommunityError),
}

impl CommunityApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}

impl From<LegacyProtocolError> for CommunityApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LegacyCommunityError> for CommunityApiError {
    fn from(value: LegacyCommunityError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for CommunityApiError {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Legacy(error) => error,
            Self::Protocol(error) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: error.to_string(),
            },
            Self::Service(LegacyCommunityError::Unauthorized) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Invalid API Key".to_owned(),
            },
            Self::Service(LegacyCommunityError::Inaccessible) => LegacyError {
                kind: LegacyErrorKind::Unauthorized,
                detail: "Inaccessible System ID".to_owned(),
            },
            Self::Service(LegacyCommunityError::NotFound) => LegacyError {
                kind: LegacyErrorKind::BadRequest,
                detail: "System not found".to_owned(),
            },
            Self::Service(LegacyCommunityError::Unavailable) => {
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
