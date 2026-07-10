//! Recoverable lifecycle for isolated `SQLite` account databases.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use pvlog_domain::AccountId;
use serde::Serialize;
use sqlx::{Connection as _, Row as _};
use thiserror::Error;
use uuid::Uuid;

use crate::MigrationError;

const PROVISIONING_LEASE_MILLIS: i64 = 15 * 60 * 1_000;
use crate::migrations::{
    apply_sqlite_account_database, is_sqlite_database, open_sqlite, sqlite_account_schema_version,
};

/// Durable account database lifecycle visible to operators.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountDatabaseLifecycle {
    /// Routing is disabled while a new database is built.
    Provisioning,
    /// The verified database is available for routing.
    Active,
    /// Routing is disabled because verification or migration failed.
    Unavailable,
    /// The database file has been moved out of the active data directory.
    Quarantined,
}

/// Result of a provisioning or deprovisioning state transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProvisioningResult {
    /// Canonical `UUIDv7` account identifier.
    pub account_id: String,
    /// Opaque basename managed beneath the configured account data root.
    pub opaque_locator: String,
    /// Resulting durable lifecycle.
    pub lifecycle: AccountDatabaseLifecycle,
    /// Quarantine basename when deprovisioning moved a file.
    pub quarantine_locator: Option<String>,
}

/// Outcome of reconciling durable state with files on disk.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationReport {
    /// Accounts whose interrupted transition was completed.
    pub resumed_accounts: Vec<String>,
    /// Active accounts disabled because their file is missing or invalid.
    pub unavailable_accounts: Vec<String>,
    /// Unknown files moved out of the active directory without guessing ownership.
    pub quarantined_orphans: Vec<String>,
    /// Safe per-account failures that require operator action.
    pub failures: Vec<String>,
}

/// Manages the split `SQLite` management/account topology through recoverable transitions.
#[derive(Clone, Debug)]
pub struct SqliteAccountProvisioner {
    management_path: PathBuf,
    accounts_dir: PathBuf,
}

impl SqliteAccountProvisioner {
    /// Creates a provisioner rooted at the configured management and account paths.
    #[must_use]
    pub fn new(management_path: PathBuf, accounts_dir: PathBuf) -> Self {
        Self {
            management_path,
            accounts_dir,
        }
    }

    /// Creates, migrates, binds, verifies, and atomically activates one account database.
    ///
    /// Repeating this operation for an already active and valid account is idempotent. Incomplete
    /// states resume from their durable registry entry and deterministic temporary path.
    ///
    /// # Errors
    ///
    /// Returns an error when the account is unknown, its durable state is unsafe to resume, a path
    /// escapes the managed root, migration fails, or integrity/account binding cannot be verified.
    pub async fn provision(
        &self,
        account_id: AccountId,
    ) -> Result<AccountProvisioningResult, SqliteProvisioningError> {
        let result = self.provision_inner(account_id).await;
        if let Err(error) = &result
            && !matches!(error, SqliteProvisioningError::ProvisioningLeaseHeld(_))
        {
            let _mark_failure = self
                .mark_unavailable(account_id, error.code(), &error.to_string())
                .await;
        }
        result
    }

    /// Disables routing and atomically moves an account database into quarantine.
    ///
    /// This method deliberately does not delete the quarantined file; retention-aware deletion is
    /// a separate operator action.
    ///
    /// # Errors
    ///
    /// Returns an error when routing metadata is absent/unsafe, the active file is missing, or the
    /// quarantine transition cannot be persisted.
    pub async fn deprovision(
        &self,
        account_id: AccountId,
    ) -> Result<AccountProvisioningResult, SqliteProvisioningError> {
        tokio::fs::create_dir_all(self.quarantine_dir()).await?;
        let registry = self.registry_entry(account_id).await?;
        let locator = validate_locator(&registry.opaque_locator)?;
        let active_path = self.accounts_dir.join(&locator);
        let quarantine_locator = format!("{locator}.quarantine");
        let quarantine_path = self.quarantine_dir().join(&quarantine_locator);
        if registry.lifecycle == "quarantined" && quarantine_path.is_file() {
            return Ok(AccountProvisioningResult {
                account_id: account_id.to_string(),
                opaque_locator: locator,
                lifecycle: AccountDatabaseLifecycle::Quarantined,
                quarantine_locator: Some(quarantine_locator),
            });
        }
        if !active_path.is_file() {
            if quarantine_path.is_file() {
                self.set_quarantined(account_id).await?;
                return Ok(AccountProvisioningResult {
                    account_id: account_id.to_string(),
                    opaque_locator: locator,
                    lifecycle: AccountDatabaseLifecycle::Quarantined,
                    quarantine_locator: Some(quarantine_locator),
                });
            }
            self.mark_unavailable(
                account_id,
                "account_file_missing",
                "active account file is missing",
            )
            .await?;
            return Err(SqliteProvisioningError::ActiveFileMissing(
                account_id.to_string(),
            ));
        }

        self.set_deleting(account_id).await?;
        if quarantine_path.exists() {
            return Err(SqliteProvisioningError::QuarantineCollision(
                quarantine_locator,
            ));
        }
        tokio::fs::rename(&active_path, &quarantine_path).await?;
        self.set_quarantined(account_id).await?;
        Ok(AccountProvisioningResult {
            account_id: account_id.to_string(),
            opaque_locator: locator,
            lifecycle: AccountDatabaseLifecycle::Quarantined,
            quarantine_locator: Some(quarantine_locator),
        })
    }

    /// Reconciles interrupted transitions, missing active files, and orphaned account files.
    ///
    /// # Errors
    ///
    /// Returns an error only when the management registry or managed directories cannot be read;
    /// individual account failures are collected in the returned report.
    pub async fn reconcile(&self) -> Result<ReconciliationReport, SqliteProvisioningError> {
        tokio::fs::create_dir_all(&self.accounts_dir).await?;
        tokio::fs::create_dir_all(self.provisioning_dir()).await?;
        tokio::fs::create_dir_all(self.quarantine_dir()).await?;
        let entries = self.registry_entries().await?;
        let registered_locators = entries
            .iter()
            .map(|entry| entry.opaque_locator.clone())
            .collect::<BTreeSet<_>>();
        let mut report = ReconciliationReport::default();

        for entry in entries {
            let account_text = entry.account_id.to_string();
            match entry.lifecycle.as_str() {
                "reserved" | "creating" | "migrating" | "verifying" => {
                    match self.provision(entry.account_id).await {
                        Ok(_) => report.resumed_accounts.push(account_text),
                        Err(error) => report.failures.push(format!("{account_text}: {error}")),
                    }
                }
                "deleting" => match self.deprovision(entry.account_id).await {
                    Ok(_) => report.resumed_accounts.push(account_text),
                    Err(error) => report.failures.push(format!("{account_text}: {error}")),
                },
                "active" => {
                    let locator = validate_locator(&entry.opaque_locator)?;
                    let path = self.accounts_dir.join(locator);
                    if !path.is_file()
                        || verify_account_database(&path, entry.account_id)
                            .await
                            .is_err()
                    {
                        self.mark_unavailable(
                            entry.account_id,
                            "account_file_unavailable",
                            "active account file is missing or failed verification",
                        )
                        .await?;
                        report.unavailable_accounts.push(account_text);
                    }
                }
                "unavailable" | "quarantined" => {}
                other => report
                    .failures
                    .push(format!("{account_text}: unsupported lifecycle {other}")),
            }
        }

        let mut files = tokio::fs::read_dir(&self.accounts_dir).await?;
        while let Some(entry) = files.next_entry().await? {
            if !entry.file_type().await?.is_file() || !is_sqlite_database(&entry.path()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if registered_locators.contains(&name) {
                continue;
            }
            let quarantine_name = format!("orphan-{}-{name}.quarantine", Uuid::now_v7());
            tokio::fs::rename(entry.path(), self.quarantine_dir().join(&quarantine_name)).await?;
            report.quarantined_orphans.push(name);
        }
        report.resumed_accounts.sort();
        report.unavailable_accounts.sort();
        report.quarantined_orphans.sort();
        report.failures.sort();
        Ok(report)
    }

    async fn provision_inner(
        &self,
        account_id: AccountId,
    ) -> Result<AccountProvisioningResult, SqliteProvisioningError> {
        tokio::fs::create_dir_all(&self.accounts_dir).await?;
        tokio::fs::create_dir_all(self.provisioning_dir()).await?;
        tokio::fs::create_dir_all(self.quarantine_dir()).await?;
        let registry = self.reserve(account_id).await?;
        let locator = validate_locator(&registry.opaque_locator)?;
        let final_path = self.accounts_dir.join(&locator);

        if registry.lifecycle == "active" {
            if !final_path.is_file() {
                return Err(SqliteProvisioningError::ActiveFileMissing(
                    account_id.to_string(),
                ));
            }
            verify_account_database(&final_path, account_id).await?;
            return Ok(active_result(account_id, locator));
        }
        if matches!(
            registry.lifecycle.as_str(),
            "unavailable" | "quarantined" | "deleting"
        ) {
            return Err(SqliteProvisioningError::UnsafeLifecycle {
                account_id: account_id.to_string(),
                lifecycle: registry.lifecycle,
            });
        }
        self.acquire_provisioning_lease(account_id).await?;
        if final_path.is_file() {
            verify_account_database(&final_path, account_id).await?;
            self.activate(account_id).await?;
            return Ok(active_result(account_id, locator));
        }

        let temporary_path = self.temporary_path(account_id);
        self.set_phase(account_id, "creating", "pending").await?;
        self.set_phase(account_id, "migrating", "running").await?;
        apply_sqlite_account_database(&temporary_path).await?;
        bind_account_database(&temporary_path, account_id).await?;
        self.set_phase(account_id, "verifying", "running").await?;
        verify_account_database(&temporary_path, account_id).await?;
        if final_path.exists() {
            return Err(SqliteProvisioningError::ActivationCollision(locator));
        }
        tokio::fs::rename(&temporary_path, &final_path).await?;
        self.activate(account_id).await?;
        Ok(active_result(account_id, locator))
    }

    async fn reserve(
        &self,
        account_id: AccountId,
    ) -> Result<RegistryEntry, SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let mut transaction = connection.begin().await?;
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM accounts WHERE id = ?")
            .bind(account_bytes(account_id))
            .fetch_optional(&mut *transaction)
            .await?;
        if status.is_none() {
            return Err(SqliteProvisioningError::AccountNotFound(
                account_id.to_string(),
            ));
        }
        let locator = format!("{account_id}.sqlite3");
        let now = epoch_millis()?;
        sqlx::query(
            "INSERT INTO account_database_registry \
             (account_id, opaque_locator, lifecycle_state, migration_state, created_at, updated_at) \
             VALUES (?, ?, 'reserved', 'pending', ?, ?) \
             ON CONFLICT(account_id) DO NOTHING",
        )
        .bind(account_bytes(account_id))
        .bind(&locator)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let row = sqlx::query(
            "SELECT opaque_locator, lifecycle_state FROM account_database_registry \
             WHERE account_id = ?",
        )
        .bind(account_bytes(account_id))
        .fetch_one(&mut *transaction)
        .await?;
        let existing_locator: String = row.get("opaque_locator");
        if existing_locator != locator {
            return Err(SqliteProvisioningError::LocatorMismatch {
                expected: locator,
                actual: existing_locator,
            });
        }
        let lifecycle: String = row.get("lifecycle_state");
        if lifecycle != "active" {
            sqlx::query("UPDATE accounts SET status = 'provisioning', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(account_bytes(account_id))
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(RegistryEntry {
            account_id,
            opaque_locator: existing_locator,
            lifecycle,
        })
    }

    async fn registry_entry(
        &self,
        account_id: AccountId,
    ) -> Result<RegistryEntry, SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let row = sqlx::query(
            "SELECT opaque_locator, lifecycle_state FROM account_database_registry \
             WHERE account_id = ?",
        )
        .bind(account_bytes(account_id))
        .fetch_optional(&mut connection)
        .await?
        .ok_or_else(|| SqliteProvisioningError::RegistryMissing(account_id.to_string()))?;
        let entry = RegistryEntry {
            account_id,
            opaque_locator: row.get("opaque_locator"),
            lifecycle: row.get("lifecycle_state"),
        };
        connection.close().await?;
        Ok(entry)
    }

    async fn registry_entries(&self) -> Result<Vec<RegistryEntry>, SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let rows = sqlx::query(
            "SELECT account_id, opaque_locator, lifecycle_state \
             FROM account_database_registry ORDER BY account_id",
        )
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.into_iter()
            .map(|row| {
                let bytes: Vec<u8> = row.get("account_id");
                let account_id = parse_account_id(&bytes)?;
                Ok(RegistryEntry {
                    account_id,
                    opaque_locator: row.get("opaque_locator"),
                    lifecycle: row.get("lifecycle_state"),
                })
            })
            .collect()
    }

    async fn set_phase(
        &self,
        account_id: AccountId,
        lifecycle: &str,
        migration_state: &str,
    ) -> Result<(), SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        sqlx::query(
            "UPDATE account_database_registry SET lifecycle_state = ?, migration_state = ?, \
             schema_version = ?, last_error_code = NULL, last_error_safe_detail = NULL, \
             updated_at = ? WHERE account_id = ?",
        )
        .bind(lifecycle)
        .bind(migration_state)
        .bind(sqlite_account_schema_version())
        .bind(epoch_millis()?)
        .bind(account_bytes(account_id))
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn acquire_provisioning_lease(
        &self,
        account_id: AccountId,
    ) -> Result<(), SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let now = epoch_millis()?;
        let owner = Uuid::now_v7().to_string();
        let result = sqlx::query(
            "UPDATE account_database_registry SET migration_owner = ?, \
             migration_lease_expires_at = ?, updated_at = ? WHERE account_id = ? \
             AND (migration_owner IS NULL OR migration_lease_expires_at <= ?)",
        )
        .bind(owner)
        .bind(now.saturating_add(PROVISIONING_LEASE_MILLIS))
        .bind(now)
        .bind(account_bytes(account_id))
        .bind(now)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(SqliteProvisioningError::ProvisioningLeaseHeld(
                account_id.to_string(),
            ))
        }
    }

    async fn activate(&self, account_id: AccountId) -> Result<(), SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let mut transaction = connection.begin().await?;
        let now = epoch_millis()?;
        sqlx::query(
            "UPDATE account_database_registry SET lifecycle_state = 'active', \
             migration_state = 'ready', schema_version = ?, migration_owner = NULL, \
             migration_lease_expires_at = NULL, last_error_code = NULL, \
             last_error_safe_detail = NULL, activated_at = COALESCE(activated_at, ?), \
             updated_at = ? WHERE account_id = ?",
        )
        .bind(sqlite_account_schema_version())
        .bind(now)
        .bind(now)
        .bind(account_bytes(account_id))
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE accounts SET status = 'active', updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(account_bytes(account_id))
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn set_deleting(&self, account_id: AccountId) -> Result<(), SqliteProvisioningError> {
        self.update_lifecycle(account_id, "deleting", "pending", "deleting")
            .await
    }

    async fn set_quarantined(&self, account_id: AccountId) -> Result<(), SqliteProvisioningError> {
        self.update_lifecycle(account_id, "quarantined", "ready", "quarantined")
            .await
    }

    async fn update_lifecycle(
        &self,
        account_id: AccountId,
        lifecycle: &str,
        migration_state: &str,
        account_status: &str,
    ) -> Result<(), SqliteProvisioningError> {
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let mut transaction = connection.begin().await?;
        let now = epoch_millis()?;
        sqlx::query(
            "UPDATE account_database_registry SET lifecycle_state = ?, migration_state = ?, \
             updated_at = ? WHERE account_id = ?",
        )
        .bind(lifecycle)
        .bind(migration_state)
        .bind(now)
        .bind(account_bytes(account_id))
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE accounts SET status = ?, updated_at = ? WHERE id = ?")
            .bind(account_status)
            .bind(now)
            .bind(account_bytes(account_id))
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn mark_unavailable(
        &self,
        account_id: AccountId,
        code: &str,
        safe_detail: &str,
    ) -> Result<(), SqliteProvisioningError> {
        if !self.management_path.is_file() {
            return Ok(());
        }
        let mut connection = open_sqlite(&self.management_path, false).await?;
        let mut transaction = connection.begin().await?;
        let now = epoch_millis()?;
        sqlx::query(
            "UPDATE account_database_registry SET lifecycle_state = 'unavailable', \
             migration_state = 'failed', last_error_code = ?, last_error_safe_detail = ?, \
             migration_owner = NULL, migration_lease_expires_at = NULL, updated_at = ? \
             WHERE account_id = ?",
        )
        .bind(code)
        .bind(safe_detail)
        .bind(now)
        .bind(account_bytes(account_id))
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE accounts SET status = 'suspended', updated_at = ? \
             WHERE id = ? AND status NOT IN ('deleted', 'quarantined')",
        )
        .bind(now)
        .bind(account_bytes(account_id))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    fn temporary_path(&self, account_id: AccountId) -> PathBuf {
        self.provisioning_dir()
            .join(format!("{account_id}.sqlite3"))
    }

    fn provisioning_dir(&self) -> PathBuf {
        self.accounts_dir.join(".provisioning")
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.accounts_dir.join(".quarantine")
    }
}

async fn bind_account_database(
    path: &Path,
    account_id: AccountId,
) -> Result<(), SqliteProvisioningError> {
    let mut connection = open_sqlite(path, false).await?;
    let existing: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT account_id FROM pvlog_account_identity WHERE singleton = 1")
            .fetch_optional(&mut connection)
            .await?;
    match existing {
        Some(existing) if existing == account_bytes(account_id) => {}
        Some(_) => {
            return Err(SqliteProvisioningError::AccountBindingMismatch(
                account_id.to_string(),
            ));
        }
        None => {
            sqlx::query(
                "INSERT INTO pvlog_account_identity (singleton, account_id, bound_at) \
                 VALUES (1, ?, ?)",
            )
            .bind(account_bytes(account_id))
            .bind(epoch_millis()?)
            .execute(&mut connection)
            .await?;
        }
    }
    connection.close().await?;
    Ok(())
}

async fn verify_account_database(
    path: &Path,
    account_id: AccountId,
) -> Result<(), SqliteProvisioningError> {
    let mut connection = open_sqlite(path, false).await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut connection)
        .await?;
    if integrity != "ok" {
        return Err(SqliteProvisioningError::IntegrityCheckFailed(integrity));
    }
    let schema_kind: String =
        sqlx::query_scalar("SELECT schema_kind FROM pvlog_schema_identity WHERE singleton = 1")
            .fetch_one(&mut connection)
            .await?;
    if schema_kind != "account" {
        return Err(SqliteProvisioningError::WrongSchemaKind(schema_kind));
    }
    let bound_account: Vec<u8> =
        sqlx::query_scalar("SELECT account_id FROM pvlog_account_identity WHERE singleton = 1")
            .fetch_one(&mut connection)
            .await?;
    if bound_account != account_bytes(account_id) {
        return Err(SqliteProvisioningError::AccountBindingMismatch(
            account_id.to_string(),
        ));
    }
    connection.close().await?;
    Ok(())
}

fn active_result(account_id: AccountId, locator: String) -> AccountProvisioningResult {
    AccountProvisioningResult {
        account_id: account_id.to_string(),
        opaque_locator: locator,
        lifecycle: AccountDatabaseLifecycle::Active,
        quarantine_locator: None,
    }
}

fn validate_locator(locator: &str) -> Result<String, SqliteProvisioningError> {
    let path = Path::new(locator);
    let mut components = path.components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        Ok(locator.to_owned())
    } else {
        Err(SqliteProvisioningError::UnsafeLocator(locator.to_owned()))
    }
}

fn account_bytes(account_id: AccountId) -> Vec<u8> {
    account_id.as_uuid().as_bytes().to_vec()
}

fn parse_account_id(bytes: &[u8]) -> Result<AccountId, SqliteProvisioningError> {
    let uuid = Uuid::from_slice(bytes).map_err(|_| SqliteProvisioningError::InvalidAccountId)?;
    AccountId::from_uuid(uuid).map_err(|_| SqliteProvisioningError::InvalidAccountId)
}

fn epoch_millis() -> Result<i64, SqliteProvisioningError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SqliteProvisioningError::ClockBeforeUnixEpoch)?
        .as_millis();
    i64::try_from(millis).map_err(|_| SqliteProvisioningError::ClockOutOfRange)
}

struct RegistryEntry {
    account_id: AccountId,
    opaque_locator: String,
    lifecycle: String,
}

/// Failure while provisioning, deprovisioning, or reconciling an account database.
#[derive(Debug, Error)]
pub enum SqliteProvisioningError {
    /// Filesystem lifecycle operation failed.
    #[error("account database filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    /// Management or account database access failed.
    #[error("account database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// Account schema migration failed.
    #[error(transparent)]
    Migration(#[from] MigrationError),
    /// The management catalog does not contain the account.
    #[error("account {0} does not exist in the management catalog")]
    AccountNotFound(String),
    /// The management catalog has no routing entry for the account.
    #[error("account {0} has no database registry entry")]
    RegistryMissing(String),
    /// Existing routing metadata does not use the deterministic locator.
    #[error("account database locator mismatch: expected {expected}, found {actual}")]
    LocatorMismatch {
        /// Deterministic locator derived from account `UUIDv7`.
        expected: String,
        /// Stored locator that cannot be trusted for activation.
        actual: String,
    },
    /// A stored locator would escape or traverse the managed root.
    #[error("unsafe account database locator: {0}")]
    UnsafeLocator(String),
    /// A lifecycle requires explicit operator recovery instead of automatic recreation.
    #[error("account {account_id} cannot be provisioned from lifecycle {lifecycle}")]
    UnsafeLifecycle {
        /// Account identifier.
        account_id: String,
        /// Durable lifecycle value.
        lifecycle: String,
    },
    /// Another process owns the non-expired account provisioning lease.
    #[error("account {0} is currently being provisioned by another process")]
    ProvisioningLeaseHeld(String),
    /// An active account has no physical database file.
    #[error("active account database file is missing for {0}")]
    ActiveFileMissing(String),
    /// Another file appeared at the final activation path.
    #[error("account database activation path already exists: {0}")]
    ActivationCollision(String),
    /// A quarantine file already occupies the deterministic retention path.
    #[error("account database quarantine path already exists: {0}")]
    QuarantineCollision(String),
    /// Integrity verification did not return `ok`.
    #[error("account database integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    /// The physical database belongs to another account.
    #[error("account database identity does not match {0}")]
    AccountBindingMismatch(String),
    /// The database has the wrong schema family marker.
    #[error("expected account schema but found {0}")]
    WrongSchemaKind(String),
    /// Registry account bytes are not a valid `UUIDv7`.
    #[error("management registry contains an invalid account identifier")]
    InvalidAccountId,
    /// The system clock cannot represent lifecycle timestamps.
    #[error("system clock is before the Unix epoch")]
    ClockBeforeUnixEpoch,
    /// The system clock cannot fit in the database timestamp representation.
    #[error("system clock is outside the supported timestamp range")]
    ClockOutOfRange,
}

impl SqliteProvisioningError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "filesystem_error",
            Self::Sqlx(_) => "database_error",
            Self::Migration(_) => "migration_error",
            Self::AccountNotFound(_) => "account_not_found",
            Self::RegistryMissing(_) => "registry_missing",
            Self::LocatorMismatch { .. } | Self::UnsafeLocator(_) => "unsafe_locator",
            Self::UnsafeLifecycle { .. } => "unsafe_lifecycle",
            Self::ProvisioningLeaseHeld(_) => "provisioning_lease_held",
            Self::ActiveFileMissing(_) => "account_file_missing",
            Self::ActivationCollision(_) => "activation_collision",
            Self::QuarantineCollision(_) => "quarantine_collision",
            Self::IntegrityCheckFailed(_) => "integrity_check_failed",
            Self::AccountBindingMismatch(_) => "account_binding_mismatch",
            Self::WrongSchemaKind(_) => "wrong_schema_kind",
            Self::InvalidAccountId => "invalid_account_id",
            Self::ClockBeforeUnixEpoch | Self::ClockOutOfRange => "clock_error",
        }
    }
}
