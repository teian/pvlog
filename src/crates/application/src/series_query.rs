//! Multi-series telemetry query execution over a validated physical query plan.

use crate::{
    QueryPlan, QueryPlanError, QueryPlanRequest, QueryResolution, SeriesField, plan_query,
};
use async_trait::async_trait;
use pvlog_domain::SystemId;
use std::{collections::BTreeSet, sync::Arc};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesUnit {
    Watts,
    WattHours,
    BasisPoints,
    MilliDegreesCelsius,
    Integer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GapKind {
    Missing,
    Suspect,
    IncompleteCoverage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesGap {
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub kind: GapKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesPoint {
    pub timestamp_epoch_millis: i64,
    pub value: i64,
    pub coverage_basis_points: u16,
    pub quality_flags: u32,
    pub provenance: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedSeries {
    pub field: SeriesField,
    pub unit: SeriesUnit,
    pub points: Vec<SeriesPoint>,
    pub gaps: Vec<SeriesGap>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesQueryResult {
    pub actual_resolution: QueryResolution,
    pub timezone: String,
    pub series: Vec<PlannedSeries>,
}

#[async_trait]
pub trait SeriesQueryRepository: Send + Sync {
    async fn execute_plan(
        &self,
        system_id: SystemId,
        plan: &QueryPlan,
    ) -> Result<Vec<PlannedSeries>, SeriesQueryRepositoryError>;
}

#[derive(Clone)]
pub struct SeriesQueryService<R> {
    repository: Arc<R>,
}

impl<R> SeriesQueryService<R>
where
    R: SeriesQueryRepository,
{
    #[must_use]
    pub const fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// Plans, executes, and validates a bounded multi-series telemetry query.
    /// # Errors
    /// Returns an error when planning fails, storage is unavailable, or storage returns data that
    /// violates the planned fields, ordering, range, units, coverage, or point bound.
    pub async fn query(
        &self,
        system_id: SystemId,
        request: QueryPlanRequest,
    ) -> Result<SeriesQueryResult, SeriesQueryError> {
        let start = request.start_epoch_millis;
        let end = request.end_epoch_millis;
        let maximum_points = request.maximum_points;
        let timezone = request.timezone.clone();
        let plan = plan_query(request)?;
        let mut series = self.repository.execute_plan(system_id, &plan).await?;
        validate_series(&mut series, &plan, start, end, maximum_points)?;
        Ok(SeriesQueryResult {
            actual_resolution: plan.actual_resolution,
            timezone,
            series,
        })
    }
}

fn validate_series(
    series: &mut [PlannedSeries],
    plan: &QueryPlan,
    start: i64,
    end: i64,
    maximum_points: u32,
) -> Result<(), SeriesQueryError> {
    let mut seen = BTreeSet::new();
    for item in series.iter() {
        if !plan.fields.contains(&item.field) || !seen.insert(item.field) {
            return Err(SeriesQueryError::UnexpectedSeries);
        }
        if item.unit != unit_for(item.field) {
            return Err(SeriesQueryError::UnitMismatch);
        }
        let result_bound = usize::try_from(maximum_points)
            .map_err(|_| SeriesQueryError::ResultBoundExceeded)?
            .saturating_add(2);
        if item.points.len() > result_bound {
            return Err(SeriesQueryError::ResultBoundExceeded);
        }
        if item.points.iter().any(|point| {
            point.timestamp_epoch_millis < start
                || point.timestamp_epoch_millis >= end
                || point.coverage_basis_points > 10_000
        }) || item
            .points
            .windows(2)
            .any(|pair| pair[0].timestamp_epoch_millis >= pair[1].timestamp_epoch_millis)
        {
            return Err(SeriesQueryError::InvalidPoint);
        }
        if plan.actual_resolution != QueryResolution::Raw
            && item.points.iter().any(|point| point.provenance.is_some())
        {
            return Err(SeriesQueryError::AggregateHasRawProvenance);
        }
        if item.gaps.iter().any(|gap| {
            gap.start_epoch_millis < start
                || gap.end_epoch_millis > end
                || gap.end_epoch_millis <= gap.start_epoch_millis
        }) {
            return Err(SeriesQueryError::InvalidGap);
        }
    }
    if seen.len() != plan.fields.len() {
        return Err(SeriesQueryError::MissingSeries);
    }
    series.sort_unstable_by_key(|item| item.field);
    Ok(())
}

const fn unit_for(field: SeriesField) -> SeriesUnit {
    match field {
        SeriesField::GenerationPower
        | SeriesField::ConsumptionPower
        | SeriesField::GridPower
        | SeriesField::BatteryPower => SeriesUnit::Watts,
        SeriesField::GenerationEnergy | SeriesField::ConsumptionEnergy => SeriesUnit::WattHours,
        SeriesField::BatteryStateOfCharge => SeriesUnit::BasisPoints,
        SeriesField::Temperature => SeriesUnit::MilliDegreesCelsius,
        SeriesField::Extended | SeriesField::Provenance => SeriesUnit::Integer,
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SeriesQueryRepositoryError {
    #[error("series storage is unavailable")]
    Unavailable,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SeriesQueryError {
    #[error("query planning failed: {0}")]
    Plan(#[from] QueryPlanError),
    #[error("series storage failed: {0}")]
    Repository(#[from] SeriesQueryRepositoryError),
    #[error("storage returned an unrequested or duplicate series")]
    UnexpectedSeries,
    #[error("storage omitted a requested series")]
    MissingSeries,
    #[error("series unit does not match the canonical field unit")]
    UnitMismatch,
    #[error("series result exceeded the maximum point bound")]
    ResultBoundExceeded,
    #[error("series contains an invalid, unordered, or out-of-range point")]
    InvalidPoint,
    #[error("aggregate series contains raw point provenance")]
    AggregateHasRawProvenance,
    #[error("series contains an invalid or out-of-range gap")]
    InvalidGap,
}
