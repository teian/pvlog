//! Exact normalization of explicit wire units into canonical integer base units.

use crate::{
    ChannelReading, ReportedDevice, device_states_of_charge_in_range,
    system_state_of_charge_basis_points, validate_reported_devices,
};
use pvlog_domain::{
    ApiCredentialId, BasisPoints, BatteryFlowState, BatteryReading, CanonicalObservation,
    ChannelId, EnergyReading, ExtendedValue, IdempotencyIdentity, MeasurementValues,
    MilliDegreesCelsius, MilliVolts, ObservationId, ObservationSource, QualityFlags, SystemId,
    UtcTimestamp, WattHours, Watts,
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
/// Optional members of a reading beyond power and interval energy.
///
/// They are grouped so that a reading which uses none of them is written as
/// `ObservationSupplements::default()`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObservationSupplements {
    /// Lifetime generation counter in watt-hours.
    pub generation_lifetime_wh: Option<i64>,
    /// Lifetime consumption counter in watt-hours.
    pub consumption_lifetime_wh: Option<i64>,
    /// Power drawn from the grid in watts.
    pub grid_import_power_watts: Option<i64>,
    /// Power fed into the grid in watts.
    pub grid_export_power_watts: Option<i64>,
    /// Energy drawn from the grid during the interval, in watt-hours.
    pub grid_import_energy_wh: Option<i64>,
    /// Energy fed into the grid during the interval, in watt-hours.
    pub grid_export_energy_wh: Option<i64>,
    /// Short text from the source, at most [`MAXIMUM_MESSAGE_CHARACTERS`] long.
    pub message: Option<String>,
    /// Identity of the devices the source reads, as far as it reports it.
    pub devices: Vec<ReportedDevice>,
}

/// Longest message a reading may carry, counted in characters.
pub const MAXIMUM_MESSAGE_CHARACTERS: usize = 30;

#[derive(Clone, Debug)]
pub struct NormalizeObservation {
    pub system_id: SystemId,
    /// API credential that delivered the reading; identifies its data source.
    pub credential_id: Option<ApiCredentialId>,
    /// Marks a reading sent only to prove the connection; it is validated like
    /// any other but kept out of the system's telemetry.
    pub test: bool,
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
    /// Readings still keyed by their wire name, before channel resolution.
    pub channel_readings: BTreeMap<String, ChannelReading>,
    pub supplements: ObservationSupplements,
    pub source: ObservationSource,
    pub idempotency_namespace: String,
    pub idempotency_key: String,
    pub quality: QualityFlags,
}

/// Normalizes an explicit-unit ingestion command without floating-point conversion.
/// # Errors
/// Returns an error for sub-base-unit precision, empty identity, invalid battery ratio, serialization failure, unregistered channels, or a supplement that is out of range, naming its wire field.
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
    if input
        .battery_state_of_charge_basis_points
        .is_some_and(|ratio| !(0..=10_000).contains(&ratio))
    {
        return Err(IngestionNormalizationError::InvalidField(
            "batteryStateOfChargeBasisPoints",
        ));
    }
    if input.battery_energy.is_some_and(|stored| stored.value < 0) {
        return Err(IngestionNormalizationError::InvalidField("batteryEnergyWh"));
    }
    if !device_states_of_charge_in_range(&input.channel_readings) {
        return Err(IngestionNormalizationError::InvalidField("channels"));
    }
    if validate_reported_devices(&input.supplements.devices).is_err() {
        return Err(IngestionNormalizationError::InvalidField("devices"));
    }
    // A state the source states for the system wins over what its devices report.
    let state_of_charge = input
        .battery_state_of_charge_basis_points
        .or_else(|| system_state_of_charge_basis_points(&input.channel_readings));
    let battery = (input.battery_energy.is_some()
        || input.battery_power.is_some()
        || state_of_charge.is_some())
    .then(|| -> Result<_, IngestionNormalizationError> {
        let power = input.battery_power.map(power).transpose()?;
        Ok(BatteryReading {
            energy: input
                .battery_energy
                .map(energy)
                .transpose()?
                .map(energy_value),
            power,
            state_of_charge: state_of_charge
                .map(BasisPoints::new)
                .transpose()
                .map_err(|_| IngestionNormalizationError::InvalidBatteryRatio)?,
            flow_state: match (input.battery_flow_state, power.map(Watts::value)) {
                // Positive battery power charges, negative power discharges.
                (BatteryFlowState::Unknown, Some(watts)) if watts > 0 => BatteryFlowState::Charging,
                (BatteryFlowState::Unknown, Some(watts)) if watts < 0 => {
                    BatteryFlowState::Discharging
                }
                (BatteryFlowState::Unknown, Some(_)) => BatteryFlowState::Idle,
                (stated, _) => stated,
            },
        })
    })
    .transpose()?;
    let supplements = &input.supplements;
    // A grid connection carries power in one direction at a time. Both
    // directions above zero in one reading mean the source mixed up its
    // registers, and storing it would count the same energy twice.
    if supplements.grid_import_power_watts.is_some_and(|w| w > 0)
        && supplements.grid_export_power_watts.is_some_and(|w| w > 0)
    {
        return Err(IngestionNormalizationError::InvalidField(
            "gridExportPowerWatts",
        ));
    }
    if supplements
        .message
        .as_ref()
        .is_some_and(|message| message.chars().count() > MAXIMUM_MESSAGE_CHARACTERS)
    {
        return Err(IngestionNormalizationError::InvalidField("message"));
    }
    let values = MeasurementValues {
        generation_power: input.generation_power.map(power).transpose()?,
        generation_energy: input.generation_energy.map(energy).transpose()?,
        consumption_power: input.consumption_power.map(power).transpose()?,
        consumption_energy: input.consumption_energy.map(energy).transpose()?,
        voltage: input.voltage_millivolts.map(MilliVolts::new),
        temperature: input
            .temperature_millidegrees_celsius
            .map(MilliDegreesCelsius::new),
        generation_lifetime: counter(
            input.supplements.generation_lifetime_wh,
            "generationLifetimeWh",
        )?,
        consumption_lifetime: counter(
            input.supplements.consumption_lifetime_wh,
            "consumptionLifetimeWh",
        )?,
        grid_import_power: counter(supplements.grid_import_power_watts, "gridImportPowerWatts")?
            .map(|value| Watts::new(value.value())),
        grid_export_power: counter(supplements.grid_export_power_watts, "gridExportPowerWatts")?
            .map(|value| Watts::new(value.value())),
        grid_import_energy: counter(supplements.grid_import_energy_wh, "gridImportEnergyWh")?,
        grid_export_energy: counter(supplements.grid_export_energy_wh, "gridExportEnergyWh")?,
        message: supplements.message.clone(),
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
/// Accepts a quantity that cannot be negative, such as a counter or a one-directional flow.
fn counter(
    value: Option<i64>,
    field: &'static str,
) -> Result<Option<WattHours>, IngestionNormalizationError> {
    match value {
        Some(value) if value < 0 => Err(IngestionNormalizationError::InvalidField(field)),
        value => Ok(value.map(WattHours::new)),
    }
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
    /// A member is out of range; carries its wire name.
    #[error("field {0} is invalid")]
    InvalidField(&'static str),
}
