//! Optimistic hot-row and archived-overlay correction orchestration.

use crate::PortError;
use async_trait::async_trait;
use pvlog_domain::{MeasurementValues, ObservationId, SystemId, UserId};
use serde::Serialize;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VersionedObservation {
    pub id: ObservationId,
    pub system_id: SystemId,
    pub values: Option<MeasurementValues>,
    pub version: u64,
    pub archived: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrectObservation {
    pub observation_id: ObservationId,
    pub system_id: SystemId,
    pub actor: UserId,
    pub expected_version: u64,
    pub replacement: Option<MeasurementValues>,
    pub reason: String,
}

#[async_trait]
pub trait CorrectionRepository: Send + Sync {
    async fn observation(
        &self,
        id: ObservationId,
    ) -> Result<Option<VersionedObservation>, PortError>;
    async fn replace_hot(&self, command: &CorrectObservation) -> Result<bool, PortError>;
    async fn append_archived_overlay(
        &self,
        command: &CorrectObservation,
    ) -> Result<bool, PortError>;
    async fn enqueue_rebuild(
        &self,
        system_id: SystemId,
        observation_id: ObservationId,
    ) -> Result<(), PortError>;
    async fn visible_observation(
        &self,
        id: ObservationId,
    ) -> Result<Option<VersionedObservation>, PortError>;
}

pub struct CorrectionService {
    repository: Arc<dyn CorrectionRepository>,
}
impl CorrectionService {
    #[must_use]
    pub fn new(repository: Arc<dyn CorrectionRepository>) -> Self {
        Self { repository }
    }
    /// Applies a replacement or deletion and returns its immediately visible merged result.
    /// # Errors
    /// Returns an error for invalid reason, missing observation, version conflict, or persistence failure.
    pub async fn correct(
        &self,
        command: CorrectObservation,
    ) -> Result<VersionedObservation, ObservationCorrectionError> {
        if command.reason.trim().is_empty() {
            return Err(ObservationCorrectionError::InvalidReason);
        }
        let current = self
            .repository
            .observation(command.observation_id)
            .await
            .map_err(ObservationCorrectionError::Repository)?
            .ok_or(ObservationCorrectionError::NotFound)?;
        if current.system_id != command.system_id || current.version != command.expected_version {
            return Err(ObservationCorrectionError::Conflict);
        }
        let applied = if current.archived {
            self.repository.append_archived_overlay(&command).await
        } else {
            self.repository.replace_hot(&command).await
        }
        .map_err(ObservationCorrectionError::Repository)?;
        if !applied {
            return Err(ObservationCorrectionError::Conflict);
        }
        self.repository
            .enqueue_rebuild(command.system_id, command.observation_id)
            .await
            .map_err(ObservationCorrectionError::Repository)?;
        self.repository
            .visible_observation(command.observation_id)
            .await
            .map_err(ObservationCorrectionError::Repository)?
            .ok_or(ObservationCorrectionError::NotFound)
    }
}

#[derive(Debug, Error)]
pub enum ObservationCorrectionError {
    #[error("correction reason is required")]
    InvalidReason,
    #[error("observation was not found")]
    NotFound,
    #[error("observation version conflict")]
    Conflict,
    #[error("correction persistence is unavailable")]
    Repository(PortError),
}
