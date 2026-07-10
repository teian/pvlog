//! `SQLite` and `PostgreSQL` persistence adapters for `PVLog`.

#![forbid(unsafe_code)]

use std::{fmt, path::PathBuf};

#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
#[cfg(feature = "sqlite")]
use sqlx::{
    SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use thiserror::Error;

mod migrations;
#[cfg(feature = "sqlite")]
mod provisioning;
#[cfg(feature = "sqlite")]
mod sqlite_router;

pub use migrations::{
    DatabaseMigrationStatus, MigrationError, MigrationKind, MigrationPlanItem, MigrationState,
    apply_migrations, ensure_schema_compatible, migration_plan, migration_status,
};
#[cfg(feature = "sqlite")]
pub use provisioning::{
    AccountDatabaseLifecycle, AccountProvisioningResult, ReconciliationReport,
    SqliteAccountProvisioner, SqliteProvisioningError,
};
#[cfg(feature = "sqlite")]
pub use sqlite_router::{
    RoutedSqliteAccount, SerializedSqliteWriter, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteCheckpointMode, SqliteCheckpointReport, SqliteRoutingError,
};

/// Database topology selected for the current process.
pub enum DatabaseTarget {
    /// Instance management database and directory of account-owned databases.
    Sqlite {
        /// Instance-wide management database file.
        management_path: PathBuf,
        /// Directory holding one database per account.
        accounts_dir: PathBuf,
    },
    /// Shared `PostgreSQL` database.
    Postgres {
        /// Secret connection URL. This value is deliberately omitted from `Debug` output.
        url: String,
    },
}

impl fmt::Debug for DatabaseTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite {
                management_path,
                accounts_dir,
            } => formatter
                .debug_struct("Sqlite")
                .field("management_path", management_path)
                .field("accounts_dir", accounts_dir)
                .finish(),
            Self::Postgres { .. } => formatter
                .debug_struct("Postgres")
                .field("url", &"[REDACTED]")
                .finish(),
        }
    }
}

/// Opens the configured database topology and executes a minimal query.
///
/// `SQLite` probes the management database and every currently provisioned account database.
/// `PostgreSQL` probes the shared database without logging its connection URL.
///
/// # Errors
///
/// Returns an error when the selected adapter is not compiled in, a database cannot be opened,
/// or the probe query fails.
pub async fn probe_database(target: &DatabaseTarget) -> Result<(), ProbeError> {
    match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => probe_sqlite(management_path, accounts_dir).await,
        DatabaseTarget::Postgres { url } => probe_postgres(url).await,
    }?;
    ensure_schema_compatible(target).await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn probe_sqlite(management_path: &PathBuf, accounts_dir: &PathBuf) -> Result<(), ProbeError> {
    if let Some(parent) = management_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::create_dir_all(accounts_dir).await?;

    probe_sqlite_file(management_path).await?;

    let mut account_paths = Vec::new();
    let mut entries = tokio::fs::read_dir(accounts_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_file() && migrations::is_sqlite_database(&entry.path()) {
            account_paths.push(entry.path());
        }
    }
    account_paths.sort();

    for account_path in account_paths {
        probe_sqlite_file(&account_path).await?;
    }

    Ok(())
}

#[cfg(not(feature = "sqlite"))]
async fn probe_sqlite(
    _management_path: &PathBuf,
    _accounts_dir: &PathBuf,
) -> Result<(), ProbeError> {
    Err(ProbeError::AdapterDisabled("sqlite"))
}

#[cfg(feature = "sqlite")]
async fn probe_sqlite_file(path: &PathBuf) -> Result<(), ProbeError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    sqlx::query("SELECT 1").execute(&mut connection).await?;
    connection.close().await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn probe_postgres(url: &str) -> Result<(), ProbeError> {
    let mut connection = PgConnection::connect(url).await?;
    sqlx::query("SELECT 1").execute(&mut connection).await?;
    connection.close().await?;
    Ok(())
}

#[cfg(not(feature = "postgres"))]
async fn probe_postgres(_url: &str) -> Result<(), ProbeError> {
    Err(ProbeError::AdapterDisabled("postgres"))
}

/// Storage startup probe failure.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// Filesystem setup or discovery failed.
    #[error("database filesystem probe failed: {0}")]
    Io(#[from] std::io::Error),
    /// A database connection or query failed.
    #[error("database probe failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// The database schema does not match the migrations embedded in this release.
    #[error(transparent)]
    Migration(#[from] MigrationError),
    /// The binary was compiled without the selected adapter.
    #[error("the {0} database adapter is not enabled in this build")]
    AdapterDisabled(&'static str),
}
