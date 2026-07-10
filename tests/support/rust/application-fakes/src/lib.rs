//! Deterministic in-memory implementations of application ports for root test suites.

#![forbid(unsafe_code)]

use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    hash::Hash,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pvlog_application::{
    AuthorizationRequest, Clock, CredentialService, EntityRepository, IdentityClaims,
    IdentityService, InsolationPoint, InsolationProvider, JobQueue, PortError, SupplyPoint,
    SupplyProvider, Transaction, UnitOfWork, WebhookRequest, WebhookResponse, WebhookSender,
};
use pvlog_domain::{
    CredentialDigest, Job, JobId, JobState, PasswordHash, SystemId, TimeRange, UtcTimestamp,
};
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;

/// Thread-safe generic entity repository.
pub struct InMemoryRepository<Entity, Id> {
    values: Mutex<HashMap<Id, Entity>>,
}

impl<Entity, Id> Default for InMemoryRepository<Entity, Id> {
    fn default() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl<Entity, Id> EntityRepository<Entity, Id> for InMemoryRepository<Entity, Id>
where
    Entity: Clone + Send + Sync + 'static,
    Id: Copy + Debug + Eq + Hash + Send + Sync + 'static,
{
    async fn find(&self, id: Id) -> Result<Option<Entity>, PortError> {
        Ok(lock(&self.values)?.get(&id).cloned())
    }

    async fn save(&self, id: Id, entity: Entity) -> Result<(), PortError> {
        lock(&self.values)?.insert(id, entity);
        Ok(())
    }

    async fn delete(&self, id: Id) -> Result<bool, PortError> {
        Ok(lock(&self.values)?.remove(&id).is_some())
    }
}

/// Clock whose instant is controlled by the test.
#[derive(Clone)]
pub struct FixedClock {
    now: Arc<Mutex<UtcTimestamp>>,
}

impl FixedClock {
    #[must_use]
    pub fn new(now: UtcTimestamp) -> Self {
        Self {
            now: Arc::new(Mutex::new(now)),
        }
    }

    /// Advances or rewinds the fake clock.
    ///
    /// # Errors
    ///
    /// Returns an error if another thread poisoned the test mutex.
    pub fn set(&self, now: UtcTimestamp) -> Result<(), PortError> {
        *lock(&self.now)? = now;
        Ok(())
    }
}

impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        match self.now.lock() {
            Ok(value) => *value,
            Err(poisoned) => **poisoned.get_ref(),
        }
    }
}

/// Deterministic non-production credential service.
#[derive(Default)]
pub struct FakeCredentialService;

#[async_trait]
impl CredentialService for FakeCredentialService {
    async fn hash_password(&self, password: &SecretString) -> Result<PasswordHash, PortError> {
        PasswordHash::new(format!(
            "fake:{}",
            blake3::hash(password.expose_secret().as_bytes())
        ))
        .map_err(|error| PortError::Rejected(error.to_string()))
    }

    async fn verify_password(
        &self,
        password: &SecretString,
        expected: &PasswordHash,
    ) -> Result<bool, PortError> {
        let actual = self.hash_password(password).await?;
        Ok(actual == *expected)
    }

    async fn digest_bearer(
        &self,
        credential: &SecretString,
    ) -> Result<CredentialDigest, PortError> {
        Ok(CredentialDigest::new(
            *blake3::hash(credential.expose_secret().as_bytes()).as_bytes(),
        ))
    }
}

/// Configurable external identity fake.
pub struct FakeIdentityService {
    pub authorization: AuthorizationRequest,
    pub claims: IdentityClaims,
}

#[async_trait]
impl IdentityService for FakeIdentityService {
    async fn begin_authorization(
        &self,
        _callback: &Url,
    ) -> Result<AuthorizationRequest, PortError> {
        Ok(self.authorization.clone())
    }

    async fn complete_authorization(
        &self,
        _callback_parameters: &[(String, String)],
    ) -> Result<IdentityClaims, PortError> {
        Ok(self.claims.clone())
    }
}

/// Webhook fake retaining exact requests.
pub struct RecordingWebhookSender {
    requests: Mutex<Vec<WebhookRequest>>,
    response: WebhookResponse,
}

impl RecordingWebhookSender {
    #[must_use]
    pub const fn new(response: WebhookResponse) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            response,
        }
    }

    /// Returns a snapshot of recorded requests.
    ///
    /// # Errors
    ///
    /// Returns an error if another thread poisoned the test mutex.
    pub fn requests(&self) -> Result<Vec<WebhookRequest>, PortError> {
        Ok(lock(&self.requests)?.clone())
    }
}

#[async_trait]
impl WebhookSender for RecordingWebhookSender {
    async fn send(&self, request: WebhookRequest) -> Result<WebhookResponse, PortError> {
        lock(&self.requests)?.push(request);
        Ok(self.response.clone())
    }
}

/// Static insolation series fake.
pub struct StaticInsolationProvider(pub Vec<InsolationPoint>);

#[async_trait]
impl InsolationProvider for StaticInsolationProvider {
    async fn query(
        &self,
        _system_id: SystemId,
        _range: TimeRange,
    ) -> Result<Vec<InsolationPoint>, PortError> {
        Ok(self.0.clone())
    }
}

/// Static regional supply series fake.
pub struct StaticSupplyProvider(pub Vec<SupplyPoint>);

#[async_trait]
impl SupplyProvider for StaticSupplyProvider {
    async fn query(
        &self,
        _region_key: &str,
        _range: TimeRange,
    ) -> Result<Vec<SupplyPoint>, PortError> {
        Ok(self.0.clone())
    }
}

/// FIFO in-memory job queue.
#[derive(Default)]
pub struct InMemoryJobQueue {
    pending: Mutex<VecDeque<Job>>,
    finished: Mutex<Vec<JobId>>,
}

#[async_trait]
impl JobQueue for InMemoryJobQueue {
    async fn enqueue(&self, job: Job) -> Result<(), PortError> {
        lock(&self.pending)?.push_back(job);
        Ok(())
    }

    async fn claim(&self, worker_id: &str, _now: UtcTimestamp) -> Result<Option<Job>, PortError> {
        let mut job = lock(&self.pending)?.pop_front();
        if let Some(claimed) = &mut job {
            claimed.state = JobState::Running {
                worker_id: worker_id.to_owned(),
            };
        }
        Ok(job)
    }

    async fn acknowledge(&self, job_id: JobId) -> Result<(), PortError> {
        lock(&self.finished)?.push(job_id);
        Ok(())
    }

    async fn retry(
        &self,
        job_id: JobId,
        reason_code: &str,
        scheduled_at: UtcTimestamp,
    ) -> Result<(), PortError> {
        let mut queue = lock(&self.pending)?;
        let Some(job) = queue.iter_mut().find(|job| job.id == job_id) else {
            return Err(PortError::NotFound);
        };
        job.state = JobState::RetryScheduled {
            reason_code: reason_code.to_owned(),
        };
        job.scheduled_at = scheduled_at;
        Ok(())
    }
}

/// Transaction fake counting terminal outcomes.
#[derive(Clone, Default)]
pub struct InMemoryUnitOfWork {
    commits: Arc<Mutex<u64>>,
    rollbacks: Arc<Mutex<u64>>,
}

impl InMemoryUnitOfWork {
    /// Returns the number of committed fake transactions.
    ///
    /// # Errors
    ///
    /// Returns an error if another thread poisoned the test mutex.
    pub fn committed(&self) -> Result<u64, PortError> {
        Ok(*lock(&self.commits)?)
    }

    /// Returns the number of rolled-back fake transactions.
    ///
    /// # Errors
    ///
    /// Returns an error if another thread poisoned the test mutex.
    pub fn rolled_back(&self) -> Result<u64, PortError> {
        Ok(*lock(&self.rollbacks)?)
    }
}

#[async_trait]
impl UnitOfWork for InMemoryUnitOfWork {
    async fn begin(&self) -> Result<Box<dyn Transaction>, PortError> {
        Ok(Box::new(InMemoryTransaction {
            commits: Arc::clone(&self.commits),
            rollbacks: Arc::clone(&self.rollbacks),
        }))
    }
}

struct InMemoryTransaction {
    commits: Arc<Mutex<u64>>,
    rollbacks: Arc<Mutex<u64>>,
}

#[async_trait]
impl Transaction for InMemoryTransaction {
    async fn commit(self: Box<Self>) -> Result<(), PortError> {
        *lock(&self.commits)? += 1;
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), PortError> {
        *lock(&self.rollbacks)? += 1;
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, PortError> {
    mutex.lock().map_err(|_| PortError::Unavailable)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use pvlog_application::{
        Clock as _, CredentialService as _, EntityRepository as _, IdentityService as _,
        InsolationProvider as _, JobQueue as _, SupplyProvider as _, UnitOfWork as _,
        WebhookRequest, WebhookResponse, WebhookSender as _,
    };
    use pvlog_domain::{Job, JobId, JobKind, JobState, SystemId, TimeRange, UserId, UtcTimestamp};
    use secrecy::SecretString;
    use url::Url;

    use super::{
        AuthorizationRequest, FakeCredentialService, FakeIdentityService, FixedClock,
        IdentityClaims, InMemoryJobQueue, InMemoryRepository, InMemoryUnitOfWork,
        RecordingWebhookSender, StaticInsolationProvider, StaticSupplyProvider,
    };

    #[tokio::test]
    async fn repository_clock_and_transaction_fakes_are_deterministic() -> Result<(), Box<dyn Error>>
    {
        let timestamp = UtcTimestamp::from_epoch_millis(1_000)?;
        let repository = InMemoryRepository::<String, UserId>::default();
        let user_id = UserId::new();
        repository.save(user_id, "Ada".to_owned()).await?;
        assert_eq!(repository.find(user_id).await?, Some("Ada".to_owned()));

        let clock = FixedClock::new(timestamp);
        assert_eq!(clock.now(), timestamp);

        let unit_of_work = InMemoryUnitOfWork::default();
        unit_of_work.begin().await?.commit().await?;
        assert_eq!(unit_of_work.committed()?, 1);
        assert_eq!(unit_of_work.rolled_back()?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn credential_and_identity_fakes_are_provider_neutral() -> Result<(), Box<dyn Error>> {
        let credentials = FakeCredentialService;
        let password = SecretString::from("correct horse battery staple".to_owned());
        let hash = credentials.hash_password(&password).await?;
        assert!(credentials.verify_password(&password, &hash).await?);

        let identity = FakeIdentityService {
            authorization: AuthorizationRequest {
                redirect_url: Url::parse("https://identity.example/authorize")?,
                state_handle: "state".to_owned(),
            },
            claims: IdentityClaims {
                subject: "subject-1".to_owned(),
                ..IdentityClaims::default()
            },
        };
        assert_eq!(
            identity
                .begin_authorization(&Url::parse("https://pvlog.example/callback")?)
                .await?
                .state_handle,
            "state"
        );
        assert_eq!(
            identity.complete_authorization(&[]).await?.subject,
            "subject-1"
        );
        Ok(())
    }

    #[tokio::test]
    async fn outbound_and_queue_fakes_record_boundary_behavior() -> Result<(), Box<dyn Error>> {
        let sender = RecordingWebhookSender::new(WebhookResponse {
            status: 204,
            retry_after_seconds: None,
        });
        sender
            .send(WebhookRequest {
                endpoint: Url::parse("https://receiver.example/events")?,
                headers: Vec::new(),
                body: b"{}".to_vec(),
            })
            .await?;
        assert_eq!(sender.requests()?.len(), 1);

        let now = UtcTimestamp::from_epoch_millis(1_000)?;
        let range = TimeRange::new(now, UtcTimestamp::from_epoch_millis(2_000)?)?;
        assert!(
            StaticInsolationProvider(Vec::new())
                .query(SystemId::new(), range)
                .await?
                .is_empty()
        );
        assert!(
            StaticSupplyProvider(Vec::new())
                .query("region", range)
                .await?
                .is_empty()
        );

        let queue = InMemoryJobQueue::default();
        let job_id = JobId::new();
        queue
            .enqueue(Job {
                id: job_id,
                account_id: None,
                kind: JobKind::RebuildRollup,
                state: JobState::Pending,
                payload: serde_json::json!({}),
                idempotency_key: "job-1".to_owned(),
                attempts: 0,
                maximum_attempts: 3,
                scheduled_at: now,
                lease_expires_at: None,
            })
            .await?;
        let claimed = queue.claim("worker-1", now).await?;
        assert!(claimed.is_some_and(|job| job.id == job_id));
        Ok(())
    }
}
