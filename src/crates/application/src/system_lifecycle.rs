//! Audited PV-system lifecycle with safe defaults and optimistic concurrency.

use crate::{Clock, PortError, RbacRepository};
use async_trait::async_trait;
use pvlog_domain::{
    AccountId, BuiltInRole, IanaTimezone, PrincipalId, RoleAssignment, RoleAssignmentId, RoleKind,
    RoleScope, SystemId, SystemLifecycle, UserId, Visibility,
};
use serde::Serialize;
use std::{str::FromStr as _, sync::Arc};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLifecycleRecord {
    pub id: SystemId,
    pub account_id: AccountId,
    pub name: String,
    pub timezone: String,
    pub visibility: Visibility,
    pub lifecycle: SystemLifecycle,
    pub version: u64,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateSystem {
    pub account_id: AccountId,
    pub actor: UserId,
    pub name: String,
    pub timezone: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateSystem {
    pub id: SystemId,
    pub actor: UserId,
    pub expected_version: u64,
    pub name: String,
    pub timezone: String,
    pub visibility: Visibility,
}

#[async_trait]
pub trait SystemLifecycleRepository: Send + Sync {
    async fn system(&self, id: SystemId) -> Result<Option<SystemLifecycleRecord>, PortError>;
    async fn create(&self, record: SystemLifecycleRecord) -> Result<(), PortError>;
    async fn save(
        &self,
        record: SystemLifecycleRecord,
        expected_version: u64,
    ) -> Result<bool, PortError>;
    async fn delete(&self, id: SystemId, expected_version: u64) -> Result<bool, PortError>;
    async fn audit(
        &self,
        actor: UserId,
        id: SystemId,
        action: &'static str,
        outcome: &'static str,
        now: i64,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait SystemLifecycleUseCases: Send + Sync {
    async fn system(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn create_system(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn update_system(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn archive_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn restore_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn delete_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        confirmed: bool,
    ) -> Result<(), SystemLifecycleError>;
}

pub struct SystemLifecycleService {
    repository: Arc<dyn SystemLifecycleRepository>,
    rbac_repository: Arc<dyn RbacRepository>,
    clock: Arc<dyn Clock>,
}
impl SystemLifecycleService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn SystemLifecycleRepository>,
        rbac_repository: Arc<dyn RbacRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            rbac_repository,
            clock,
        }
    }
    /// Creates a private active system with validated required fields.
    /// # Errors
    /// Returns an error for invalid input, time, or persistence failure.
    pub async fn create(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        validate(&request.name, &request.timezone)?;
        let owner_role_id = self.owner_role_id(request.account_id).await?;
        let granted_at = self.clock.now();
        let now =
            i64::try_from(granted_at.epoch_millis()).map_err(|_| SystemLifecycleError::Time)?;
        let record = SystemLifecycleRecord {
            id: SystemId::new(),
            account_id: request.account_id,
            name: request.name,
            timezone: request.timezone,
            visibility: Visibility::Private,
            lifecycle: SystemLifecycle::Active,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        self.repository
            .create(record.clone())
            .await
            .map_err(SystemLifecycleError::Repository)?;
        if let Err(error) = self
            .rbac_repository
            .save_assignment(&RoleAssignment {
                id: RoleAssignmentId::new(),
                principal: PrincipalId::User(request.actor),
                role_id: owner_role_id,
                scope: RoleScope::System {
                    account_id: request.account_id,
                    system_id: record.id,
                },
                granted_by: request.actor,
                granted_at,
                expires_at: None,
            })
            .await
        {
            let _ = self.repository.delete(record.id, record.version).await;
            return Err(SystemLifecycleError::Repository(error));
        }
        self.repository
            .audit(request.actor, record.id, "system.created", "succeeded", now)
            .await
            .map_err(SystemLifecycleError::Repository)?;
        Ok(record)
    }

    async fn owner_role_id(
        &self,
        account_id: AccountId,
    ) -> Result<pvlog_domain::RoleId, SystemLifecycleError> {
        self.rbac_repository
            .roles(Some(account_id))
            .await
            .map_err(SystemLifecycleError::Repository)?
            .into_iter()
            .find(|record| record.role.kind == RoleKind::BuiltIn(BuiltInRole::AccountOwner))
            .map(|record| record.role.id)
            .ok_or_else(|| {
                SystemLifecycleError::Repository(PortError::Rejected(
                    "account_owner_role_missing".to_owned(),
                ))
            })
    }
    /// Updates mutable system fields using an expected version.
    /// # Errors
    /// Returns an error for invalid input, missing system, conflict, or persistence failure.
    pub async fn update(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        validate(&request.name, &request.timezone)?;
        let mut record = self.load(request.id).await?;
        record.name = request.name;
        record.timezone = request.timezone;
        record.visibility = request.visibility;
        record.updated_at = self.now()?;
        record.version = request.expected_version.saturating_add(1);
        self.persist(
            record,
            request.expected_version,
            request.actor,
            "system.updated",
        )
        .await
    }
    /// Archives an active system without deleting data.
    /// # Errors
    /// Returns an error for missing system, conflict, or persistence failure.
    pub async fn archive(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.transition(
            id,
            actor,
            version,
            SystemLifecycle::Archived,
            "system.archived",
        )
        .await
    }
    /// Restores an archived system.
    /// # Errors
    /// Returns an error for missing system, conflict, or persistence failure.
    pub async fn restore(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.transition(
            id,
            actor,
            version,
            SystemLifecycle::Active,
            "system.restored",
        )
        .await
    }
    /// Permanently deletes only after explicit confirmation.
    /// # Errors
    /// Returns an error for missing confirmation, conflict, or persistence failure.
    pub async fn delete(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        confirmed: bool,
    ) -> Result<(), SystemLifecycleError> {
        if !confirmed {
            return Err(SystemLifecycleError::ConfirmationRequired);
        }
        if !self
            .repository
            .delete(id, version)
            .await
            .map_err(SystemLifecycleError::Repository)?
        {
            return Err(SystemLifecycleError::Conflict);
        }
        self.repository
            .audit(actor, id, "system.deleted", "succeeded", self.now()?)
            .await
            .map_err(SystemLifecycleError::Repository)
    }
    async fn transition(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        lifecycle: SystemLifecycle,
        action: &'static str,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        let mut record = self.load(id).await?;
        record.lifecycle = lifecycle;
        record.version = version.saturating_add(1);
        record.updated_at = self.now()?;
        self.persist(record, version, actor, action).await
    }
    async fn persist(
        &self,
        record: SystemLifecycleRecord,
        version: u64,
        actor: UserId,
        action: &'static str,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        if !self
            .repository
            .save(record.clone(), version)
            .await
            .map_err(SystemLifecycleError::Repository)?
        {
            return Err(SystemLifecycleError::Conflict);
        }
        self.repository
            .audit(actor, record.id, action, "succeeded", record.updated_at)
            .await
            .map_err(SystemLifecycleError::Repository)?;
        Ok(record)
    }
    async fn load(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.repository
            .system(id)
            .await
            .map_err(SystemLifecycleError::Repository)?
            .ok_or(SystemLifecycleError::NotFound)
    }
    fn now(&self) -> Result<i64, SystemLifecycleError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| SystemLifecycleError::Time)
    }
}

#[async_trait]
impl SystemLifecycleUseCases for SystemLifecycleService {
    async fn system(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.load(id).await
    }

    async fn create_system(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.create(request).await
    }
    async fn update_system(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.update(request).await
    }
    async fn archive_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.archive(id, actor, version).await
    }
    async fn restore_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.restore(id, actor, version).await
    }
    async fn delete_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        confirmed: bool,
    ) -> Result<(), SystemLifecycleError> {
        self.delete(id, actor, version, confirmed).await
    }
}
fn validate(name: &str, timezone: &str) -> Result<(), SystemLifecycleError> {
    if name.trim().is_empty() || IanaTimezone::from_str(timezone).is_err() {
        Err(SystemLifecycleError::InvalidInput)
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SystemLifecycleError {
    #[error("system input is invalid")]
    InvalidInput,
    #[error("system was not found")]
    NotFound,
    #[error("system version conflict")]
    Conflict,
    #[error("explicit destructive confirmation is required")]
    ConfirmationRequired,
    #[error("clock value is invalid")]
    Time,
    #[error("system persistence is unavailable")]
    Repository(PortError),
}
