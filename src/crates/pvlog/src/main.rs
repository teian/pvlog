//! `PVLog` server, worker, and operator command entrypoint.

#![forbid(unsafe_code)]

use std::{io, sync::Arc};

use clap::{Parser, Subcommand};
use pvlog::SystemClock;
use pvlog::authentication::{ManagementRequestAuthenticator, ManagementRequestAuthorizer};
use pvlog::config::{ConfigError, DatabaseBackend, RuntimeConfig};
use pvlog_application::{
    Argon2CredentialConfig, Argon2CredentialService, CommonPasswordHook,
    DiscardingRecoveryNotifier, LocalCredentialRepository, LocalPasswordPolicy,
    LocalPasswordService, LocalPasswordUseCases, LocalUserPolicy, SystemLifecycleRepository,
    SystemLifecycleService, SystemLifecycleUseCases, UserLifecycleRepository, UserLifecycleService,
    UserLifecycleUseCases,
};
use pvlog_storage::{
    DatabaseMigrationStatus, DatabaseTarget, MigrationError, MigrationPlanItem, ProbeError,
    apply_migrations, migration_plan, migration_status, probe_database,
};
use secrecy::ExposeSecret as _;
use serde::Serialize;
use thiserror::Error;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "pvlog",
    version,
    about = "Self-hosted photovoltaic data platform"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the Axum HTTP server.
    Server,
    /// Start the background worker.
    Worker {
        /// Perform a single readiness cycle and exit.
        #[arg(long)]
        once: bool,
    },
    /// Inspect or explicitly apply database schema migrations.
    Migrate {
        /// Emit stable machine-readable JSON instead of text.
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        action: MigrationCommand,
    },
}

#[derive(Debug, Subcommand)]
enum MigrationCommand {
    /// Report applied, pending, dirty, changed, and unknown migrations.
    Status,
    /// Show the ordered migrations that would be applied without changing a database.
    Plan,
    /// Acquire migration locks and apply every pending migration.
    Apply,
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    let _subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let cli = Cli::parse();
    let config = RuntimeConfig::load()?;
    let target = database_target(&config);

    match cli.command {
        Command::Server => run_server(&config, &target).await,
        Command::Worker { once: true } => {
            pvlog_worker::run_once(&target).await?;
            Ok(())
        }
        Command::Worker { once: false } => Err(StartupError::ContinuousWorkerUnavailable),
        Command::Migrate { json, action } => run_migration_command(&target, action, json).await,
    }
}

async fn run_migration_command(
    target: &DatabaseTarget,
    action: MigrationCommand,
    json: bool,
) -> Result<(), StartupError> {
    match action {
        MigrationCommand::Status => {
            let statuses = migration_status(target).await?;
            print_statuses(&statuses, json)?;
        }
        MigrationCommand::Plan => {
            let plan = migration_plan(target).await?;
            print_plan(&plan, json)?;
        }
        MigrationCommand::Apply => {
            let statuses = apply_migrations(target).await?;
            print_statuses(&statuses, json)?;
        }
    }
    Ok(())
}

fn print_statuses(statuses: &[DatabaseMigrationStatus], json: bool) -> Result<(), StartupError> {
    if json {
        print_json(statuses)?;
        return Ok(());
    }
    for status in statuses {
        println!(
            "{} kind={:?} current={} target={} compatible={}",
            status.database,
            status.kind,
            display_version(status.current_version),
            display_version(status.target_version),
            status.compatible
        );
        for migration in &status.migrations {
            println!(
                "  {:04} {:?} {} {}",
                migration.version, migration.state, migration.checksum, migration.description
            );
        }
    }
    Ok(())
}

fn print_plan(plan: &[MigrationPlanItem], json: bool) -> Result<(), StartupError> {
    if json {
        print_json(plan)?;
        return Ok(());
    }
    if plan.is_empty() {
        println!("no pending migrations");
    } else {
        for migration in plan {
            println!(
                "{} {:04} {} {}",
                migration.database, migration.version, migration.checksum, migration.description
            );
        }
    }
    Ok(())
}

fn print_json<T>(value: &T) -> Result<(), StartupError>
where
    T: Serialize + ?Sized,
{
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn display_version(version: Option<i64>) -> String {
    version.map_or_else(|| "none".to_owned(), |version| version.to_string())
}

fn database_target(config: &RuntimeConfig) -> DatabaseTarget {
    match config.database.backend {
        DatabaseBackend::Sqlite => DatabaseTarget::Sqlite {
            management_path: config.database.sqlite.management_path.clone(),
            accounts_dir: config.database.sqlite.accounts_dir.clone(),
        },
        DatabaseBackend::Postgres => DatabaseTarget::Postgres {
            url: config.database.postgres.url.expose_secret().to_owned(),
        },
    }
}

async fn run_server(config: &RuntimeConfig, target: &DatabaseTarget) -> Result<(), StartupError> {
    probe_database(target).await?;
    let user_lifecycle = compose_user_lifecycle(config, target)?;
    let local_password = compose_local_password(config, target)?;
    let request_authenticator = compose_request_authenticator(config, target)?;
    let request_authorizer = compose_request_authorizer(target)?;
    let system_lifecycle = compose_system_lifecycle(target)?;
    let listener = tokio::net::TcpListener::bind(config.http.bind).await?;
    tracing::info!(address = %listener.local_addr()?, database = ?target, "server listening");
    let router = pvlog_api::with_request_authentication(
        pvlog_api::router(env!("CARGO_PKG_VERSION"))
            .merge(pvlog_api::user_lifecycle_router(user_lifecycle))
            .merge(pvlog_api::local_password_router(local_password))
            .merge(pvlog_api::systems_router(
                system_lifecycle,
                request_authorizer,
            )),
        request_authenticator,
    );
    axum::serve(listener, router).await?;
    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn compose_request_authenticator(
    config: &RuntimeConfig,
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_api::RequestAuthenticator>, StartupError> {
    let repository: Arc<dyn pvlog_storage::ManagementRepository> = match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Arc::new(pvlog_storage::SqliteManagementRepository::new(
                    management_path.clone(),
                ))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = management_path;
                return Err(StartupError::AdapterDisabled("sqlite"));
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Arc::new(pvlog_storage::PostgresManagementRepository::new(
                    url.clone(),
                ))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = url;
                return Err(StartupError::AdapterDisabled("postgres"));
            }
        }
    };
    Ok(Arc::new(ManagementRequestAuthenticator::new(
        repository,
        Arc::new(SystemClock),
        &config.security.session_secret,
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_request_authorizer(
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_api::ModernRequestAuthorizer>, StartupError> {
    Ok(Arc::new(ManagementRequestAuthorizer::new(
        compose_management_repository(target)?,
        Arc::new(SystemClock),
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_system_lifecycle(
    target: &DatabaseTarget,
) -> Result<Arc<dyn SystemLifecycleUseCases>, StartupError> {
    let management = compose_management_repository(target)?;
    let repository: Arc<dyn SystemLifecycleRepository> = match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => {
            #[cfg(feature = "sqlite")]
            {
                let router = pvlog_storage::SqliteAccountPoolRouter::new(
                    management_path.clone(),
                    accounts_dir.clone(),
                    pvlog_storage::SqliteAccountPoolConfig::default(),
                )
                .map_err(|_| StartupError::SystemLifecycleRouting)?;
                Arc::new(pvlog_storage::SqliteSystemLifecycleRepository::new(
                    router, management,
                ))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = (management_path, accounts_dir);
                return Err(StartupError::AdapterDisabled("sqlite"));
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Arc::new(pvlog_storage::PostgresSystemLifecycleRepository::new(
                    url.clone(),
                    management,
                ))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = url;
                return Err(StartupError::AdapterDisabled("postgres"));
            }
        }
    };
    Ok(Arc::new(SystemLifecycleService::new(
        repository,
        Arc::new(SystemClock),
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_management_repository(
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_storage::ManagementRepository>, StartupError> {
    match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Ok(Arc::new(pvlog_storage::SqliteManagementRepository::new(
                    management_path.clone(),
                )))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = management_path;
                Err(StartupError::AdapterDisabled("sqlite"))
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Ok(Arc::new(pvlog_storage::PostgresManagementRepository::new(
                    url.clone(),
                )))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = url;
                Err(StartupError::AdapterDisabled("postgres"))
            }
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
fn compose_user_lifecycle(
    config: &RuntimeConfig,
    target: &DatabaseTarget,
) -> Result<Arc<dyn UserLifecycleUseCases>, StartupError> {
    let repository: Arc<dyn UserLifecycleRepository> = match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Arc::new(pvlog_storage::SqliteUserLifecycleRepository::new(
                    management_path.clone(),
                ))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = management_path;
                return Err(StartupError::AdapterDisabled("sqlite"));
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Arc::new(pvlog_storage::PostgresUserLifecycleRepository::new(
                    url.clone(),
                ))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = url;
                return Err(StartupError::AdapterDisabled("postgres"));
            }
        }
    };
    Ok(Arc::new(UserLifecycleService::new(
        repository,
        Arc::new(argon2_credentials(config)),
        Arc::new(SystemClock),
        LocalUserPolicy {
            allow_self_registration: config.auth.local.allow_self_registration,
            require_verified_email: config.auth.local.require_verified_email,
            invitation_lifetime_seconds: 86_400,
        },
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_local_password(
    config: &RuntimeConfig,
    target: &DatabaseTarget,
) -> Result<Arc<dyn LocalPasswordUseCases>, StartupError> {
    let repository: Arc<dyn LocalCredentialRepository> = match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Arc::new(pvlog_storage::SqliteUserLifecycleRepository::new(
                    management_path.clone(),
                ))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = management_path;
                return Err(StartupError::AdapterDisabled("sqlite"));
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Arc::new(pvlog_storage::PostgresUserLifecycleRepository::new(
                    url.clone(),
                ))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = url;
                return Err(StartupError::AdapterDisabled("postgres"));
            }
        }
    };
    Ok(Arc::new(LocalPasswordService::new(
        repository,
        Arc::new(argon2_credentials(config)),
        Arc::new(SystemClock),
        Arc::new(CommonPasswordHook::default()),
        Arc::new(DiscardingRecoveryNotifier),
        LocalPasswordPolicy {
            minimum_length: config.auth.local.password_minimum_length,
            maximum_length: config.auth.local.password_maximum_length,
            maximum_failed_attempts: config.auth.local.maximum_failed_attempts,
            lockout_seconds: config.auth.local.lockout_seconds,
            recovery_lifetime_seconds: config.auth.local.recovery_lifetime_seconds,
        },
    )))
}

fn argon2_credentials(config: &RuntimeConfig) -> Argon2CredentialService {
    Argon2CredentialService::new(
        Argon2CredentialConfig {
            memory_kib: config.auth.local.argon2_memory_kib,
            time_cost: config.auth.local.argon2_time_cost,
            parallelism: config.auth.local.argon2_parallelism,
        },
        &config.security.session_secret,
    )
}

#[derive(Debug, Error)]
enum StartupError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Storage(#[from] ProbeError),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error("failed to serialize command output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("continuous worker execution is not implemented yet; pass --once")]
    ContinuousWorkerUnavailable,
    #[error("failed to initialize account lifecycle routing")]
    SystemLifecycleRouting,
    #[allow(dead_code)]
    #[error("the {0} database adapter is not enabled in this build")]
    AdapterDisabled(&'static str),
    #[error("HTTP server failed: {0}")]
    Io(#[from] io::Error),
}
