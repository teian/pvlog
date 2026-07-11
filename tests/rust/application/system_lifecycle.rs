use async_trait::async_trait;
use pvlog_application::{
    Clock, CreateSystem, PortError, SystemLifecycleError, SystemLifecycleRecord,
    SystemLifecycleRepository, SystemLifecycleService, UpdateSystem,
};
use pvlog_domain::{AccountId, SystemId, SystemLifecycle, UserId, UtcTimestamp, Visibility};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn lifecycle_uses_safe_defaults_versions_audit_and_confirmation() -> Result<(), Box<dyn Error>>
{
    let repository = Arc::new(FakeRepository::default());
    let actor = UserId::new();
    let service = SystemLifecycleService::new(repository.clone(), Arc::new(FixedClock));
    let created = service
        .create(CreateSystem {
            account_id: AccountId::new(),
            actor,
            name: "Roof".to_owned(),
            timezone: "Europe/Berlin".to_owned(),
        })
        .await?;
    assert_eq!(
        (created.visibility, created.lifecycle, created.version),
        (Visibility::Private, SystemLifecycle::Active, 1)
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
