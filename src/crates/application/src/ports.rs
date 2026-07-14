use std::fmt::Debug;

use async_trait::async_trait;
use pvlog_domain::{CredentialDigest, Job, JobId, PasswordHash, SystemId, TimeRange, UtcTimestamp};
use secrecy::SecretString;
use thiserror::Error;
use url::Url;

/// Common repository contract implemented by database adapters and in-memory fakes.
#[async_trait]
pub trait EntityRepository<Entity, Id>: Send + Sync
where
    Entity: Clone + Send + Sync + 'static,
    Id: Copy + Debug + Send + Sync + 'static,
{
    async fn find(&self, id: Id) -> Result<Option<Entity>, PortError>;
    async fn save(&self, id: Id, entity: Entity) -> Result<(), PortError>;
    async fn delete(&self, id: Id) -> Result<bool, PortError>;
}

/// Injectable time source for deterministic policy and expiry tests.
pub trait Clock: Send + Sync {
    fn now(&self) -> UtcTimestamp;
}

/// Password and bearer-credential primitive boundary.
#[async_trait]
pub trait CredentialService: Send + Sync {
    async fn hash_password(&self, password: &SecretString) -> Result<PasswordHash, PortError>;
    async fn verify_password(
        &self,
        password: &SecretString,
        expected: &PasswordHash,
    ) -> Result<bool, PortError>;
    async fn digest_bearer(&self, credential: &SecretString)
    -> Result<CredentialDigest, PortError>;
    /// Reports whether a valid encoded verifier uses parameters older than current policy.
    ///
    /// # Errors
    ///
    /// Returns an error when the encoded verifier cannot be parsed safely.
    fn password_needs_rehash(&self, _encoded: &PasswordHash) -> Result<bool, PortError> {
        Ok(false)
    }
}

/// Resolves an administrator-configured secret reference at a narrow protocol boundary.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    async fn resolve(&self, secret_reference: &str) -> Result<SecretString, PortError>;
}

/// Browser redirect information returned by an external identity connector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationRequest {
    pub redirect_url: Url,
    pub state_handle: String,
}

/// Provider-neutral verified identity claims.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IdentityClaims {
    pub subject: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub avatar_url: Option<Url>,
}

/// Standards-based external identity protocol boundary.
#[async_trait]
pub trait IdentityService: Send + Sync {
    async fn begin_authorization(&self, callback: &Url) -> Result<AuthorizationRequest, PortError>;
    async fn complete_authorization(
        &self,
        callback_parameters: &[(String, String)],
    ) -> Result<IdentityClaims, PortError>;
}

/// Exact outbound webhook request after signing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookRequest {
    pub endpoint: Url,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Bounded webhook response metadata retained for delivery classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookResponse {
    pub status: u16,
    pub retry_after_seconds: Option<u32>,
}

/// Outbound HTTP boundary used by durable webhook delivery jobs.
#[async_trait]
pub trait WebhookSender: Send + Sync {
    async fn send(&self, request: WebhookRequest) -> Result<WebhookResponse, PortError>;
}

/// Insolation sample in integer watts per square metre.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InsolationPoint {
    pub timestamp: UtcTimestamp,
    pub watts_per_square_metre: u32,
}

/// External solar-resource data boundary.
#[async_trait]
pub trait InsolationProvider: Send + Sync {
    async fn query(
        &self,
        system_id: SystemId,
        range: TimeRange,
    ) -> Result<Vec<InsolationPoint>, PortError>;
}

/// Provider-neutral request for one immutable normalized weather run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherDataRequest {
    pub system_id: SystemId,
    pub kind: pvlog_domain::WeatherDataKind,
    pub range: TimeRange,
    pub spatial_coverage: pvlog_domain::SpatialCoverage,
    pub issued_before: Option<UtcTimestamp>,
}

/// External weather boundary that preserves forecast, observation, and reanalysis identity.
#[async_trait]
pub trait WeatherDataProvider: Send + Sync {
    async fn query(
        &self,
        request: &WeatherDataRequest,
    ) -> Result<pvlog_domain::NormalizedWeatherRun, PortError>;
}

/// Regional electricity supply/demand sample in watts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupplyPoint {
    pub timestamp: UtcTimestamp,
    pub supply_watts: i64,
    pub demand_watts: i64,
}

/// External regional supply-series boundary.
#[async_trait]
pub trait SupplyProvider: Send + Sync {
    async fn query(
        &self,
        region_key: &str,
        range: TimeRange,
    ) -> Result<Vec<SupplyPoint>, PortError>;
}

/// Durable background queue boundary.
#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: Job) -> Result<(), PortError>;
    async fn claim(&self, worker_id: &str, now: UtcTimestamp) -> Result<Option<Job>, PortError>;
    async fn acknowledge(&self, job_id: JobId) -> Result<(), PortError>;
    async fn retry(
        &self,
        job_id: JobId,
        reason_code: &str,
        scheduled_at: UtcTimestamp,
    ) -> Result<(), PortError>;
}

/// Active transaction handle with explicit terminal behavior.
#[async_trait]
pub trait Transaction: Send {
    async fn commit(self: Box<Self>) -> Result<(), PortError>;
    async fn rollback(self: Box<Self>) -> Result<(), PortError>;
}

/// Transaction factory used by application use cases spanning repositories.
#[async_trait]
pub trait UnitOfWork: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn Transaction>, PortError>;
}

/// Safe application-port failure classification.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PortError {
    #[error("requested entity was not found")]
    NotFound,
    #[error("operation conflicts with current state")]
    Conflict,
    #[error("external dependency is temporarily unavailable")]
    Unavailable,
    #[error("operation was rejected: {0}")]
    Rejected(String),
}
