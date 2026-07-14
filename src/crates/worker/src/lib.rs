//! Background job execution for `PVLog`.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pvlog_domain::{JobId, ProviderId, SystemId, TimeRange, WeatherDataRunId};
use pvlog_storage::{
    DatabaseTarget, JobRecord, JobRetryDisposition, OperationalRepository,
    OperationalRepositoryError, ProbeError, YieldResultRepository, probe_database,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Semaphore;

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

const WEATHER_POLL_JOB: &str = "weather_provider_poll";
const YIELD_CALCULATION_JOB: &str = "yield_forecast_calculation";
const YIELD_REBUILD_JOB: &str = "yield_interval_rebuild";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherPollJobPayload {
    pub provider_id: ProviderId,
    pub system_id: SystemId,
    pub range_start: i64,
    pub range_end: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct YieldCalculationJobPayload {
    pub system_id: SystemId,
    pub weather_run_id: WeatherDataRunId,
    pub range_start: i64,
    pub range_end: i64,
    pub configuration_digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct YieldRebuildJobPayload {
    pub system_id: SystemId,
    pub range_start: i64,
    pub range_end: i64,
    pub invalidation_ids: Vec<uuid::Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YieldJobPolicy {
    pub lease_milliseconds: i64,
    pub base_retry_milliseconds: i64,
    pub maximum_retry_milliseconds: i64,
    pub maximum_attempts: i32,
    pub concurrency: usize,
}

impl Default for YieldJobPolicy {
    fn default() -> Self {
        Self {
            lease_milliseconds: 30_000,
            base_retry_milliseconds: 1_000,
            maximum_retry_milliseconds: 300_000,
            maximum_attempts: 5,
            concurrency: 2,
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("yield job failed with safe code {safe_code}")]
pub struct YieldJobExecutionError {
    pub safe_code: &'static str,
}

#[async_trait]
pub trait YieldJobHandler: Send + Sync {
    async fn poll_weather(
        &self,
        payload: WeatherPollJobPayload,
    ) -> Result<(), YieldJobExecutionError>;

    async fn calculate_yield(
        &self,
        payload: YieldCalculationJobPayload,
    ) -> Result<(), YieldJobExecutionError>;

    async fn rebuild_yield_intervals(
        &self,
        payload: YieldRebuildJobPayload,
    ) -> Result<(), YieldJobExecutionError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YieldJobOutcome {
    Completed,
    RetryAt(i64),
    DeadLetter,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct YieldMetricsSnapshot {
    pub provider_fresh_runs: u64,
    pub provider_stale_runs: u64,
    pub provider_failures: u64,
    pub forecast_age_milliseconds: Option<u64>,
    pub calculation_lag_milliseconds: Option<u64>,
    pub calculations_completed: u64,
    pub calculations_failed: u64,
    pub invalidation_backlog: u64,
    pub complete_results: u64,
    pub partial_results: u64,
    pub unavailable_results: u64,
    pub model_identifier: Option<String>,
    pub model_revision: Option<u16>,
    pub dead_letters: u64,
}

#[derive(Debug, Default)]
pub struct YieldOperationalMetrics {
    snapshot: Mutex<YieldMetricsSnapshot>,
}

impl YieldOperationalMetrics {
    pub fn record_provider_result(&self, freshness: pvlog_application::ExternalDataFreshness) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            match freshness {
                pvlog_application::ExternalDataFreshness::Fresh => {
                    snapshot.provider_fresh_runs = snapshot.provider_fresh_runs.saturating_add(1);
                }
                pvlog_application::ExternalDataFreshness::StaleDegraded => {
                    snapshot.provider_stale_runs = snapshot.provider_stale_runs.saturating_add(1);
                }
            }
        }
    }

    pub fn record_provider_failure(&self) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.provider_failures = snapshot.provider_failures.saturating_add(1);
        }
    }

    pub fn record_calculation(
        &self,
        completed: bool,
        lag_milliseconds: u64,
        completeness: &pvlog_domain::ForecastCompleteness,
        model: &pvlog_domain::ModelVersion,
    ) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.calculation_lag_milliseconds = Some(lag_milliseconds);
            if completed {
                snapshot.calculations_completed = snapshot.calculations_completed.saturating_add(1);
            } else {
                snapshot.calculations_failed = snapshot.calculations_failed.saturating_add(1);
            }
            match completeness {
                pvlog_domain::ForecastCompleteness::Complete => {
                    snapshot.complete_results = snapshot.complete_results.saturating_add(1);
                }
                pvlog_domain::ForecastCompleteness::Partial { .. } => {
                    snapshot.partial_results = snapshot.partial_results.saturating_add(1);
                }
                pvlog_domain::ForecastCompleteness::Unavailable { .. } => {
                    snapshot.unavailable_results = snapshot.unavailable_results.saturating_add(1);
                }
            }
            snapshot.model_identifier = Some(model.identifier.clone());
            snapshot.model_revision = Some(model.revision);
        }
    }

    pub fn set_queue_diagnostics(
        &self,
        invalidation_backlog: u64,
        dead_letters: u64,
        forecast_age_milliseconds: Option<u64>,
    ) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.invalidation_backlog = invalidation_backlog;
            snapshot.dead_letters = dead_letters;
            snapshot.forecast_age_milliseconds = forecast_age_milliseconds;
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> YieldMetricsSnapshot {
        self.snapshot.lock().map_or_else(
            |_| YieldMetricsSnapshot::default(),
            |snapshot| snapshot.clone(),
        )
    }
}

pub struct YieldJobCoordinator {
    repository: Arc<dyn OperationalRepository>,
    policy: YieldJobPolicy,
    permits: Semaphore,
}

impl YieldJobCoordinator {
    /// Creates a bounded job coordinator.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid lease, retry, attempt, or concurrency policy.
    pub fn new(
        repository: Arc<dyn OperationalRepository>,
        policy: YieldJobPolicy,
    ) -> Result<Self, YieldJobError> {
        if policy.lease_milliseconds <= 0
            || policy.base_retry_milliseconds <= 0
            || policy.maximum_retry_milliseconds < policy.base_retry_milliseconds
            || policy.maximum_attempts <= 0
            || policy.concurrency == 0
        {
            return Err(YieldJobError::InvalidPolicy);
        }
        Ok(Self {
            repository,
            permits: Semaphore::new(policy.concurrency),
            policy,
        })
    }

    /// Enqueues an idempotent provider poll carrying only safe identifiers and bounds.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid idempotency or persistence failure.
    pub async fn enqueue_weather_poll(
        &self,
        payload: WeatherPollJobPayload,
        idempotency_key: String,
        now: i64,
    ) -> Result<JobId, YieldJobError> {
        self.enqueue(WEATHER_POLL_JOB, payload, idempotency_key, now)
            .await
    }

    /// Enqueues an idempotent yield calculation carrying immutable input references.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid idempotency or persistence failure.
    pub async fn enqueue_yield_calculation(
        &self,
        payload: YieldCalculationJobPayload,
        idempotency_key: String,
        now: i64,
    ) -> Result<JobId, YieldJobError> {
        self.enqueue(YIELD_CALCULATION_JOB, payload, idempotency_key, now)
            .await
    }

    /// Coalesces intersecting pending invalidations into one bounded rebuild job.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid range conversion, repository access, or job persistence.
    pub async fn enqueue_pending_rebuild(
        &self,
        invalidations: &dyn YieldResultRepository,
        system_id: SystemId,
        range: TimeRange,
        limit: u32,
        now: i64,
    ) -> Result<Option<JobId>, YieldJobError> {
        let mut pending = invalidations
            .pending_invalidations(system_id, range, limit)
            .await?;
        if pending.is_empty() {
            return Ok(None);
        }
        pending.sort_by_key(|item| (item.range.start, item.range.end, item.id));
        let start = pending
            .iter()
            .map(|item| item.range.start.epoch_millis())
            .min()
            .ok_or(YieldJobError::InvalidPayload)?;
        let end = pending
            .iter()
            .map(|item| item.range.end.epoch_millis())
            .max()
            .ok_or(YieldJobError::InvalidPayload)?;
        let invalidation_ids = pending.iter().map(|item| item.id).collect::<Vec<_>>();
        let idempotency_key = format!(
            "yield-rebuild:{system_id}:{start}:{end}:{}",
            invalidation_ids
                .iter()
                .map(uuid::Uuid::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        self.enqueue(
            YIELD_REBUILD_JOB,
            YieldRebuildJobPayload {
                system_id,
                range_start: i64::try_from(start).map_err(|_| YieldJobError::InvalidPayload)?,
                range_end: i64::try_from(end).map_err(|_| YieldJobError::InvalidPayload)?,
                invalidation_ids,
            },
            idempotency_key,
            now,
        )
        .await
        .map(Some)
    }

    async fn enqueue<T: Serialize>(
        &self,
        kind: &str,
        payload: T,
        idempotency_key: String,
        now: i64,
    ) -> Result<JobId, YieldJobError> {
        if idempotency_key.trim().is_empty() {
            return Err(YieldJobError::InvalidPayload);
        }
        let id = deterministic_job_id(
            self.repository.account_id().as_uuid(),
            kind,
            &idempotency_key,
        )?;
        if self.repository.job(id).await?.is_some() {
            return Ok(id);
        }
        self.repository
            .save_job(&JobRecord {
                id,
                job_kind: kind.to_owned(),
                state: "pending".to_owned(),
                payload: serde_json::to_value(payload)?,
                idempotency_key: Some(idempotency_key),
                priority: 0,
                attempt_count: 0,
                max_attempts: self.policy.maximum_attempts,
                available_at: now,
                created_at: now,
                updated_at: now,
            })
            .await?;
        Ok(id)
    }

    /// Leases and executes at most one job while holding a concurrency permit.
    ///
    /// # Errors
    ///
    /// Returns an error when leasing, payload decoding, completion, or retry persistence fails.
    pub async fn execute_one(
        &self,
        owner: &str,
        now: i64,
        handler: &dyn YieldJobHandler,
    ) -> Result<Option<YieldJobOutcome>, YieldJobError> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| YieldJobError::CoordinatorClosed)?;
        let lease_until = now
            .checked_add(self.policy.lease_milliseconds)
            .ok_or(YieldJobError::InvalidPolicy)?;
        let Some(lease) = self.repository.lease_job(owner, now, lease_until).await? else {
            return Ok(None);
        };
        let execution = match lease.job.job_kind.as_str() {
            WEATHER_POLL_JOB => {
                handler
                    .poll_weather(serde_json::from_value(lease.job.payload.clone())?)
                    .await
            }
            YIELD_CALCULATION_JOB => {
                handler
                    .calculate_yield(serde_json::from_value(lease.job.payload.clone())?)
                    .await
            }
            YIELD_REBUILD_JOB => {
                handler
                    .rebuild_yield_intervals(serde_json::from_value(lease.job.payload.clone())?)
                    .await
            }
            _ => Err(YieldJobExecutionError {
                safe_code: "unsupported_job_kind",
            }),
        };
        match execution {
            Ok(()) => {
                self.repository
                    .complete_job(lease.job.id, owner, now)
                    .await?;
                Ok(Some(YieldJobOutcome::Completed))
            }
            Err(error) => Ok(Some(
                match self
                    .repository
                    .retry_job(
                        lease.job.id,
                        owner,
                        now,
                        self.policy.base_retry_milliseconds,
                        self.policy.maximum_retry_milliseconds,
                        error.safe_code,
                    )
                    .await?
                {
                    JobRetryDisposition::RetryAt(at) => YieldJobOutcome::RetryAt(at),
                    JobRetryDisposition::DeadLetter => YieldJobOutcome::DeadLetter,
                },
            )),
        }
    }
}

fn deterministic_job_id(
    account_id: uuid::Uuid,
    kind: &str,
    idempotency_key: &str,
) -> Result<JobId, YieldJobError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(account_id.as_bytes());
    hasher.update(kind.as_bytes());
    hasher.update(&[0]);
    hasher.update(idempotency_key.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    JobId::from_uuid(uuid::Uuid::from_bytes(bytes)).map_err(|_| YieldJobError::InvalidPayload)
}

#[derive(Debug, Error)]
pub enum YieldJobError {
    #[error("yield job policy is invalid")]
    InvalidPolicy,
    #[error("yield job payload is invalid")]
    InvalidPayload,
    #[error("yield job coordinator is closed")]
    CoordinatorClosed,
    #[error("yield job persistence failed: {0}")]
    Repository(#[from] OperationalRepositoryError),
    #[error("yield result persistence failed: {0}")]
    YieldRepository(#[from] pvlog_storage::YieldResultRepositoryError),
    #[error("yield job payload serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}
