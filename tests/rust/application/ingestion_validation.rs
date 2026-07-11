use pvlog_application::{
    IngestionValidationError, IngestionValidationPolicy, validate_observation,
};
use pvlog_domain::{
    CanonicalObservation, EnergyReading, IdempotencyIdentity, MeasurementValues,
    NetCalculationMode, ObservationId, ObservationSource, ObservationSourceKind,
    PowerCalculationMode, QualityFlags, SystemId, UtcTimestamp, WattHours, Watts,
};
use std::error::Error;

#[test]
fn validation_covers_derivation_physical_limits_and_counter_resets() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let previous = observation(system, 0, 1_000, 0, None);
    let current = observation(system, 3_600_000, 1_100, 0, None);
    let policy = policy();
    let derived = validate_observation(current, Some(&previous), policy)?;
    assert_eq!(derived.values.generation_power.map(Watts::value), Some(100));
    assert!(derived.quality.contains(QualityFlags::DERIVED));
    let invalid_counter = observation(system, 3_600_000, 900, 0, None);
    assert_eq!(
        validate_observation(invalid_counter, Some(&previous), policy),
        Err(IngestionValidationError::CounterTransition)
    );
    let reset = observation(system, 3_600_000, 20, 1, None);
    assert!(validate_observation(reset, Some(&previous), policy).is_ok());
    let excessive = observation(system, 3_600_000, 1_100, 0, Some(2_000));
    assert_eq!(
        validate_observation(excessive, Some(&previous), policy),
        Err(IngestionValidationError::PhysicalLimit)
    );
    Ok(())
}
fn policy() -> IngestionValidationPolicy {
    IngestionValidationPolicy {
        effective_capacity_watts: 1_000,
        maximum_power_basis_points: 15_000,
        earliest_timestamp_millis: 0,
        latest_timestamp_millis: 10_000_000,
        power_mode: PowerCalculationMode::DeriveFromEnergy,
        net_mode: NetCalculationMode::Measured,
    }
}
fn observation(
    system_id: SystemId,
    at: i64,
    total: i64,
    reset_sequence: u32,
    power: Option<i64>,
) -> CanonicalObservation {
    let timestamp =
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(at));
    CanonicalObservation {
        id: ObservationId::new(),
        system_id,
        observed_at: timestamp,
        received_at: timestamp,
        values: MeasurementValues {
            generation_power: power.map(Watts::new),
            generation_energy: Some(EnergyReading::Cumulative {
                total: WattHours::new(total),
                reset_sequence,
            }),
            ..MeasurementValues::default()
        },
        source: ObservationSource {
            kind: ObservationSourceKind::ModernApi,
            source_reference: None,
        },
        idempotency: IdempotencyIdentity {
            namespace: "test".to_owned(),
            key: format!("{at}"),
            payload_hash: [0; 32],
        },
        quality: QualityFlags::NONE,
    }
}
