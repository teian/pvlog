//! Modern HTTP adapter boundary over canonical ingestion and correction use cases.

use crate::{
    BatchIngestionMode, BatchIngestionResult, CorrectObservation, NormalizeObservation, PortError,
    VersionedObservation,
};
use async_trait::async_trait;
use pvlog_domain::{CanonicalObservation, ObservationId};
use thiserror::Error;

#[async_trait]
pub trait ModernTelemetryUseCases: Send + Sync {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<CanonicalObservation, ModernTelemetryError>;
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
    #[error("telemetry observation was not found")]
    NotFound,
    #[error("telemetry version conflict")]
    Conflict,
    #[error("ingestion is overloaded")]
    Overloaded { retry_after_seconds: u32 },
    #[error("telemetry persistence is unavailable")]
    Repository(PortError),
}
