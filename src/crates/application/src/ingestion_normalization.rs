//! Exact normalization of explicit wire units into canonical integer base units.

use pvlog_domain::{
    BasisPoints, BatteryFlowState, BatteryReading, CanonicalObservation, ChannelId, EnergyReading,
    ExtendedValue, IdempotencyIdentity, MeasurementValues, MilliDegreesCelsius, MilliVolts,
    ObservationId, ObservationSource, QualityFlags, SystemId, UtcTimestamp, WattHours, Watts,
};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerUnit {
    Watts,
    Milliwatts,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnergyUnit {
    WattHours,
    MilliwattHours,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnergyInput {
    pub value: i64,
    pub unit: EnergyUnit,
    pub cumulative: bool,
    pub reset_sequence: u32,
}
#[derive(Clone, Debug)]
pub struct NormalizeObservation {
    pub system_id: SystemId,
    pub observed_at: UtcTimestamp,
    pub received_at: UtcTimestamp,
    pub generation_power: Option<(i64, PowerUnit)>,
    pub generation_energy: Option<EnergyInput>,
    pub consumption_power: Option<(i64, PowerUnit)>,
    pub consumption_energy: Option<EnergyInput>,
    pub voltage_millivolts: Option<u32>,
    pub temperature_millidegrees_celsius: Option<i32>,
    pub battery_energy: Option<EnergyInput>,
    pub battery_power: Option<(i64, PowerUnit)>,
    pub battery_state_of_charge_basis_points: Option<i32>,
    pub battery_flow_state: BatteryFlowState,
    pub extended: BTreeMap<ChannelId, ExtendedValue>,
    pub registered_channels: BTreeSet<ChannelId>,
    pub source: ObservationSource,
    pub idempotency_namespace: String,
    pub idempotency_key: String,
    pub quality: QualityFlags,
}

/// Normalizes an explicit-unit ingestion command without floating-point conversion.
/// # Errors
/// Returns an error for sub-base-unit precision, empty identity, invalid battery ratio, serialization failure, or unregistered channels.
pub fn normalize_observation(
    input: NormalizeObservation,
) -> Result<CanonicalObservation, IngestionNormalizationError> {
    if input.idempotency_namespace.trim().is_empty() || input.idempotency_key.trim().is_empty() {
        return Err(IngestionNormalizationError::InvalidIdentity);
    }
    if input
        .extended
        .keys()
        .any(|channel| !input.registered_channels.contains(channel))
    {
        return Err(IngestionNormalizationError::UnregisteredChannel);
    }
    let battery = (input.battery_energy.is_some()
        || input.battery_power.is_some()
        || input.battery_state_of_charge_basis_points.is_some())
    .then(|| -> Result<_, IngestionNormalizationError> {
        Ok(BatteryReading {
            energy: input
                .battery_energy
                .map(energy)
                .transpose()?
                .map(energy_value),
            power: input.battery_power.map(power).transpose()?,
            state_of_charge: input
                .battery_state_of_charge_basis_points
                .map(BasisPoints::new)
                .transpose()
                .map_err(|_| IngestionNormalizationError::InvalidBatteryRatio)?,
            flow_state: input.battery_flow_state,
        })
    })
    .transpose()?;
    let values = MeasurementValues {
        generation_power: input.generation_power.map(power).transpose()?,
        generation_energy: input.generation_energy.map(energy).transpose()?,
        consumption_power: input.consumption_power.map(power).transpose()?,
        consumption_energy: input.consumption_energy.map(energy).transpose()?,
        voltage: input.voltage_millivolts.map(MilliVolts::new),
        temperature: input
            .temperature_millidegrees_celsius
            .map(MilliDegreesCelsius::new),
        battery,
        extended: input.extended,
        ..MeasurementValues::default()
    };
    let payload_hash = *blake3::hash(
        &serde_json::to_vec(&values).map_err(|_| IngestionNormalizationError::Serialization)?,
    )
    .as_bytes();
    Ok(CanonicalObservation {
        id: ObservationId::new(),
        system_id: input.system_id,
        observed_at: input.observed_at,
        received_at: input.received_at,
        values,
        source: input.source,
        idempotency: IdempotencyIdentity {
            namespace: input.idempotency_namespace,
            key: input.idempotency_key,
            payload_hash,
        },
        quality: input.quality,
    })
}
fn energy_value(reading: EnergyReading) -> WattHours {
    match reading {
        EnergyReading::Interval(value) | EnergyReading::Cumulative { total: value, .. } => value,
    }
}
fn power((value, unit): (i64, PowerUnit)) -> Result<Watts, IngestionNormalizationError> {
    Ok(Watts::new(match unit {
        PowerUnit::Watts => value,
        PowerUnit::Milliwatts => exact_div(value, 1_000)?,
    }))
}
fn energy(input: EnergyInput) -> Result<EnergyReading, IngestionNormalizationError> {
    let value = WattHours::new(match input.unit {
        EnergyUnit::WattHours => input.value,
        EnergyUnit::MilliwattHours => exact_div(input.value, 1_000)?,
    });
    Ok(if input.cumulative {
        EnergyReading::Cumulative {
            total: value,
            reset_sequence: input.reset_sequence,
        }
    } else {
        EnergyReading::Interval(value)
    })
}
fn exact_div(value: i64, divisor: i64) -> Result<i64, IngestionNormalizationError> {
    if value % divisor == 0 {
        Ok(value / divisor)
    } else {
        Err(IngestionNormalizationError::PrecisionLoss)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum IngestionNormalizationError {
    #[error("value cannot be represented in canonical base units without precision loss")]
    PrecisionLoss,
    #[error("idempotency identity is invalid")]
    InvalidIdentity,
    #[error("extended channel is not registered")]
    UnregisteredChannel,
    #[error("battery state of charge is invalid")]
    InvalidBatteryRatio,
    #[error("canonical payload serialization failed")]
    Serialization,
}
