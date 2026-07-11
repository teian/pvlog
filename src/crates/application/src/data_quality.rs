//! Deterministic data-quality detection over accepted and rejected ingestion metadata.

use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySample {
    pub timestamp_epoch_millis: i64,
    pub source_reference: String,
    /// Stable hash of the canonical measurement values at this timestamp.
    pub value_fingerprint: [u8; 32],
    pub quality_flags: u32,
    pub cumulative_energy_wh: Option<u64>,
    pub counter_reset_sequence: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedIngestion {
    pub timestamp_epoch_millis: i64,
    pub source_reference: String,
    pub reason_code: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataQualityPolicy {
    pub expected_interval_millis: u64,
    pub interval_tolerance_millis: u64,
    pub suspect_quality_mask: u32,
    pub aggregate_lag_threshold_millis: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataQualityKind {
    MissingInterval,
    SuspectObservation,
    SourceConflict,
    CounterReset,
    RejectedIngestion,
    AggregateLag,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataQualityIssue {
    pub kind: DataQualityKind,
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    pub source_references: Vec<String>,
    pub reason_code: Option<String>,
}

/// Detects quality issues without constructing or interpolating observation values.
/// # Errors
/// Returns an error for invalid ranges, policy, unordered samples, or timestamps outside the range.
pub fn detect_data_quality(
    range_start_epoch_millis: i64,
    range_end_epoch_millis: i64,
    samples: &[QualitySample],
    rejected: &[RejectedIngestion],
    aggregate_watermark_epoch_millis: Option<i64>,
    now_epoch_millis: i64,
    policy: DataQualityPolicy,
) -> Result<Vec<DataQualityIssue>, DataQualityError> {
    validate_inputs(
        range_start_epoch_millis,
        range_end_epoch_millis,
        samples,
        rejected,
        policy,
    )?;
    let expected = i64::try_from(policy.expected_interval_millis)
        .map_err(|_| DataQualityError::InvalidPolicy)?;
    let tolerance = i64::try_from(policy.interval_tolerance_millis)
        .map_err(|_| DataQualityError::InvalidPolicy)?;
    let mut issues = Vec::new();

    detect_missing_intervals(
        range_start_epoch_millis,
        range_end_epoch_millis,
        samples,
        expected,
        tolerance,
        &mut issues,
    );
    for sample in samples {
        if sample.quality_flags & policy.suspect_quality_mask != 0 {
            issues.push(DataQualityIssue {
                kind: DataQualityKind::SuspectObservation,
                start_epoch_millis: sample.timestamp_epoch_millis,
                end_epoch_millis: sample.timestamp_epoch_millis.saturating_add(1),
                source_references: vec![sample.source_reference.clone()],
                reason_code: None,
            });
        }
    }
    for same_time in
        samples.chunk_by(|left, right| left.timestamp_epoch_millis == right.timestamp_epoch_millis)
    {
        if same_time.len() > 1
            && same_time
                .iter()
                .any(|sample| sample.value_fingerprint != same_time[0].value_fingerprint)
        {
            let mut sources = same_time
                .iter()
                .map(|sample| sample.source_reference.clone())
                .collect::<Vec<_>>();
            sources.sort();
            sources.dedup();
            issues.push(DataQualityIssue {
                kind: DataQualityKind::SourceConflict,
                start_epoch_millis: same_time[0].timestamp_epoch_millis,
                end_epoch_millis: same_time[0].timestamp_epoch_millis.saturating_add(1),
                source_references: sources,
                reason_code: None,
            });
        }
    }
    detect_counter_resets(samples, &mut issues);
    issues.extend(rejected.iter().map(|item| DataQualityIssue {
        kind: DataQualityKind::RejectedIngestion,
        start_epoch_millis: item.timestamp_epoch_millis,
        end_epoch_millis: item.timestamp_epoch_millis.saturating_add(1),
        source_references: vec![item.source_reference.clone()],
        reason_code: Some(item.reason_code.clone()),
    }));
    if let Some(watermark) = aggregate_watermark_epoch_millis {
        let threshold = i64::try_from(policy.aggregate_lag_threshold_millis)
            .map_err(|_| DataQualityError::InvalidPolicy)?;
        if now_epoch_millis.saturating_sub(watermark) > threshold {
            issues.push(DataQualityIssue {
                kind: DataQualityKind::AggregateLag,
                start_epoch_millis: watermark,
                end_epoch_millis: now_epoch_millis,
                source_references: Vec::new(),
                reason_code: Some("aggregate_watermark_lag".to_owned()),
            });
        }
    }
    issues.sort_by_key(|issue| (issue.start_epoch_millis, issue.kind));
    Ok(issues)
}

fn validate_inputs(
    start: i64,
    end: i64,
    samples: &[QualitySample],
    rejected: &[RejectedIngestion],
    policy: DataQualityPolicy,
) -> Result<(), DataQualityError> {
    if end <= start {
        return Err(DataQualityError::InvalidRange);
    }
    if policy.expected_interval_millis == 0 {
        return Err(DataQualityError::InvalidPolicy);
    }
    if samples.windows(2).any(|pair| {
        pair[0].timestamp_epoch_millis > pair[1].timestamp_epoch_millis
            || (pair[0].timestamp_epoch_millis == pair[1].timestamp_epoch_millis
                && pair[0].source_reference > pair[1].source_reference)
    }) {
        return Err(DataQualityError::UnorderedSamples);
    }
    if samples
        .iter()
        .map(|sample| sample.timestamp_epoch_millis)
        .chain(rejected.iter().map(|item| item.timestamp_epoch_millis))
        .any(|timestamp| timestamp < start || timestamp >= end)
    {
        return Err(DataQualityError::TimestampOutsideRange);
    }
    Ok(())
}

fn detect_missing_intervals(
    start: i64,
    end: i64,
    samples: &[QualitySample],
    expected: i64,
    tolerance: i64,
    issues: &mut Vec<DataQualityIssue>,
) {
    let unique_timestamps = samples
        .iter()
        .map(|sample| sample.timestamp_epoch_millis)
        .fold(Vec::new(), |mut timestamps, timestamp| {
            if timestamps.last() != Some(&timestamp) {
                timestamps.push(timestamp);
            }
            timestamps
        });
    let mut previous = start.saturating_sub(expected);
    for timestamp in unique_timestamps
        .into_iter()
        .chain(std::iter::once(end.saturating_add(expected)))
    {
        let expected_next = previous.saturating_add(expected);
        if timestamp.saturating_sub(expected_next) > tolerance {
            issues.push(DataQualityIssue {
                kind: DataQualityKind::MissingInterval,
                start_epoch_millis: expected_next.max(start),
                end_epoch_millis: timestamp.min(end),
                source_references: Vec::new(),
                reason_code: Some("not_reported".to_owned()),
            });
        }
        previous = timestamp;
    }
}

fn detect_counter_resets(samples: &[QualitySample], issues: &mut Vec<DataQualityIssue>) {
    for pair in samples.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.timestamp_epoch_millis == current.timestamp_epoch_millis {
            continue;
        }
        let reset_sequence_advanced = previous
            .counter_reset_sequence
            .zip(current.counter_reset_sequence)
            .is_some_and(|(before, after)| after > before);
        let counter_decreased = previous
            .cumulative_energy_wh
            .zip(current.cumulative_energy_wh)
            .is_some_and(|(before, after)| after < before);
        if reset_sequence_advanced || counter_decreased {
            issues.push(DataQualityIssue {
                kind: DataQualityKind::CounterReset,
                start_epoch_millis: current.timestamp_epoch_millis,
                end_epoch_millis: current.timestamp_epoch_millis.saturating_add(1),
                source_references: vec![current.source_reference.clone()],
                reason_code: Some(
                    if reset_sequence_advanced {
                        "declared_counter_reset"
                    } else {
                        "counter_decreased"
                    }
                    .to_owned(),
                ),
            });
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DataQualityError {
    #[error("data-quality range must be non-empty")]
    InvalidRange,
    #[error("data-quality policy is invalid")]
    InvalidPolicy,
    #[error("quality samples must be ordered by timestamp and source")]
    UnorderedSamples,
    #[error("a quality event is outside the requested range")]
    TimestampOutsideRange,
}
