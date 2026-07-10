use std::{collections::BTreeMap, error::Error};

use pvlog_domain::{
    CanonicalObservation, Correction, CorrectionId, IdempotencyIdentity, MeasurementValues,
    ObservationId, ObservationSource, ObservationSourceKind, QualityFlags, SystemId, TimeRange,
    UtcTimestamp, Watts,
};

#[test]
fn time_ranges_are_half_open() -> Result<(), Box<dyn Error>> {
    let start = UtcTimestamp::from_epoch_millis(1_000)?;
    let end = UtcTimestamp::from_epoch_millis(2_000)?;
    let range = TimeRange::new(start, end)?;

    assert!(range.contains(start));
    assert!(!range.contains(end));
    assert!(TimeRange::new(end, start).is_err());
    Ok(())
}

#[test]
fn canonical_observations_retain_provenance_idempotency_and_quality() -> Result<(), Box<dyn Error>>
{
    let observed_at = UtcTimestamp::from_epoch_millis(1_000)?;
    let identity = IdempotencyIdentity {
        namespace: "api-credential:example".to_owned(),
        key: "reading-42".to_owned(),
        payload_hash: [7; 32],
    };
    let observation = CanonicalObservation {
        id: ObservationId::new(),
        system_id: SystemId::new(),
        observed_at,
        received_at: UtcTimestamp::from_epoch_millis(1_100)?,
        values: MeasurementValues {
            generation_power: Some(Watts::new(4_200)),
            extended: BTreeMap::new(),
            ..MeasurementValues::default()
        },
        source: ObservationSource {
            kind: ObservationSourceKind::ModernApi,
            source_reference: None,
        },
        idempotency: identity.clone(),
        quality: QualityFlags::ESTIMATED | QualityFlags::DERIVED,
    };

    assert_eq!(observation.idempotency, identity);
    assert!(observation.quality.contains(QualityFlags::DERIVED));
    Ok(())
}

#[test]
fn corrections_are_versioned_immutable_overlays() -> Result<(), Box<dyn Error>> {
    let correction = Correction {
        id: CorrectionId::new(),
        observation_id: ObservationId::new(),
        corrected_at: UtcTimestamp::from_epoch_millis(5_000)?,
        replacement: None,
        reason_code: "duplicate_source_value".to_owned(),
        generation: 2,
    };

    assert!(correction.replacement.is_none());
    assert_eq!(correction.generation, 2);
    Ok(())
}
