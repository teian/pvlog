//! Migration contract tests for `SQLite` and optional `PostgreSQL`.

use std::{error::Error, fs::File, path::Path, str::FromStr as _};

use pvlog_storage::{
    DatabaseTarget, MigrationError, MigrationKind, MigrationState, apply_migrations,
    ensure_schema_compatible, migration_plan, migration_status, probe_database,
};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

#[tokio::test]
async fn sqlite_management_and_accounts_plan_apply_and_verify_independently()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(
        directory.path(),
        &["account-b.sqlite3", "account-a.sqlite3"],
    )?;
    File::create(directory.path().join("accounts/README.txt"))?;

    let initial = migration_status(&target).await?;
    assert_eq!(initial.len(), 3);
    assert_eq!(initial[0].kind, MigrationKind::SqliteManagement);
    assert!(initial.iter().all(|status| !status.compatible));
    assert_eq!(migration_plan(&target).await?.len(), 3);
    assert!(ensure_schema_compatible(&target).await.is_err());

    let applied = apply_migrations(&target).await?;
    assert!(applied.iter().all(|status| status.compatible));
    assert!(applied.iter().all(|status| {
        status
            .migrations
            .iter()
            .all(|migration| migration.state == MigrationState::Applied)
    }));
    assert!(migration_plan(&target).await?.is_empty());
    ensure_schema_compatible(&target).await?;
    probe_database(&target).await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_checksum_changes_fail_closed_at_startup() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &[])?;
    apply_migrations(&target).await?;

    let management_path = directory.path().join("management.sqlite3");
    let mut connection = sqlite_connection(&management_path).await?;
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
        .execute(&mut connection)
        .await?;
    connection.close().await?;

    let status = migration_status(&target).await?;
    assert_eq!(
        status[0].migrations[0].state,
        MigrationState::ChecksumMismatch
    );
    assert!(!status[0].compatible);
    assert!(ensure_schema_compatible(&target).await.is_err());
    assert!(probe_database(&target).await.is_err());
    Ok(())
}

#[tokio::test]
async fn sqlite_nonexpired_lease_blocks_apply_and_expired_lease_is_recovered()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &[])?;
    let management_path = directory.path().join("management.sqlite3");
    let mut connection = sqlite_connection(&management_path).await?;
    sqlx::query(
        "CREATE TABLE _pvlog_migration_lease (\
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1), \
         owner TEXT NOT NULL, expires_at INTEGER NOT NULL)",
    )
    .execute(&mut connection)
    .await?;
    sqlx::query(
        "INSERT INTO _pvlog_migration_lease (singleton, owner, expires_at) \
         VALUES (1, 'other-process', 4102444800)",
    )
    .execute(&mut connection)
    .await?;
    connection.close().await?;

    assert!(matches!(
        apply_migrations(&target).await,
        Err(MigrationError::LeaseHeld { .. })
    ));

    let mut connection = sqlite_connection(&management_path).await?;
    sqlx::query("UPDATE _pvlog_migration_lease SET expires_at = 0 WHERE singleton = 1")
        .execute(&mut connection)
        .await?;
    connection.close().await?;

    let statuses = apply_migrations(&target).await?;
    assert!(statuses[0].compatible);
    Ok(())
}

#[tokio::test]
async fn sqlite_account_failure_does_not_prevent_later_healthy_account_migration()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &["account-a.sqlite3"])?;
    apply_migrations(&target).await?;

    let accounts_dir = directory.path().join("accounts");
    let broken_path = accounts_dir.join("account-a.sqlite3");
    let mut connection = sqlite_connection(&broken_path).await?;
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    File::create(accounts_dir.join("account-b.sqlite3"))?;

    assert!(matches!(
        apply_migrations(&target).await,
        Err(MigrationError::AccountFailures(_))
    ));
    let statuses = migration_status(&target).await?;
    let healthy = statuses
        .iter()
        .find(|status| status.database == "sqlite-account:account-b.sqlite3")
        .ok_or("healthy account migration status is missing")?;
    let broken = statuses
        .iter()
        .find(|status| status.database == "sqlite-account:account-a.sqlite3")
        .ok_or("broken account migration status is missing")?;
    assert!(healthy.compatible);
    assert!(!broken.compatible);
    Ok(())
}

#[tokio::test]
async fn postgres_uses_checksum_verified_migrations_when_configured() -> Result<(), Box<dyn Error>>
{
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    let target = DatabaseTarget::Postgres { url };

    apply_migrations(&target).await?;
    let statuses = migration_status(&target).await?;
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].kind, MigrationKind::Postgres);
    assert!(statuses[0].compatible);
    ensure_schema_compatible(&target).await?;
    Ok(())
}

fn sqlite_target(root: &Path, accounts: &[&str]) -> Result<DatabaseTarget, Box<dyn Error>> {
    let accounts_dir = root.join("accounts");
    std::fs::create_dir_all(&accounts_dir)?;
    for account in accounts {
        File::create(accounts_dir.join(account))?;
    }
    Ok(DatabaseTarget::Sqlite {
        management_path: root.join("management.sqlite3"),
        accounts_dir,
    })
}

async fn sqlite_connection(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
    let options =
        SqliteConnectOptions::from_str(path.to_string_lossy().as_ref())?.create_if_missing(true);
    SqliteConnection::connect_with(&options).await
}
