use async_trait::async_trait;
use pvlog_application::{
    Clock, CreateSystem, PortError, RbacRepository, RbacRoleRecord, SystemLifecycleError,
    SystemLifecycleRecord, SystemLifecycleRepository, SystemLifecycleService, UpdateSystem,
    built_in_account_roles,
};
use pvlog_domain::{
    AccountId, BuiltInRole, PrincipalId, RoleAssignment, RoleAssignmentId, RoleId, RoleKind,
    RoleScope, SystemId, SystemLifecycle, UserId, UtcTimestamp, Visibility,
};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn lifecycle_uses_safe_defaults_versions_audit_and_confirmation() -> Result<(), Box<dyn Error>>
{
    let repository = Arc::new(FakeRepository::default());
    let actor = UserId::new();
    let account_id = AccountId::new();
    let rbac = Arc::new(FakeRbacRepository::new(account_id, actor));
    let service =
        SystemLifecycleService::new(repository.clone(), rbac.clone(), Arc::new(FixedClock));
    let created = service
        .create(CreateSystem {
            account_id,
            actor,
            name: "Roof".to_owned(),
            timezone: "Europe/Berlin".to_owned(),
        })
        .await?;
    assert_eq!(
        (created.visibility, created.lifecycle, created.version),
        (Visibility::Private, SystemLifecycle::Active, 1)
    );
    let owner_assignment = rbac.owner_assignment()?;
    assert_eq!(owner_assignment.principal, PrincipalId::User(actor));
    assert_eq!(
        rbac.role_kind(owner_assignment.role_id)?,
        RoleKind::BuiltIn(BuiltInRole::AccountOwner)
    );
    assert_eq!(owner_assignment.granted_by, actor);
    assert_eq!(owner_assignment.expires_at, None);
    assert_eq!(
        owner_assignment.scope,
        RoleScope::System {
            account_id,
            system_id: created.id,
        }
    );
    let updated = service
        .update(UpdateSystem {
            id: created.id,
            actor,
            expected_version: 1,
            name: "Roof PV".to_owned(),
            timezone: "Europe/Berlin".to_owned(),
            visibility: Visibility::Unlisted,
        })
        .await?;
    assert_eq!(updated.version, 2);
    assert!(matches!(
        service.archive(created.id, actor, 1).await,
        Err(SystemLifecycleError::Conflict)
    ));
    let archived = service.archive(created.id, actor, 2).await?;
    assert_eq!(archived.lifecycle, SystemLifecycle::Archived);
    assert!(matches!(
        service.delete(created.id, actor, 3, false).await,
        Err(SystemLifecycleError::ConfirmationRequired)
    ));
    service.delete(created.id, actor, 3, true).await?;
    assert_eq!(repository.audit_count()?, 4);
    Ok(())
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH)
    }
}
#[derive(Default)]
struct FakeRepository(Mutex<State>);
#[derive(Default)]
struct State {
    record: Option<SystemLifecycleRecord>,
    audits: u32,
}
impl FakeRepository {
    fn audit_count(&self) -> Result<u32, Box<dyn Error>> {
        Ok(self.0.lock().map_err(|_| "poisoned")?.audits)
    }
}
#[async_trait]
impl SystemLifecycleRepository for FakeRepository {
    async fn system(&self, id: SystemId) -> Result<Option<SystemLifecycleRecord>, PortError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .record
            .as_ref()
            .filter(|record| record.id == id)
            .cloned())
    }
    async fn create(&self, record: SystemLifecycleRecord) -> Result<(), PortError> {
        self.0.lock().map_err(|_| PortError::Unavailable)?.record = Some(record);
        Ok(())
    }
    async fn save(&self, record: SystemLifecycleRecord, expected: u64) -> Result<bool, PortError> {
        let mut state = self.0.lock().map_err(|_| PortError::Unavailable)?;
        if state
            .record
            .as_ref()
            .is_none_or(|stored| stored.version != expected)
        {
            return Ok(false);
        }
        state.record = Some(record);
        Ok(true)
    }
    async fn delete(&self, id: SystemId, expected: u64) -> Result<bool, PortError> {
        let mut state = self.0.lock().map_err(|_| PortError::Unavailable)?;
        if state
            .record
            .as_ref()
            .is_none_or(|stored| stored.id != id || stored.version != expected)
        {
            return Ok(false);
        }
        state.record = None;
        Ok(true)
    }
    async fn audit(
        &self,
        _actor: UserId,
        _id: SystemId,
        _action: &'static str,
        _outcome: &'static str,
        _now: i64,
    ) -> Result<(), PortError> {
        self.0.lock().map_err(|_| PortError::Unavailable)?.audits += 1;
        Ok(())
    }
}

struct FakeRbacRepository {
    roles: Vec<RbacRoleRecord>,
    assignments: Mutex<Vec<RoleAssignment>>,
}

impl FakeRbacRepository {
    fn new(account_id: AccountId, creator: UserId) -> Self {
        Self {
            roles: built_in_account_roles(account_id, creator, 0),
            assignments: Mutex::new(Vec::new()),
        }
    }

    fn owner_assignment(&self) -> Result<RoleAssignment, Box<dyn Error>> {
        self.assignments
            .lock()
            .map_err(|_| Box::<dyn Error>::from("poisoned"))?
            .first()
            .cloned()
            .ok_or_else(|| "owner assignment missing".into())
    }

    fn role_kind(&self, id: RoleId) -> Result<RoleKind, Box<dyn Error>> {
        self.roles
            .iter()
            .find(|record| record.role.id == id)
            .map(|record| record.role.kind.clone())
            .ok_or_else(|| "assigned role missing".into())
    }
}

#[async_trait]
impl RbacRepository for FakeRbacRepository {
    async fn roles(&self, account_id: Option<AccountId>) -> Result<Vec<RbacRoleRecord>, PortError> {
        Ok(self
            .roles
            .iter()
            .filter(|record| record.role.account_id == account_id)
            .cloned()
            .collect())
    }

    async fn role(&self, id: RoleId) -> Result<Option<RbacRoleRecord>, PortError> {
        Ok(self
            .roles
            .iter()
            .find(|record| record.role.id == id)
            .cloned())
    }

    async fn save_role(&self, _record: &RbacRoleRecord) -> Result<(), PortError> {
        Ok(())
    }

    async fn delete_custom_role(&self, _id: RoleId) -> Result<bool, PortError> {
        Ok(false)
    }

    async fn active_assignments(
        &self,
        principal: PrincipalId,
        _now: i64,
    ) -> Result<Vec<RoleAssignment>, PortError> {
        Ok(self
            .assignments
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .iter()
            .filter(|assignment| assignment.principal == principal)
            .cloned()
            .collect())
    }

    async fn save_assignment(&self, assignment: &RoleAssignment) -> Result<(), PortError> {
        self.assignments
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push(assignment.clone());
        Ok(())
    }

    async fn revoke_assignment(&self, _id: RoleAssignmentId, _now: i64) -> Result<bool, PortError> {
        Ok(false)
    }
}
