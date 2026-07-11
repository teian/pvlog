use std::{error::Error, sync::Arc};

use pvlog_application::{
    Clock, CreateSystem, SystemLifecycleService, SystemLifecycleUseCases, UpdateSystem,
};
use pvlog_domain::{AccountId, SystemLifecycle, UserId, UtcTimestamp, Visibility};
use pvlog_storage::{
    AccountRecord, DatabaseTarget, ManagementRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteManagementRepository,
    SqliteSystemLifecycleRepository, UserRecord, apply_migrations,
};
use tempfile::TempDir;

const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
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
        Arc::new(FixedClock),
    );
    let created = service
        .create_system(CreateSystem {
            account_id,
            actor: user_id,
            name: "Roof".to_owned(),
            timezone: "Europe/Berlin".to_owned(),
        })
        .await?;
    assert_eq!(created.lifecycle, SystemLifecycle::Active);
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
        })
        .await?;
    assert_eq!(updated.version, 2);
    assert!(
        service
            .update_system(UpdateSystem {
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

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(NOW))
    }
}
