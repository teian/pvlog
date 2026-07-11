//! Configuration-aware physical validation and deterministic derivation.

use pvlog_domain::{
    CanonicalObservation, EnergyReading, GridFlow, NetCalculationMode, NetPositiveDirection,
    PowerCalculationMode, QualityFlags, WattHours, Watts,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IngestionValidationPolicy {
    pub effective_capacity_watts: u64,
    pub maximum_power_basis_points: u32,
    pub earliest_timestamp_millis: i64,
    pub latest_timestamp_millis: i64,
    pub power_mode: PowerCalculationMode,
    pub net_mode: NetCalculationMode,
}

/// Validates one canonical observation and derives power only when configured and deterministic.
/// # Errors
/// Returns field-classified errors for timestamps, physical limits, dependent fields, counter transitions, grid semantics, or non-deterministic derivation.
pub fn validate_observation(
    mut observation: CanonicalObservation,
    previous: Option<&CanonicalObservation>,
    policy: IngestionValidationPolicy,
) -> Result<CanonicalObservation, IngestionValidationError> {
    if previous.is_some_and(|previous| previous.system_id != observation.system_id) {
        return Err(IngestionValidationError::DependentField);
    }
    let observed_at = i64::try_from(observation.observed_at.epoch_millis())
        .map_err(|_| IngestionValidationError::Timestamp)?;
    if observed_at < policy.earliest_timestamp_millis
        || observed_at > policy.latest_timestamp_millis
    {
        return Err(IngestionValidationError::Timestamp);
    }
    let maximum = policy
        .effective_capacity_watts
        .checked_mul(u64::from(policy.maximum_power_basis_points))
        .and_then(|value| value.checked_div(10_000))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(IngestionValidationError::PhysicalLimit)?;
    for power in [
        observation.values.generation_power,
        observation.values.consumption_power,
    ]
    .into_iter()
    .flatten()
    {
        if power.value().unsigned_abs() > maximum.unsigned_abs() {
            return Err(IngestionValidationError::PhysicalLimit);
        }
    }
    validate_grid(observation.values.grid, policy.net_mode)?;
    if let Some(battery) = observation.values.battery
        && battery.energy.is_none()
        && battery.power.is_none()
        && battery.state_of_charge.is_none()
    {
        return Err(IngestionValidationError::DependentField);
    }
    if observation.values.generation_power.is_none()
        && policy.power_mode == PowerCalculationMode::DeriveFromEnergy
        && let Some(previous_observation) = previous
        && let (Some(current), Some(previous_energy)) = (
            observation.values.generation_energy,
            previous_observation.values.generation_energy,
        )
    {
        observation.values.generation_power = Some(derive_power(
            current,
            previous_energy,
            elapsed_millis(previous_observation.observed_at, observation.observed_at)?,
        )?);
        observation.quality = observation.quality | QualityFlags::DERIVED;
    }
    validate_counter(
        observation.values.generation_energy,
        previous.and_then(|value| value.values.generation_energy),
    )?;
    validate_counter(
        observation.values.consumption_energy,
        previous.and_then(|value| value.values.consumption_energy),
    )?;
    Ok(observation)
}

fn elapsed_millis(
    previous: pvlog_domain::UtcTimestamp,
    current: pvlog_domain::UtcTimestamp,
) -> Result<i64, IngestionValidationError> {
    i64::try_from(current.epoch_millis() - previous.epoch_millis())
        .ok()
        .filter(|value| *value > 0)
        .ok_or(IngestionValidationError::Derivation)
}
fn derive_power(
    current: EnergyReading,
    previous: EnergyReading,
    elapsed_millis: i64,
) -> Result<Watts, IngestionValidationError> {
    let delta = energy_delta(current, previous)?;
    delta
        .value()
        .checked_mul(3_600_000)
        .and_then(|value| value.checked_div(elapsed_millis))
        .map(Watts::new)
        .ok_or(IngestionValidationError::Derivation)
}
fn energy_delta(
    current: EnergyReading,
    previous: EnergyReading,
) -> Result<WattHours, IngestionValidationError> {
    match (current, previous) {
        (EnergyReading::Interval(value), _) => Ok(value),
        (
            EnergyReading::Cumulative {
                total: current,
                reset_sequence,
            },
            EnergyReading::Cumulative {
                total: previous,
                reset_sequence: prior,
            },
        ) if reset_sequence == prior && current.value() >= previous.value() => {
            Ok(WattHours::new(current.value() - previous.value()))
        }
        (
            EnergyReading::Cumulative {
                total,
                reset_sequence,
            },
            EnergyReading::Cumulative {
                reset_sequence: prior,
                ..
            },
        ) if reset_sequence == prior.saturating_add(1) => Ok(total),
        _ => Err(IngestionValidationError::CounterTransition),
    }
}
fn validate_counter(
    current: Option<EnergyReading>,
    previous: Option<EnergyReading>,
) -> Result<(), IngestionValidationError> {
    if let (Some(current @ EnergyReading::Cumulative { .. }), Some(previous)) = (current, previous)
    {
        let _ = energy_delta(current, previous)?;
    }
    Ok(())
}
fn validate_grid(
    grid: Option<GridFlow>,
    mode: NetCalculationMode,
) -> Result<(), IngestionValidationError> {
    match (grid, mode) {
        (
            Some(GridFlow::Net {
                positive: NetPositiveDirection::Import,
                ..
            }),
            NetCalculationMode::ExportPositive,
        )
        | (
            Some(GridFlow::Net {
                positive: NetPositiveDirection::Export,
                ..
            }),
            NetCalculationMode::ImportPositive,
        ) => Err(IngestionValidationError::GridSemantics),
        (
            Some(GridFlow::Split {
                import_power,
                export_power,
            }),
            _,
        ) if import_power.value() < 0 || export_power.value() < 0 => {
            Err(IngestionValidationError::DependentField)
        }
        _ => Ok(()),
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum IngestionValidationError {
    #[error("timestamp is outside the accepted range")]
    Timestamp,
    #[error("measurement exceeds the effective physical limit")]
    PhysicalLimit,
    #[error("dependent measurement fields are invalid")]
    DependentField,
    #[error("grid net semantics do not match system configuration")]
    GridSemantics,
    #[error("cumulative counter transition is invalid")]
    CounterTransition,
    #[error("power derivation is not deterministic")]
    Derivation,
}
