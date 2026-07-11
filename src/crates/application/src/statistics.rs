//! Deterministic statistics over pre-resolved local-calendar summary buckets.

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatisticsPeriod {
    Daily,
    Monthly,
    Yearly,
    Lifetime,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatisticsBucket {
    pub generation_wh: Option<u64>,
    pub consumption_wh: Option<u64>,
    pub grid_import_wh: Option<u64>,
    pub grid_export_wh: Option<u64>,
    pub peak_generation_watts: Option<i64>,
    pub minimum_temperature_milli_celsius: Option<i64>,
    pub maximum_temperature_milli_celsius: Option<i64>,
    pub battery_charge_wh: Option<u64>,
    pub battery_discharge_wh: Option<u64>,
    pub minimum_battery_basis_points: Option<u16>,
    pub maximum_battery_basis_points: Option<u16>,
    pub revenue_minor_units: Option<i64>,
    pub cost_minor_units: Option<i64>,
    pub covered_millis: u64,
    pub expected_millis: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnergyStatistics {
    pub period: StatisticsPeriod,
    pub generation_wh: Option<u64>,
    pub consumption_wh: Option<u64>,
    pub grid_import_wh: Option<u64>,
    pub grid_export_wh: Option<u64>,
    pub self_consumption_wh: Option<u64>,
    pub efficiency_wh_per_kw: Option<u64>,
    pub peak_generation_watts: Option<i64>,
    pub minimum_temperature_milli_celsius: Option<i64>,
    pub maximum_temperature_milli_celsius: Option<i64>,
    pub battery_charge_wh: Option<u64>,
    pub battery_discharge_wh: Option<u64>,
    pub minimum_battery_basis_points: Option<u16>,
    pub maximum_battery_basis_points: Option<u16>,
    pub revenue_minor_units: Option<i64>,
    pub cost_minor_units: Option<i64>,
    pub net_financial_minor_units: Option<i64>,
    pub coverage_basis_points: u16,
}

/// Combines summary buckets without inventing values for unavailable measurements.
/// # Errors
/// Returns an error for empty input, zero capacity, invalid coverage, or arithmetic overflow.
pub fn calculate_statistics(
    period: StatisticsPeriod,
    buckets: &[StatisticsBucket],
    effective_capacity_watts: Option<u64>,
) -> Result<EnergyStatistics, StatisticsError> {
    if buckets.is_empty() {
        return Err(StatisticsError::NoData);
    }
    if effective_capacity_watts == Some(0) {
        return Err(StatisticsError::InvalidCapacity);
    }
    if buckets
        .iter()
        .any(|bucket| bucket.covered_millis > bucket.expected_millis)
    {
        return Err(StatisticsError::InvalidCoverage);
    }

    let generation_wh = checked_optional_sum(buckets.iter().map(|b| b.generation_wh))?;
    let consumption_wh = checked_optional_sum(buckets.iter().map(|b| b.consumption_wh))?;
    let grid_import_wh = checked_optional_sum(buckets.iter().map(|b| b.grid_import_wh))?;
    let grid_export_wh = checked_optional_sum(buckets.iter().map(|b| b.grid_export_wh))?;
    let battery_charge_wh = checked_optional_sum(buckets.iter().map(|b| b.battery_charge_wh))?;
    let battery_discharge_wh =
        checked_optional_sum(buckets.iter().map(|b| b.battery_discharge_wh))?;
    let revenue_minor_units =
        checked_optional_signed_sum(buckets.iter().map(|bucket| bucket.revenue_minor_units))?;
    let cost_minor_units =
        checked_optional_signed_sum(buckets.iter().map(|bucket| bucket.cost_minor_units))?;
    let covered = checked_sum(buckets.iter().map(|bucket| bucket.covered_millis))?;
    let expected = checked_sum(buckets.iter().map(|bucket| bucket.expected_millis))?;
    if expected == 0 {
        return Err(StatisticsError::InvalidCoverage);
    }
    let coverage = covered
        .checked_mul(10_000)
        .ok_or(StatisticsError::Overflow)?
        / expected;

    let self_consumption_wh = generation_wh
        .zip(grid_export_wh)
        .map(|(generation, export)| generation.saturating_sub(export));
    let efficiency_wh_per_kw = generation_wh
        .zip(effective_capacity_watts)
        .map(|(generation, capacity)| {
            generation
                .checked_mul(1_000)
                .ok_or(StatisticsError::Overflow)
                .map(|scaled| scaled / capacity)
        })
        .transpose()?;
    let net_financial_minor_units = revenue_minor_units
        .zip(cost_minor_units)
        .map(|(revenue, cost)| revenue.checked_sub(cost).ok_or(StatisticsError::Overflow))
        .transpose()?;

    Ok(EnergyStatistics {
        period,
        generation_wh,
        consumption_wh,
        grid_import_wh,
        grid_export_wh,
        self_consumption_wh,
        efficiency_wh_per_kw,
        peak_generation_watts: buckets.iter().filter_map(|b| b.peak_generation_watts).max(),
        minimum_temperature_milli_celsius: buckets
            .iter()
            .filter_map(|b| b.minimum_temperature_milli_celsius)
            .min(),
        maximum_temperature_milli_celsius: buckets
            .iter()
            .filter_map(|b| b.maximum_temperature_milli_celsius)
            .max(),
        battery_charge_wh,
        battery_discharge_wh,
        minimum_battery_basis_points: buckets
            .iter()
            .filter_map(|b| b.minimum_battery_basis_points)
            .min(),
        maximum_battery_basis_points: buckets
            .iter()
            .filter_map(|b| b.maximum_battery_basis_points)
            .max(),
        revenue_minor_units,
        cost_minor_units,
        net_financial_minor_units,
        coverage_basis_points: u16::try_from(coverage).map_err(|_| StatisticsError::Overflow)?,
    })
}

fn checked_optional_sum(
    mut values: impl Iterator<Item = Option<u64>>,
) -> Result<Option<u64>, StatisticsError> {
    values.try_fold(None, |total, value| match (total, value) {
        (None, None) => Ok(None),
        (Some(total), None) => Ok(Some(total)),
        (None, Some(value)) => Ok(Some(value)),
        (Some(total), Some(value)) => total
            .checked_add(value)
            .map(Some)
            .ok_or(StatisticsError::Overflow),
    })
}

fn checked_optional_signed_sum(
    mut values: impl Iterator<Item = Option<i64>>,
) -> Result<Option<i64>, StatisticsError> {
    values.try_fold(None, |total, value| match (total, value) {
        (None, None) => Ok(None),
        (Some(total), None) => Ok(Some(total)),
        (None, Some(value)) => Ok(Some(value)),
        (Some(total), Some(value)) => total
            .checked_add(value)
            .map(Some)
            .ok_or(StatisticsError::Overflow),
    })
}

fn checked_sum(mut values: impl Iterator<Item = u64>) -> Result<u64, StatisticsError> {
    values.try_fold(0_u64, |total, value| {
        total.checked_add(value).ok_or(StatisticsError::Overflow)
    })
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum StatisticsError {
    #[error("statistics require at least one summary bucket")]
    NoData,
    #[error("effective capacity must be greater than zero")]
    InvalidCapacity,
    #[error("statistics coverage is invalid")]
    InvalidCoverage,
    #[error("statistics arithmetic overflowed")]
    Overflow,
}
