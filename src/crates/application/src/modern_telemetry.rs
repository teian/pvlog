//! Modern HTTP adapter boundary over canonical ingestion and correction use cases.

use crate::{
    BatchIngestionMode, BatchIngestionResult, CorrectObservation, NormalizeObservation, PortError,
    RejectedChannel, VersionedObservation,
};
use async_trait::async_trait;
use pvlog_domain::{CanonicalObservation, ObservationId};
use thiserror::Error;

/// An accepted reading together with the channel values it could not keep.
#[derive(Clone, Debug)]
pub struct IngestedObservation {
    pub observation: CanonicalObservation,
    /// Channel values that contradicted their channel's definition. The rest
    /// of the reading was stored.
    pub rejected_channels: Vec<RejectedChannel>,
}

impl From<CanonicalObservation> for IngestedObservation {
    fn from(observation: CanonicalObservation) -> Self {
        Self {
            observation,
            rejected_channels: Vec::new(),
        }
    }
}

#[async_trait]
pub trait ModernTelemetryUseCases: Send + Sync {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<IngestedObservation, ModernTelemetryError>;
    async fn ingest_batch(
        &self,
        commands: Vec<NormalizeObservation>,
        mode: BatchIngestionMode,
    ) -> Result<BatchIngestionResult, ModernTelemetryError>;
    async fn correct(
        &self,
        command: CorrectObservation,
    ) -> Result<VersionedObservation, ModernTelemetryError>;
    async fn delete(
        &self,
        command: CorrectObservation,
    ) -> Result<ObservationId, ModernTelemetryError>;
}

#[derive(Debug, Error)]
pub enum ModernTelemetryError {
    #[error("telemetry request is invalid")]
    Invalid,
    /// One member of the request is invalid; carries its wire name.
    #[error("telemetry field {0} is invalid")]
    InvalidField(&'static str),
    #[error("telemetry observation was not found")]
    NotFound,
    #[error("telemetry version conflict")]
    Conflict,
    #[error("ingestion is overloaded")]
    Overloaded { retry_after_seconds: u32 },
    #[error("telemetry persistence is unavailable")]
    Repository(PortError),
}
