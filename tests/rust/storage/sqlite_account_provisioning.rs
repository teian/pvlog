//! Recoverable `SQLite` account database lifecycle tests.

use std::{error::Error, path::Path, str::FromStr as _};

use pvlog_domain::AccountId;
use pvlog_storage::{
    AccountDatabaseLifecycle, DatabaseTarget, SqliteAccountProvisioner, SqliteProvisioningError,
    apply_migrations,
};
use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

#[tokio::test]
async fn provisioning_binds_verifies_and_atomically_activates_an_opaque_database()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let account_id = setup.create_account("home").await?;

    let result = setup.provisioner.provision(account_id).await?;
    assert_eq!(result.lifecycle, AccountDatabaseLifecycle::Active);
    assert_eq!(result.opaque_locator, format!("{account_id}.sqlite3"));
    assert!(!result.opaque_locator.contains("home"));
    let account_path = setup.accounts_dir.join(&result.opaque_locator);
    assert!(account_path.is_file());
    assert!(
        !setup
            .accounts_dir
            .join(".provisioning")
            .join(&result.opaque_locator)
            .exists()
    );

    let mut account = sqlite_connection(&account_path).await?;
    let bound_id: Vec<u8> =
        sqlx::query_scalar("SELECT account_id FROM pvlog_account_identity WHERE singleton = 1")
            .fetch_one(&mut account)
            .await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut account)
        .await?;
    account.close().await?;
    assert_eq!(bound_id, account_id.as_uuid().as_bytes());
    assert_eq!(integrity, "ok");

    let registry = setup.registry(account_id).await?;
    assert_eq!(registry.0, "active");
    assert_eq!(registry.1, "ready");
    assert_eq!(registry.2, 3);
    assert_eq!(setup.account_status(account_id).await?, "active");

    let repeated = setup.provisioner.provision(account_id).await?;
    assert_eq!(repeated, result);
    Ok(())
}

#[tokio::test]
async fn interrupted_activation_is_resumed_from_the_verified_final_file()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let account_id = setup.create_account("resume").await?;
    let result = setup.provisioner.provision(account_id).await?;
    setup
        .set_registry_state(account_id, "verifying", "running")
        .await?;

    let report = setup.provisioner.reconcile().await?;
    assert_eq!(report.resumed_accounts, vec![account_id.to_string()]);
    assert!(report.failures.is_empty());
    assert!(setup.accounts_dir.join(result.opaque_locator).is_file());
    assert_eq!(setup.registry(account_id).await?.0, "active");
    Ok(())
}

#[tokio::test]
async fn deprovisioning_disables_routing_and_quarantines_without_deleting()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let account_id = setup.create_account("retained").await?;
    let active = setup.provisioner.provision(account_id).await?;

    let quarantined = setup.provisioner.deprovision(account_id).await?;
    assert_eq!(quarantined.lifecycle, AccountDatabaseLifecycle::Quarantined);
    assert!(!setup.accounts_dir.join(&active.opaque_locator).exists());
    let quarantine_locator = quarantined
        .quarantine_locator
        .as_deref()
        .ok_or("quarantine locator is missing")?;
    assert!(
        setup
            .accounts_dir
            .join(".quarantine")
            .join(quarantine_locator)
            .is_file()
    );
    assert_eq!(setup.registry(account_id).await?.0, "quarantined");
    assert_eq!(setup.account_status(account_id).await?, "quarantined");

    let repeated = setup.provisioner.deprovision(account_id).await?;
    assert_eq!(repeated, quarantined);

    setup
        .set_registry_state(account_id, "deleting", "pending")
        .await?;
    let report = setup.provisioner.reconcile().await?;
    assert_eq!(report.resumed_accounts, vec![account_id.to_string()]);
    assert_eq!(setup.registry(account_id).await?.0, "quarantined");
    Ok(())
}

#[tokio::test]
async fn reconciliation_isolates_missing_accounts_and_quarantines_orphans()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let account_id = setup.create_account("missing").await?;
    let active = setup.provisioner.provision(account_id).await?;
    tokio::fs::remove_file(setup.accounts_dir.join(active.opaque_locator)).await?;
    let orphan = setup.accounts_dir.join("unregistered.sqlite3");
    tokio::fs::write(&orphan, []).await?;

    let report = setup.provisioner.reconcile().await?;
    assert_eq!(report.unavailable_accounts, vec![account_id.to_string()]);
    assert_eq!(report.quarantined_orphans, vec!["unregistered.sqlite3"]);
    assert!(report.failures.is_empty());
    assert_eq!(setup.registry(account_id).await?.0, "unavailable");
    assert_eq!(setup.account_status(account_id).await?, "suspended");
    assert!(!orphan.exists());
    Ok(())
}

#[tokio::test]
async fn nonexpired_provisioning_lease_blocks_a_second_process_without_disabling_account()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let account_id = setup.create_account("leased").await?;
    let mut management = sqlite_connection(&setup.management_path).await?;
    sqlx::query(
        "INSERT INTO account_database_registry \
         (account_id, opaque_locator, lifecycle_state, migration_state, migration_owner, \
          migration_lease_expires_at, created_at, updated_at) \
         VALUES (?, ?, 'creating', 'running', 'other-process', 4102444800000, 1, 1)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(format!("{account_id}.sqlite3"))
    .execute(&mut management)
    .await?;
    management.close().await?;

    assert!(matches!(
        setup.provisioner.provision(account_id).await,
        Err(SqliteProvisioningError::ProvisioningLeaseHeld(_))
    ));
    let registry = setup.registry(account_id).await?;
    assert_eq!(registry.0, "creating");
    assert_eq!(registry.1, "running");
    assert_eq!(setup.account_status(account_id).await?, "provisioning");
    Ok(())
}

struct Setup {
    _directory: TempDir,
    management_path: std::path::PathBuf,
    accounts_dir: std::path::PathBuf,
    provisioner: SqliteAccountProvisioner,
}

impl Setup {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        let target = DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        };
        apply_migrations(&target).await?;
        let provisioner =
            SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone());
        Ok(Self {
            _directory: directory,
            management_path,
            accounts_dir,
            provisioner,
        })
    }

    async fn create_account(&self, slug: &str) -> Result<AccountId, Box<dyn Error>> {
        let account_id = AccountId::new();
        let mut management = sqlite_connection(&self.management_path).await?;
        sqlx::query(
            "INSERT INTO accounts \
             (id, slug, display_name, status, created_at, updated_at) \
             VALUES (?, ?, ?, 'provisioning', 1, 1)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(slug)
        .bind(slug)
        .execute(&mut management)
        .await?;
        management.close().await?;
        Ok(account_id)
    }

    async fn registry(
        &self,
        account_id: AccountId,
    ) -> Result<(String, String, i64), Box<dyn Error>> {
        let mut management = sqlite_connection(&self.management_path).await?;
        let row = sqlx::query(
            "SELECT lifecycle_state, migration_state, schema_version \
             FROM account_database_registry WHERE account_id = ?",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .fetch_one(&mut management)
        .await?;
        let result = (
            row.get("lifecycle_state"),
            row.get("migration_state"),
            row.get("schema_version"),
        );
        management.close().await?;
        Ok(result)
    }

    async fn account_status(&self, account_id: AccountId) -> Result<String, Box<dyn Error>> {
        let mut management = sqlite_connection(&self.management_path).await?;
        let status = sqlx::query_scalar("SELECT status FROM accounts WHERE id = ?")
            .bind(account_id.as_uuid().as_bytes().as_slice())
            .fetch_one(&mut management)
            .await?;
        management.close().await?;
        Ok(status)
    }

    async fn set_registry_state(
        &self,
        account_id: AccountId,
        lifecycle: &str,
        migration: &str,
    ) -> Result<(), Box<dyn Error>> {
        let mut management = sqlite_connection(&self.management_path).await?;
        sqlx::query(
            "UPDATE account_database_registry SET lifecycle_state = ?, migration_state = ? \
             WHERE account_id = ?",
        )
        .bind(lifecycle)
        .bind(migration)
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .execute(&mut management)
        .await?;
        management.close().await?;
        Ok(())
    }
}

async fn sqlite_connection(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(path.to_string_lossy().as_ref())?
        .create_if_missing(true)
        .foreign_keys(true);
    SqliteConnection::connect_with(&options).await
}
