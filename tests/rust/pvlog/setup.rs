//! Setup status, first-data verification and honest data states of a system.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use pvlog::{
    authentication::ManagementApiTokenRepository, dashboard::StorageDashboardApi,
    data_sources::StorageDataSourceApi, forecasting::StorageForecastApi, setup::StorageSetupApi,
    telemetry::StorageTelemetryApi,
};
use pvlog_api::{
    CreateDataSourceInput, DashboardApiUseCases as _, DashboardRestriction, DataSourceApiError,
    DataSourceApiUseCases as _, DataState, SetupApiError, SetupApiUseCases as _,
    SetupStatusResponse,
};
use pvlog_application::{
    ApiTokenService, Clock, ModernTelemetryUseCases as _, NormalizeObservation, PowerUnit,
    SetupItemKey, SetupItemState,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, BatteryFlowState, MembershipId, ObservationSource,
    ObservationSourceKind, QualityFlags, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository as _, AccountRecord, DatabaseTarget, ManagementRepository,
    MembershipRecord, PostgresAccountConfigurationRepository, PostgresManagementRepository,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteAccountProvisioner, SqliteManagementRepository, SystemConfigurationRecord,
    SystemRegistryRecord, UserRecord, apply_migrations,
};
use tempfile::TempDir;

const START: i64 = 1_800_000_000_000;
const STATUS_INTERVAL_SECONDS: i64 = 300;

#[tokio::test]
async fn sqlite_setup_follows_the_system() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    let target = DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    };
    apply_migrations(&target).await?;
    let management: Arc<dyn ManagementRepository> =
        Arc::new(SqliteManagementRepository::new(management_path.clone()));
    let (owner, account_id) = seed_owner(management.as_ref(), "provisioning").await?;
    SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone())
        .provision(account_id)
        .await?;
    let router = SqliteAccountPoolRouter::new(
        management_path,
        accounts_dir,
        SqliteAccountPoolConfig::default(),
    )?;
    let system_id = SystemId::new();
    SqliteAccountConfigurationRepository::new(router.route(account_id).await?)
        .save_system(&system(system_id))
        .await?;
    management
        .save_system_registry(&registry(system_id, account_id))
        .await?;
    verify(target, management, owner, account_id, system_id).await
}

#[tokio::test]
async fn postgres_setup_follows_the_system_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    let target = DatabaseTarget::Postgres { url: url.clone() };
    apply_migrations(&target).await?;
    let management: Arc<dyn ManagementRepository> =
        Arc::new(PostgresManagementRepository::new(url.clone()));
    let (owner, account_id) = seed_owner(management.as_ref(), "active").await?;
    let system_id = SystemId::new();
    PostgresAccountConfigurationRepository::new(url, account_id)
        .save_system(&system(system_id))
        .await?;
    management
        .save_system_registry(&registry(system_id, account_id))
        .await?;
    verify(target, management, owner, account_id, system_id).await
}

#[allow(clippy::too_many_lines)]
async fn verify(
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    owner: UserId,
    account_id: AccountId,
    system_id: SystemId,
) -> Result<(), Box<dyn Error>> {
    let clock = Arc::new(TestClock(AtomicI64::new(START)));
    let sources = StorageDataSourceApi::new(
        target.clone(),
        ApiTokenService::new(
            Arc::new(ManagementApiTokenRepository::new(management.clone())),
            clock.clone(),
            [3; 32],
        ),
        management.clone(),
        clock.clone(),
        "https://pvlog.example",
    );
    let telemetry = StorageTelemetryApi::new(target.clone(), management.clone(), clock.clone());
    let setup = StorageSetupApi::new(
        target.clone(),
        Arc::new(StorageForecastApi::new(
            target.clone(),
            management.clone(),
            clock.clone(),
            86_400_000,
        )),
        clock.clone(),
    );
    let dashboard = StorageDashboardApi::new(target, management, clock.clone());
    let restriction = DashboardRestriction {
        account_id: Some(account_id),
        system_id: Some(system_id),
    };

    // A system that has only a name.
    let status = setup.status(account_id, system_id).await.map_err(failed)?;
    assert_eq!(
        state(&status, SetupItemKey::DataSource),
        SetupItemState::Recommended
    );
    assert_eq!(
        state(&status, SetupItemKey::FirstData),
        SetupItemState::Blocked
    );
    assert_eq!(
        state(&status, SetupItemKey::Location),
        SetupItemState::Optional
    );
    assert_eq!(
        state(&status, SetupItemKey::Equipment),
        SetupItemState::Optional
    );
    assert_eq!(status.data_state, DataState::None);
    assert_eq!(status.last_received_at_epoch_millis, None);
    let first = setup
        .first_data(account_id, system_id)
        .await
        .map_err(failed)?;
    assert!(!first.received);
    assert!(first.reading.is_none());
    let empty = dashboard
        .dashboard(owner, restriction)
        .await
        .map_err(|_| "dashboard unavailable")?;
    assert_eq!(
        empty.data_state,
        DataState::None,
        "a system without readings is not live"
    );
    assert_eq!(empty.system_count, 1);
    assert_eq!(empty.age_seconds, 0);

    // A data source exists, but nothing arrived yet.
    let source = sources
        .create(
            owner,
            account_id,
            system_id,
            CreateDataSourceInput {
                name: "Roof logger".to_owned(),
                profile: "smalog".to_owned(),
            },
        )
        .await
        .map_err(source_failed)?
        .data_source;
    let status = setup.status(account_id, system_id).await.map_err(failed)?;
    assert_eq!(
        state(&status, SetupItemKey::DataSource),
        SetupItemState::Done
    );
    assert_eq!(
        state(&status, SetupItemKey::FirstData),
        SetupItemState::Recommended
    );

    // A test reading is shown, and is not data.
    sources
        .send_test_reading(owner, account_id, system_id, source.id)
        .await
        .map_err(source_failed)?;
    let first = setup
        .first_data(account_id, system_id)
        .await
        .map_err(failed)?;
    assert!(!first.received);
    let shown = first.reading.ok_or("the test reading is shown")?;
    assert!(shown.test);
    assert_eq!(shown.data_source_name.as_deref(), Some("Roof logger"));
    assert_eq!(shown.generation_power_watts, Some(100));
    let status = setup.status(account_id, system_id).await.map_err(failed)?;
    assert_eq!(
        state(&status, SetupItemKey::FirstData),
        SetupItemState::Recommended
    );
    assert_eq!(status.data_state, DataState::None);

    // The first real reading.
    clock.advance(60_000);
    telemetry
        .ingest(reading(system_id, source.credential_id, START + 60_000))
        .await?;
    let first = setup
        .first_data(account_id, system_id)
        .await
        .map_err(failed)?;
    assert!(first.received);
    let shown = first.reading.ok_or("the reading is shown")?;
    assert!(!shown.test);
    assert_eq!(shown.measured_at_epoch_millis, START + 60_000);
    assert_eq!(shown.generation_power_watts, Some(1_200));
    assert_eq!(shown.data_source_id, Some(source.id));
    assert_eq!(shown.data_source_name.as_deref(), Some("Roof logger"));
    let status = setup.status(account_id, system_id).await.map_err(failed)?;
    assert_eq!(
        state(&status, SetupItemKey::FirstData),
        SetupItemState::Done
    );
    assert!(
        status
            .items
            .iter()
            .all(|entry| entry.state != SetupItemState::Recommended),
        "a working system is not asked for anything"
    );
    assert_eq!(status.data_state, DataState::Current);
    assert_eq!(status.last_received_at_epoch_millis, Some(START + 60_000));
    let live = dashboard
        .dashboard(owner, restriction)
        .await
        .map_err(|_| "dashboard unavailable")?;
    assert_eq!(live.data_state, DataState::Current);

    // Silence beyond three status intervals.
    clock.set(START + 60_000 + 3 * STATUS_INTERVAL_SECONDS * 1_000 + 1);
    let status = setup.status(account_id, system_id).await.map_err(failed)?;
    assert_eq!(status.data_state, DataState::Stale);
    assert_eq!(
        dashboard
            .dashboard(owner, restriction)
            .await
            .map_err(|_| "dashboard unavailable")?
            .data_state,
        DataState::Stale
    );

    assert!(matches!(
        setup.status(account_id, SystemId::new()).await,
        Err(SetupApiError::NotFound)
    ));
    Ok(())
}

fn state(status: &SetupStatusResponse, key: SetupItemKey) -> SetupItemState {
    status
        .items
        .iter()
        .find(|entry| entry.key == key)
        .map_or_else(|| panic!("{key:?} missing"), |entry| entry.state)
}

fn failed(error: SetupApiError) -> String {
    format!("setup operation failed: {error:?}")
}

fn source_failed(error: DataSourceApiError) -> String {
    format!("data source operation failed: {error:?}")
}

fn reading(
    system_id: SystemId,
    credential_id: Option<ApiCredentialId>,
    measured_at: i64,
) -> NormalizeObservation {
    let observed_at =
        UtcTimestamp::from_epoch_millis(measured_at).unwrap_or_else(|_| unreachable!());
    NormalizeObservation {
        system_id,
        credential_id,
        test: false,
        observed_at,
        received_at: observed_at,
        generation_power: Some((1_200, PowerUnit::Watts)),
        generation_energy: None,
        consumption_power: None,
        consumption_energy: None,
        voltage_millivolts: None,
        temperature_millidegrees_celsius: None,
        battery_energy: None,
        battery_power: None,
        battery_state_of_charge_basis_points: None,
        battery_flow_state: BatteryFlowState::Unknown,
        extended: BTreeMap::new(),
        registered_channels: BTreeSet::new(),
        channel_readings: BTreeMap::new(),
        source: ObservationSource {
            kind: ObservationSourceKind::ModernApi,
            source_reference: None,
        },
        idempotency_namespace: "setup-test".to_owned(),
        idempotency_key: measured_at.to_string(),
        quality: QualityFlags::NONE,
    }
}

async fn seed_owner(
    management: &dyn ManagementRepository,
    account_status: &str,
) -> Result<(UserId, AccountId), Box<dyn Error>> {
    let owner = UserId::new();
    let account_id = AccountId::new();
    management
        .save_user(&UserRecord {
            id: owner,
            email: format!("owner-{owner}@example.test"),
            display_name: "Owner".to_owned(),
            status: "active".to_owned(),
            created_at: START,
            updated_at: START,
        })
        .await?;
    management
        .save_account(&AccountRecord {
            id: account_id,
            slug: format!("setup-{account_id}"),
            display_name: "Setup".to_owned(),
            status: account_status.to_owned(),
            created_by: Some(owner),
            created_at: START,
            updated_at: START,
        })
        .await?;
    management
        .save_membership(&MembershipRecord {
            id: MembershipId::new(),
            account_id,
            user_id: owner,
            status: "active".to_owned(),
            joined_at: Some(START),
            created_at: START,
            updated_at: START,
        })
        .await?;
    Ok((owner, account_id))
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Setup".to_owned(),
        description: String::new(),
        timezone: "UTC".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: STATUS_INTERVAL_SECONDS,
        power_calculation_mode: "reported".to_owned(),
        net_calculation_mode: "separate_flows".to_owned(),
        created_at: START,
        updated_at: START,
    }
}

fn registry(system_id: SystemId, account_id: AccountId) -> SystemRegistryRecord {
    SystemRegistryRecord {
        system_id,
        account_id,
        created_at: START,
        updated_at: START,
    }
}

/// A clock a test can move, so staleness does not depend on real time.
struct TestClock(AtomicI64);

impl TestClock {
    fn set(&self, epoch_millis: i64) {
        self.0.store(epoch_millis, Ordering::SeqCst);
    }

    fn advance(&self, millis: i64) {
        self.0.fetch_add(millis, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(self.0.load(Ordering::SeqCst))
            .unwrap_or_else(|_| unreachable!())
    }
}
