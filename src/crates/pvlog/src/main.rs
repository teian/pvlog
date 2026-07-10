//! `PVLog` server, worker, and operator command entrypoint.

#![forbid(unsafe_code)]

use std::io;

use clap::{Parser, Subcommand};
use pvlog::config::{ConfigError, DatabaseBackend, RuntimeConfig};
use pvlog_storage::{DatabaseTarget, ProbeError, probe_database};
use secrecy::ExposeSecret as _;
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
    }
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
    #[error("continuous worker execution is not implemented yet; pass --once")]
    ContinuousWorkerUnavailable,
    #[error("HTTP server failed: {0}")]
    Io(#[from] io::Error),
}
