//! Crash-safe overlay folding with generation-checked segment replacement.

use crate::{ArchivedSegmentBytes, SegmentCodecError, SegmentPoint, encode_segment_v1};
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OverlayFoldKey {
    pub system_id: Uuid,
    pub range_start: i64,
    pub range_end: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlayFoldState {
    pub phase: OverlayFoldPhase,
    pub expected_generation: u64,
    /// Only overlays at or below this revision belong to this fold attempt.
    pub overlay_revision_watermark: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OverlayFoldPhase {
    Pending,
    SegmentReplaced,
    OverlaysDeleted,
    Completed,
}

#[async_trait]
pub trait OverlayFoldRepository: Send + Sync {
    async fn prepare(&self, key: OverlayFoldKey) -> Result<OverlayFoldState, OverlayFoldError>;
    async fn merged_points(
        &self,
        key: OverlayFoldKey,
        overlay_revision_watermark: u64,
    ) -> Result<Vec<SegmentPoint>, OverlayFoldError>;
    /// Atomically installs the replacement only if the current generation matches.
    /// Replaying the same replacement after it was committed must return `true`.
    async fn replace_segment(
        &self,
        key: OverlayFoldKey,
        expected_generation: u64,
        replacement: &ArchivedSegmentBytes,
    ) -> Result<bool, OverlayFoldError>;
    async fn delete_overlays_through(
        &self,
        key: OverlayFoldKey,
        overlay_revision_watermark: u64,
    ) -> Result<(), OverlayFoldError>;
    async fn advance(
        &self,
        key: OverlayFoldKey,
        phase: OverlayFoldPhase,
    ) -> Result<(), OverlayFoldError>;
}

pub struct OverlayFoldService {
    repository: Arc<dyn OverlayFoldRepository>,
}

impl OverlayFoldService {
    #[must_use]
    pub fn new(repository: Arc<dyn OverlayFoldRepository>) -> Self {
        Self { repository }
    }

    /// Folds the prepared overlay snapshot into a replacement segment and resumes after crashes.
    /// # Errors
    /// Returns an error for invalid ranges, generation conflicts, encoding, or persistence failure.
    pub async fn fold(&self, key: OverlayFoldKey) -> Result<OverlayFoldPhase, OverlayFoldError> {
        if key.range_end <= key.range_start {
            return Err(OverlayFoldError::InvalidRange);
        }
        let state = self.repository.prepare(key).await?;
        let mut phase = state.phase;
        if phase == OverlayFoldPhase::Pending {
            let points = self
                .repository
                .merged_points(key, state.overlay_revision_watermark)
                .await?;
            let replacement = encode_segment_v1(key.system_id, &points)?;
            if !self
                .repository
                .replace_segment(key, state.expected_generation, &replacement)
                .await?
            {
                return Err(OverlayFoldError::GenerationConflict);
            }
            phase = OverlayFoldPhase::SegmentReplaced;
            self.repository.advance(key, phase).await?;
        }
        if phase == OverlayFoldPhase::SegmentReplaced {
            self.repository
                .delete_overlays_through(key, state.overlay_revision_watermark)
                .await?;
            phase = OverlayFoldPhase::OverlaysDeleted;
            self.repository.advance(key, phase).await?;
        }
        if phase == OverlayFoldPhase::OverlaysDeleted {
            phase = OverlayFoldPhase::Completed;
            self.repository.advance(key, phase).await?;
        }
        Ok(phase)
    }
}

#[derive(Debug, Error)]
pub enum OverlayFoldError {
    #[error("overlay fold range is invalid")]
    InvalidRange,
    #[error("segment generation changed while folding overlays")]
    GenerationConflict,
    #[error("segment codec failed: {0}")]
    Codec(#[from] SegmentCodecError),
    #[error("overlay fold persistence failed: {0}")]
    Persistence(&'static str),
}
