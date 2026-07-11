//! Bounded atomic and partial telemetry batch orchestration.

use crate::PortError;
use async_trait::async_trait;
use pvlog_domain::CanonicalObservation;
use serde::Serialize;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchIngestionMode {
    Atomic,
    Partial,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchItemStatus {
    Inserted,
    Duplicate,
    Invalid,
    Failed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BatchItemOutcome {
    pub index: usize,
    pub status: BatchItemStatus,
    pub code: Option<&'static str>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BatchIngestionResult {
    pub outcomes: Vec<BatchItemOutcome>,
}

#[async_trait]
pub trait BatchIngestionRepository: Send + Sync {
    async fn validate(&self, observation: &CanonicalObservation) -> Result<(), &'static str>;
    async fn insert_atomic(
        &self,
        observations: &[CanonicalObservation],
    ) -> Result<Vec<BatchItemStatus>, PortError>;
    async fn insert_one(
        &self,
        observation: &CanonicalObservation,
    ) -> Result<BatchItemStatus, PortError>;
}

pub struct BatchIngestionService {
    repository: Arc<dyn BatchIngestionRepository>,
    maximum_items: usize,
    maximum_body_bytes: usize,
}
impl BatchIngestionService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn BatchIngestionRepository>,
        maximum_items: usize,
        maximum_body_bytes: usize,
    ) -> Self {
        Self {
            repository,
            maximum_items,
            maximum_body_bytes,
        }
    }
    /// Processes one bounded batch with stable input-index outcomes.
    /// # Errors
    /// Returns an error when request bounds are exceeded or an atomic write fails.
    pub async fn ingest(
        &self,
        observations: Vec<CanonicalObservation>,
        body_bytes: usize,
        mode: BatchIngestionMode,
    ) -> Result<BatchIngestionResult, BatchIngestionError> {
        if observations.is_empty()
            || observations.len() > self.maximum_items
            || body_bytes > self.maximum_body_bytes
        {
            return Err(BatchIngestionError::RequestLimit);
        }
        let mut validation = Vec::with_capacity(observations.len());
        for (index, observation) in observations.iter().enumerate() {
            if let Err(code) = self.repository.validate(observation).await {
                validation.push(BatchItemOutcome {
                    index,
                    status: BatchItemStatus::Invalid,
                    code: Some(code),
                });
            }
        }
        if mode == BatchIngestionMode::Atomic {
            if !validation.is_empty() {
                return Ok(BatchIngestionResult {
                    outcomes: validation,
                });
            }
            let statuses = self
                .repository
                .insert_atomic(&observations)
                .await
                .map_err(BatchIngestionError::Repository)?;
            if statuses.len() != observations.len() {
                return Err(BatchIngestionError::InvalidRepositoryOutcome);
            }
            return Ok(BatchIngestionResult {
                outcomes: statuses
                    .into_iter()
                    .enumerate()
                    .map(|(index, status)| BatchItemOutcome {
                        index,
                        status,
                        code: None,
                    })
                    .collect(),
            });
        }
        let mut outcomes = Vec::with_capacity(observations.len());
        for (index, observation) in observations.iter().enumerate() {
            if let Some(invalid) = validation.iter().find(|outcome| outcome.index == index) {
                outcomes.push(invalid.clone());
                continue;
            }
            let status = self
                .repository
                .insert_one(observation)
                .await
                .unwrap_or(BatchItemStatus::Failed);
            outcomes.push(BatchItemOutcome {
                index,
                status,
                code: (status == BatchItemStatus::Failed).then_some("persistence_failed"),
            });
        }
        Ok(BatchIngestionResult { outcomes })
    }
}

#[derive(Debug, Error)]
pub enum BatchIngestionError {
    #[error("batch request exceeds configured limits")]
    RequestLimit,
    #[error("atomic batch persistence failed")]
    Repository(PortError),
    #[error("batch repository returned an invalid outcome count")]
    InvalidRepositoryOutcome,
}
