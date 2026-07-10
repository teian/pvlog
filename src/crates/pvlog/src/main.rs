//! `PVLog` server, worker, and operator command entrypoint.

#![forbid(unsafe_code)]

use std::io;

use clap::{Parser, Subcommand};
use pvlog::config::{ConfigError, DatabaseBackend, RuntimeConfig};
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
    let listener = tokio::net::TcpListener::bind(config.http.bind).await?;
    tracing::info!(address = %listener.local_addr()?, database = ?target, "server listening");
    axum::serve(listener, pvlog_api::router(env!("CARGO_PKG_VERSION"))).await?;
    Ok(())
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
    #[error("HTTP server failed: {0}")]
    Io(#[from] io::Error),
}
