//! Management-level dispatch across account-local durable queues.

use crate::{JobLease, OperationalRepository, OperationalRepositoryError};
use std::sync::Arc;

pub struct ManagementJobDispatcher {
    accounts: Vec<Arc<dyn OperationalRepository>>,
}

impl ManagementJobDispatcher {
    #[must_use]
    pub fn new(mut accounts: Vec<Arc<dyn OperationalRepository>>) -> Self {
        accounts.sort_unstable_by_key(|repository| repository.account_id().as_uuid());
        Self { accounts }
    }

    /// Leases the first available job using stable account ordering.
    /// # Errors
    /// Returns the first account-local persistence error.
    pub async fn lease_next(
        &self,
        owner: &str,
        now: i64,
        lease_expires_at: i64,
    ) -> Result<Option<JobLease>, OperationalRepositoryError> {
        for repository in &self.accounts {
            if let Some(lease) = repository.lease_job(owner, now, lease_expires_at).await? {
                return Ok(Some(lease));
            }
        }
        Ok(None)
    }
}
