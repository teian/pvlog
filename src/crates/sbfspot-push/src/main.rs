use clap::{Args, Parser, Subcommand};
use pvlog_sbfspot_push::{
    PushConfig, PvlogClient, PvlogClientConfig, SbfspotSource, push_pending, run_service,
};
use std::{error::Error, ffi::OsString, path::PathBuf, time::Duration};
use tracing_subscriber::EnvFilter;
use url::Url;

#[derive(Debug, Parser)]
#[command(version, about = "Push SBFspot SQLite telemetry to PVLog")]
struct Cli {
    #[command(flatten)]
    connection: ConnectionArgs,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct ConnectionArgs {
    /// Path to the `SBFspot.db` `SQLite` database.
    #[arg(long, env = "SBFSPOT_DATABASE")]
    database: PathBuf,
    /// `PVLog` server root or `/api/v1` URL.
    #[arg(long, env = "PVLOG_URL")]
    pvlog_url: Url,
    /// Destination `PVLog` system UUID.
    #[arg(long, env = "PVLOG_SYSTEM_ID")]
    system_id: String,
    /// Account API key with the `telemetry:write` permission.
    #[arg(long, env = "PVLOG_API_KEY", hide_env_values = true)]
    api_key: String,
    /// Durable local cursor file. Defaults next to the `SBFspot` database.
    #[arg(long, env = "SBFSPOT_PUSH_STATE")]
    checkpoint: Option<PathBuf>,
    /// Maximum observations in one API request (1..=1000).
    #[arg(long, env = "SBFSPOT_PUSH_BATCH_SIZE", default_value_t = 500)]
    batch_size: usize,
    /// Initial Unix timestamp in seconds, used only when no checkpoint exists.
    #[arg(long, env = "SBFSPOT_PUSH_START_AT", default_value_t = 0)]
    start_at: i64,
    /// Per-request timeout in seconds.
    #[arg(long, env = "SBFSPOT_PUSH_TIMEOUT", default_value_t = 30)]
    timeout: u64,
    /// Maximum attempts for transient API and network failures.
    #[arg(long, env = "SBFSPOT_PUSH_MAX_ATTEMPTS", default_value_t = 8)]
    maximum_attempts: u32,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Catch up once and exit.
    Once {
        /// Read and transform every row without sending data or saving a cursor.
        #[arg(long)]
        dry_run: bool,
    },
    /// Stay active and poll the database after each catch-up pass.
    Run {
        /// Seconds between database polling passes.
        #[arg(long, env = "SBFSPOT_PUSH_POLL_INTERVAL", default_value_t = 30)]
        poll_interval: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    initialize_logging();
    let cli = Cli::parse();
    let checkpoint_path = cli
        .connection
        .checkpoint
        .clone()
        .unwrap_or_else(|| default_checkpoint(&cli.connection.database));
    let source = SbfspotSource::open(&cli.connection.database).await?;
    let client = PvlogClient::new(PvlogClientConfig {
        base_url: cli.connection.pvlog_url,
        system_id: cli.connection.system_id,
        api_key: cli.connection.api_key,
        request_timeout: Duration::from_secs(cli.connection.timeout),
        maximum_attempts: cli.connection.maximum_attempts,
    })?;
    let mut push_config = PushConfig {
        batch_size: cli.connection.batch_size,
        checkpoint_path,
        initial_timestamp: cli.connection.start_at,
        dry_run: false,
    };

    match cli.command {
        Command::Once { dry_run } => {
            push_config.dry_run = dry_run;
            let summary = push_pending(&source, &client, &push_config).await?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::Run { poll_interval } => {
            run_service(
                &source,
                &client,
                &push_config,
                Duration::from_secs(poll_interval),
            )
            .await?;
        }
    }
    Ok(())
}

fn default_checkpoint(database: &std::path::Path) -> PathBuf {
    let mut path: OsString = database.as_os_str().to_owned();
    path.push(".pvlog-state.json");
    PathBuf::from(path)
}

fn initialize_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
