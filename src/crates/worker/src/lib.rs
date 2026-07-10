//! Background job execution for `PVLog`.

#![forbid(unsafe_code)]

use pvlog_storage::{DatabaseTarget, ProbeError, probe_database};

/// Performs one worker readiness cycle against its configured database.
///
/// # Errors
///
/// Returns an error if the worker cannot reach every database it is responsible for.
pub async fn run_once(target: &DatabaseTarget) -> Result<(), ProbeError> {
    probe_database(target).await?;
    tracing::info!(database = ?target, "worker database readiness cycle completed");
    Ok(())
}
