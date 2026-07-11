//! Provider-neutral contracts for optional insolation and regional supply data.

use async_trait::async_trait;
use pvlog_domain::{ProviderId, UtcTimestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::{InsolationPoint, PortError, SupplyPoint};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalDataKind {
    Insolation,
    RegionalSupply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDataLicense {
    pub identifier: String,
    pub attribution: String,
    pub source_url: Url,
    pub redistribution_permitted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDataConfiguration {
    pub provider_id: ProviderId,
    pub kind: ExternalDataKind,
    pub adapter: String,
    pub endpoint: Url,
    pub credential_secret_reference: Option<String>,
    pub request_timeout_milliseconds: u32,
    pub cache_ttl_seconds: u32,
    pub license: ExternalDataLicense,
    pub enabled: bool,
}

impl ExternalDataConfiguration {
    /// Validates administrator-supplied configuration without assuming or bundling a dataset.
    ///
    /// # Errors
    ///
    /// Returns a field-specific error when the adapter, endpoint, timeout, cache, or required
    /// licensing metadata is invalid.
    pub fn validate(&self) -> Result<(), ProviderConfigurationError> {
        if self.adapter.trim().is_empty() {
            return Err(ProviderConfigurationError::MissingAdapter);
        }
        if !matches!(self.endpoint.scheme(), "https" | "http") {
            return Err(ProviderConfigurationError::UnsupportedEndpoint);
        }
        if self.request_timeout_milliseconds == 0 || self.request_timeout_milliseconds > 30_000 {
            return Err(ProviderConfigurationError::InvalidTimeout);
        }
        if self.cache_ttl_seconds == 0 {
            return Err(ProviderConfigurationError::InvalidCacheTtl);
        }
        if self.license.identifier.trim().is_empty() || self.license.attribution.trim().is_empty() {
            return Err(ProviderConfigurationError::MissingLicenseMetadata);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProviderConfigurationError {
    #[error("provider adapter is required")]
    MissingAdapter,
    #[error("provider endpoint must use HTTP or HTTPS")]
    UnsupportedEndpoint,
    #[error("provider timeout must be between 1 and 30000 milliseconds")]
    InvalidTimeout,
    #[error("provider cache TTL must be positive")]
    InvalidCacheTtl,
    #[error("provider license identifier and attribution are required")]
    MissingLicenseMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDataCacheKey {
    pub provider_id: ProviderId,
    pub resource_key: String,
    pub range_start: UtcTimestamp,
    pub range_end: UtcTimestamp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDataProvenance {
    pub provider_id: ProviderId,
    pub adapter: String,
    pub source_url: Url,
    pub license_identifier: String,
    pub attribution: String,
    pub fetched_at: UtcTimestamp,
    pub valid_until: UtcTimestamp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalDataCacheEntry {
    Insolation {
        points: Vec<InsolationPoint>,
        provenance: ExternalDataProvenance,
    },
    RegionalSupply {
        points: Vec<SupplyPoint>,
        provenance: ExternalDataProvenance,
    },
}

#[async_trait]
pub trait ExternalDataCacheRepository: Send + Sync {
    async fn get(
        &self,
        key: &ExternalDataCacheKey,
    ) -> Result<Option<ExternalDataCacheEntry>, PortError>;
    async fn put(
        &self,
        key: &ExternalDataCacheKey,
        entry: &ExternalDataCacheEntry,
    ) -> Result<(), PortError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CircuitBreakerPolicy {
    pub failure_threshold: u16,
    pub recovery_timeout_milliseconds: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CircuitState {
    Closed,
    Opened { retry_at_epoch_millis: i128 },
    HalfOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CircuitBreaker {
    policy: CircuitBreakerPolicy,
    failures: u16,
    state: CircuitState,
}

impl CircuitBreaker {
    #[must_use]
    pub const fn new(policy: CircuitBreakerPolicy) -> Self {
        Self {
            policy,
            failures: 0,
            state: CircuitState::Closed,
        }
    }

    #[must_use]
    pub const fn state(self) -> CircuitState {
        self.state
    }

    pub fn allow(&mut self, now: UtcTimestamp) -> bool {
        match self.state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Opened {
                retry_at_epoch_millis,
            } if now.epoch_millis() >= retry_at_epoch_millis => {
                self.state = CircuitState::HalfOpen;
                true
            }
            CircuitState::Opened { .. } => false,
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.state = CircuitState::Closed;
    }

    pub fn record_failure(&mut self, now: UtcTimestamp) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= self.policy.failure_threshold.max(1) {
            self.state = CircuitState::Opened {
                retry_at_epoch_millis: now.epoch_millis()
                    + i128::from(self.policy.recovery_timeout_milliseconds),
            };
        }
    }
}
