//! `PVLog` server, worker, and operator command entrypoint.

#![forbid(unsafe_code)]

use std::{io, path::PathBuf, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig as _;
use pvlog::SystemClock;
use pvlog::authentication::{
    ManagementAuditApi, ManagementConnectorApi, ManagementIdentityApi, ManagementRbacApi,
    ManagementReadiness, ManagementRequestAuthenticator, ManagementRequestAuthorizer,
    ManagementSessionBootstrap, session_digest_key,
};
use pvlog::config::{ConfigError, DatabaseBackend, RuntimeConfig};
use pvlog::inverters::ManagementInverterApi;
use pvlog::operator_bundle::{
    export_account_bundle, export_bundle, export_postgres_bundle, import_bundle, verify_bundle,
};
use pvlog_application::{
    Argon2CredentialConfig, Argon2CredentialService, BrowserSessionPolicy,
    BrowserSessionRepository, BrowserSessionService, BrowserSessionUseCases, Clock,
    CommonPasswordHook, DiscardingRecoveryNotifier, EquipmentCatalog,
    ExternalIdentityLinkingRepository, ExternalIdentityLinkingService,
    ExternalIdentityLinkingUseCases, ExternalLoginPolicy, LocalCredentialRepository,
    LocalPasswordPolicy, LocalPasswordService, LocalPasswordUseCases, LocalUserPolicy,
    SystemLifecycleRepository, SystemLifecycleService, SystemLifecycleUseCases,
    UserLifecycleRepository, UserLifecycleService, UserLifecycleUseCases,
};
use pvlog_storage::{
    DatabaseMigrationStatus, DatabaseTarget, MigrationError, MigrationPlanItem, ProbeError,
    apply_migrations, migration_plan, migration_status, probe_database,
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
    let system_lifecycle = compose_system_lifecycle(target)?;
    let browser_sessions = compose_browser_sessions(config, target)?;
    let session_bootstrap = Arc::new(ManagementSessionBootstrap::new(
        compose_management_repository(target)?,
    ));
    let audit_api = Arc::new(ManagementAuditApi::new(compose_management_repository(
        target,
    )?));
    let rbac_api = compose_rbac_api(target)?;
    let identity_api = compose_identity_api(target)?;
    let connector_api = Arc::new(ManagementConnectorApi::new(&config.auth.connectors));
    let inverter_api = Arc::new(ManagementInverterApi::new(target.clone()));
    let readiness = Arc::new(ManagementReadiness::new(target.clone()));
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
    Ok(Arc::new(ManagementRbacApi::new(
        repository,
        Arc::new(SystemClock),
    )))
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
            idle_lifetime_seconds: 1_800,
            absolute_lifetime_seconds: 28_800,
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
    #[error("telemetry initialization failed: {0}")]
    Telemetry(String),
    #[error("failed to serialize command output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema is not compatible with this release")]
    IncompatibleSchema,
    #[error("failed to initialize account lifecycle routing")]
    SystemLifecycleRouting,
    #[allow(dead_code)]
    #[error("the {0} database adapter is not enabled in this build")]
    AdapterDisabled(&'static str),
    #[error("HTTP server failed: {0}")]
    Io(#[from] io::Error),
}
