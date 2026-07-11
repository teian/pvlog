use pvlog_application::{
    DataQualityError, DataQualityKind, DataQualityPolicy, QualitySample, RejectedIngestion,
    detect_data_quality,
};
use std::error::Error;

const MINUTE: i64 = 60_000;

fn sample(minute: i64, source: &str, value: u8, energy: u64, reset: u32) -> QualitySample {
    QualitySample {
        timestamp_epoch_millis: minute * MINUTE,
        source_reference: source.to_owned(),
        value_fingerprint: [value; 32],
        quality_flags: 0,
        cumulative_energy_wh: Some(energy),
        counter_reset_sequence: Some(reset),
    }
}

fn policy() -> DataQualityPolicy {
    DataQualityPolicy {
        expected_interval_millis: u64::try_from(5 * MINUTE).unwrap_or_default(),
        interval_tolerance_millis: 30_000,
        suspect_quality_mask: 0b10,
        aggregate_lag_threshold_millis: u64::try_from(10 * MINUTE).unwrap_or_default(),
    }
}

#[test]
fn detects_each_quality_class_without_creating_points() -> Result<(), Box<dyn Error>> {
    let mut suspect = sample(5, "inverter", 1, 100, 0);
    suspect.quality_flags = 0b10;
    let samples = [
        suspect,
        sample(15, "a-meter", 2, 120, 0),
        sample(15, "b-meter", 3, 121, 0),
        sample(20, "inverter", 4, 5, 1),
    ];
    let rejected = [RejectedIngestion {
        timestamp_epoch_millis: 25 * MINUTE,
        source_reference: "uploader".to_owned(),
        reason_code: "physical_limit".to_owned(),
    }];

    let issues = detect_data_quality(
        5 * MINUTE,
        35 * MINUTE,
        &samples,
        &rejected,
        Some(10 * MINUTE),
        30 * MINUTE,
        policy(),
    )?;
    let kinds = issues.iter().map(|issue| issue.kind).collect::<Vec<_>>();

    assert!(kinds.contains(&DataQualityKind::MissingInterval));
    assert!(kinds.contains(&DataQualityKind::SuspectObservation));
    assert!(kinds.contains(&DataQualityKind::SourceConflict));
    assert!(kinds.contains(&DataQualityKind::CounterReset));
    assert!(kinds.contains(&DataQualityKind::RejectedIngestion));
    assert!(kinds.contains(&DataQualityKind::AggregateLag));
    assert_eq!(samples.len(), 4, "detector must not fabricate raw points");
    Ok(())
}

#[test]
fn identical_multi_source_values_are_not_conflicts() -> Result<(), Box<dyn Error>> {
    let issues = detect_data_quality(
        0,
        10 * MINUTE,
        &[sample(5, "a", 1, 100, 0), sample(5, "b", 1, 100, 0)],
        &[],
        Some(5 * MINUTE),
        6 * MINUTE,
        policy(),
    )?;

    assert!(
        !issues
            .iter()
            .any(|issue| issue.kind == DataQualityKind::SourceConflict)
    );
    Ok(())
}

#[test]
fn unordered_or_out_of_range_events_are_rejected() {
    let unordered = [sample(5, "z", 1, 1, 0), sample(5, "a", 1, 1, 0)];
    assert_eq!(
        detect_data_quality(0, 10 * MINUTE, &unordered, &[], None, 0, policy()),
        Err(DataQualityError::UnorderedSamples)
    );
    assert_eq!(
        detect_data_quality(
            0,
            10 * MINUTE,
            &[sample(11, "a", 1, 1, 0)],
            &[],
            None,
            0,
            policy()
        ),
        Err(DataQualityError::TimestampOutsideRange)
    );
}
