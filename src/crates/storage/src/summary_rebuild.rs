//! Idempotent daily and lifetime summary rebuild planning.

use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SummaryDay(pub i32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyAggregate {
    pub system_id: Uuid,
    pub day: SummaryDay,
    pub generation_wh: i64,
    pub consumption_wh: i64,
    pub quality_flags: u32,
    pub source_revision: u64,
    pub calendar_year: i32,
    pub calendar_month: u8,
    pub modeled: ModeledYieldAggregate,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModeledYieldAggregate {
    pub expected_energy_wh: Option<i64>,
    pub expected_lower_wh: Option<i64>,
    pub expected_upper_wh: Option<i64>,
    pub forecast_energy_wh: Option<i64>,
    pub forecast_lower_wh: Option<i64>,
    pub forecast_upper_wh: Option<i64>,
    pub actual_coverage_basis_points: u16,
    pub expected_coverage_basis_points: u16,
    pub forecast_coverage_basis_points: u16,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SummaryPeriod {
    Day(SummaryDay),
    Month { year: i32, month: u8 },
    Year(i32),
    Lifetime,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModeledSummary {
    pub actual_energy_wh: i128,
    pub expected_energy_wh: Option<i128>,
    pub forecast_energy_wh: Option<i128>,
    pub expected_lower_wh: Option<i128>,
    pub expected_upper_wh: Option<i128>,
    pub forecast_lower_wh: Option<i128>,
    pub forecast_upper_wh: Option<i128>,
    pub generation_performance_basis_points: Option<u32>,
    pub forecast_realization_basis_points: Option<u32>,
    pub actual_coverage_basis_points: u16,
    pub expected_coverage_basis_points: u16,
    pub forecast_coverage_basis_points: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SummaryProjection {
    pub daily: BTreeMap<(Uuid, SummaryDay), DailyAggregate>,
    pub lifetime: BTreeMap<Uuid, LifetimeAggregate>,
    invalidated: BTreeSet<(Uuid, SummaryDay)>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LifetimeAggregate {
    pub generation_wh: i128,
    pub consumption_wh: i128,
    pub quality_flags: u32,
    pub through_day: Option<SummaryDay>,
}

impl SummaryProjection {
    /// Invalidates a changed day and its dependent lifetime summary.
    pub fn invalidate(&mut self, system_id: Uuid, day: SummaryDay) {
        self.invalidated.insert((system_id, day));
        self.lifetime.remove(&system_id);
    }

    /// Reconciles one authoritative daily aggregate and rebuilds its lifetime dependency.
    /// Replaying the same source revision and values is a no-op.
    pub fn reconcile(&mut self, aggregate: &DailyAggregate) -> bool {
        let key = (aggregate.system_id, aggregate.day);
        let changed = self.daily.get(&key) != Some(aggregate);
        self.daily.insert(key, aggregate.clone());
        self.invalidated.remove(&key);
        self.rebuild_lifetime(aggregate.system_id);
        changed
    }

    pub fn invalidated_days(&self) -> impl Iterator<Item = (Uuid, SummaryDay)> + '_ {
        self.invalidated.iter().copied()
    }

    /// Rebuilds daily, monthly, yearly, or lifetime modeled metrics with independent coverage.
    #[must_use]
    pub fn modeled_summary(&self, system_id: Uuid, period: SummaryPeriod) -> ModeledSummary {
        let mut summary = ModeledSummary::default();
        let mut any = false;
        for daily in self.daily.values().filter(|daily| {
            daily.system_id == system_id
                && match period {
                    SummaryPeriod::Day(day) => daily.day == day,
                    SummaryPeriod::Month { year, month } => {
                        daily.calendar_year == year && daily.calendar_month == month
                    }
                    SummaryPeriod::Year(year) => daily.calendar_year == year,
                    SummaryPeriod::Lifetime => true,
                }
        }) {
            summary.actual_energy_wh += i128::from(daily.generation_wh);
            accumulate_optional(
                &mut summary.expected_energy_wh,
                daily.modeled.expected_energy_wh,
            );
            accumulate_optional(
                &mut summary.forecast_energy_wh,
                daily.modeled.forecast_energy_wh,
            );
            accumulate_optional(
                &mut summary.expected_lower_wh,
                daily.modeled.expected_lower_wh,
            );
            accumulate_optional(
                &mut summary.expected_upper_wh,
                daily.modeled.expected_upper_wh,
            );
            accumulate_optional(
                &mut summary.forecast_lower_wh,
                daily.modeled.forecast_lower_wh,
            );
            accumulate_optional(
                &mut summary.forecast_upper_wh,
                daily.modeled.forecast_upper_wh,
            );
            summary.actual_coverage_basis_points = minimum_coverage(
                summary.actual_coverage_basis_points,
                daily.modeled.actual_coverage_basis_points,
                any,
            );
            summary.expected_coverage_basis_points = minimum_coverage(
                summary.expected_coverage_basis_points,
                daily.modeled.expected_coverage_basis_points,
                any,
            );
            summary.forecast_coverage_basis_points = minimum_coverage(
                summary.forecast_coverage_basis_points,
                daily.modeled.forecast_coverage_basis_points,
                any,
            );
            any = true;
        }
        summary.generation_performance_basis_points = ratio(
            summary.actual_energy_wh,
            summary.expected_energy_wh,
            summary.actual_coverage_basis_points,
            summary.expected_coverage_basis_points,
        );
        summary.forecast_realization_basis_points = ratio(
            summary.actual_energy_wh,
            summary.forecast_energy_wh,
            summary.actual_coverage_basis_points,
            summary.forecast_coverage_basis_points,
        );
        summary
    }

    fn rebuild_lifetime(&mut self, system_id: Uuid) {
        let mut lifetime = LifetimeAggregate::default();
        for aggregate in self
            .daily
            .values()
            .filter(|aggregate| aggregate.system_id == system_id)
        {
            lifetime.generation_wh += i128::from(aggregate.generation_wh);
            lifetime.consumption_wh += i128::from(aggregate.consumption_wh);
            lifetime.quality_flags |= aggregate.quality_flags;
            lifetime.through_day = Some(
                lifetime
                    .through_day
                    .map_or(aggregate.day, |day| day.max(aggregate.day)),
            );
        }
        self.lifetime.insert(system_id, lifetime);
    }
}

fn accumulate_optional(total: &mut Option<i128>, value: Option<i64>) {
    if let Some(value) = value {
        *total = Some(total.unwrap_or(0) + i128::from(value));
    }
}

fn minimum_coverage(current: u16, next: u16, initialized: bool) -> u16 {
    if initialized { current.min(next) } else { next }
}

fn ratio(
    actual: i128,
    modeled: Option<i128>,
    actual_coverage: u16,
    modeled_coverage: u16,
) -> Option<u32> {
    let modeled = modeled?;
    if actual < 0 || modeled <= 0 || actual_coverage == 0 || modeled_coverage == 0 {
        return None;
    }
    u32::try_from(actual.checked_mul(10_000)? / modeled).ok()
}
