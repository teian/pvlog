//! Leased, resumable system-day compaction with cleanup after verification.

use crate::{ArchivedSegmentBytes, SegmentCodecError, SegmentPoint, encode_segment_v1};
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompactionKey {
    pub system_id: Uuid,
    pub range_start: i64,
    pub range_end: i64,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CompactionPhase {
    Pending,
    SegmentWritten,
    RollupsWritten,
    Verified,
    HotRowsDeleted,
    Completed,
}

#[async_trait]
pub trait CompactionRepository: Send + Sync {
    async fn acquire_lease(
        &self,
        key: CompactionKey,
        owner: &str,
        lease_expires_at: i64,
    ) -> Result<Option<CompactionPhase>, CompactionError>;
    async fn stable_points(&self, key: CompactionKey)
    -> Result<Vec<SegmentPoint>, CompactionError>;
    async fn write_segment(
        &self,
        key: CompactionKey,
        segment: &ArchivedSegmentBytes,
    ) -> Result<(), CompactionError>;
    async fn write_rollups(&self, key: CompactionKey) -> Result<(), CompactionError>;
    async fn verify_segment_and_rollups(&self, key: CompactionKey)
    -> Result<bool, CompactionError>;
    async fn delete_redundant_hot_rows(&self, key: CompactionKey) -> Result<(), CompactionError>;
    async fn advance(
        &self,
        key: CompactionKey,
        phase: CompactionPhase,
    ) -> Result<(), CompactionError>;
    async fn release_lease(&self, key: CompactionKey, owner: &str) -> Result<(), CompactionError>;
}

pub struct CompactionService {
    repository: Arc<dyn CompactionRepository>,
    lease_millis: i64,
}
impl CompactionService {
    #[must_use]
    pub fn new(repository: Arc<dyn CompactionRepository>, lease_millis: i64) -> Self {
        Self {
            repository,
            lease_millis,
        }
    }
    /// Compacts or resumes one stable interval through verified cleanup.
    /// # Errors
    /// Returns an error for unavailable leases, invalid ranges, encoding, durability verification, or persistence failure.
    pub async fn compact(
        &self,
        key: CompactionKey,
        owner: &str,
        now: i64,
    ) -> Result<CompactionPhase, CompactionError> {
        if key.range_end <= key.range_start || owner.trim().is_empty() {
            return Err(CompactionError::InvalidRequest);
        }
        let lease_expires_at = now
            .checked_add(self.lease_millis)
            .ok_or(CompactionError::InvalidRequest)?;
        let Some(mut phase) = self
            .repository
            .acquire_lease(key, owner, lease_expires_at)
            .await?
        else {
            return Err(CompactionError::LeaseUnavailable);
        };
        let result = async {
            if phase == CompactionPhase::Pending {
                let points = self.repository.stable_points(key).await?;
                let segment = encode_segment_v1(key.system_id, &points)?;
                self.repository.write_segment(key, &segment).await?;
                phase = CompactionPhase::SegmentWritten;
                self.repository.advance(key, phase).await?;
            }
            if phase == CompactionPhase::SegmentWritten {
                self.repository.write_rollups(key).await?;
                phase = CompactionPhase::RollupsWritten;
                self.repository.advance(key, phase).await?;
            }
            if phase == CompactionPhase::RollupsWritten {
                if !self.repository.verify_segment_and_rollups(key).await? {
                    return Err(CompactionError::Verification);
                }
                phase = CompactionPhase::Verified;
                self.repository.advance(key, phase).await?;
            }
            if phase == CompactionPhase::Verified {
                self.repository.delete_redundant_hot_rows(key).await?;
                phase = CompactionPhase::HotRowsDeleted;
                self.repository.advance(key, phase).await?;
            }
            if phase == CompactionPhase::HotRowsDeleted {
                phase = CompactionPhase::Completed;
                self.repository.advance(key, phase).await?;
            }
            Ok(phase)
        }
        .await;
        let release = self.repository.release_lease(key, owner).await;
        result.and(release.map(|()| phase))
    }
}

#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("compaction request is invalid")]
    InvalidRequest,
    #[error("compaction lease is unavailable")]
    LeaseUnavailable,
    #[error("segment or rollup verification failed")]
    Verification,
    #[error("segment codec failed: {0}")]
    Codec(#[from] SegmentCodecError),
    #[error("compaction persistence failed: {0}")]
    Persistence(&'static str),
}
