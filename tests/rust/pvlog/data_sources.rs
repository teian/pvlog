//! A data source owns an upload credential for exactly one system, hands out its
//! configuration once, and reports whether readings still arrive.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use pvlog::{
    authentication::ManagementApiTokenRepository, data_sources::StorageDataSourceApi,
    telemetry::StorageTelemetryApi,
};
use pvlog_api::{
    CreateDataSourceInput, DataSourceApiError, DataSourceApiUseCases as _, RenameDataSourceInput,
};
use pvlog_application::{
    ApiTokenError, ApiTokenService, Clock, ConnectionState, ModernTelemetryUseCases as _,
    NormalizeObservation, PowerUnit,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, BatteryFlowState, ObservationSource,
    ObservationSourceKind, QualityFlags, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository as _, AccountRecord, DatabaseTarget, ManagementRepository,
    PostgresAccountConfigurationRepository, PostgresManagementRepository,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteAccountProvisioner, SqliteManagementRepository, SystemConfigurationRecord,
    SystemRegistryRecord, UserRecord, apply_migrations,
};
use secrecy::ExposeSecret as _;
use tempfile::TempDir;

const START: i64 = 1_800_000_000_000;
const STATUS_INTERVAL_SECONDS: i64 = 300;
const DIGEST_KEY: [u8; 32] = [7; 32];

#[tokio::test]
async fn sqlite_data_source_lifecycle() -> Result<(), Box<dyn Error>> {
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
    let configuration = SqliteAccountConfigurationRepository::new(router.route(account_id).await?);
    let systems = [SystemId::new(), SystemId::new()];
    for system_id in systems {
        configuration.save_system(&system(system_id)).await?;
        management
            .save_system_registry(&registry(system_id, account_id))
            .await?;
    }
    verify_lifecycle(target, management, owner, account_id, systems).await
}

#[tokio::test]
async fn postgres_data_source_lifecycle_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    let target = DatabaseTarget::Postgres { url: url.clone() };
    apply_migrations(&target).await?;
    let management: Arc<dyn ManagementRepository> =
        Arc::new(PostgresManagementRepository::new(url.clone()));
    let (owner, account_id) = seed_owner(management.as_ref(), "active").await?;
    let configuration = PostgresAccountConfigurationRepository::new(url, account_id);
    let systems = [SystemId::new(), SystemId::new()];
    for system_id in systems {
        configuration.save_system(&system(system_id)).await?;
        management
            .save_system_registry(&registry(system_id, account_id))
            .await?;
    }
    verify_lifecycle(target, management, owner, account_id, systems).await
}

#[allow(clippy::too_many_lines)]
async fn verify_lifecycle(
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    owner: UserId,
    account_id: AccountId,
    [system_id, other_system]: [SystemId; 2],
) -> Result<(), Box<dyn Error>> {
    let clock = Arc::new(TestClock(AtomicI64::new(START)));
    let tokens = || {
        ApiTokenService::new(
            Arc::new(ManagementApiTokenRepository::new(management.clone())),
            clock.clone(),
            DIGEST_KEY,
        )
    };
    let api = StorageDataSourceApi::new(
        target.clone(),
        tokens(),
        management.clone(),
        clock.clone(),
        "https://pvlog.example/",
    );
    let telemetry = StorageTelemetryApi::new(target, management.clone(), clock.clone());
    let verifier = tokens();

    assert!(matches!(
        api.create(owner, account_id, system_id, input("Roof", "solar-wizard"))
            .await,
        Err(DataSourceApiError::InvalidInput("profile"))
    ));
    assert!(matches!(
        api.create(owner, account_id, system_id, input("   ", "smalog"))
            .await,
        Err(DataSourceApiError::InvalidInput("name"))
    ));

    let created = api
        .create(
            owner,
            account_id,
            system_id,
            input(" Roof logger ", "smalog"),
        )
        .await
        .map_err(failure)?;
    let source = created.data_source;
    assert_eq!(source.name, "Roof logger");
    assert_eq!(source.state, ConnectionState::NeverConnected);
    assert_eq!(source.server_url, "https://pvlog.example");
    let configuration = created.configuration.ok_or("a smalog source needs a key")?;
    let key = configuration.api_key;
    assert_eq!(configuration.system_id, system_id);
    let config_file = configuration
        .snippets
        .iter()
        .find(|snippet| snippet.id == "smalog-config")
        .ok_or("config.toml snippet missing")?;
    assert!(config_file.content.contains(&system_id.to_string()));
    assert!(config_file.content.contains("https://pvlog.example"));
    assert!(
        !config_file.content.contains(key.expose_secret()),
        "the shareable file refers to the key through a variable"
    );
    assert!(configuration.snippets.iter().any(
        |snippet| snippet.id == "environment" && snippet.content.contains(key.expose_secret())
    ));

    assert!(matches!(
        api.create(
            owner,
            account_id,
            system_id,
            input("Roof logger", "generic")
        )
        .await,
        Err(DataSourceApiError::NameTaken)
    ));

    // The credential uploads for its own system and nothing else.
    let credential = verifier
        .verify(&key, ApiScope::TelemetryWrite, account_id, Some(system_id))
        .await?;
    assert_eq!(credential.system_id, Some(system_id));
    assert_eq!(
        credential.scopes,
        BTreeSet::from([ApiScope::TelemetryWrite])
    );
    assert!(invalid(
        verifier
            .verify(
                &key,
                ApiScope::TelemetryWrite,
                account_id,
                Some(other_system)
            )
            .await
    ));
    for scope in [
        ApiScope::TelemetryRead,
        ApiScope::SystemsRead,
        ApiScope::SystemsWrite,
    ] {
        assert!(invalid(
            verifier
                .verify(&key, scope, account_id, Some(system_id))
                .await
        ));
    }

    // The credential names its data source, so the account's key list can
    // show it as managed; a hand-made key name does not.
    assert_eq!(
        pvlog::data_sources::credential_data_source(&credential.name),
        Some(source.id)
    );
    assert_eq!(
        pvlog::data_sources::credential_data_source("Home Assistant"),
        None
    );
    assert_eq!(
        pvlog::data_sources::credential_data_source("data-source:not-a-uuid:1"),
        None
    );

    // Two sources feed one system; each is credited with its own readings.
    let second = api
        .create(owner, account_id, system_id, input("Garage", "generic"))
        .await
        .map_err(failure)?;
    let second_credential = second
        .data_source
        .credential_id
        .ok_or("second key missing")?;
    telemetry
        .ingest(reading(system_id, Some(credential.id), START))
        .await?;
    clock.advance(60_000);
    telemetry
        .ingest(reading(system_id, Some(credential.id), START + 60_000))
        .await?;
    telemetry
        .ingest(reading(system_id, Some(second_credential), START + 61_000))
        .await?;
    let listed = api.list(account_id, system_id).await.map_err(failure)?;
    assert_eq!(listed.len(), 2);
    let roof = listed
        .iter()
        .find(|entry| entry.id == source.id)
        .ok_or("roof source missing")?;
    let garage = listed
        .iter()
        .find(|entry| entry.id == second.data_source.id)
        .ok_or("garage source missing")?;
    assert_eq!(roof.state, ConnectionState::Receiving);
    assert_eq!(roof.readings_last_day, 2);
    assert_eq!(roof.last_reading_at_epoch_millis, Some(START + 60_000));
    assert_eq!(garage.readings_last_day, 1);
    assert_eq!(garage.last_reading_at_epoch_millis, Some(START + 61_000));

    // A test reading proves the connection without becoming telemetry.
    let sent = api
        .send_test_reading(owner, account_id, system_id, source.id)
        .await
        .map_err(failure)?;
    assert!(sent.test);
    assert_eq!(sent.data_source_id, Some(source.id));
    let mut from_logger = reading(system_id, Some(second_credential), START + 62_000);
    from_logger.test = true;
    telemetry.ingest(from_logger).await?;
    let after_tests = api.list(account_id, system_id).await.map_err(failure)?;
    let counts = |entries: &[pvlog_api::DataSourceResponse]| -> Vec<u64> {
        entries
            .iter()
            .map(|entry| entry.readings_last_day)
            .collect()
    };
    assert_eq!(
        counts(&after_tests),
        counts(&listed),
        "test readings are not counted as delivered data"
    );
    assert_eq!(
        after_tests
            .iter()
            .map(|entry| entry.last_reading_at_epoch_millis)
            .collect::<Vec<_>>(),
        listed
            .iter()
            .map(|entry| entry.last_reading_at_epoch_millis)
            .collect::<Vec<_>>()
    );
    api.delete_test_readings(owner, account_id, system_id)
        .await
        .map_err(failure)?;
    assert_eq!(
        counts(&api.list(account_id, system_id).await.map_err(failure)?),
        counts(&listed),
        "removing test readings removes no real reading"
    );

    // Exactly three status intervals of silence are still tolerated.
    clock.set(START + 60_000 + 3 * STATUS_INTERVAL_SECONDS * 1_000);
    assert_eq!(
        api.read(account_id, system_id, source.id)
            .await
            .map_err(failure)?
            .state,
        ConnectionState::Receiving
    );
    clock.advance(1);
    let stale = api
        .read(account_id, system_id, source.id)
        .await
        .map_err(failure)?;
    assert_eq!(stale.state, ConnectionState::Stale);
    assert_eq!(stale.last_reading_at_epoch_millis, Some(START + 60_000));

    // Revoking the key from the account's key list disconnects the source.
    verifier.revoke(account_id, credential.id).await?;
    assert_eq!(
        api.read(account_id, system_id, source.id)
            .await
            .map_err(failure)?
            .state,
        ConnectionState::Disconnected
    );

    let regenerated = api
        .regenerate(owner, account_id, system_id, source.id)
        .await
        .map_err(failure)?;
    let new_key = regenerated
        .configuration
        .ok_or("regeneration returns a configuration")?
        .api_key;
    assert_ne!(new_key.expose_secret(), key.expose_secret());
    assert!(invalid(
        verifier
            .verify(&key, ApiScope::TelemetryWrite, account_id, Some(system_id))
            .await
    ));
    verifier
        .verify(
            &new_key,
            ApiScope::TelemetryWrite,
            account_id,
            Some(system_id),
        )
        .await?;
    assert_eq!(regenerated.data_source.state, ConnectionState::Stale);

    let renamed = api
        .rename(
            owner,
            account_id,
            system_id,
            source.id,
            RenameDataSourceInput {
                name: "Roof".to_owned(),
            },
        )
        .await
        .map_err(failure)?;
    assert_eq!(renamed.name, "Roof");

    // Deleting keeps the readings and ends the credential.
    api.delete(owner, account_id, system_id, source.id)
        .await
        .map_err(failure)?;
    assert!(invalid(
        verifier
            .verify(
                &new_key,
                ApiScope::TelemetryWrite,
                account_id,
                Some(system_id)
            )
            .await
    ));
    assert!(matches!(
        api.read(account_id, system_id, source.id).await,
        Err(DataSourceApiError::NotFound)
    ));
    let remaining = api.list(account_id, system_id).await.map_err(failure)?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0].readings_last_day, 1,
        "the other source is unaffected"
    );
    // The name of a removed source can be used again.
    api.create(owner, account_id, system_id, input("Roof", "import"))
        .await
        .map_err(failure)?;

    let audit = management.account_audit(account_id, 100).await?;
    let actions: BTreeSet<&str> = audit
        .iter()
        .filter(|event| event.target_type == "data_source")
        .map(|event| event.action.as_str())
        .collect();
    assert_eq!(
        actions,
        BTreeSet::from([
            "system.data_source.created",
            "system.data_source.credential.regenerated",
            "system.data_source.deleted",
            "system.data_source.renamed",
            "system.data_source.test_reading.sent",
        ])
    );
    for event in &audit {
        let recorded = event.safe_metadata.to_string();
        assert!(!recorded.contains(key.expose_secret()));
        assert!(!recorded.contains(new_key.expose_secret()));
        if event.target_type == "data_source" {
            assert_eq!(event.actor_id, Some(owner.as_uuid()));
        }
    }
    Ok(())
}

fn invalid<T>(result: Result<T, ApiTokenError>) -> bool {
    matches!(result, Err(ApiTokenError::InvalidToken))
}

fn failure(error: DataSourceApiError) -> String {
    format!("data source operation failed: {error:?}")
}

fn input(name: &str, profile: &str) -> CreateDataSourceInput {
    CreateDataSourceInput {
        name: name.to_owned(),
        profile: profile.to_owned(),
    }
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
        idempotency_namespace: format!("test:{credential_id:?}"),
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
            slug: format!("sources-{account_id}"),
            display_name: "Sources".to_owned(),
            status: account_status.to_owned(),
            created_by: Some(owner),
            created_at: START,
            updated_at: START,
        })
        .await?;
    Ok((owner, account_id))
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Sources".to_owned(),
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
