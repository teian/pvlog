//! Deterministic telemetry rollup construction over timezone-resolved UTC windows.

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RollupGranularity {
    FifteenMinutes,
    Hourly,
    Daily,
    Monthly,
    Yearly,
}

/// A local-calendar bucket resolved to an unambiguous half-open UTC interval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollupWindow {
    pub granularity: RollupGranularity,
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    /// Human-readable local bucket identity including offset where it can repeat at DST fallback.
    pub local_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RollupSample {
    pub timestamp_epoch_millis: i64,
    pub value: i64,
    /// Duration represented by this sample, clipped to its bucket by the ingest/derivation layer.
    pub covered_millis: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelemetryRollup {
    pub window: RollupWindow,
    pub sum: i128,
    pub min: i64,
    pub max: i64,
    pub count: u64,
    pub first: i64,
    pub last: i64,
    /// Covered milliseconds capped to the bucket duration.
    pub covered_millis: u64,
    /// Coverage in basis points (`10_000` means complete).
    pub coverage_basis_points: u16,
}

/// Builds rollups for pre-resolved timezone/calendar windows.
/// # Errors
/// Returns an error for overlapping/invalid windows, mismatched granularity, duplicate sample
/// timestamps, samples outside all windows, or arithmetic overflow.
pub fn build_rollups(
    granularity: RollupGranularity,
    samples: &[RollupSample],
    windows: &[RollupWindow],
) -> Result<Vec<TelemetryRollup>, RollupBuildError> {
    validate_windows(granularity, windows)?;
    if samples
        .windows(2)
        .any(|pair| pair[0].timestamp_epoch_millis >= pair[1].timestamp_epoch_millis)
    {
        return Err(RollupBuildError::SampleOrdering);
    }
    let mut result = Vec::new();
    let mut sample_index = 0;
    for window in windows {
        let first_index = sample_index;
        while sample_index < samples.len()
            && samples[sample_index].timestamp_epoch_millis < window.end_epoch_millis
        {
            if samples[sample_index].timestamp_epoch_millis < window.start_epoch_millis {
                return Err(RollupBuildError::SampleOutsideWindows);
            }
            sample_index += 1;
        }
        let bucket = &samples[first_index..sample_index];
        if let Some(first_sample) = bucket.first() {
            let duration = u64::try_from(window.end_epoch_millis - window.start_epoch_millis)
                .map_err(|_| RollupBuildError::Overflow)?;
            let covered = bucket
                .iter()
                .try_fold(0_u64, |total, sample| {
                    total
                        .checked_add(sample.covered_millis)
                        .ok_or(RollupBuildError::Overflow)
                })?
                .min(duration);
            let coverage = covered
                .checked_mul(10_000)
                .ok_or(RollupBuildError::Overflow)?
                / duration;
            result.push(TelemetryRollup {
                window: window.clone(),
                sum: bucket.iter().map(|sample| i128::from(sample.value)).sum(),
                min: bucket
                    .iter()
                    .map(|sample| sample.value)
                    .min()
                    .unwrap_or_default(),
                max: bucket
                    .iter()
                    .map(|sample| sample.value)
                    .max()
                    .unwrap_or_default(),
                count: u64::try_from(bucket.len()).map_err(|_| RollupBuildError::Overflow)?,
                first: first_sample.value,
                last: bucket
                    .last()
                    .map_or(first_sample.value, |sample| sample.value),
                covered_millis: covered,
                coverage_basis_points: u16::try_from(coverage)
                    .map_err(|_| RollupBuildError::Overflow)?,
            });
        }
    }
    if sample_index != samples.len() {
        return Err(RollupBuildError::SampleOutsideWindows);
    }
    Ok(result)
}

fn validate_windows(
    granularity: RollupGranularity,
    windows: &[RollupWindow],
) -> Result<(), RollupBuildError> {
    for (index, window) in windows.iter().enumerate() {
        if window.granularity != granularity
            || window.end_epoch_millis <= window.start_epoch_millis
            || index > 0 && windows[index - 1].end_epoch_millis > window.start_epoch_millis
        {
            return Err(RollupBuildError::InvalidWindow);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RollupBuildError {
    #[error("rollup windows are invalid, overlapping, or use another granularity")]
    InvalidWindow,
    #[error("rollup samples must have unique ascending timestamps")]
    SampleOrdering,
    #[error("a rollup sample is outside the supplied timezone windows")]
    SampleOutsideWindows,
    #[error("rollup arithmetic overflowed")]
    Overflow,
}

impl From<std::num::TryFromIntError> for RollupBuildError {
    fn from(_: std::num::TryFromIntError) -> Self {
        Self::Overflow
    }
}
