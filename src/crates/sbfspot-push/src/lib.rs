//! Resumable `SBFspot` `SQLite` to `PVLog` telemetry uploader.

mod checkpoint;
mod client;
mod source;

pub use checkpoint::{Checkpoint, CheckpointStore};
pub use client::{ApiError, PvlogClient, PvlogClientConfig};
pub use source::{
    ChannelValue, DeviceEvent, DeviceIdentity, EventPosition, Reading, SbfspotError, SbfspotSource,
    SourceGroup,
};

use serde::Serialize;
use std::{path::PathBuf, time::Duration};
use thiserror::Error;

/// Most events `PVLog` accepts in one request.
const EVENTS_PER_REQUEST: usize = 1000;

/// Runtime settings for one catch-up pass.
#[derive(Clone, Debug)]
pub struct PushConfig {
    pub batch_size: usize,
    pub checkpoint_path: PathBuf,
    pub initial_timestamp: i64,
    pub dry_run: bool,
}

/// Summary returned after the source has been caught up.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushSummary {
    pub batches: u64,
    pub observations: u64,
    pub last_timestamp: Option<i64>,
    /// Events uploaded in this pass.
    pub events: u64,
    /// Groups of values the database does not hold. Everything else was delivered.
    pub unavailable: Vec<SourceGroup>,
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
    let unavailable = source.unavailable_groups();
    if !unavailable.is_empty() {
        tracing::warn!(
            ?unavailable,
            "the SBFspot database lacks optional tables; their values are not delivered"
        );
    }
    loop {
        let summary = push_pending(source, client, config).await?;
        tracing::info!(
            batches = summary.batches,
            observations = summary.observations,
            events = summary.events,
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

/// Sends all observations and events newer than the durable checkpoint.
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
    let mut checkpoint = store.load().await?.unwrap_or(Checkpoint {
        last_timestamp: config.initial_timestamp,
        last_event: None,
    });
    let mut summary = PushSummary {
        unavailable: source.unavailable_groups(),
        ..PushSummary::default()
    };

    let mut readings = source
        .read_after(checkpoint.last_timestamp, config.batch_size)
        .await?;
    while let Some(last_timestamp) = readings.last().map(|reading| reading.timestamp) {
        // The next batch is read while this one is sent; the checkpoint still
        // advances only once PVLog accepted the batch.
        let (sent, next) = tokio::join!(
            async {
                if config.dry_run {
                    Ok(())
                } else {
                    client.send(&readings).await
                }
            },
            source.read_after(last_timestamp, config.batch_size),
        );
        sent?;
        checkpoint.last_timestamp = last_timestamp;
        if !config.dry_run {
            store.save(checkpoint).await?;
        }
        summary.batches += 1;
        summary.observations += u64::try_from(readings.len()).unwrap_or(u64::MAX);
        summary.last_timestamp = Some(last_timestamp);
        readings = next?;
    }

    // Events from before the start time are history nobody asked for.
    let mut position = checkpoint.last_event.unwrap_or(EventPosition {
        timestamp: config.initial_timestamp,
        serial: i64::MAX,
        entry_id: i64::MAX,
    });
    loop {
        let events = source
            .read_events_after(position, EVENTS_PER_REQUEST.min(config.batch_size))
            .await?;
        let Some(last) = events.last().map(|event| event.position) else {
            break;
        };
        position = last;
        checkpoint.last_event = Some(position);
        if !config.dry_run {
            client.send_events(&events).await?;
            store.save(checkpoint).await?;
        }
        summary.events += u64::try_from(events.len()).unwrap_or(u64::MAX);
    }
    Ok(summary)
}

#[derive(Debug, Error)]
pub enum PushError {
    #[error("batch size must be between 1 and 1000, got {0}")]
    InvalidBatchSize(usize),
    #[error(transparent)]
    Source(#[from] SbfspotError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Checkpoint(#[from] checkpoint::CheckpointError),
    #[error("failed to install the shutdown handler: {0}")]
    Shutdown(std::io::Error),
}
