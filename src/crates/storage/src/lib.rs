//! `SQLite` and `PostgreSQL` persistence adapters for `PVLog`.

#![forbid(unsafe_code)]

use std::{fmt, path::PathBuf};

use sqlx::Connection as _;
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
#[cfg(feature = "sqlite")]
use sqlx::{
    SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use thiserror::Error;

mod account_repository;
mod browser_session_repository;
mod compaction;
mod integrity_planner;
mod job_dispatch;
mod management_repository;
mod merged_reader;
mod migrations;
mod operational_repository;
mod overlay_folding;
#[cfg(feature = "sqlite")]
mod provisioning;
mod rbac_repository;
mod rollup_builder;
mod segment_codec;
#[cfg(feature = "sqlite")]
mod sqlite_projection;
#[cfg(feature = "sqlite")]
mod sqlite_router;
mod summary_rebuild;
mod system_lifecycle_repository;
mod telemetry_repository;
mod user_lifecycle_repository;

#[cfg(feature = "postgres")]
pub use account_repository::PostgresAccountConfigurationRepository;
#[cfg(feature = "sqlite")]
pub use account_repository::SqliteAccountConfigurationRepository;
pub use account_repository::{
    AccountAuditRecord, AccountConfigurationRepository, AccountRepositoryError,
    ChannelDefinitionRecord, EquipmentRecord, SystemConfigurationRecord, TariffRecord,
};
pub use compaction::{
    CompactionError, CompactionKey, CompactionPhase, CompactionRepository, CompactionService,
};
pub use integrity_planner::{
    IntegrityIssue, IntegrityReport, IntegritySnapshot, RepairAction, plan_integrity_repairs,
};
pub use job_dispatch::ManagementJobDispatcher;
pub use management_repository::{
    AccountRecord, ApiCredentialRecord, AuditRecord, AuthorizationGrant, ManagementRepository,
    ManagementRepositoryError, MembershipRecord, PostgresManagementRepository, RoutingBackend,
    RoutingRecord, SessionRecord, SqliteManagementRepository, SystemRegistryRecord, UserRecord,
};
pub use merged_reader::{
    MergedReadError, RawObservation, RawObservationOrigin, merge_raw_observations,
};
pub use migrations::{
    DatabaseMigrationStatus, MigrationError, MigrationKind, MigrationPlanItem, MigrationState,
    apply_migrations, ensure_schema_compatible, migration_plan, migration_status,
};
#[cfg(feature = "postgres")]
pub use operational_repository::PostgresOperationalRepository;
#[cfg(feature = "sqlite")]
pub use operational_repository::SqliteOperationalRepository;
pub use operational_repository::{
    AlertRuleRecord, DailySummaryRecord, JobLease, JobRecord, JobRetryDisposition,
    LifetimeSummaryRecord, OperationalRepository, OperationalRepositoryError, ProviderRecord,
    RollupRecord, TeamRecord, TeamRollupRecord, WebhookSubscriptionRecord,
};
pub use overlay_folding::{
    OverlayFoldError, OverlayFoldKey, OverlayFoldPhase, OverlayFoldRepository, OverlayFoldService,
    OverlayFoldState,
};
#[cfg(feature = "sqlite")]
pub use provisioning::{
    AccountDatabaseLifecycle, AccountProvisioningResult, ReconciliationReport,
    SqliteAccountProvisioner, SqliteProvisioningError,
};
#[cfg(feature = "postgres")]
pub use rbac_repository::PostgresRbacRepository;
#[cfg(feature = "sqlite")]
pub use rbac_repository::SqliteRbacRepository;
pub use rollup_builder::{
    RollupBuildError, RollupGranularity, RollupSample, RollupWindow, TelemetryRollup, build_rollups,
};
pub use segment_codec::{
    ArchivedSegmentBytes, SegmentCodecError, SegmentPoint, decode_segment_v1, encode_segment_v1,
};
#[cfg(feature = "sqlite")]
pub use sqlite_projection::{
    ProjectionActivityState, ProjectionError, ProjectionInvalidationReason,
    ProjectionLocationPrecision, ProjectionReconciliationReport, ProjectionVisibility,
    SqliteProjectionCoordinator, SystemDiscoveryProjection, SystemProjectionEvent,
    append_projection_event,
};
#[cfg(feature = "sqlite")]
pub use sqlite_router::{
    RoutedSqliteAccount, SerializedSqliteWriter, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteCheckpointMode, SqliteCheckpointReport, SqliteRoutingError,
};
pub use summary_rebuild::{DailyAggregate, LifetimeAggregate, SummaryDay, SummaryProjection};
#[cfg(feature = "postgres")]
pub use system_lifecycle_repository::PostgresSystemLifecycleRepository;
#[cfg(feature = "sqlite")]
pub use system_lifecycle_repository::SqliteSystemLifecycleRepository;
#[cfg(feature = "postgres")]
pub use telemetry_repository::PostgresTelemetryRepository;
#[cfg(feature = "sqlite")]
pub use telemetry_repository::SqliteTelemetryRepository;
pub use telemetry_repository::{
    CorrectionRecord, IdempotencyOutcome, IdempotencyRecord, ObservationInsertOutcome,
    StoredObservation, TelemetryRepository, TelemetryRepositoryError, TransactionalObservation,
};
#[cfg(feature = "postgres")]
pub use user_lifecycle_repository::PostgresUserLifecycleRepository;
#[cfg(feature = "sqlite")]
pub use user_lifecycle_repository::SqliteUserLifecycleRepository;

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
