//! Resumable `SBFspot` `SQLite` to `PVLog` telemetry uploader.

mod checkpoint;
mod client;
mod source;

pub use checkpoint::{Checkpoint, CheckpointStore};
pub use client::{ApiError, PvlogClient, PvlogClientConfig};
pub use source::{Reading, SbfspotError, SbfspotSource};

use serde::Serialize;
use std::{path::PathBuf, time::Duration};
use thiserror::Error;

/// Runtime settings for one catch-up pass.
#[derive(Clone, Debug)]
pub struct PushConfig {
    pub batch_size: usize,
    pub checkpoint_path: PathBuf,
    pub initial_timestamp: i64,
    pub dry_run: bool,
}

/// Summary returned after the source has been caught up.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushSummary {
    pub batches: u64,
    pub observations: u64,
    pub last_timestamp: Option<i64>,
}

/// Runs repeated catch-up passes until shutdown is requested.
///
/// # Errors
/// Returns the first source, checkpoint, or API error encountered.
pub async fn run_service(
    source: &SbfspotSource,
    client: &PvlogClient,
    config: &PushConfig,
    poll_interval: Duration,
) -> Result<(), PushError> {
    loop {
        let summary = push_pending(source, client, config).await?;
        tracing::info!(
            batches = summary.batches,
            observations = summary.observations,
            last_timestamp = summary.last_timestamp,
            "SBFspot catch-up pass complete"
        );
        tokio::select! {
            () = tokio::time::sleep(poll_interval) => {}
            result = tokio::signal::ctrl_c() => {
                result.map_err(PushError::Shutdown)?;
                tracing::info!("shutdown requested");
                return Ok(());
            }
        }
    }
}

/// Sends all observations newer than the durable checkpoint.
///
/// # Errors
/// Returns an error without advancing the checkpoint if a batch is not accepted.
pub async fn push_pending(
    source: &SbfspotSource,
    client: &PvlogClient,
    config: &PushConfig,
) -> Result<PushSummary, PushError> {
    if !(1..=1000).contains(&config.batch_size) {
        return Err(PushError::InvalidBatchSize(config.batch_size));
    }
    let store = CheckpointStore::new(config.checkpoint_path.clone());
    let mut cursor = store
        .load()
        .await?
        .map_or(config.initial_timestamp, |state| state.last_timestamp);
    let mut summary = PushSummary::default();

    loop {
        let readings = source.read_after(cursor, config.batch_size).await?;
        if readings.is_empty() {
            return Ok(summary);
        }
        let last_timestamp = readings
            .last()
            .map(|reading| reading.timestamp)
            .ok_or(PushError::EmptyBatch)?;

        if !config.dry_run {
            client.send(&readings).await?;
            store.save(Checkpoint { last_timestamp }).await?;
        }
        cursor = last_timestamp;
        summary.batches += 1;
        summary.observations += u64::try_from(readings.len()).unwrap_or(u64::MAX);
        summary.last_timestamp = Some(last_timestamp);
    }
}

#[derive(Debug, Error)]
pub enum PushError {
    #[error("batch size must be between 1 and 1000, got {0}")]
    InvalidBatchSize(usize),
    #[error("SBFspot returned an unexpected empty batch")]
    EmptyBatch,
    #[error(transparent)]
    Source(#[from] SbfspotError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Checkpoint(#[from] checkpoint::CheckpointError),
    #[error("failed to install the shutdown handler: {0}")]
    Shutdown(std::io::Error),
}
