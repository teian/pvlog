//! Canonical reading fields beyond power and interval energy, on both databases.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::{Arc, atomic::AtomicI64, atomic::Ordering},
};

use pvlog::telemetry::StorageTelemetryApi;
use pvlog_application::{
    BatchIngestionMode, Clock, EnergyInput, EnergyUnit, ModernTelemetryError,
    ModernTelemetryUseCases as _, NormalizeObservation, ObservationSupplements,
};
use pvlog_domain::{
    AccountId, BatteryFlowState, MembershipId, ObservationSource, ObservationSourceKind,
    QualityFlags, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository as _, AccountRecord, DatabaseTarget, ManagementRepository,
    MembershipRecord, PostgresAccountConfigurationRepository, PostgresManagementRepository,
    PostgresTelemetryRepository, SqliteAccountConfigurationRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteManagementRepository,
    SqliteTelemetryRepository, StoredObservation, SystemConfigurationRecord, SystemRegistryRecord,
    TelemetryRepository, UserRecord, apply_migrations,
};
use tempfile::TempDir;

const START: i64 = 1_800_000_000_000;
const STEP: i64 = 300_000;

/// One system with its telemetry service and direct access to what was stored.
struct Fixture {
    telemetry: StorageTelemetryApi,
    stored: Box<dyn TelemetryRepository>,
    system_id: SystemId,
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    /// The account database, where aggregates can be read directly.
    sqlite: Option<pvlog_storage::RoutedSqliteAccount>,
    postgres_url: Option<String>,
    _directory: Option<TempDir>,
}

impl Fixture {
    async fn sqlite() -> Result<Self, Box<dyn Error>> {
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
        let account_id = seed_account(management.as_ref(), "provisioning").await?;
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
        Ok(Self {
            telemetry: StorageTelemetryApi::new(target.clone(), management.clone(), clock()),
            stored: Box::new(SqliteTelemetryRepository::new(
                router.route(account_id).await?,
            )),
            system_id,
            target,
            management,
            sqlite: Some(router.route(account_id).await?),
            postgres_url: None,
            _directory: Some(directory),
        })
    }

    async fn postgres(url: String) -> Result<Self, Box<dyn Error>> {
        let target = DatabaseTarget::Postgres { url: url.clone() };
        apply_migrations(&target).await?;
        let management: Arc<dyn ManagementRepository> =
            Arc::new(PostgresManagementRepository::new(url.clone()));
        let account_id = seed_account(management.as_ref(), "active").await?;
        let system_id = SystemId::new();
        PostgresAccountConfigurationRepository::new(url.clone(), account_id)
            .save_system(&system(system_id))
            .await?;
        management
            .save_system_registry(&registry(system_id, account_id))
            .await?;
        Ok(Self {
            telemetry: StorageTelemetryApi::new(target.clone(), management.clone(), clock()),
            stored: Box::new(PostgresTelemetryRepository::new(url.clone(), account_id)),
            system_id,
            target,
            management,
            sqlite: None,
            postgres_url: Some(url),
            _directory: None,
        })
    }

    /// A reading at `step` intervals after the start, with nothing but its time.
    fn reading(&self, step: i64) -> NormalizeObservation {
        let measured_at = START + step * STEP;
        let observed_at =
            UtcTimestamp::from_epoch_millis(measured_at).unwrap_or_else(|_| unreachable!());
        NormalizeObservation {
            system_id: self.system_id,
            credential_id: None,
            test: false,
            observed_at,
            received_at: observed_at,
            generation_power: None,
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
            supplements: ObservationSupplements::default(),
            source: ObservationSource {
                kind: ObservationSourceKind::ModernApi,
                source_reference: Some("meter".to_owned()),
            },
            idempotency_namespace: "fields-test".to_owned(),
            idempotency_key: measured_at.to_string(),
            quality: QualityFlags::NONE,
        }
    }

    /// The stored reading at `step`.
    async fn at(&self, step: i64) -> Result<StoredObservation, Box<dyn Error>> {
        let measured_at = START + step * STEP;
        self.stored
            .observations(self.system_id, measured_at, measured_at + 1)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| format!("no reading stored at step {step}").into())
    }
}

fn flagged(observation: &StoredObservation, flag: QualityFlags) -> bool {
    u16::try_from(observation.quality_flags)
        .ok()
        .and_then(|bits| QualityFlags::from_bits(bits).ok())
        .is_some_and(|flags| flags.contains(flag))
}

fn interval(value: i64) -> Option<EnergyInput> {
    Some(EnergyInput {
        value,
        unit: EnergyUnit::WattHours,
        cumulative: false,
        reset_sequence: 0,
    })
}

async fn lifetime_counters(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let counter = |step: i64, generation: i64, consumption: Option<i64>| {
        let mut reading = fixture.reading(step);
        reading.supplements.generation_lifetime_wh = Some(generation);
        reading.supplements.consumption_lifetime_wh = consumption;
        reading
    };

    // The first counter has no predecessor: the energy before it is unknown.
    fixture
        .telemetry
        .ingest(counter(0, 1_000, Some(500)))
        .await?;
    let first = fixture.at(0).await?;
    assert_eq!(first.generation_energy_wh, None);
    assert_eq!(first.measurements.generation_lifetime_wh, Some(1_000));
    assert_eq!(first.measurements.consumption_lifetime_wh, Some(500));
    assert!(!flagged(&first, QualityFlags::DERIVED));

    // A higher counter: the difference is the interval energy.
    fixture
        .telemetry
        .ingest(counter(1, 1_250, Some(530)))
        .await?;
    let second = fixture.at(1).await?;
    assert_eq!(second.generation_energy_wh, Some(250));
    assert_eq!(second.consumption_energy_wh, Some(30));
    assert_eq!(second.measurements.generation_lifetime_wh, Some(1_250));
    assert!(flagged(&second, QualityFlags::DERIVED));
    assert!(!flagged(&second, QualityFlags::COUNTER_RESET));

    // A lower counter is a reset: nothing is derived, and the reading says so.
    fixture.telemetry.ingest(counter(2, 100, None)).await?;
    let reset = fixture.at(2).await?;
    assert_eq!(reset.generation_energy_wh, None);
    assert_eq!(reset.measurements.generation_lifetime_wh, Some(100));
    assert!(flagged(&reset, QualityFlags::COUNTER_RESET));
    assert!(!flagged(&reset, QualityFlags::DERIVED));

    // After the reset the counter counts from its new start.
    fixture.telemetry.ingest(counter(3, 160, None)).await?;
    assert_eq!(fixture.at(3).await?.generation_energy_wh, Some(60));

    // Interval energy and a counter together: both are stored, the supplied
    // interval energy is used.
    let mut both = counter(4, 400, None);
    both.generation_energy = interval(77);
    fixture.telemetry.ingest(both).await?;
    let both = fixture.at(4).await?;
    assert_eq!(both.generation_energy_wh, Some(77));
    assert_eq!(both.measurements.generation_lifetime_wh, Some(400));
    assert!(!flagged(&both, QualityFlags::DERIVED));

    // Another source has its own counter and no predecessor here.
    let mut other = counter(5, 9_000_000, None);
    other.source.source_reference = Some("second-meter".to_owned());
    fixture.telemetry.ingest(other).await?;
    assert_eq!(fixture.at(5).await?.generation_energy_wh, None);

    // In an atomic batch nothing is stored until all is valid, so a later item
    // takes its predecessor from the batch.
    fixture
        .telemetry
        .ingest_batch(
            vec![counter(6, 450, None), counter(7, 520, None)],
            BatchIngestionMode::Atomic,
        )
        .await?;
    assert_eq!(fixture.at(6).await?.generation_energy_wh, Some(50));
    assert_eq!(fixture.at(7).await?.generation_energy_wh, Some(70));

    // A counter cannot be negative; the rejection names the field.
    assert!(matches!(
        fixture.telemetry.ingest(counter(8, -1, None)).await,
        Err(ModernTelemetryError::InvalidField("generationLifetimeWh"))
    ));
    Ok(())
}

#[tokio::test]
async fn sqlite_derives_interval_energy_from_lifetime_counters() -> Result<(), Box<dyn Error>> {
    lifetime_counters(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_derives_interval_energy_from_lifetime_counters_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    lifetime_counters(Fixture::postgres(url).await?).await
}

async fn grid_flows(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // Feeding in: export power and energy, and what the meter imported meanwhile.
    let mut exporting = fixture.reading(0);
    exporting.supplements.grid_export_power_watts = Some(800);
    exporting.supplements.grid_export_energy_wh = Some(60);
    exporting.supplements.grid_import_energy_wh = Some(5);
    fixture.telemetry.ingest(exporting).await?;
    let stored = fixture.at(0).await?.measurements;
    assert_eq!(stored.grid_export_power_watts, Some(800));
    assert_eq!(stored.grid_export_energy_wh, Some(60));
    assert_eq!(stored.grid_import_energy_wh, Some(5));
    assert_eq!(stored.grid_import_power_watts, None);
    assert_eq!(
        stored.net_grid_power_watts, None,
        "the net flow is unknown while one direction is missing"
    );

    // Drawing from the grid, with the other direction reported as zero.
    let mut importing = fixture.reading(1);
    importing.supplements.grid_import_power_watts = Some(300);
    importing.supplements.grid_export_power_watts = Some(0);
    importing.supplements.grid_import_energy_wh = Some(25);
    fixture.telemetry.ingest(importing).await?;
    let stored = fixture.at(1).await?.measurements;
    assert_eq!(stored.grid_import_power_watts, Some(300));
    assert_eq!(stored.net_grid_power_watts, Some(300));

    // Both directions at once contradict each other.
    let mut contradicting = fixture.reading(2);
    contradicting.supplements.grid_import_power_watts = Some(100);
    contradicting.supplements.grid_export_power_watts = Some(100);
    assert!(matches!(
        fixture.telemetry.ingest(contradicting).await,
        Err(ModernTelemetryError::InvalidField("gridExportPowerWatts"))
    ));
    let mut negative = fixture.reading(2);
    negative.supplements.grid_import_energy_wh = Some(-1);
    assert!(matches!(
        fixture.telemetry.ingest(negative).await,
        Err(ModernTelemetryError::InvalidField("gridImportEnergyWh"))
    ));
    assert!(
        fixture.at(2).await.is_err(),
        "a rejected reading is not stored"
    );

    // Aggregates exist on SQLite only; PostgreSQL has no aggregation cycle yet.
    let Some(account) = &fixture.sqlite else {
        return Ok(());
    };
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let mut connection = account.acquire().await?;
    let system = fixture.system_id.as_uuid().as_bytes().to_vec();
    for resolution in ["fifteen_minute", "hour", "day", "month", "year"] {
        let sums: (Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT SUM(grid_import_energy_sum_wh),SUM(grid_export_energy_sum_wh) \
             FROM telemetry_rollups WHERE system_id=? AND scope_kind='system' AND resolution=?",
        )
        .bind(system.clone())
        .bind(resolution)
        .fetch_one(&mut *connection)
        .await?;
        assert_eq!(sums, (Some(30), Some(60)), "{resolution} rollups");
    }
    let daily: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT grid_import_energy_wh,grid_export_energy_wh FROM system_daily_summaries \
         WHERE system_id=?",
    )
    .bind(system.clone())
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(daily, (Some(30), Some(60)));
    let lifetime: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT grid_import_energy_wh,grid_export_energy_wh FROM system_lifetime_summaries \
         WHERE system_id=?",
    )
    .bind(system)
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(lifetime, (Some(30), Some(60)));
    Ok(())
}

#[tokio::test]
async fn sqlite_stores_and_aggregates_grid_flows() -> Result<(), Box<dyn Error>> {
    grid_flows(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_stores_grid_flows_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    grid_flows(Fixture::postgres(url).await?).await
}

/// The statements that fill the measurement columns from the provenance of
/// readings stored before those columns were written.
fn backfill_statements(migration: &str) -> Vec<String> {
    migration
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
        .split(';')
        .map(str::trim)
        .filter(|statement| statement.starts_with("UPDATE"))
        .map(ToOwned::to_owned)
        .collect()
}

#[allow(clippy::too_many_lines)]
async fn temperature_and_voltage(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    for (step, temperature, voltage) in [
        (0, Some(18_500), Some(229_000)),
        (1, Some(31_250), Some(231_000)),
        (2, Some(-2_000), None),
        (3, None, None),
    ] {
        let mut reading = fixture.reading(step);
        reading.temperature_millidegrees_celsius = temperature;
        reading.voltage_millivolts = voltage;
        fixture.telemetry.ingest(reading).await?;
    }
    let stored = fixture.at(1).await?;
    assert_eq!(stored.measurements.temperature_millidegrees_c, Some(31_250));
    assert_eq!(stored.measurements.voltage_millivolts, Some(231_000));
    assert_eq!(
        fixture.at(3).await?.measurements.temperature_millidegrees_c,
        None
    );

    // A reading from before the columns were written: values in provenance only.
    let mut legacy = fixture.at(3).await?;
    legacy.id = pvlog_domain::ObservationId::new();
    legacy.measured_at = START + 4 * STEP;
    legacy.idempotency_identity = Some("legacy".to_owned());
    legacy.canonical_hash = [9; 32];
    legacy.provenance = serde_json::json!({
        "temperatureMilliCelsius": 40_000,
        "voltageMillivolts": 235_000,
    });
    fixture.stored.insert_observation(&legacy).await?;
    // And one whose column already holds a value that differs from its provenance.
    let mut kept = legacy.clone();
    kept.id = pvlog_domain::ObservationId::new();
    kept.measured_at = START + 5 * STEP;
    kept.idempotency_identity = Some("kept".to_owned());
    kept.canonical_hash = [8; 32];
    kept.measurements.temperature_millidegrees_c = Some(12_000);
    fixture.stored.insert_observation(&kept).await?;

    // Running the backfill twice gives the same result as running it once.
    for _ in 0..2 {
        if let Some(account) = &fixture.sqlite {
            let mut writer = account.acquire_writer().await?;
            for statement in backfill_statements(include_str!(
                "../../../src/crates/storage/migrations/sqlite-account/0013_temperature_and_voltage.sql"
            )) {
                sqlx::query(sqlx::AssertSqlSafe(statement))
                    .execute(writer.connection())
                    .await?;
            }
        }
        if let Some(url) = &fixture.postgres_url {
            use sqlx::Connection as _;
            let mut connection = sqlx::PgConnection::connect(url).await?;
            for statement in backfill_statements(include_str!(
                "../../../src/crates/storage/migrations/postgres/0016_temperature_and_voltage.sql"
            )) {
                sqlx::query(sqlx::AssertSqlSafe(statement))
                    .execute(&mut connection)
                    .await?;
            }
        }
        let filled = fixture.at(4).await?.measurements;
        assert_eq!(filled.temperature_millidegrees_c, Some(40_000));
        assert_eq!(filled.voltage_millivolts, Some(235_000));
        let kept = fixture.at(5).await?.measurements;
        assert_eq!(kept.temperature_millidegrees_c, Some(12_000));
        assert_eq!(kept.voltage_millivolts, Some(235_000));
        assert_eq!(
            fixture.at(3).await?.measurements.temperature_millidegrees_c,
            None
        );
    }

    let Some(account) = &fixture.sqlite else {
        return Ok(());
    };
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let mut connection = account.acquire().await?;
    let day: (Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT temperature_min_millidegrees_c,temperature_max_millidegrees_c,\
         voltage_mean_millivolts FROM telemetry_rollups \
         WHERE system_id=? AND scope_kind='system' AND resolution='day'",
    )
    .bind(fixture.system_id.as_uuid().as_bytes().to_vec())
    .fetch_one(&mut *connection)
    .await?;
    // Minimum and maximum over all readings of the day; the mean voltage over
    // the four readings that reported one.
    assert_eq!(
        day,
        (
            Some(-2_000),
            Some(40_000),
            Some((229_000 + 231_000 + 235_000 + 235_000) / 4)
        )
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_stores_backfills_and_aggregates_temperature_and_voltage()
-> Result<(), Box<dyn Error>> {
    temperature_and_voltage(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_stores_and_backfills_temperature_and_voltage_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    temperature_and_voltage(Fixture::postgres(url).await?).await
}

async fn reading_message(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // Thirty characters are accepted, also when they are not ASCII.
    let longest = "Störung: Netzspannung zu hoch!";
    assert_eq!(longest.chars().count(), 30);
    let mut reading = fixture.reading(0);
    reading.supplements.message = Some(longest.to_owned());
    let accepted = fixture.telemetry.ingest(reading).await?;
    assert_eq!(accepted.values.message.as_deref(), Some(longest));
    assert_eq!(
        fixture.at(0).await?.measurements.message.as_deref(),
        Some(longest)
    );

    // One more is rejected, and the rejection names the field.
    let mut reading = fixture.reading(1);
    reading.supplements.message = Some(format!("{longest}!"));
    assert!(matches!(
        fixture.telemetry.ingest(reading).await,
        Err(ModernTelemetryError::InvalidField("message"))
    ));
    assert!(fixture.at(1).await.is_err());

    // A reading without a message has none.
    fixture.telemetry.ingest(fixture.reading(2)).await?;
    assert_eq!(fixture.at(2).await?.measurements.message, None);
    Ok(())
}

#[tokio::test]
async fn sqlite_accepts_and_returns_the_reading_message() -> Result<(), Box<dyn Error>> {
    reading_message(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_accepts_and_returns_the_reading_message_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    reading_message(Fixture::postgres(url).await?).await
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(TestClock(AtomicI64::new(START)))
}

async fn seed_account(
    management: &dyn ManagementRepository,
    account_status: &str,
) -> Result<AccountId, Box<dyn Error>> {
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
            slug: format!("fields-{account_id}"),
            display_name: "Fields".to_owned(),
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
    Ok(account_id)
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Fields".to_owned(),
        description: String::new(),
        timezone: "UTC".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: STEP / 1_000,
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

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(self.0.load(Ordering::SeqCst))
            .unwrap_or_else(|_| unreachable!())
    }
}
