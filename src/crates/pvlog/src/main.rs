//! `PVLog` server, worker, and operator command entrypoint.

#![forbid(unsafe_code)]

use std::{io, path::PathBuf, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig as _;
use pvlog::SystemClock;
use pvlog::administration::ManagementAdministrationApi;
use pvlog::api_keys::ManagementAccountApiKeyService;
use pvlog::authentication::{
    ManagementApiTokenRepository, ManagementAuditApi, ManagementConnectorApi,
    ManagementIdentityApi, ManagementRbacApi, ManagementReadiness, ManagementRequestAuthenticator,
    ManagementRequestAuthorizer, ManagementSessionBootstrap, session_digest_key,
};
use pvlog::config::{ConfigError, DatabaseBackend, RuntimeConfig};
use pvlog::inverters::ManagementInverterApi;
use pvlog::notifications::ManagementNotificationApi;
use pvlog::operator_bundle::{
    export_account_bundle, export_bundle, export_postgres_bundle, import_bundle, verify_bundle,
};
use pvlog_application::{
    ApiTokenService, Argon2CredentialConfig, Argon2CredentialService, BrowserSessionPolicy,
    BrowserSessionRepository, BrowserSessionService, BrowserSessionUseCases, Clock,
    CommonPasswordHook, DiscardingRecoveryNotifier, EquipmentCatalog,
    ExternalIdentityLinkingRepository, ExternalIdentityLinkingService,
    ExternalIdentityLinkingUseCases, ExternalLoginPolicy, LocalCredentialRepository,
    LocalPasswordPolicy, LocalPasswordService, LocalPasswordUseCases, LocalUserPolicy,
    SystemLifecycleRepository, SystemLifecycleService, SystemLifecycleUseCases,
    UserLifecycleRepository, UserLifecycleService, UserLifecycleUseCases,
};
use pvlog_domain::{AccountId, JobId, SystemId, TimeRange, UtcTimestamp};
use pvlog_storage::{
    DatabaseMigrationStatus, DatabaseTarget, MigrationError, MigrationPlanItem,
    OperationalRepository, ProbeError, YieldInvalidationReason, YieldInvalidationRecord,
    YieldInvalidationState, YieldResultRepository, apply_migrations, migration_plan,
    migration_status, probe_database,
};
use secrecy::ExposeSecret as _;
use serde::Serialize;
use thiserror::Error;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

struct EmptyDashboardApi;

#[async_trait::async_trait]
impl pvlog_api::DashboardApiUseCases for EmptyDashboardApi {
    async fn dashboard(
        &self,
    ) -> Result<pvlog_api::DashboardResponse, pvlog_api::DashboardApiError> {
        let now = i64::try_from(SystemClock.now().epoch_millis())
            .map_err(|_| pvlog_api::DashboardApiError::Unavailable)?;
        Ok(pvlog_api::DashboardResponse {
            observed_at_epoch_millis: 0,
            age_seconds: u64::try_from(now / 1_000).unwrap_or(u64::MAX),
            freshness_threshold_seconds: 60,
            generation_watts: 0.0,
            consumption_watts: None,
            grid_watts: None,
            battery_basis_points: None,
            coverage_basis_points: 0,
            recent_alerts: Vec::new(),
            ingestion: pvlog_api::DashboardIngestionResponse {
                accepted_today: 0,
                rejected_today: 0,
                lag_seconds: 0,
            },
        })
    }
}

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
        /// Seconds to wait between worker cycles.
        #[arg(long, default_value_t = 30)]
        interval_seconds: u64,
    },
    /// Verify database reachability and schema compatibility without mutation.
    Doctor {
        /// Emit stable machine-readable JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Inspect or explicitly apply database schema migrations.
    Migrate {
        /// Emit stable machine-readable JSON instead of text.
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        action: MigrationCommand,
    },
    /// Export a versioned, checksummed operator bundle.
    Export {
        output: PathBuf,
        /// Restrict the bundle to one opaque account database filename.
        #[arg(long)]
        account_database: Option<String>,
        /// Package a consistent archive produced by the `PostgreSQL` backup hook.
        #[arg(long)]
        postgres_archive: Option<PathBuf>,
    },
    /// Validate and restore an operator bundle into an empty destination.
    Import {
        bundle: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Verify an operator bundle without restoring it.
    Verify { bundle: PathBuf },
    /// Plan, enqueue, inspect, cancel, or retry deterministic yield recalculation jobs.
    Forecast {
        #[command(subcommand)]
        action: ForecastCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ForecastCommand {
    /// Recalculate one bounded system range.
    Recalculate {
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        system_id: String,
        #[arg(long)]
        start_epoch_millis: i64,
        #[arg(long)]
        end_epoch_millis: i64,
        /// Print the deterministic plan without changing storage.
        #[arg(long)]
        dry_run: bool,
    },
    /// Report durable job progress.
    Progress {
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        job_id: String,
    },
    /// Cancel a pending, retrying, leased, or failed job.
    Cancel {
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        job_id: String,
    },
    /// Requeue a failed, dead-lettered, or cancelled job from attempt zero.
    Retry {
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        job_id: String,
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
    let cli = Cli::parse();
    let config = RuntimeConfig::load()?;
    let telemetry = init_observability(&config)?;
    let target = database_target(&config);

    let result = match cli.command {
        Command::Server => run_server(&config, &target).await,
        Command::Worker {
            once: true,
            interval_seconds: _,
        } => {
            pvlog_worker::run_once(&target).await?;
            Ok(())
        }
        Command::Worker {
            once: false,
            interval_seconds,
        } => run_worker(&target, interval_seconds).await,
        Command::Doctor { json } => run_doctor(&target, json).await,
        Command::Migrate { json, action } => run_migration_command(&target, action, json).await,
        Command::Export {
            output,
            account_database,
            postgres_archive,
        } => {
            let manifest = if let Some(archive) = postgres_archive {
                export_postgres_bundle(&target, &output, &archive)?
            } else if let Some(account_database) = account_database {
                export_account_bundle(&target, &output, &account_database).await?
            } else {
                export_bundle(&target, &output).await?
            };
            print_json(&manifest)
        }
        Command::Import { bundle, dry_run } => {
            print_json(&import_bundle(&target, &bundle, dry_run)?)
        }
        Command::Verify { bundle } => print_json(&verify_bundle(&bundle)?),
        Command::Forecast { action } => run_forecast_command(&target, action).await,
    };
    if let Some(providers) = telemetry {
        providers
            .traces
            .shutdown()
            .map_err(|error| StartupError::Telemetry(error.to_string()))?;
        providers
            .metrics
            .shutdown()
            .map_err(|error| StartupError::Telemetry(error.to_string()))?;
    }
    result
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForecastOperationOutput {
    action: &'static str,
    account_id: String,
    job_id: Option<String>,
    state: String,
    start_epoch_millis: Option<i64>,
    end_epoch_millis: Option<i64>,
}

async fn run_forecast_command(
    target: &DatabaseTarget,
    action: ForecastCommand,
) -> Result<(), StartupError> {
    match action {
        ForecastCommand::Recalculate {
            account_id,
            system_id,
            start_epoch_millis,
            end_epoch_millis,
            dry_run,
        } => {
            let account_id = parse_account_id(&account_id)?;
            let system_id = parse_system_id(&system_id)?;
            let range = forecast_range(start_epoch_millis, end_epoch_millis)?;
            let invalidation_id = deterministic_invalidation_id(
                account_id,
                system_id,
                start_epoch_millis,
                end_epoch_millis,
            );
            if dry_run {
                return print_json(&ForecastOperationOutput {
                    action: "recalculate",
                    account_id: account_id.to_string(),
                    job_id: None,
                    state: format!("dry_run:{invalidation_id}"),
                    start_epoch_millis: Some(start_epoch_millis),
                    end_epoch_millis: Some(end_epoch_millis),
                });
            }
            let now = command_now()?;
            let (operations, results) = forecast_repositories(target, account_id).await?;
            let key = format!(
                "operator-recalculation:{system_id}:{start_epoch_millis}:{end_epoch_millis}"
            );
            results
                .insert_invalidation(&YieldInvalidationRecord {
                    id: invalidation_id,
                    system_id,
                    range,
                    reason: YieldInvalidationReason::ModelVersion,
                    state: YieldInvalidationState::Pending,
                    idempotency_key: key,
                    created_at: now,
                    completed_at: None,
                })
                .await
                .map_err(|error| StartupError::ForecastOperation(error.to_string()))?;
            let coordinator = pvlog_worker::YieldJobCoordinator::new(
                operations,
                pvlog_worker::YieldJobPolicy::default(),
            )
            .map_err(|error| StartupError::ForecastOperation(error.to_string()))?;
            let job_id = coordinator
                .enqueue_pending_rebuild(results.as_ref(), system_id, range, 1, now)
                .await
                .map_err(|error| StartupError::ForecastOperation(error.to_string()))?
                .ok_or_else(|| {
                    StartupError::ForecastOperation("recalculation was not enqueued".to_owned())
                })?;
            print_json(&ForecastOperationOutput {
                action: "recalculate",
                account_id: account_id.to_string(),
                job_id: Some(job_id.to_string()),
                state: "pending".to_owned(),
                start_epoch_millis: Some(start_epoch_millis),
                end_epoch_millis: Some(end_epoch_millis),
            })
        }
        ForecastCommand::Progress { account_id, job_id } => {
            let account_id = parse_account_id(&account_id)?;
            let job_id = parse_job_id(&job_id)?;
            let (operations, _) = forecast_repositories(target, account_id).await?;
            let job = operations
                .job(job_id)
                .await
                .map_err(|error| StartupError::ForecastOperation(error.to_string()))?
                .ok_or_else(|| StartupError::ForecastOperation("job was not found".to_owned()))?;
            print_json(&ForecastOperationOutput {
                action: "progress",
                account_id: account_id.to_string(),
                job_id: Some(job_id.to_string()),
                state: format!("{}:{}/{}", job.state, job.attempt_count, job.max_attempts),
                start_epoch_millis: None,
                end_epoch_millis: None,
            })
        }
        ForecastCommand::Cancel { account_id, job_id } => {
            mutate_forecast_job(target, &account_id, &job_id, false).await
        }
        ForecastCommand::Retry { account_id, job_id } => {
            mutate_forecast_job(target, &account_id, &job_id, true).await
        }
    }
}

async fn mutate_forecast_job(
    target: &DatabaseTarget,
    account_id: &str,
    job_id: &str,
    retry: bool,
) -> Result<(), StartupError> {
    let account_id = parse_account_id(account_id)?;
    let job_id = parse_job_id(job_id)?;
    let (operations, _) = forecast_repositories(target, account_id).await?;
    let changed = if retry {
        operations.requeue_job(job_id, command_now()?).await
    } else {
        operations.cancel_job(job_id, command_now()?).await
    }
    .map_err(|error| StartupError::ForecastOperation(error.to_string()))?;
    if !changed {
        return Err(StartupError::ForecastOperation(
            "job state does not allow this transition".to_owned(),
        ));
    }
    print_json(&ForecastOperationOutput {
        action: if retry { "retry" } else { "cancel" },
        account_id: account_id.to_string(),
        job_id: Some(job_id.to_string()),
        state: if retry { "pending" } else { "cancelled" }.to_owned(),
        start_epoch_millis: None,
        end_epoch_millis: None,
    })
}

async fn forecast_repositories(
    target: &DatabaseTarget,
    account_id: AccountId,
) -> Result<
    (
        Arc<dyn OperationalRepository>,
        Box<dyn YieldResultRepository>,
    ),
    StartupError,
> {
    match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => {
            let router = pvlog_storage::SqliteAccountPoolRouter::new(
                management_path.clone(),
                accounts_dir.clone(),
                pvlog_storage::SqliteAccountPoolConfig::default(),
            )
            .map_err(|error| StartupError::ForecastOperation(error.to_string()))?;
            let account = router
                .route(account_id)
                .await
                .map_err(|error| StartupError::ForecastOperation(error.to_string()))?;
            Ok((
                Arc::new(pvlog_storage::SqliteOperationalRepository::new(
                    management_path.clone(),
                    account.clone(),
                )),
                Box::new(pvlog_storage::SqliteYieldResultRepository::new(account)),
            ))
        }
        DatabaseTarget::Postgres { url } => Ok((
            Arc::new(pvlog_storage::PostgresOperationalRepository::new(
                url.clone(),
                account_id,
            )),
            Box::new(pvlog_storage::PostgresYieldResultRepository::new(
                url.clone(),
                account_id,
            )),
        )),
    }
}

fn forecast_range(start: i64, end: i64) -> Result<TimeRange, StartupError> {
    const MAXIMUM_RANGE_MILLIS: i64 = 366 * 86_400_000;
    if end <= start || end.saturating_sub(start) > MAXIMUM_RANGE_MILLIS {
        return Err(StartupError::ForecastOperation(
            "range must be non-empty and no longer than 366 days".to_owned(),
        ));
    }
    TimeRange::new(
        UtcTimestamp::from_epoch_millis(start)
            .map_err(|error| StartupError::ForecastOperation(error.to_string()))?,
        UtcTimestamp::from_epoch_millis(end)
            .map_err(|error| StartupError::ForecastOperation(error.to_string()))?,
    )
    .map_err(|error| StartupError::ForecastOperation(error.to_string()))
}

fn parse_account_id(value: &str) -> Result<AccountId, StartupError> {
    AccountId::from_uuid(parse_uuid(value)?)
        .map_err(|error| StartupError::ForecastOperation(error.to_string()))
}

fn parse_system_id(value: &str) -> Result<SystemId, StartupError> {
    SystemId::from_uuid(parse_uuid(value)?)
        .map_err(|error| StartupError::ForecastOperation(error.to_string()))
}

fn parse_job_id(value: &str) -> Result<JobId, StartupError> {
    JobId::from_uuid(parse_uuid(value)?)
        .map_err(|error| StartupError::ForecastOperation(error.to_string()))
}

fn parse_uuid(value: &str) -> Result<uuid::Uuid, StartupError> {
    uuid::Uuid::parse_str(value)
        .map_err(|_| StartupError::ForecastOperation("identifier is not a UUID".to_owned()))
}

fn deterministic_invalidation_id(
    account_id: AccountId,
    system_id: SystemId,
    start: i64,
    end: i64,
) -> uuid::Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(account_id.as_uuid().as_bytes());
    hasher.update(system_id.as_uuid().as_bytes());
    hasher.update(&start.to_be_bytes());
    hasher.update(&end.to_be_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}

fn command_now() -> Result<i64, StartupError> {
    i64::try_from(SystemClock.now().epoch_millis())
        .map_err(|_| StartupError::ForecastOperation("current time is out of range".to_owned()))
}

fn init_observability(
    config: &RuntimeConfig,
) -> Result<Option<ObservabilityProviders>, StartupError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let format = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true);
    if config.telemetry.enabled {
        let endpoint = config
            .telemetry
            .otlp_endpoint
            .as_ref()
            .ok_or_else(|| StartupError::Telemetry("OTLP endpoint is required".to_owned()))?;
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint.as_str())
            .build()
            .map_err(|error| StartupError::Telemetry(error.to_string()))?;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("pvlog-server");
        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(endpoint.as_str())
            .build()
            .map_err(|error| StartupError::Telemetry(error.to_string()))?;
        let metrics = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
            .with_periodic_exporter(metric_exporter)
            .build();
        opentelemetry::global::set_meter_provider(metrics.clone());
        tracing_subscriber::registry()
            .with(filter)
            .with(format)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()
            .map_err(|error| StartupError::Telemetry(error.to_string()))?;
        Ok(Some(ObservabilityProviders {
            traces: provider,
            metrics,
        }))
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(format)
            .try_init()
            .map_err(|error| StartupError::Telemetry(error.to_string()))?;
        Ok(None)
    }
}

struct ObservabilityProviders {
    traces: opentelemetry_sdk::trace::SdkTracerProvider,
    metrics: opentelemetry_sdk::metrics::SdkMeterProvider,
}

async fn run_worker(target: &DatabaseTarget, interval_seconds: u64) -> Result<(), StartupError> {
    let interval = Duration::from_secs(interval_seconds.max(1));
    let mut ticker = tokio::time::interval(interval);
    tracing::info!(interval_seconds = interval.as_secs(), "worker started");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("worker received shutdown signal");
                return Ok(());
            }
            _ = ticker.tick() => {
                if let Err(error) = pvlog_worker::run_once(target).await {
                    tracing::error!(%error, "worker readiness cycle failed");
                }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorReport {
    database_ready: bool,
    schema_compatible: bool,
    migrations: Vec<DatabaseMigrationStatus>,
}

async fn run_doctor(target: &DatabaseTarget, json: bool) -> Result<(), StartupError> {
    probe_database(target).await?;
    let migrations = migration_status(target).await?;
    let report = DoctorReport {
        database_ready: true,
        schema_compatible: migrations.iter().all(|status| status.compatible),
        migrations,
    };
    if json {
        print_json(&report)?;
    } else {
        println!("database_ready={}", report.database_ready);
        println!("schema_compatible={}", report.schema_compatible);
        print_statuses(&report.migrations, false)?;
    }
    report
        .schema_compatible
        .then_some(())
        .ok_or(StartupError::IncompatibleSchema)
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
    let equipment_catalog = Arc::new(EquipmentCatalog::bundled()?);
    let user_lifecycle = compose_user_lifecycle(config, target)?;
    let local_password = compose_local_password(config, target)?;
    let request_authenticator = compose_request_authenticator(config, target)?;
    let request_authorizer = compose_request_authorizer(target)?;
    let account_api_keys = compose_account_api_keys(config, target)?;
    let system_lifecycle = compose_system_lifecycle(target)?;
    let browser_sessions = compose_browser_sessions(config, target)?;
    let session_bootstrap = Arc::new(ManagementSessionBootstrap::new(
        compose_management_repository(target)?,
        compose_rbac_repository(target)?,
        Arc::new(SystemClock),
        target.clone(),
    ));
    let audit_api = Arc::new(ManagementAuditApi::new(compose_management_repository(
        target,
    )?));
    let rbac_api = compose_rbac_api(target)?;
    let identity_api = compose_identity_api(target)?;
    let connector_api = Arc::new(ManagementConnectorApi::new(&config.auth.connectors));
    let inverter_api = Arc::new(ManagementInverterApi::new(
        target.clone(),
        equipment_catalog.clone(),
    ));
    let readiness = Arc::new(ManagementReadiness::new(target.clone()));
    let administration = Arc::new(ManagementAdministrationApi::new(
        compose_administration_repository(target)?,
        target.clone(),
    ));
    let notifications = Arc::new(ManagementNotificationApi::new(target.clone()));
    let reporting = Arc::new(pvlog::reporting::StorageReportingApi::new(target.clone()));
    let geocoding = Arc::new(
        pvlog::geocoding::PhotonGeocodingApi::new(config.geocoding.endpoint.clone())
            .map_err(|_| StartupError::Geocoding)?,
    );
    let listener = tokio::net::TcpListener::bind(config.http.bind).await?;
    tracing::info!(address = %listener.local_addr()?, database = ?target, "server listening");
    let api_router = pvlog_api::with_request_authentication(
        pvlog_api::router(env!("CARGO_PKG_VERSION"))
            .merge(pvlog_api::readiness_router(readiness))
            .merge(pvlog_api::user_lifecycle_router(
                user_lifecycle,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::local_password_router(
                local_password,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::systems_router(
                system_lifecycle,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::account_api_keys_router(
                account_api_keys,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::sessions_router(
                compose_local_password(config, target)?,
                browser_sessions,
                session_bootstrap,
            ))
            .merge(pvlog_api::audit_router(
                audit_api,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::rbac_router(rbac_api, request_authorizer.clone()))
            .merge(pvlog_api::identities_router(identity_api))
            .merge(pvlog_api::connectors_router(
                connector_api,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::inverters_router(
                inverter_api,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::equipment_catalog_router(equipment_catalog))
            .merge(pvlog_api::geocoding_router(geocoding))
            .merge(pvlog_api::administration_router(
                administration,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::authorized_notifications_router(
                notifications,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::reporting_router(
                reporting,
                request_authorizer.clone(),
            ))
            .merge(pvlog_api::dashboard_router(Arc::new(EmptyDashboardApi))),
        request_authenticator,
    );
    let router = api_router.merge(pvlog::embedded_ui::router(
        env!("CARGO_PKG_VERSION"),
        config.telemetry.enabled,
        config
            .telemetry
            .otlp_endpoint
            .as_ref()
            .map(ToString::to_string),
    ));
    axum::serve(listener, router).await?;
    Ok(())
}

fn compose_administration_repository(
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_storage::AdministrationRepository>, StartupError> {
    match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Ok(Arc::new(
                    pvlog_storage::SqliteAdministrationRepository::new(management_path.clone()),
                ))
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
                Ok(Arc::new(
                    pvlog_storage::PostgresAdministrationRepository::new(url.clone()),
                ))
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
fn compose_identity_api(
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_api::IdentityApiUseCases>, StartupError> {
    let repository: Arc<dyn ExternalIdentityLinkingRepository> = match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Arc::new(pvlog_storage::SqliteExternalIdentityRepository::new(
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
                Arc::new(pvlog_storage::PostgresExternalIdentityRepository::new(
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
    let service: Arc<dyn ExternalIdentityLinkingUseCases> =
        Arc::new(ExternalIdentityLinkingService::new(
            repository,
            Arc::new(SystemClock),
            ExternalLoginPolicy::default(),
        ));
    Ok(Arc::new(ManagementIdentityApi::new(service)))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_rbac_api(
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_api::RbacApiUseCases>, StartupError> {
    Ok(Arc::new(ManagementRbacApi::new(
        compose_rbac_repository(target)?,
        Arc::new(SystemClock),
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_rbac_repository(
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_application::RbacRepository>, StartupError> {
    let repository: Arc<dyn pvlog_application::RbacRepository> = match target {
        DatabaseTarget::Sqlite {
            management_path, ..
        } => {
            #[cfg(feature = "sqlite")]
            {
                Arc::new(pvlog_storage::SqliteRbacRepository::new(
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
                Arc::new(pvlog_storage::PostgresRbacRepository::new(url.clone()))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = url;
                return Err(StartupError::AdapterDisabled("postgres"));
            }
        }
    };
    Ok(repository)
}

#[allow(clippy::unnecessary_wraps)]
fn compose_browser_sessions(
    config: &RuntimeConfig,
    target: &DatabaseTarget,
) -> Result<Arc<dyn BrowserSessionUseCases>, StartupError> {
    let repository: Arc<dyn BrowserSessionRepository> = match target {
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
    Ok(Arc::new(BrowserSessionService::new(
        repository,
        Arc::new(SystemClock),
        session_digest_key(&config.security.session_secret),
        BrowserSessionPolicy {
            idle_lifetime_seconds: 7 * 24 * 60 * 60,
            absolute_lifetime_seconds: 7 * 24 * 60 * 60,
            max_concurrent_sessions: 8,
            secure_cookies: config.http.secure_cookies,
        },
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_request_authenticator(
    config: &RuntimeConfig,
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_api::RequestAuthenticator>, StartupError> {
    let repository = compose_management_repository(target)?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    Ok(Arc::new(ManagementRequestAuthenticator::new(
        repository,
        clock,
        &config.security.session_secret,
    )))
}

#[allow(clippy::unnecessary_wraps)]
fn compose_account_api_keys(
    config: &RuntimeConfig,
    target: &DatabaseTarget,
) -> Result<Arc<dyn pvlog_api::AccountApiKeyUseCases>, StartupError> {
    let repository = compose_management_repository(target)?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let token_repository = Arc::new(ManagementApiTokenRepository::new(repository.clone()));
    let service = ApiTokenService::new(
        token_repository,
        clock.clone(),
        session_digest_key(&config.security.session_secret),
    );
    Ok(Arc::new(ManagementAccountApiKeyService::new(
        service, repository, clock,
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
    let rbac = compose_rbac_repository(target)?;
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
        rbac,
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
            password_minimum_length: config.auth.local.password_minimum_length,
            password_maximum_length: config.auth.local.password_maximum_length,
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
    #[error(transparent)]
    Bundle(#[from] pvlog::operator_bundle::BundleError),
    #[error(transparent)]
    EquipmentCatalog(#[from] pvlog_application::EquipmentCatalogError),
    #[error("failed to initialize geocoding client")]
    Geocoding,
    #[error("telemetry initialization failed: {0}")]
    Telemetry(String),
    #[error("failed to serialize command output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema is not compatible with this release")]
    IncompatibleSchema,
    #[error("failed to initialize account lifecycle routing")]
    SystemLifecycleRouting,
    #[error("forecast operation failed: {0}")]
    ForecastOperation(String),
    #[allow(dead_code)]
    #[error("the {0} database adapter is not enabled in this build")]
    AdapterDisabled(&'static str),
    #[error("HTTP server failed: {0}")]
    Io(#[from] io::Error),
}
