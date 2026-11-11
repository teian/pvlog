use async_trait::async_trait;
use pvlog_application::{
    Clock, CreateSystem, PortError, RbacRepository, RbacRoleRecord, SystemDetails,
    SystemLifecycleError, SystemLifecycleRecord, SystemLifecycleRepository, SystemLifecycleService,
    UpdateSystem, built_in_account_roles,
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
            timezone: Some("Europe/Berlin".to_owned()),
            client_timezone: None,
            details: SystemDetails::default(),
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
            details: SystemDetails {
                description: "Wohnhaus Dach".to_owned(),
                commissioning_date: Some("2019-04-15".to_owned()),
                ..SystemDetails::default()
            },
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

/// Builds a service over fresh fakes for one account and actor.
fn service() -> (SystemLifecycleService, AccountId, UserId) {
    let actor = UserId::new();
    let account_id = AccountId::new();
    let service = SystemLifecycleService::new(
        Arc::new(FakeRepository::default()),
        Arc::new(FakeRbacRepository::new(account_id, actor)),
        Arc::new(FixedClock),
    );
    (service, account_id, actor)
}

fn name_only(account_id: AccountId, actor: UserId) -> CreateSystem {
    CreateSystem {
        account_id,
        actor,
        name: "Roof".to_owned(),
        timezone: None,
        client_timezone: Some("Europe/Vienna".to_owned()),
        details: SystemDetails::default(),
    }
}

#[tokio::test]
async fn a_system_is_created_from_a_name_alone() -> Result<(), Box<dyn Error>> {
    let (service, account_id, actor) = service();
    let created = service.create(name_only(account_id, actor)).await?;

    assert_eq!(created.timezone, "Europe/Vienna");
    assert_eq!(created.status_interval_seconds, 300);
    assert_eq!(created.location_label, None);
    assert_eq!(created.latitude_microdegrees, None);
    assert_eq!(created.declared_capacity_watts, None);
    assert_eq!(created.commissioning_date, None);
    Ok(())
}

#[tokio::test]
async fn the_timezone_follows_the_country_of_the_location_unless_chosen()
-> Result<(), Box<dyn Error>> {
    let (service, account_id, actor) = service();
    // The client sits in Vienna; the plant stands in Lisbon.
    let located = CreateSystem {
        details: SystemDetails {
            location_label: Some("  Lisbon  ".to_owned()),
            country_code: Some("pt".to_owned()),
            latitude_microdegrees: Some(38_720_000),
            longitude_microdegrees: Some(-9_140_000),
            ..SystemDetails::default()
        },
        ..name_only(account_id, actor)
    };
    let derived = service.create(located.clone()).await?;
    assert_eq!(derived.timezone, "Europe/Lisbon");
    assert_eq!(derived.location_label.as_deref(), Some("Lisbon"));
    assert_eq!(derived.country_code.as_deref(), Some("PT"));

    let chosen = service
        .create(CreateSystem {
            timezone: Some("Atlantic/Madeira".to_owned()),
            ..located.clone()
        })
        .await?;
    assert_eq!(chosen.timezone, "Atlantic/Madeira");

    // Coordinates without a country cannot be placed, so the client decides.
    let unplaced = service
        .create(CreateSystem {
            details: SystemDetails {
                country_code: None,
                ..located.details
            },
            ..located
        })
        .await?;
    assert_eq!(unplaced.timezone, "Europe/Vienna");
    Ok(())
}

#[tokio::test]
async fn each_invalid_field_is_named() -> Result<(), Box<dyn Error>> {
    let (service, account_id, actor) = service();
    let base = name_only(account_id, actor);
    let with = |details: SystemDetails| CreateSystem {
        details,
        ..base.clone()
    };
    let cases = [
        (
            CreateSystem {
                name: "  ".to_owned(),
                ..base.clone()
            },
            "name_required",
        ),
        (
            CreateSystem {
                client_timezone: None,
                ..base.clone()
            },
            "timezone_required",
        ),
        (
            CreateSystem {
                timezone: Some("Mars/Olympus".to_owned()),
                ..base.clone()
            },
            "timezone_unknown",
        ),
        (
            with(SystemDetails {
                latitude_microdegrees: Some(1),
                ..SystemDetails::default()
            }),
            "coordinates_incomplete",
        ),
        (
            with(SystemDetails {
                latitude_microdegrees: Some(90_000_001),
                longitude_microdegrees: Some(0),
                ..SystemDetails::default()
            }),
            "latitude_out_of_range",
        ),
        (
            with(SystemDetails {
                latitude_microdegrees: Some(0),
                longitude_microdegrees: Some(-180_000_001),
                ..SystemDetails::default()
            }),
            "longitude_out_of_range",
        ),
        (
            with(SystemDetails {
                country_code: Some("DEU".to_owned()),
                ..SystemDetails::default()
            }),
            "country_code_invalid",
        ),
        (
            with(SystemDetails {
                status_interval_seconds: Some(29),
                ..SystemDetails::default()
            }),
            "status_interval_out_of_range",
        ),
        (
            with(SystemDetails {
                declared_capacity_watts: Some(0),
                ..SystemDetails::default()
            }),
            "declared_capacity_not_positive",
        ),
        (
            with(SystemDetails {
                commissioning_date: Some("15.04.2019".to_owned()),
                ..SystemDetails::default()
            }),
            "commissioning_date_invalid",
        ),
    ];
    for (request, expected) in cases {
        match service.create(request).await {
            Err(SystemLifecycleError::InvalidField(code)) => assert_eq!(code, expected),
            other => return Err(format!("expected {expected}, got {other:?}").into()),
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_update_replaces_details_and_keeps_an_unstated_interval() -> Result<(), Box<dyn Error>> {
    let (service, account_id, actor) = service();
    let created = service
        .create(CreateSystem {
            details: SystemDetails {
                status_interval_seconds: Some(60),
                ..SystemDetails::default()
            },
            ..name_only(account_id, actor)
        })
        .await?;
    let updated = service
        .update(UpdateSystem {
            id: created.id,
            actor,
            expected_version: created.version,
            name: "Roof".to_owned(),
            timezone: "Europe/Vienna".to_owned(),
            visibility: Visibility::Private,
            details: SystemDetails {
                location_label: Some("Graz".to_owned()),
                latitude_microdegrees: Some(47_070_000),
                longitude_microdegrees: Some(15_440_000),
                declared_capacity_watts: Some(9_900),
                ..SystemDetails::default()
            },
        })
        .await?;

    assert_eq!(updated.status_interval_seconds, 60);
    assert_eq!(updated.location_label.as_deref(), Some("Graz"));
    assert_eq!(updated.latitude_microdegrees, Some(47_070_000));
    assert_eq!(updated.declared_capacity_watts, Some(9_900));
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
