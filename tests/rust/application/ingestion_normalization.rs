use pvlog_application::{
    IngestionNormalizationError, NormalizeObservation, PowerUnit, normalize_observation,
};
use pvlog_domain::{
    BatteryFlowState, ChannelId, ExtendedValue, ObservationSource, ObservationSourceKind,
    QualityFlags, SystemId, UtcTimestamp,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

#[test]
fn explicit_units_provenance_battery_and_registered_channels_normalize_exactly()
-> Result<(), Box<dyn Error>> {
    let channel = ChannelId::new();
    let timestamp = UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH);
    let input = NormalizeObservation {
        system_id: SystemId::new(),
        credential_id: None,
        test: false,
        observed_at: timestamp,
        received_at: timestamp,
        generation_power: Some((1_234_000, PowerUnit::Milliwatts)),
        generation_energy: None,
        consumption_power: None,
        consumption_energy: None,
        voltage_millivolts: Some(230_000),
        temperature_millidegrees_celsius: Some(21_500),
        battery_energy: None,
        battery_power: Some((-500, PowerUnit::Watts)),
        battery_state_of_charge_basis_points: Some(7_500),
        battery_flow_state: BatteryFlowState::Discharging,
        extended: BTreeMap::from([(channel, ExtendedValue::DecimalScaled(42))]),
        registered_channels: BTreeSet::from([channel]),
        channel_readings: std::collections::BTreeMap::new(),
        supplements: pvlog_application::ObservationSupplements::default(),
        source: ObservationSource {
            kind: ObservationSourceKind::ModernApi,
            source_reference: Some("uploader-1".to_owned()),
        },
        idempotency_namespace: "principal".to_owned(),
        idempotency_key: "request-1".to_owned(),
        quality: QualityFlags::NONE,
    };
    let observation = normalize_observation(input.clone())?;
    assert_eq!(
        observation
            .values
            .generation_power
            .map(pvlog_domain::Watts::value),
        Some(1_234)
    );
    assert!(observation.values.battery.is_some());
    assert_eq!(observation.source.kind, ObservationSourceKind::ModernApi);
    let mut imprecise = input;
    imprecise.generation_power = Some((1, PowerUnit::Milliwatts));
    assert_eq!(
        normalize_observation(imprecise),
        Err(IngestionNormalizationError::PrecisionLoss)
    );
    Ok(())
}

#[test]
fn a_counter_step_is_an_increase_a_reset_or_unknown() {
    use pvlog_application::{CounterStep, counter_step};
    assert_eq!(counter_step(None, 500), CounterStep::NoPredecessor);
    assert_eq!(counter_step(Some(500), 500), CounterStep::Increase(0));
    assert_eq!(counter_step(Some(500), 740), CounterStep::Increase(240));
    assert_eq!(counter_step(Some(500), 499), CounterStep::Reset);
}
