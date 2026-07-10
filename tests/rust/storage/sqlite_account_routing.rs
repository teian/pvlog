//! Bounded and hardened `SQLite` account pool routing contracts.

use std::{error::Error, time::Duration};

use pvlog_domain::AccountId;
use pvlog_storage::{
    DatabaseTarget, SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SqliteCheckpointMode, SqliteRoutingError, apply_migrations,
};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

#[tokio::test]
async fn routing_is_lazy_bounded_and_evicts_unreferenced_idle_pools() -> Result<(), Box<dyn Error>>
{
    let setup = RoutingSetup::new(2).await?;
    let router = setup.router(1, Duration::ZERO)?;
    let first = router.route(setup.accounts[0]).await?;
    assert_eq!(router.open_pool_count().await, 1);
    assert_eq!(first.pooled_connection_count(), 0);
    assert!(matches!(
        router.route(setup.accounts[1]).await,
        Err(SqliteRoutingError::PoolCapacity { limit: 1 })
    ));

    drop(first);
    assert_eq!(router.evict_idle().await, 1);
    let second = router.route(setup.accounts[1]).await?;
    assert_eq!(router.open_pool_count().await, 1);
    drop(second);
    Ok(())
}

#[tokio::test]
async fn account_connections_enforce_pragmas_binding_and_serialized_writers()
-> Result<(), Box<dyn Error>> {
    let setup = RoutingSetup::new(1).await?;
    let router = setup.router(2, Duration::from_mins(1))?;
    let account = router.route(setup.accounts[0]).await?;
    assert_eq!(account.pooled_connection_count(), 0);
    let mut connection = account.acquire().await?;
    assert!(account.pooled_connection_count() >= 1);
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await?;
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *connection)
        .await?;
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode, "wal");
    assert_eq!(busy_timeout, 1_234);
    drop(connection);

    let first_writer = account.acquire_writer().await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), account.acquire_writer())
            .await
            .is_err()
    );
    drop(first_writer);
    let _second_writer =
        tokio::time::timeout(Duration::from_secs(1), account.acquire_writer()).await??;
    Ok(())
}

#[tokio::test]
async fn checkpoints_integrity_and_lifecycle_changes_are_enforced() -> Result<(), Box<dyn Error>> {
    let setup = RoutingSetup::new(1).await?;
    let router = setup.router(2, Duration::from_mins(1))?;
    let account = router.route(setup.accounts[0]).await?;
    {
        let mut writer = account.acquire_writer().await?;
        sqlx::query(
            "INSERT INTO account_audit_events \
             (id, occurred_at, actor_type, action, target_type, outcome, event_hash) \
             VALUES (?, 1, 'worker', 'routing.test', 'account', 'succeeded', ?)",
        )
        .bind(AccountId::new().as_uuid().as_bytes().as_slice())
        .bind(vec![7_u8; 32])
        .execute(writer.connection())
        .await?;
    }
    let checkpoint = account.checkpoint(SqliteCheckpointMode::Passive).await?;
    assert!(checkpoint.busy >= 0);
    assert!(checkpoint.log_frames >= checkpoint.checkpointed_frames);
    account.integrity_probe().await?;

    setup
        .set_lifecycle(setup.accounts[0], "unavailable")
        .await?;
    assert!(matches!(
        router.route(setup.accounts[0]).await,
        Err(SqliteRoutingError::RouteInactive { lifecycle, .. }) if lifecycle == "unavailable"
    ));
    Ok(())
}

#[tokio::test]
async fn routing_rejects_non_file_opaque_locators_beneath_the_managed_root()
-> Result<(), Box<dyn Error>> {
    let setup = RoutingSetup::new(1).await?;
    let unsafe_locator = "directory.sqlite3";
    tokio::fs::create_dir(setup.accounts_dir.join(unsafe_locator)).await?;
    let mut management = setup.management_connection().await?;
    sqlx::query("UPDATE account_database_registry SET opaque_locator = ? WHERE account_id = ?")
        .bind(unsafe_locator)
        .bind(setup.accounts[0].as_uuid().as_bytes().as_slice())
        .execute(&mut management)
        .await?;
    management.close().await?;

    let router = setup.router(2, Duration::from_mins(1))?;
    assert!(matches!(
        router.route(setup.accounts[0]).await,
        Err(SqliteRoutingError::UnsafeLocator(locator)) if locator == unsafe_locator
    ));
    Ok(())
}

struct RoutingSetup {
    _directory: TempDir,
    management_path: std::path::PathBuf,
    accounts_dir: std::path::PathBuf,
    accounts: Vec<AccountId>,
}

impl RoutingSetup {
    async fn new(account_count: usize) -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        apply_migrations(&DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        })
        .await?;
        let mut management = sqlite_connection(&management_path).await?;
        let mut accounts = Vec::with_capacity(account_count);
        for index in 0..account_count {
            let account_id = AccountId::new();
            sqlx::query(
                "INSERT INTO accounts \
                 (id, slug, display_name, status, created_at, updated_at) \
                 VALUES (?, ?, ?, 'provisioning', 1, 1)",
            )
            .bind(account_id.as_uuid().as_bytes().as_slice())
            .bind(format!("routing-{index}-{account_id}"))
            .bind(format!("Routing {index}"))
            .execute(&mut management)
            .await?;
            accounts.push(account_id);
        }
        management.close().await?;
        let provisioner =
            SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone());
        for account_id in &accounts {
            provisioner.provision(*account_id).await?;
        }
        Ok(Self {
            _directory: directory,
            management_path,
            accounts_dir,
            accounts,
        })
    }

    fn router(
        &self,
        max_open_account_pools: usize,
        idle_pool_timeout: Duration,
    ) -> Result<SqliteAccountPoolRouter, SqliteRoutingError> {
        SqliteAccountPoolRouter::new(
            self.management_path.clone(),
            self.accounts_dir.clone(),
            SqliteAccountPoolConfig {
                max_open_account_pools,
                max_connections_per_account: 2,
                busy_timeout: Duration::from_millis(1_234),
                acquire_timeout: Duration::from_secs(1),
                idle_pool_timeout,
            },
        )
    }

    async fn set_lifecycle(
        &self,
        account_id: AccountId,
        lifecycle: &str,
    ) -> Result<(), sqlx::Error> {
        let mut management = self.management_connection().await?;
        sqlx::query(
            "UPDATE account_database_registry SET lifecycle_state = ? WHERE account_id = ?",
        )
        .bind(lifecycle)
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .execute(&mut management)
        .await?;
        management.close().await?;
        Ok(())
    }

    async fn management_connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        sqlite_connection(&self.management_path).await
    }
}

async fn sqlite_connection(path: &std::path::Path) -> Result<SqliteConnection, sqlx::Error> {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await
}
