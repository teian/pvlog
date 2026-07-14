//! Resolution-aware planning for telemetry reads.

use chrono_tz::Tz;
use serde::Serialize;
use std::collections::BTreeSet;
use thiserror::Error;

const MILLIS_PER_MINUTE: u64 = 60_000;
const MILLIS_PER_HOUR: u64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: u64 = 24 * MILLIS_PER_HOUR;

/// A queryable canonical telemetry field.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesField {
    GenerationPower,
    GenerationEnergy,
    ForecastPower,
    ForecastEnergy,
    ExpectedEnergy,
    GenerationPerformance,
    ForecastRealization,
    ConsumptionPower,
    ConsumptionEnergy,
    GridPower,
    BatteryPower,
    BatteryStateOfCharge,
    Temperature,
    Extended,
    Provenance,
}

impl SeriesField {
    const fn supports_rollups(self) -> bool {
        !matches!(self, Self::Provenance)
    }
}

/// Resolution requested by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestedResolution {
    Auto,
    Raw,
    FifteenMinutes,
    Hourly,
    Daily,
    Monthly,
    Yearly,
}

/// Physical representation selected by the planner.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryResolution {
    Raw,
    FifteenMinutes,
    Hourly,
    Daily,
    Monthly,
    Yearly,
}

impl QueryResolution {
    const fn approximate_bucket_millis(self) -> Option<u64> {
        match self {
            Self::Raw => None,
            Self::FifteenMinutes => Some(15 * MILLIS_PER_MINUTE),
            Self::Hourly => Some(MILLIS_PER_HOUR),
            Self::Daily => Some(MILLIS_PER_DAY),
            Self::Monthly => Some(30 * MILLIS_PER_DAY),
            Self::Yearly => Some(365 * MILLIS_PER_DAY),
        }
    }
}

/// Raw stores required for a range. Corrections are merged by the raw reader for every variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawSources {
    Hot,
    ArchivedSegments,
    HotAndArchivedSegments,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuerySource {
    Raw(RawSources),
    Rollup(QueryResolution),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPlan {
    pub source: QuerySource,
    pub actual_resolution: QueryResolution,
    pub estimated_points: u64,
    pub timezone: Tz,
    pub fields: BTreeSet<SeriesField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPlanRequest {
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub requested_resolution: RequestedResolution,
    pub fields: BTreeSet<SeriesField>,
    pub timezone: String,
    pub maximum_points: u32,
    /// Expected source cadence used to bound exact raw reads.
    pub expected_raw_interval_millis: u64,
    /// Observations before this instant have been compacted to segments.
    pub hot_data_start_epoch_millis: i64,
    pub available_rollups: BTreeSet<QueryResolution>,
}

/// Produces a deterministic physical read plan without accessing storage.
///
/// Automatic planning chooses the finest rollup that satisfies the point budget. A field that
/// cannot be represented by rollups forces a bounded raw query. Explicit resolutions are never
/// silently substituted.
/// # Errors
/// Returns an error for invalid bounds, timezone, fields, unavailable explicit resolutions, or a
/// plan that exceeds the caller's point budget.
pub fn plan_query(request: QueryPlanRequest) -> Result<QueryPlan, QueryPlanError> {
    validate_request(&request)?;
    let timezone = request
        .timezone
        .parse::<Tz>()
        .map_err(|_| QueryPlanError::InvalidTimezone)?;
    let duration = u64::try_from(request.end_epoch_millis - request.start_epoch_millis)
        .map_err(|_| QueryPlanError::InvalidRange)?;
    let raw_points = points_for_interval(duration, request.expected_raw_interval_millis)?;
    let rollups_supported = request.fields.iter().all(|field| field.supports_rollups());

    let resolution = match request.requested_resolution {
        RequestedResolution::Raw => bounded_raw(raw_points, request.maximum_points)?,
        RequestedResolution::Auto if !rollups_supported => {
            bounded_raw(raw_points, request.maximum_points)?
        }
        RequestedResolution::Auto => select_automatic_resolution(&request, duration, raw_points)?,
        explicit => {
            let resolution = explicit_resolution(explicit);
            if !rollups_supported {
                return Err(QueryPlanError::FieldRequiresRaw);
            }
            if !request.available_rollups.contains(&resolution) {
                return Err(QueryPlanError::ResolutionUnavailable);
            }
            let points = points_for_resolution(duration, resolution)?;
            ensure_budget(points, request.maximum_points)?;
            resolution
        }
    };

    let (source, estimated_points) = if resolution == QueryResolution::Raw {
        (QuerySource::Raw(raw_sources(&request)), raw_points)
    } else {
        (
            QuerySource::Rollup(resolution),
            points_for_resolution(duration, resolution)?,
        )
    };
    Ok(QueryPlan {
        source,
        actual_resolution: resolution,
        estimated_points,
        timezone,
        fields: request.fields,
    })
}

fn validate_request(request: &QueryPlanRequest) -> Result<(), QueryPlanError> {
    if request.end_epoch_millis <= request.start_epoch_millis {
        return Err(QueryPlanError::InvalidRange);
    }
    if request.fields.is_empty() {
        return Err(QueryPlanError::NoFields);
    }
    if request.maximum_points == 0 {
        return Err(QueryPlanError::InvalidPointBudget);
    }
    if request.expected_raw_interval_millis == 0 {
        return Err(QueryPlanError::InvalidRawInterval);
    }
    if request.available_rollups.contains(&QueryResolution::Raw) {
        return Err(QueryPlanError::InvalidAvailableResolution);
    }
    Ok(())
}

fn select_automatic_resolution(
    request: &QueryPlanRequest,
    duration: u64,
    raw_points: u64,
) -> Result<QueryResolution, QueryPlanError> {
    if raw_points <= u64::from(request.maximum_points) {
        return Ok(QueryResolution::Raw);
    }
    for resolution in [
        QueryResolution::FifteenMinutes,
        QueryResolution::Hourly,
        QueryResolution::Daily,
        QueryResolution::Monthly,
        QueryResolution::Yearly,
    ] {
        if request.available_rollups.contains(&resolution)
            && points_for_resolution(duration, resolution)? <= u64::from(request.maximum_points)
        {
            return Ok(resolution);
        }
    }
    Err(QueryPlanError::PointBudgetExceeded)
}

fn bounded_raw(raw_points: u64, maximum_points: u32) -> Result<QueryResolution, QueryPlanError> {
    ensure_budget(raw_points, maximum_points)?;
    Ok(QueryResolution::Raw)
}

fn ensure_budget(points: u64, maximum_points: u32) -> Result<(), QueryPlanError> {
    if points > u64::from(maximum_points) {
        Err(QueryPlanError::PointBudgetExceeded)
    } else {
        Ok(())
    }
}

fn explicit_resolution(requested: RequestedResolution) -> QueryResolution {
    match requested {
        RequestedResolution::FifteenMinutes => QueryResolution::FifteenMinutes,
        RequestedResolution::Hourly => QueryResolution::Hourly,
        RequestedResolution::Daily => QueryResolution::Daily,
        RequestedResolution::Monthly => QueryResolution::Monthly,
        RequestedResolution::Yearly => QueryResolution::Yearly,
        RequestedResolution::Auto | RequestedResolution::Raw => QueryResolution::Raw,
    }
}

fn points_for_resolution(
    duration_millis: u64,
    resolution: QueryResolution,
) -> Result<u64, QueryPlanError> {
    let interval = resolution
        .approximate_bucket_millis()
        .ok_or(QueryPlanError::InvalidAvailableResolution)?;
    points_for_interval(duration_millis, interval)
}

fn points_for_interval(duration_millis: u64, interval_millis: u64) -> Result<u64, QueryPlanError> {
    duration_millis
        .checked_add(interval_millis - 1)
        .ok_or(QueryPlanError::PointEstimateOverflow)
        .map(|value| value / interval_millis + 1)
}

fn raw_sources(request: &QueryPlanRequest) -> RawSources {
    if request.end_epoch_millis <= request.hot_data_start_epoch_millis {
        RawSources::ArchivedSegments
    } else if request.start_epoch_millis >= request.hot_data_start_epoch_millis {
        RawSources::Hot
    } else {
        RawSources::HotAndArchivedSegments
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum QueryPlanError {
    #[error("query range must be a non-empty half-open interval")]
    InvalidRange,
    #[error("query timezone must be a valid IANA timezone")]
    InvalidTimezone,
    #[error("at least one series field is required")]
    NoFields,
    #[error("maximum points must be greater than zero")]
    InvalidPointBudget,
    #[error("expected raw interval must be greater than zero")]
    InvalidRawInterval,
    #[error("available rollups may contain only aggregate resolutions")]
    InvalidAvailableResolution,
    #[error("the requested resolution is unavailable")]
    ResolutionUnavailable,
    #[error("a requested field requires raw observations")]
    FieldRequiresRaw,
    #[error("the selected query would exceed the maximum point budget")]
    PointBudgetExceeded,
    #[error("the query point estimate overflowed")]
    PointEstimateOverflow,
}
