//! Background job execution for `PVLog`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_domain::{JobId, ProviderId, SystemId, WeatherDataRunId};
use pvlog_storage::{
    DatabaseTarget, JobRecord, JobRetryDisposition, OperationalRepository,
    OperationalRepositoryError, ProbeError, probe_database,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YieldJobOutcome {
    Completed,
    RetryAt(i64),
    DeadLetter,
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
    #[error("yield job payload serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}
