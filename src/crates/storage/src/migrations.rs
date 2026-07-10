//! Explicit, checksum-verified schema migration orchestration.

use std::{collections::BTreeMap, fmt::Write as _, path::Path};
#[cfg(feature = "sqlite")]
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _, migrate::Migrator};
#[cfg(feature = "sqlite")]
use sqlx::{
    SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use thiserror::Error;
#[cfg(feature = "sqlite")]
use uuid::Uuid;

use crate::DatabaseTarget;

#[cfg(feature = "sqlite")]
const MIGRATIONS_TABLE: &str = "_sqlx_migrations";
#[cfg(feature = "sqlite")]
const SQLITE_LEASE_SECONDS: i64 = 15 * 60;

#[cfg(feature = "postgres")]
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");
#[cfg(feature = "sqlite")]
static SQLITE_MANAGEMENT_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite-management");
#[cfg(feature = "sqlite")]
static SQLITE_ACCOUNT_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite-account");

/// Schema family targeted by a migration catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationKind {
    /// Shared `PostgreSQL` schema.
    Postgres,
    /// Instance-wide `SQLite` management catalog.
    SqliteManagement,
    /// Isolated `SQLite` account data database.
    SqliteAccount,
}

/// State of one migration relative to a database.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationState {
    /// The embedded migration was applied with the expected checksum.
    Applied,
    /// The embedded migration has not been applied yet.
    Pending,
    /// An applied migration has been modified since it ran.
    ChecksumMismatch,
    /// The database contains a migration unknown to this release.
    Unknown,
    /// A previous non-transactional migration did not finish successfully.
    Dirty,
}

/// Status of one known or database-reported migration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatusEntry {
    /// Monotonically increasing migration version.
    pub version: i64,
    /// Stable migration description.
    pub description: String,
    /// Lowercase checksum of the migration SQL.
    pub checksum: String,
    /// Relationship between embedded and applied migration state.
    pub state: MigrationState,
}

/// Compatibility status for one physical database.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseMigrationStatus {
    /// Safe operator-facing database label; `PostgreSQL` URLs are never included.
    pub database: String,
    /// Schema family expected for this database.
    pub kind: MigrationKind,
    /// Highest version currently recorded by the database.
    pub current_version: Option<i64>,
    /// Highest version embedded in this release.
    pub target_version: Option<i64>,
    /// Whether startup with this release is safe.
    pub compatible: bool,
    /// Deterministically ordered migration details.
    pub migrations: Vec<MigrationStatusEntry>,
}

/// One pending migration returned by the read-only plan command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlanItem {
    /// Safe operator-facing database label.
    pub database: String,
    /// Schema family expected for this database.
    pub kind: MigrationKind,
    /// Migration version that would be applied.
    pub version: i64,
    /// Stable migration description.
    pub description: String,
    /// Lowercase checksum of the migration SQL.
    pub checksum: String,
}

/// Discovers every configured physical database and reports migration compatibility without
/// changing schema state.
///
/// # Errors
///
/// Returns an error when database discovery, connection, or migration metadata reads fail.
pub async fn migration_status(
    target: &DatabaseTarget,
) -> Result<Vec<DatabaseMigrationStatus>, MigrationError> {
    match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => sqlite_status(management_path, accounts_dir).await,
        DatabaseTarget::Postgres { url } => postgres_status(url).await,
    }
}

/// Returns the deterministic list of migrations that an apply command would execute.
///
/// # Errors
///
/// Returns an error when current migration status cannot be inspected.
pub async fn migration_plan(
    target: &DatabaseTarget,
) -> Result<Vec<MigrationPlanItem>, MigrationError> {
    let statuses = migration_status(target).await?;
    let mut plan = Vec::new();
    for status in statuses {
        for migration in status
            .migrations
            .into_iter()
            .filter(|migration| migration.state == MigrationState::Pending)
        {
            plan.push(MigrationPlanItem {
                database: status.database.clone(),
                kind: status.kind,
                version: migration.version,
                description: migration.description,
                checksum: migration.checksum,
            });
        }
    }
    Ok(plan)
}

/// Applies all pending migrations under engine-appropriate migration locks.
///
/// `PostgreSQL` uses the advisory lock built into `SQLx`'s `PostgreSQL` migrator. `SQLite` uses a
/// persistent expiring lease in each management or account database. The management database is
/// migrated first; account failures are collected so a broken account does not prevent later
/// healthy accounts from being processed.
///
/// # Errors
///
/// Returns an error when the management/`PostgreSQL` migration fails or one or more `SQLite`
/// account databases fail after the management migration succeeds.
pub async fn apply_migrations(
    target: &DatabaseTarget,
) -> Result<Vec<DatabaseMigrationStatus>, MigrationError> {
    match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => apply_sqlite(management_path, accounts_dir).await?,
        DatabaseTarget::Postgres { url } => apply_postgres(url).await?,
    }
    migration_status(target).await
}

/// Rejects process startup unless every configured database exactly matches this release.
///
/// # Errors
///
/// Returns [`MigrationError::IncompatibleSchema`] when migrations are pending, dirty, modified,
/// or unknown to the running release.
pub async fn ensure_schema_compatible(target: &DatabaseTarget) -> Result<(), MigrationError> {
    let statuses = migration_status(target).await?;
    let incompatible = statuses
        .iter()
        .filter(|status| !status.compatible)
        .map(|status| status.database.as_str())
        .collect::<Vec<_>>();
    if incompatible.is_empty() {
        Ok(())
    } else {
        Err(MigrationError::IncompatibleSchema(incompatible.join(", ")))
    }
}

#[cfg(feature = "sqlite")]
async fn sqlite_status(
    management_path: &Path,
    accounts_dir: &Path,
) -> Result<Vec<DatabaseMigrationStatus>, MigrationError> {
    let mut statuses = vec![
        inspect_sqlite_database(
            management_path,
            "sqlite-management".to_owned(),
            MigrationKind::SqliteManagement,
            &SQLITE_MANAGEMENT_MIGRATOR,
        )
        .await?,
    ];
    for account_path in discover_account_databases(accounts_dir).await? {
        let label = sqlite_account_label(&account_path);
        statuses.push(
            inspect_sqlite_database(
                &account_path,
                label,
                MigrationKind::SqliteAccount,
                &SQLITE_ACCOUNT_MIGRATOR,
            )
            .await?,
        );
    }
    Ok(statuses)
}

#[cfg(not(feature = "sqlite"))]
async fn sqlite_status(
    _management_path: &Path,
    _accounts_dir: &Path,
) -> Result<Vec<DatabaseMigrationStatus>, MigrationError> {
    Err(MigrationError::AdapterDisabled("sqlite"))
}

#[cfg(feature = "postgres")]
async fn postgres_status(url: &str) -> Result<Vec<DatabaseMigrationStatus>, MigrationError> {
    let mut connection = PgConnection::connect(url).await?;
    let table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&mut connection)
            .await?;
    let applied = if table_exists {
        sqlx::query("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await?
            .into_iter()
            .map(|row| AppliedRow {
                version: row.get("version"),
                checksum: row.get("checksum"),
                success: row.get("success"),
            })
            .collect()
    } else {
        Vec::new()
    };
    connection.close().await?;
    Ok(vec![build_status(
        "postgres".to_owned(),
        MigrationKind::Postgres,
        &POSTGRES_MIGRATOR,
        applied,
    )])
}

#[cfg(not(feature = "postgres"))]
async fn postgres_status(_url: &str) -> Result<Vec<DatabaseMigrationStatus>, MigrationError> {
    Err(MigrationError::AdapterDisabled("postgres"))
}

#[cfg(feature = "sqlite")]
async fn inspect_sqlite_database(
    path: &Path,
    label: String,
    kind: MigrationKind,
    migrator: &Migrator,
) -> Result<DatabaseMigrationStatus, MigrationError> {
    if !path.is_file() {
        return Ok(build_status(label, kind, migrator, Vec::new()));
    }
    let mut connection = open_sqlite(path, false).await?;
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
    )
    .bind(MIGRATIONS_TABLE)
    .fetch_one(&mut connection)
    .await?;
    let applied = if table_exists {
        sqlx::query("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await?
            .into_iter()
            .map(|row| AppliedRow {
                version: row.get("version"),
                checksum: row.get("checksum"),
                success: row.get("success"),
            })
            .collect()
    } else {
        Vec::new()
    };
    connection.close().await?;
    Ok(build_status(label, kind, migrator, applied))
}

fn build_status(
    database: String,
    kind: MigrationKind,
    migrator: &Migrator,
    applied_rows: Vec<AppliedRow>,
) -> DatabaseMigrationStatus {
    let current_version = applied_rows.iter().map(|row| row.version).max();
    let target_version = migrator.iter().map(|migration| migration.version).max();
    let mut applied = applied_rows
        .into_iter()
        .map(|row| (row.version, row))
        .collect::<BTreeMap<_, _>>();
    let mut migrations = Vec::new();

    for migration in migrator.iter() {
        let entry = match applied.remove(&migration.version) {
            Some(row) => MigrationStatusEntry {
                version: migration.version,
                description: migration.description.to_string(),
                checksum: encode_checksum(&migration.checksum),
                state: if !row.success {
                    MigrationState::Dirty
                } else if row.checksum == migration.checksum.as_ref() {
                    MigrationState::Applied
                } else {
                    MigrationState::ChecksumMismatch
                },
            },
            None => MigrationStatusEntry {
                version: migration.version,
                description: migration.description.to_string(),
                checksum: encode_checksum(&migration.checksum),
                state: MigrationState::Pending,
            },
        };
        migrations.push(entry);
    }

    migrations.extend(applied.into_values().map(|row| MigrationStatusEntry {
        version: row.version,
        description: "unknown applied migration".to_owned(),
        checksum: encode_checksum(&row.checksum),
        state: if row.success {
            MigrationState::Unknown
        } else {
            MigrationState::Dirty
        },
    }));
    migrations.sort_by_key(|migration| migration.version);
    let compatible = migrations
        .iter()
        .all(|migration| migration.state == MigrationState::Applied);

    DatabaseMigrationStatus {
        database,
        kind,
        current_version,
        target_version,
        compatible,
        migrations,
    }
}

#[cfg(feature = "postgres")]
async fn apply_postgres(url: &str) -> Result<(), MigrationError> {
    let mut connection = PgConnection::connect(url).await?;
    POSTGRES_MIGRATOR.run(&mut connection).await?;
    connection.close().await?;
    Ok(())
}

#[cfg(not(feature = "postgres"))]
async fn apply_postgres(_url: &str) -> Result<(), MigrationError> {
    Err(MigrationError::AdapterDisabled("postgres"))
}

#[cfg(feature = "sqlite")]
async fn apply_sqlite(management_path: &Path, accounts_dir: &Path) -> Result<(), MigrationError> {
    if let Some(parent) = management_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::create_dir_all(accounts_dir).await?;
    apply_sqlite_database(
        management_path,
        "sqlite-management",
        &SQLITE_MANAGEMENT_MIGRATOR,
    )
    .await?;

    let mut failures = Vec::new();
    for account_path in discover_account_databases(accounts_dir).await? {
        let label = sqlite_account_label(&account_path);
        if let Err(error) =
            apply_sqlite_database(&account_path, &label, &SQLITE_ACCOUNT_MIGRATOR).await
        {
            failures.push(format!("{label}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(MigrationError::AccountFailures(failures.join("; ")))
    }
}

#[cfg(not(feature = "sqlite"))]
async fn apply_sqlite(_management_path: &Path, _accounts_dir: &Path) -> Result<(), MigrationError> {
    Err(MigrationError::AdapterDisabled("sqlite"))
}

#[cfg(feature = "sqlite")]
async fn apply_sqlite_database(
    path: &Path,
    label: &str,
    migrator: &Migrator,
) -> Result<(), MigrationError> {
    let mut connection = open_sqlite(path, true).await?;
    let owner = Uuid::now_v7().to_string();
    acquire_sqlite_lease(&mut connection, label, &owner).await?;
    let migration_result = migrator.run(&mut connection).await;
    let release_result = release_sqlite_lease(&mut connection, &owner).await;
    match migration_result {
        Ok(()) => release_result?,
        Err(error) => {
            let _release_failure = release_result;
            return Err(error.into());
        }
    }
    connection.close().await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn acquire_sqlite_lease(
    connection: &mut SqliteConnection,
    label: &str,
    owner: &str,
) -> Result<(), MigrationError> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _pvlog_migration_lease (\
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1), \
         owner TEXT NOT NULL, expires_at INTEGER NOT NULL)",
    )
    .execute(&mut *connection)
    .await?;
    let now = unix_timestamp()?;
    let expires_at = now.saturating_add(SQLITE_LEASE_SECONDS);
    let result = sqlx::query(
        "INSERT INTO _pvlog_migration_lease (singleton, owner, expires_at) VALUES (1, ?, ?) \
         ON CONFLICT(singleton) DO UPDATE SET owner = excluded.owner, \
         expires_at = excluded.expires_at WHERE _pvlog_migration_lease.expires_at <= ?",
    )
    .bind(owner)
    .bind(expires_at)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        let held_until: i64 =
            sqlx::query_scalar("SELECT expires_at FROM _pvlog_migration_lease WHERE singleton = 1")
                .fetch_one(&mut *connection)
                .await?;
        Err(MigrationError::LeaseHeld {
            database: label.to_owned(),
            expires_at: held_until,
        })
    }
}

#[cfg(feature = "sqlite")]
async fn release_sqlite_lease(
    connection: &mut SqliteConnection,
    owner: &str,
) -> Result<(), MigrationError> {
    sqlx::query("DELETE FROM _pvlog_migration_lease WHERE singleton = 1 AND owner = ?")
        .bind(owner)
        .execute(connection)
        .await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn open_sqlite(path: &Path, create: bool) -> Result<SqliteConnection, MigrationError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));
    Ok(SqliteConnection::connect_with(&options).await?)
}

#[cfg(feature = "sqlite")]
async fn discover_account_databases(accounts_dir: &Path) -> Result<Vec<PathBuf>, MigrationError> {
    if !accounts_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let mut entries = tokio::fs::read_dir(accounts_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_file() && is_sqlite_database(&entry.path()) {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

#[cfg(feature = "sqlite")]
pub(crate) fn is_sqlite_database(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("sqlite" | "sqlite3" | "db")
    )
}

#[cfg(feature = "sqlite")]
fn sqlite_account_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || "sqlite-account".to_owned(),
        |name| format!("sqlite-account:{}", name.to_string_lossy()),
    )
}

fn encode_checksum(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        if write!(&mut encoded, "{byte:02x}").is_err() {
            unreachable!("writing to a String cannot fail");
        }
    }
    encoded
}

#[cfg(feature = "sqlite")]
fn unix_timestamp() -> Result<i64, MigrationError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MigrationError::ClockBeforeUnixEpoch)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| MigrationError::ClockOutOfRange)
}

struct AppliedRow {
    version: i64,
    checksum: Vec<u8>,
    success: bool,
}

/// Failure while planning, applying, or checking schema migrations.
#[derive(Debug, Error)]
pub enum MigrationError {
    /// Filesystem discovery or setup failed.
    #[error("migration filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    /// Database access failed.
    #[error("migration database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// `SQLx` rejected migration metadata or execution.
    #[error("migration execution failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// A non-expired `SQLite` migration lease belongs to another process.
    #[error("migration lease for {database} is held until Unix timestamp {expires_at}")]
    LeaseHeld {
        /// Safe database label.
        database: String,
        /// Lease expiry as Unix seconds.
        expires_at: i64,
    },
    /// One or more account databases failed while independent accounts continued.
    #[error("one or more SQLite account migrations failed: {0}")]
    AccountFailures(String),
    /// Startup found pending, dirty, modified, or unknown migrations.
    #[error(
        "database schema is incompatible with this release: {0}; run `pvlog migrate status` and `pvlog migrate apply`"
    )]
    IncompatibleSchema(String),
    /// The selected adapter is not compiled into this binary.
    #[error("the {0} database adapter is not enabled in this build")]
    AdapterDisabled(&'static str),
    /// The host clock cannot produce lease timestamps.
    #[error("system clock is before the Unix epoch")]
    ClockBeforeUnixEpoch,
    /// The host clock cannot fit in the database lease representation.
    #[error("system clock is outside the supported lease timestamp range")]
    ClockOutOfRange,
}
