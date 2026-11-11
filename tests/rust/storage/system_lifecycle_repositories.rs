use std::{error::Error, sync::Arc};

use pvlog_application::{
    Clock, CreateSystem, RbacRepository, SystemDetails, SystemLifecycleRecord,
    SystemLifecycleRepository as _, SystemLifecycleService, SystemLifecycleUseCases, UpdateSystem,
    built_in_account_roles,
};
use pvlog_domain::{
    AccountId, BuiltInRole, MembershipId, PrincipalId, RoleKind, RoleScope, SystemId,
    SystemLifecycle, UserId, UtcTimestamp, Visibility,
};
use pvlog_storage::{
    AccountRecord, DatabaseTarget, ManagementRepository, MembershipRecord,
    PostgresManagementRepository, PostgresSystemLifecycleRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteManagementRepository,
    SqliteRbacRepository, SqliteSystemLifecycleRepository, UserRecord, apply_migrations,
};
use sqlx::{Connection as _, PgConnection};
use tempfile::TempDir;

const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn sqlite_system_lifecycle_uses_registry_and_optimistic_concurrency()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    })
    .await?;
    let management = Arc::new(SqliteManagementRepository::new(management_path.clone()));
    let user_id = UserId::new();
    let account_id = AccountId::new();
    management
        .save_user(&UserRecord {
            id: user_id,
            email: "owner@example.test".to_owned(),
            display_name: "Owner".to_owned(),
            status: "active".to_owned(),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    management
        .save_account(&AccountRecord {
            id: account_id,
            slug: "owner-account".to_owned(),
            display_name: "Owner account".to_owned(),
            status: "provisioning".to_owned(),
            created_by: Some(user_id),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    management
        .save_membership(&MembershipRecord {
            id: MembershipId::new(),
            account_id,
            user_id,
            status: "active".to_owned(),
            joined_at: Some(NOW),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    let rbac = Arc::new(SqliteRbacRepository::new(management_path.clone()));
    for role in built_in_account_roles(account_id, user_id, NOW) {
        rbac.save_role(&role).await?;
    }
    SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone())
        .provision(account_id)
        .await?;
    let router = SqliteAccountPoolRouter::new(
        management_path,
        accounts_dir,
        SqliteAccountPoolConfig::default(),
    )?;
    let service = SystemLifecycleService::new(
        Arc::new(SqliteSystemLifecycleRepository::new(
            router,
            management.clone(),
        )),
        rbac.clone(),
        Arc::new(FixedClock),
    );
    let created = service
        .create_system(CreateSystem {
            account_id,
            actor: user_id,
            name: "Roof".to_owned(),
            timezone: Some("Europe/Berlin".to_owned()),
            client_timezone: None,
            details: SystemDetails {
                description: "Garage".to_owned(),
                commissioning_date: Some("2019-04-15".to_owned()),
                location_label: Some("Leipzig".to_owned()),
                country_code: Some("DE".to_owned()),
                latitude_microdegrees: Some(51_340_000),
                longitude_microdegrees: Some(12_370_000),
                status_interval_seconds: Some(600),
                declared_capacity_watts: Some(9_900),
            },
        })
        .await?;
    assert_eq!(created.lifecycle, SystemLifecycle::Active);
    assert_eq!(
        service.system(created.id).await?,
        created,
        "every setup field is read back as it was stored"
    );
    let assignments = rbac
        .active_assignments(PrincipalId::User(user_id), NOW + 1)
        .await?;
    let owner_assignment = assignments
        .iter()
        .find(|assignment| {
            assignment.scope
                == (RoleScope::System {
                    account_id,
                    system_id: created.id,
                })
        })
        .ok_or("system owner assignment missing")?;
    assert!(
        rbac.role(owner_assignment.role_id)
            .await?
            .is_some_and(|record| {
                record.role.kind == RoleKind::BuiltIn(BuiltInRole::AccountOwner)
            })
    );
    assert_eq!(
        management
            .system_registry(created.id)
            .await?
            .map(|record| record.account_id),
        Some(account_id)
    );
    let updated = service
        .update_system(UpdateSystem {
            id: created.id,
            actor: user_id,
            expected_version: created.version,
            name: "South roof".to_owned(),
            timezone: "Europe/Berlin".to_owned(),
            visibility: Visibility::Unlisted,
            details: SystemDetails {
                location_label: Some("Halle".to_owned()),
                latitude_microdegrees: Some(51_480_000),
                longitude_microdegrees: Some(11_970_000),
                declared_capacity_watts: Some(12_000),
                ..SystemDetails::default()
            },
        })
        .await?;
    assert_eq!(updated.version, 2);
    let stored = service.system(created.id).await?;
    assert_eq!(stored.location_label.as_deref(), Some("Halle"));
    assert_eq!(stored.longitude_microdegrees, Some(11_970_000));
    assert_eq!(stored.declared_capacity_watts, Some(12_000));
    assert_eq!(
        stored.status_interval_seconds, 600,
        "an unstated interval is kept"
    );
    assert_eq!(stored.commissioning_date, None);
    assert!(
        service
            .update_system(UpdateSystem {
                details: SystemDetails::default(),
                id: created.id,
                actor: user_id,
                expected_version: created.version,
                name: "Stale".to_owned(),
                timezone: "Europe/Berlin".to_owned(),
                visibility: Visibility::Private,
            })
            .await
            .is_err()
    );
    Ok(())
}

/// The server database stores and returns the same setup fields as the embedded one.
#[tokio::test]
async fn postgres_system_lifecycle_keeps_setup_fields_when_configured() -> Result<(), Box<dyn Error>>
{
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let account_id = AccountId::new();
    let mut connection = PgConnection::connect(&url).await?;
    sqlx::query(
        "INSERT INTO management.accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES ($1,$2,'Lifecycle','active',1,1)",
    )
    .bind(account_id.as_uuid())
    .bind(format!("lifecycle-{account_id}"))
    .execute(&mut connection)
    .await?;
    connection.close().await?;

    let management = Arc::new(PostgresManagementRepository::new(url.clone()));
    let repository = PostgresSystemLifecycleRepository::new(url, management);
    let created = SystemLifecycleRecord {
        id: SystemId::new(),
        account_id,
        name: "Roof".to_owned(),
        description: "Garage".to_owned(),
        commissioning_date: Some("2019-04-15".to_owned()),
        timezone: "Europe/Berlin".to_owned(),
        location_label: Some("Leipzig".to_owned()),
        country_code: Some("DE".to_owned()),
        latitude_microdegrees: Some(51_340_000),
        longitude_microdegrees: Some(12_370_000),
        status_interval_seconds: 600,
        declared_capacity_watts: Some(9_900),
        visibility: Visibility::Private,
        lifecycle: SystemLifecycle::Active,
        version: 1,
        created_at: NOW,
        updated_at: NOW,
    };
    repository.create(created.clone()).await?;
    assert_eq!(repository.system(created.id).await?, Some(created.clone()));

    let changed = SystemLifecycleRecord {
        location_label: None,
        country_code: None,
        latitude_microdegrees: None,
        longitude_microdegrees: None,
        status_interval_seconds: 60,
        declared_capacity_watts: Some(12_000),
        commissioning_date: None,
        version: 2,
        ..created.clone()
    };
    assert!(repository.save(changed.clone(), 1).await?);
    assert_eq!(repository.system(created.id).await?, Some(changed));
    assert!(
        !repository.save(created, 1).await?,
        "a stale version changes nothing"
    );
    Ok(())
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(NOW))
    }
}
