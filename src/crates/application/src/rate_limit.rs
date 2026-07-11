//! Deterministic principal quotas and rate-limit response metadata.

use crate::{Clock, PortError};
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrincipalQuota {
    pub requests: u32,
    pub window_seconds: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitDecision {
    pub used: u32,
    pub resets_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitMetadata {
    pub limit: u32,
    pub remaining: u32,
    pub resets_at: i64,
    pub retry_after_seconds: Option<u32>,
}

#[async_trait]
pub trait RateLimitRepository: Send + Sync {
    async fn increment(
        &self,
        principal_key: &str,
        window_started_at: i64,
        window_seconds: u32,
    ) -> Result<RateLimitDecision, PortError>;
}

pub struct RateLimitService {
    repository: Arc<dyn RateLimitRepository>,
    clock: Arc<dyn Clock>,
}
impl RateLimitService {
    #[must_use]
    pub fn new(repository: Arc<dyn RateLimitRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }
    /// Atomically admits one principal request within the configured fixed window.
    ///
    /// # Errors
    /// Returns an error for invalid policy, exceeded quota, time failure, or storage failure.
    pub async fn admit(
        &self,
        principal_key: &str,
        quota: PrincipalQuota,
    ) -> Result<RateLimitMetadata, RateLimitError> {
        if principal_key.trim().is_empty() || quota.requests == 0 || quota.window_seconds == 0 {
            return Err(RateLimitError::InvalidPolicy);
        }
        let now =
            i64::try_from(self.clock.now().epoch_millis()).map_err(|_| RateLimitError::Time)?;
        let window_millis = i64::from(quota.window_seconds) * 1_000;
        let window_started_at = now - now.rem_euclid(window_millis);
        let decision = self
            .repository
            .increment(principal_key, window_started_at, quota.window_seconds)
            .await
            .map_err(RateLimitError::Repository)?;
        let remaining = quota.requests.saturating_sub(decision.used);
        let retry_after_seconds = (decision.used > quota.requests).then(|| {
            let remaining_millis = (decision.resets_at - now).max(0);
            u32::try_from((remaining_millis + 999) / 1_000).unwrap_or(u32::MAX)
        });
        let metadata = RateLimitMetadata {
            limit: quota.requests,
            remaining,
            resets_at: decision.resets_at,
            retry_after_seconds,
        };
        if retry_after_seconds.is_some() {
            Err(RateLimitError::Exceeded(metadata))
        } else {
            Ok(metadata)
        }
    }
    #[must_use]
    pub fn legacy_headers(
        metadata: RateLimitMetadata,
        requested: bool,
    ) -> Vec<(&'static str, String)> {
        if !requested {
            return Vec::new();
        }
        vec![
            ("X-Rate-Limit-Limit", metadata.limit.to_string()),
            ("X-Rate-Limit-Remaining", metadata.remaining.to_string()),
            ("X-Rate-Limit-Reset", metadata.resets_at.to_string()),
        ]
    }
}

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("rate-limit policy is invalid")]
    InvalidPolicy,
    #[error("principal quota exceeded")]
    Exceeded(RateLimitMetadata),
    #[error("clock value is invalid")]
    Time,
    #[error("rate-limit persistence is unavailable")]
    Repository(PortError),
}
