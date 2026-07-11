//! Deterministic merged raw telemetry reads across hot, archived, and correction data.

use std::collections::{BTreeMap, HashMap};
use thiserror::Error;
use uuid::Uuid;

/// Physical source from which a raw observation was read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawObservationOrigin {
    Segment,
    Hot,
    Overlay,
}

/// One logical raw observation candidate before source precedence is applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawObservation {
    pub observation_id: Uuid,
    pub timestamp_epoch_millis: i64,
    pub generation_power_watts: Option<i64>,
    pub extended: BTreeMap<Uuid, i64>,
    pub source_kind: String,
    pub source_reference: String,
    pub received_at_epoch_millis: i64,
    pub quality_flags: u32,
    pub revision: u64,
    pub deleted: bool,
    pub origin: RawObservationOrigin,
    /// Segment schema version, present only for archived candidates.
    pub segment_version: Option<u16>,
}

/// Merges raw candidates using overlay > hot > segment precedence.
///
/// Candidates are deduplicated by stable observation identity. Within one physical source the
/// highest revision wins. Tombstone overlays suppress the logical observation. Results are
/// ordered by timestamp and observation ID so pagination does not depend on load order.
/// # Errors
/// Returns an error when an archived candidate uses an unsupported segment version.
pub fn merge_raw_observations(
    segments: impl IntoIterator<Item = RawObservation>,
    hot: impl IntoIterator<Item = RawObservation>,
    overlays: impl IntoIterator<Item = RawObservation>,
) -> Result<Vec<RawObservation>, MergedReadError> {
    let mut selected = HashMap::<Uuid, RawObservation>::new();
    for (expected_origin, candidates) in [
        (
            RawObservationOrigin::Segment,
            segments.into_iter().collect::<Vec<_>>(),
        ),
        (RawObservationOrigin::Hot, hot.into_iter().collect()),
        (
            RawObservationOrigin::Overlay,
            overlays.into_iter().collect(),
        ),
    ] {
        for candidate in candidates {
            if candidate.origin != expected_origin {
                return Err(MergedReadError::OriginMismatch);
            }
            if candidate.origin == RawObservationOrigin::Segment
                && candidate.segment_version != Some(1)
            {
                return Err(MergedReadError::UnsupportedSegmentVersion(
                    candidate.segment_version,
                ));
            }
            let replace = selected
                .get(&candidate.observation_id)
                .is_none_or(|current| {
                    precedence(candidate.origin) > precedence(current.origin)
                        || (candidate.origin == current.origin
                            && candidate.revision > current.revision)
                });
            if replace {
                selected.insert(candidate.observation_id, candidate);
            }
        }
    }
    let mut merged = selected
        .into_values()
        .filter(|candidate| !candidate.deleted)
        .collect::<Vec<_>>();
    merged.sort_unstable_by_key(|candidate| {
        (candidate.timestamp_epoch_millis, candidate.observation_id)
    });
    Ok(merged)
}

const fn precedence(origin: RawObservationOrigin) -> u8 {
    match origin {
        RawObservationOrigin::Segment => 0,
        RawObservationOrigin::Hot => 1,
        RawObservationOrigin::Overlay => 2,
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MergedReadError {
    #[error("raw observation was supplied in the wrong physical source collection")]
    OriginMismatch,
    #[error("unsupported archived segment version: {0:?}")]
    UnsupportedSegmentVersion(Option<u16>),
}
