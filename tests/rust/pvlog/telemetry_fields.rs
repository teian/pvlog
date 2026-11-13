//! Canonical reading fields beyond power and interval energy, on both databases.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::{Arc, atomic::AtomicI64, atomic::Ordering},
};

use pvlog::{
    channels::StorageChannelApi, daily_summaries::StorageDailySummaryApi,
    reporting::StorageReportingApi, telemetry::StorageTelemetryApi,
};
use pvlog_api::{
    ChannelApiError, ChannelApiUseCases as _, ChannelBindingInput, DailySummaryApiError,
    DailySummaryApiUseCases as _, ReportingApiUseCases as _, SummarySource,
};
use pvlog_application::{
    BatchIngestionMode, ChannelReading, Clock, ComputedDay, DayBasis, DayEnergy, DecimalValue,
    EnergyInput, EnergyUnit, ModernTelemetryError, ModernTelemetryUseCases as _,
    NormalizeObservation, ObservationSupplements, PowerUnit, ReportedDevice, SuppliedDailySummary,
    SuppliedDay, effective_day,
};
use pvlog_domain::{
    AccountId, BatteryFlowState, EquipmentValueProvenance, InverterId, MembershipId,
    ObservationSource, ObservationSourceKind, QualityFlags, StringId, SystemId, UserId,
    UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository, AccountRecord, DatabaseTarget, InverterRecord,
    ManagementRepository, MembershipRecord, PostgresAccountConfigurationRepository,
    PostgresManagementRepository, PostgresTelemetryRepository, PvStringRecord,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteAccountProvisioner, SqliteManagementRepository, SqliteTelemetryRepository,
    StoredObservation, SystemConfigurationRecord, SystemRegistryRecord, TelemetryRepository,
    UserRecord, apply_migrations,
};
use tempfile::TempDir;

const START: i64 = 1_800_000_000_000;
const STEP: i64 = 300_000;

/// One system with its telemetry service and direct access to what was stored.
struct Fixture {
    telemetry: StorageTelemetryApi,
    stored: Box<dyn TelemetryRepository>,
    system_id: SystemId,
    account_id: AccountId,
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    /// The clock of the telemetry service; a test moves it forward.
    clock: Arc<TestClock>,
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
        let clock = Arc::new(TestClock(AtomicI64::new(START)));
        Ok(Self {
            telemetry: StorageTelemetryApi::new(target.clone(), management.clone(), clock.clone()),
            clock,
            stored: Box::new(SqliteTelemetryRepository::new(
                router.route(account_id).await?,
            )),
            system_id,
            account_id,
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
        let clock = Arc::new(TestClock(AtomicI64::new(START)));
        Ok(Self {
            telemetry: StorageTelemetryApi::new(target.clone(), management.clone(), clock.clone()),
            clock,
            stored: Box::new(PostgresTelemetryRepository::new(url.clone(), account_id)),
            system_id,
            account_id,
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

/// Readings through the night the clocks go back are aggregated: the local
/// hour from 02:00 to 03:00 occurs twice and becomes two hourly buckets.
#[tokio::test]
async fn sqlite_aggregates_the_hour_that_occurs_twice_when_the_clocks_go_back()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::sqlite().await?;
    let Some(account) = &fixture.sqlite else {
        return Ok(());
    };
    let system = fixture.system_id.as_uuid().as_bytes().to_vec();
    sqlx::query("UPDATE systems SET timezone='Europe/Berlin' WHERE id=?")
        .bind(system.clone())
        .execute(&mut *account.acquire().await?)
        .await?;
    // 2026-10-25 00:00 UTC is 02:00 summer time; 01:00 UTC is 02:00 winter time.
    let night = 1_792_886_400_000_i64;
    for quarter in 0..12 {
        let measured_at = night - 3_600_000 + quarter * 900_000;
        let mut reading = fixture.reading(0);
        reading.observed_at = UtcTimestamp::from_epoch_millis(measured_at)?;
        reading.received_at = reading.observed_at;
        reading.idempotency_key = measured_at.to_string();
        reading.generation_energy = interval(10);
        fixture.telemetry.ingest(reading).await?;
    }

    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let mut connection = account.acquire().await?;
    let hours: Vec<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT bucket_start,generation_energy_sum_wh FROM telemetry_rollups \
         WHERE system_id=? AND scope_kind='system' AND resolution='hour' ORDER BY bucket_start",
    )
    .bind(system.clone())
    .fetch_all(&mut *connection)
    .await?;
    // 01:00 summer time, 02:00 summer time, 02:00 winter time: one hour each.
    assert_eq!(
        hours,
        vec![
            (night - 3_600_000, Some(40)),
            (night, Some(40)),
            (night + 3_600_000, Some(40)),
        ]
    );
    let quarters: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM telemetry_rollups \
         WHERE system_id=? AND scope_kind='system' AND resolution='fifteen_minute'",
    )
    .bind(system.clone())
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(quarters, 12);
    let day: Option<i64> = sqlx::query_scalar(
        "SELECT generation_energy_wh FROM system_daily_summaries WHERE system_id=? AND local_date='2026-10-25'",
    )
    .bind(system)
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(day, Some(120));
    Ok(())
}

/// History of several years is aggregated one local year per cycle, and a
/// reading that arrives meanwhile is picked up once the rebuild finished.
#[tokio::test]
async fn sqlite_aggregates_years_of_history_one_year_per_cycle() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::sqlite().await?;
    let Some(account) = &fixture.sqlite else {
        return Ok(());
    };
    let system = fixture.system_id.as_uuid().as_bytes().to_vec();
    // Noon of 1 June 2024, 2025 and 2026 (UTC, the fixture's timezone).
    let years = [1_717_243_200_000_i64, 1_748_779_200_000, 1_780_315_200_000];
    let ingest = |measured_at: i64, energy: i64| {
        let mut reading = fixture.reading(0);
        reading.observed_at =
            UtcTimestamp::from_epoch_millis(measured_at).unwrap_or_else(|_| unreachable!());
        reading.received_at = reading.observed_at;
        reading.idempotency_key = measured_at.to_string();
        reading.generation_energy = interval(energy);
        reading
    };
    // The aggregation stamps what it writes with the real time; the history
    // arrived before it, the late reading after it.
    fixture
        .clock
        .0
        .store(years[2] + 3_600_000, Ordering::SeqCst);
    for at in years {
        fixture.telemetry.ingest(ingest(at, 100)).await?;
    }
    let yearly = || async {
        let mut connection = account.acquire().await?;
        let rows: Vec<(i64, Option<i64>)> = sqlx::query_as(
            "SELECT bucket_start,generation_energy_sum_wh FROM telemetry_rollups \
             WHERE system_id=? AND scope_kind='system' AND resolution='year' ORDER BY bucket_start",
        )
        .bind(system.clone())
        .fetch_all(&mut *connection)
        .await?;
        let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM aggregation_progress")
            .fetch_one(&mut *connection)
            .await?;
        Ok::<_, Box<dyn Error>>((rows.len(), running))
    };

    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    assert_eq!(
        yearly().await?,
        (1, 1),
        "the first year, and the rebuild runs on"
    );
    // A reading of the year already rebuilt arrives while the rebuild runs.
    fixture.clock.0.store(
        chrono::Utc::now().timestamp_millis() + 3_600_000,
        Ordering::SeqCst,
    );
    fixture
        .telemetry
        .ingest(ingest(years[0] + 900_000, 50))
        .await?;
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    assert_eq!(yearly().await?, (2, 1));
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    assert_eq!(
        yearly().await?,
        (3, 0),
        "every year, and the rebuild is done"
    );
    // The next cycle rebuilds the year of the late reading.
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let mut connection = account.acquire().await?;
    let first: Option<i64> = sqlx::query_scalar(
        "SELECT generation_energy_sum_wh FROM telemetry_rollups \
         WHERE system_id=? AND scope_kind='system' AND resolution='year' ORDER BY bucket_start LIMIT 1",
    )
    .bind(system.clone())
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(first, Some(150));
    let lifetime: Option<i64> = sqlx::query_scalar(
        "SELECT generation_energy_wh FROM system_lifetime_summaries WHERE system_id=?",
    )
    .bind(system)
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(lifetime, Some(350));
    Ok(())
}

/// A new reading rewrites the fine aggregates of its own day only, while its
/// month and year still count every reading.
#[tokio::test]
async fn sqlite_rewrites_only_the_days_that_received_readings() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::sqlite().await?;
    let Some(account) = &fixture.sqlite else {
        return Ok(());
    };
    let system = fixture.system_id.as_uuid().as_bytes().to_vec();
    // Noon of 1 and 2 June 2026 (UTC, the fixture's timezone).
    let (first_day, second_day) = (1_780_315_200_000_i64, 1_780_401_600_000_i64);
    let ingest = |measured_at: i64, energy: i64| {
        let mut reading = fixture.reading(0);
        reading.observed_at =
            UtcTimestamp::from_epoch_millis(measured_at).unwrap_or_else(|_| unreachable!());
        reading.received_at = reading.observed_at;
        reading.idempotency_key = measured_at.to_string();
        reading.generation_energy = interval(energy);
        reading
    };
    fixture
        .clock
        .0
        .store(second_day + 3_600_000, Ordering::SeqCst);
    fixture.telemetry.ingest(ingest(first_day, 100)).await?;
    fixture.telemetry.ingest(ingest(second_day, 200)).await?;
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let rollup = |resolution: &'static str, start: i64| {
        let system = system.clone();
        async move {
            let mut connection = account.acquire().await?;
            let row: (Option<i64>, i64) = sqlx::query_as(
                "SELECT generation_energy_sum_wh,calculated_at FROM telemetry_rollups \
                 WHERE system_id=? AND scope_kind='system' AND resolution=? \
                 AND bucket_start<=? AND bucket_end>? ",
            )
            .bind(system)
            .bind(resolution)
            .bind(start)
            .bind(start)
            .fetch_one(&mut *connection)
            .await?;
            Ok::<_, Box<dyn Error>>(row)
        }
    };
    let (_, first_built) = rollup("day", first_day).await?;

    fixture.clock.0.store(
        chrono::Utc::now().timestamp_millis() + 3_600_000,
        Ordering::SeqCst,
    );
    fixture
        .telemetry
        .ingest(ingest(second_day + 900_000, 50))
        .await?;
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;

    assert_eq!(
        rollup("day", first_day).await?,
        (Some(100), first_built),
        "the day without new readings stays as it was"
    );
    assert_eq!(rollup("day", second_day).await?.0, Some(250));
    assert_eq!(rollup("month", second_day).await?.0, Some(350));
    assert_eq!(rollup("year", second_day).await?.0, Some(350));
    Ok(())
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
    let accepted = fixture.telemetry.ingest(reading).await?.observation;
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

fn channel_value(text: &str, unit: Option<&str>) -> ChannelReading {
    ChannelReading {
        value: DecimalValue::parse(text),
        unit: unit.map(ToOwned::to_owned),
        ..ChannelReading::default()
    }
}

#[allow(clippy::too_many_lines)]
async fn typed_channels(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    const TRACKER_VOLTAGE: &str = "inv.2000123456.mppt.1.voltage";
    let channels = pvlog::channels::channel_repository(
        &fixture.target,
        fixture.management.as_ref(),
        fixture.system_id,
    )
    .await
    .ok_or("channel storage is unavailable")?;
    let channel = |key: &'static str| {
        let channels = &channels;
        async move {
            channels
                .channel(fixture.system_id, key)
                .await?
                .ok_or_else(|| Box::<dyn Error>::from(format!("channel {key} is missing")))
        }
    };

    let mut first = fixture.reading(0);
    first.channel_readings = BTreeMap::from([
        // A key of the device key scheme.
        (
            TRACKER_VOLTAGE.to_owned(),
            channel_value("384120", Some("mV")),
        ),
        // A decimal value under a key nobody defined.
        ("pvoutput.v7".to_owned(), channel_value("12.34", Some("V"))),
        // The earlier form.
        (
            "meter".to_owned(),
            ChannelReading {
                power_watts: Some(1_500),
                ..ChannelReading::default()
            },
        ),
    ]);
    let accepted = fixture.telemetry.ingest(first).await?;
    assert_eq!(accepted.rejected_channels, Vec::new());

    // The scheme's definition wins over anything the reading could imply.
    let voltage = channel(TRACKER_VOLTAGE).await?;
    assert_eq!(
        (
            voltage.data_type.as_str(),
            voltage.unit.as_str(),
            voltage.scale,
            voltage.aggregation.as_str(),
            voltage.target_kind.as_str(),
        ),
        ("integer", "mV", 0, "mean", "unassigned")
    );
    assert_eq!(voltage.last_value, Some(384_120));

    // The decimal keeps its precision: 12.34 at three decimal places.
    let extended = channel("pvoutput.v7").await?;
    assert_eq!(
        (
            extended.data_type.as_str(),
            extended.unit.as_str(),
            extended.scale
        ),
        ("decimal", "V", 3)
    );
    assert_eq!(extended.last_value, Some(12_340));
    let meter = channel("meter").await?;
    assert_eq!((meter.unit.as_str(), meter.scale), ("W", 0));
    assert_eq!(meter.last_value, Some(1_500));

    // Two values contradict their channel. They are left out and named; the
    // rest of the reading is stored.
    let mut second = fixture.reading(1);
    second.generation_power = Some((1_600, pvlog_application::PowerUnit::Watts));
    second.channel_readings = BTreeMap::from([
        (TRACKER_VOLTAGE.to_owned(), channel_value("384", Some("V"))),
        (
            "pvoutput.v7".to_owned(),
            channel_value("12.3456", Some("V")),
        ),
        (
            "meter".to_owned(),
            ChannelReading {
                power_watts: Some(1_600),
                ..ChannelReading::default()
            },
        ),
    ]);
    let accepted = fixture.telemetry.ingest(second).await?;
    assert_eq!(
        accepted
            .rejected_channels
            .iter()
            .map(|rejected| (rejected.channel_key.as_str(), rejected.reason))
            .collect::<Vec<_>>(),
        vec![
            (TRACKER_VOLTAGE, "unit_mismatch"),
            ("pvoutput.v7", "type_mismatch"),
        ]
    );
    assert_eq!(fixture.at(1).await?.generation_power_watts, Some(1_600));
    let values = |id| {
        let channels = &channels;
        async move {
            Ok::<_, Box<dyn Error>>(
                channels
                    .channel_values(id, START, START + 10 * STEP)
                    .await?
                    .into_iter()
                    .map(|value| (value.measured_at, value.value))
                    .collect::<Vec<_>>(),
            )
        }
    };
    assert_eq!(values(voltage.id).await?, vec![(START, 384_120)]);
    assert_eq!(values(extended.id).await?, vec![(START, 12_340)]);
    assert_eq!(
        values(meter.id).await?,
        vec![(START, 1_500), (START + STEP, 1_600)]
    );

    // In a batch the item is stored and reports what it could not keep.
    let mut item = fixture.reading(2);
    item.channel_readings = BTreeMap::from([
        (TRACKER_VOLTAGE.to_owned(), channel_value("0.5", None)),
        ("pvoutput.v7".to_owned(), channel_value("-0.001", None)),
    ]);
    for mode in [BatchIngestionMode::Partial, BatchIngestionMode::Atomic] {
        let mut item = item.clone();
        if mode == BatchIngestionMode::Atomic {
            item.observed_at = UtcTimestamp::from_epoch_millis(START + 3 * STEP)?;
            "atomic".clone_into(&mut item.idempotency_key);
        }
        let result = fixture.telemetry.ingest_batch(vec![item], mode).await?;
        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(
            result.outcomes[0]
                .rejected_channels
                .iter()
                .map(|rejected| (rejected.channel_key.as_str(), rejected.reason))
                .collect::<Vec<_>>(),
            vec![(TRACKER_VOLTAGE, "type_mismatch")]
        );
    }
    assert_eq!(
        values(extended.id).await?,
        vec![
            (START, 12_340),
            (START + 2 * STEP, -1),
            (START + 3 * STEP, -1)
        ]
    );

    // The channel resource lists, binds and retires them on either database.
    let api = StorageChannelApi::new(fixture.target.clone(), fixture.management.clone(), clock());
    let failed = |error: ChannelApiError| format!("channel request failed: {error:?}");
    let listed = api.list(fixture.system_id).await.map_err(failed)?;
    assert_eq!(
        listed
            .iter()
            .map(|channel| (
                channel.channel_key.as_str(),
                channel.unit.as_str(),
                channel.scale,
                channel.last_value
            ))
            .collect::<Vec<_>>(),
        vec![
            (TRACKER_VOLTAGE, "mV", 0, Some(384_120)),
            ("meter", "W", 0, Some(1_600)),
            ("pvoutput.v7", "V", 3, Some(-1)),
        ]
    );
    let bound = api
        .bind(
            UserId::new(),
            fixture.system_id,
            "meter",
            ChannelBindingInput {
                target_kind: "plant".to_owned(),
                target_id: None,
                display_name: Some("House meter".to_owned()),
                aggregation: Some("max".to_owned()),
            },
        )
        .await
        .map_err(failed)?;
    assert_eq!(
        (
            bound.target_kind.as_str(),
            bound.display_name.as_str(),
            bound.aggregation.as_str()
        ),
        ("plant", "House meter", "max")
    );
    assert!(matches!(
        api.bind(
            UserId::new(),
            fixture.system_id,
            "meter",
            ChannelBindingInput {
                target_kind: "inverter".to_owned(),
                target_id: Some(uuid::Uuid::now_v7()),
                display_name: None,
                aggregation: None,
            },
        )
        .await,
        Err(ChannelApiError::InvalidInput("unknown_target_equipment"))
    ));
    api.remove(fixture.system_id, "pvoutput.v7")
        .await
        .map_err(failed)?;
    assert!(matches!(
        api.remove(fixture.system_id, "never-seen").await,
        Err(ChannelApiError::NotFound)
    ));
    assert_eq!(api.list(fixture.system_id).await.map_err(failed)?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn sqlite_stores_typed_channel_values_and_names_contradictions() -> Result<(), Box<dyn Error>>
{
    typed_channels(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_stores_typed_channel_values_and_names_contradictions_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    typed_channels(Fixture::postgres(url).await?).await
}

async fn battery(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // The system's battery: state of charge, charging power and stored energy.
    let mut charging = fixture.reading(0);
    charging.battery_state_of_charge_basis_points = Some(6_250);
    charging.battery_power = Some((1_500, PowerUnit::Watts));
    charging.battery_energy = interval(4_000);
    let accepted = fixture.telemetry.ingest(charging).await?.observation;
    assert_eq!(
        accepted.values.battery.map(|battery| battery.flow_state),
        Some(BatteryFlowState::Charging)
    );
    let stored = fixture.at(0).await?.measurements;
    assert_eq!(stored.battery_state_basis_points, Some(6_250));
    assert_eq!(stored.battery_power_watts, Some(1_500));
    assert_eq!(stored.battery_energy_wh, Some(4_000));

    let mut discharging = fixture.reading(1);
    discharging.battery_power = Some((-700, PowerUnit::Watts));
    discharging.battery_energy = interval(3_900);
    let accepted = fixture.telemetry.ingest(discharging).await?.observation;
    assert_eq!(
        accepted.values.battery.map(|battery| battery.flow_state),
        Some(BatteryFlowState::Discharging)
    );
    let stored = fixture.at(1).await?.measurements;
    assert_eq!(stored.battery_power_watts, Some(-700));
    assert_eq!(
        stored.battery_state_basis_points, None,
        "a state that was not reported stays missing"
    );

    // A battery is never more than full or less than empty.
    for ratio in [10_001, -1] {
        let mut out_of_range = fixture.reading(2);
        out_of_range.battery_state_of_charge_basis_points = Some(ratio);
        assert!(matches!(
            fixture.telemetry.ingest(out_of_range).await,
            Err(ModernTelemetryError::InvalidField(
                "batteryStateOfChargeBasisPoints"
            ))
        ));
    }
    let mut negative = fixture.reading(2);
    negative.battery_energy = interval(-1);
    assert!(matches!(
        fixture.telemetry.ingest(negative).await,
        Err(ModernTelemetryError::InvalidField("batteryEnergyWh"))
    ));
    let mut overfull = fixture.reading(2);
    overfull.channel_readings.insert(
        "inv.2000123456.battery.soc".to_owned(),
        channel_value("1001", None),
    );
    assert!(matches!(
        fixture.telemetry.ingest(overfull).await,
        Err(ModernTelemetryError::InvalidField("channels"))
    ));
    assert!(
        fixture.at(2).await.is_err(),
        "a rejected reading is not stored"
    );

    // An inverter reports its battery: the values are kept for that inverter
    // in the logger's units, and the system's state follows them.
    let mut device = fixture.reading(3);
    for (measurement, value) in [("soc", "815"), ("voltage", "51200")] {
        device.channel_readings.insert(
            format!("inv.2000123456.battery.{measurement}"),
            channel_value(value, None),
        );
    }
    let accepted = fixture.telemetry.ingest(device).await?;
    assert_eq!(accepted.rejected_channels, Vec::new());
    let channels = pvlog::channels::channel_repository(
        &fixture.target,
        fixture.management.as_ref(),
        fixture.system_id,
    )
    .await
    .ok_or("channel storage is unavailable")?;
    for (measurement, unit, value) in [("soc", "‰", 815), ("voltage", "mV", 51_200)] {
        let channel = channels
            .channel(
                fixture.system_id,
                &format!("inv.2000123456.battery.{measurement}"),
            )
            .await?
            .ok_or("battery channel is missing")?;
        assert_eq!(channel.unit, unit);
        assert_eq!(channel.last_value, Some(value), "{measurement}");
    }
    assert_eq!(
        fixture.at(3).await?.measurements.battery_state_basis_points,
        Some(8_150),
        "the system's latest battery state reflects the device"
    );

    // Aggregates exist on SQLite only; PostgreSQL has no aggregation cycle yet.
    let Some(account) = &fixture.sqlite else {
        return Ok(());
    };
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let mut connection = account.acquire().await?;
    let change: Option<i64> = sqlx::query_scalar(
        "SELECT battery_energy_delta_wh FROM telemetry_rollups \
         WHERE system_id=? AND scope_kind='system' AND resolution='day'",
    )
    .bind(fixture.system_id.as_uuid().as_bytes().to_vec())
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(change, Some(-100), "the battery lost 100 Wh over the day");
    Ok(())
}

#[tokio::test]
async fn sqlite_stores_battery_measurements_and_their_change() -> Result<(), Box<dyn Error>> {
    battery(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_stores_battery_measurements_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    battery(Fixture::postgres(url).await?).await
}

const ROOF: &str = "2000123456";
const GARAGE: &str = "3000654321";

fn roof_inverter(system_id: SystemId) -> InverterRecord {
    let inverter_id = InverterId::new();
    InverterRecord {
        id: inverter_id,
        system_id,
        name: "Roof".to_owned(),
        manufacturer: Some("SMA".to_owned()),
        model: Some("typed by hand".to_owned()),
        serial_reference: Some(ROOF.to_owned()),
        rated_power_watts: None,
        catalog_entry_id: None,
        catalog_revision: None,
        value_provenance: EquipmentValueProvenance::Manual,
        specification_snapshot: None,
        effective_from: 0,
        effective_to: None,
        created_at: 1,
        updated_at: 1,
        firmware_version: None,
        firmware_changed_at: None,
        strings: vec![PvStringRecord {
            id: StringId::new(),
            inverter_id,
            name: "South".to_owned(),
            panel_count: 10,
            panel_manufacturer: None,
            panel_model: None,
            rated_power_watts: 4_000,
            module_catalog_entry_id: None,
            module_catalog_revision: None,
            value_provenance: EquipmentValueProvenance::Manual,
            module_specification_snapshot: None,
            module_peak_power_watts: None,
            total_peak_power_watts: None,
            orientation_degrees: None,
            tilt_degrees: None,
            temperature_coefficient_ppm_per_celsius: None,
            shading: Vec::new(),
            effective_from: 0,
            effective_to: None,
            created_at: 1,
            updated_at: 1,
        }],
    }
}

#[allow(clippy::too_many_lines)]
async fn device_identity(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let configuration: Box<dyn AccountConfigurationRepository> = match &fixture.sqlite {
        Some(account) => Box::new(SqliteAccountConfigurationRepository::new(account.clone())),
        None => Box::new(PostgresAccountConfigurationRepository::new(
            fixture.postgres_url.clone().ok_or("no database")?,
            fixture.account_id,
        )),
    };
    configuration
        .save_inverter_aggregate(&roof_inverter(fixture.system_id))
        .await?;
    let discovery = pvlog::discovery::discovery_repository(
        &fixture.target,
        fixture.management.as_ref(),
        fixture.system_id,
    )
    .await
    .ok_or("discovery storage is unavailable")?;
    let roof = |firmware: &str| ReportedDevice {
        id: ROOF.to_owned(),
        family_id: Some("9305".to_owned()),
        configured_name: Some("Dach".to_owned()),
        reported_name: Some("SN: 2000123456".to_owned()),
        model: Some("SB 3000TL-21".to_owned()),
        firmware: Some(firmware.to_owned()),
        transport: Some("bluetooth".to_owned()),
        ..ReportedDevice::default()
    };
    let inverter = || async {
        configuration
            .effective_inverters(fixture.system_id, START)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Box::<dyn Error>::from("the inverter is gone"))
    };

    // A logger says who its devices are: one is a known inverter, one is new.
    let mut first = fixture.reading(0);
    first.supplements.devices = vec![
        roof("3.10.18.R"),
        ReportedDevice {
            id: GARAGE.to_owned(),
            model: Some("SB 1.5-1VL-40".to_owned()),
            firmware: Some("2.4.6.R".to_owned()),
            configured_name: Some("Garage".to_owned()),
            ..ReportedDevice::default()
        },
    ];
    fixture.telemetry.ingest(first).await?;
    let known = inverter().await?;
    assert_eq!(known.firmware_version.as_deref(), Some("3.10.18.R"));
    assert_eq!(known.firmware_changed_at, Some(START));
    assert_eq!(known.model.as_deref(), Some("SB 3000TL-21"));
    assert_eq!(known.name, "Roof", "the name the owner chose stays");
    let proposals = discovery.discovery_proposals(fixture.system_id).await?;
    assert_eq!(
        proposals
            .iter()
            .map(|proposal| (
                proposal.device_serial.as_str(),
                proposal.reported_name.as_deref(),
                proposal.model.as_deref(),
                proposal.firmware.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![(
            GARAGE,
            Some("Garage"),
            Some("SB 1.5-1VL-40"),
            Some("2.4.6.R")
        )],
        "only the device that matches no inverter is proposed"
    );

    // The same firmware reported later is no change.
    fixture.clock.0.store(START + STEP, Ordering::SeqCst);
    let mut second = fixture.reading(1);
    second.supplements.devices = vec![roof("3.10.18.R")];
    fixture.telemetry.ingest(second).await?;
    assert_eq!(inverter().await?.firmware_changed_at, Some(START));

    // A new firmware version is shown with the time it was first reported.
    // What the logger leaves out this time stays as it was reported before.
    fixture.clock.0.store(START + 2 * STEP, Ordering::SeqCst);
    let mut third = fixture.reading(2);
    third.supplements.devices = vec![ReportedDevice {
        id: ROOF.to_owned(),
        firmware: Some("3.20.13.R".to_owned()),
        ..ReportedDevice::default()
    }];
    fixture.telemetry.ingest(third).await?;
    let updated = inverter().await?;
    assert_eq!(updated.firmware_version.as_deref(), Some("3.20.13.R"));
    assert_eq!(updated.firmware_changed_at, Some(START + 2 * STEP));
    assert_eq!(updated.model.as_deref(), Some("SB 3000TL-21"));

    let reported = discovery.reported_devices(fixture.system_id).await?;
    assert_eq!(
        reported
            .iter()
            .map(|device| device.identity.device_serial.as_str())
            .collect::<Vec<_>>(),
        vec![ROOF, GARAGE]
    );
    let latest = &reported[0];
    assert_eq!(latest.identity.kind.as_deref(), Some("inverter"));
    assert_eq!(latest.identity.family_id.as_deref(), Some("9305"));
    assert_eq!(latest.identity.configured_name.as_deref(), Some("Dach"));
    assert_eq!(
        latest.identity.reported_name.as_deref(),
        Some("SN: 2000123456")
    );
    assert_eq!(latest.identity.firmware.as_deref(), Some("3.20.13.R"));
    assert_eq!(latest.identity.transport.as_deref(), Some("bluetooth"));
    assert_eq!(
        (latest.first_reported_at, latest.last_reported_at),
        (START, START + 2 * STEP)
    );

    // A device that cannot be named in a channel key, or is listed twice, is refused.
    for devices in [
        vec![ReportedDevice {
            id: "20 00".to_owned(),
            ..ReportedDevice::default()
        }],
        vec![roof("1"), roof("2")],
        vec![ReportedDevice {
            id: ROOF.to_owned(),
            firmware: Some(" ".to_owned()),
            ..ReportedDevice::default()
        }],
    ] {
        let mut refused = fixture.reading(3);
        refused.supplements.devices = devices;
        assert!(matches!(
            fixture.telemetry.ingest(refused).await,
            Err(ModernTelemetryError::InvalidField("devices"))
        ));
    }
    assert_eq!(
        inverter().await?.firmware_version.as_deref(),
        Some("3.20.13.R"),
        "a refused reading changes nothing"
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_keeps_device_identity_and_updates_the_matched_inverter()
-> Result<(), Box<dyn Error>> {
    device_identity(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_keeps_device_identity_and_updates_the_matched_inverter_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    device_identity(Fixture::postgres(url).await?).await
}

fn summary(date: &str, generation: i64, complete: bool) -> SuppliedDailySummary {
    SuppliedDailySummary {
        date: date.to_owned(),
        generation_energy_wh: Some(generation),
        consumption_energy_wh: Some(5_000),
        complete,
        ..SuppliedDailySummary::default()
    }
}

#[allow(clippy::too_many_lines)]
async fn daily_summaries(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let failed = |error: DailySummaryApiError| format!("daily summary failed: {error:?}");
    let summaries = StorageDailySummaryApi::new(fixture.target.clone(), clock());
    let reporting = StorageReportingApi::new(fixture.target.clone());
    let source = || SummarySource {
        credential_id: None,
        identity: "user:history".to_owned(),
    };
    let (account_id, system_id) = (fixture.account_id, fixture.system_id);

    // A day that has nothing but a supplied summary.
    let day = summaries
        .supply(
            account_id,
            system_id,
            source(),
            SuppliedDailySummary {
                peak_power_watts: Some(7_400),
                peak_time: Some("12:35".to_owned()),
                weather_condition: Some("Fine".to_owned()),
                temperature_min_millidegrees_celsius: Some(9_000),
                temperature_max_millidegrees_celsius: Some(24_000),
                generation_lifetime_wh: Some(18_400_000),
                export_energy_wh: Some(6_500),
                import_energy_wh_by_band: [("peak".to_owned(), 1_200), ("offPeak".to_owned(), 800)]
                    .into(),
                ..summary("2026-06-10", 12_000, false)
            },
        )
        .await
        .map_err(failed)?;
    assert_eq!(day.basis, DayBasis::SuppliedIncomplete);
    assert_eq!(day.generation_energy_wh, Some(12_000));
    assert!(day.computed.is_none());
    let supplied = day.supplied.ok_or("the supplied summary is returned")?;
    assert_eq!(supplied.peak_time.as_deref(), Some("12:35"));
    assert_eq!(supplied.weather_condition.as_deref(), Some("Fine"));
    assert_eq!(supplied.generation_lifetime_wh, Some(18_400_000));
    assert_eq!(supplied.import_energy_wh_by_band.get("offPeak"), Some(&800));

    let statistics = reporting.statistics(account_id, system_id).await?;
    assert_eq!(statistics.generation_energy_wh, Some(12_000));
    assert_eq!(statistics.monthly.len(), 1);
    assert_eq!(statistics.monthly[0].generation_energy_wh, Some(12_000));
    assert_eq!(statistics.monthly[0].consumption_energy_wh, Some(5_000));
    assert_eq!(
        statistics.monthly[0].supplied_days, 1,
        "the month says that it rests on a supplied day"
    );
    assert_eq!(statistics.yearly.len(), 1);
    assert_eq!(statistics.yearly[0].year, 2026);
    assert_eq!(statistics.yearly[0].generation_energy_wh, Some(12_000));
    assert_eq!(statistics.yearly[0].supplied_days, 1);
    let seasonal = reporting.seasonal(account_id, system_id).await?;
    let summer = seasonal
        .seasons
        .iter()
        .find(|season| season.season == "summer")
        .ok_or("summer is reported")?;
    assert_eq!(
        (summer.generation_energy_wh, summer.measured_days),
        (12_000, 1)
    );

    // The current day is resubmitted with more energy and finally marked
    // complete: each submission replaces the one before.
    for (generation, complete) in [(14_000, false), (15_000, true)] {
        summaries
            .supply(
                account_id,
                system_id,
                source(),
                summary("2026-06-10", generation, complete),
            )
            .await
            .map_err(failed)?;
    }
    let days = summaries
        .list(
            account_id,
            system_id,
            "2026-06-01".to_owned(),
            "2026-06-30".to_owned(),
        )
        .await
        .map_err(failed)?;
    assert_eq!(days.len(), 1, "a resubmission creates no duplicate");
    assert_eq!(days[0].basis, DayBasis::SuppliedComplete);
    assert_eq!(days[0].generation_energy_wh, Some(15_000));
    assert_eq!(
        days[0].supplied.as_ref().and_then(|s| s.peak_power_watts),
        None,
        "the replacement is complete, not merged with its predecessor"
    );
    let statistics = reporting.statistics(account_id, system_id).await?;
    assert_eq!(statistics.generation_energy_wh, Some(15_000));
    assert_eq!(statistics.monthly[0].generation_energy_wh, Some(15_000));
    assert_eq!(statistics.monthly[0].supplied_days, 1);

    // A second day in another year.
    summaries
        .supply(
            account_id,
            system_id,
            source(),
            summary("2025-12-31", 900, true),
        )
        .await
        .map_err(failed)?;
    let statistics = reporting.statistics(account_id, system_id).await?;
    assert_eq!(
        statistics
            .yearly
            .iter()
            .map(|year| (year.year, year.generation_energy_wh))
            .collect::<Vec<_>>(),
        vec![(2025, Some(900)), (2026, Some(15_000))]
    );

    // Members out of range are rejected by name.
    for (broken, field) in [
        (summary("2026-02-30", 1, false), "date"),
        (summary("2026-06-11", -1, false), "generationEnergyWh"),
        (
            SuppliedDailySummary {
                peak_time: Some("25:00".to_owned()),
                ..summary("2026-06-11", 1, false)
            },
            "peakTime",
        ),
        (
            SuppliedDailySummary {
                import_energy_wh_by_band: [("night".to_owned(), 1)].into(),
                ..summary("2026-06-11", 1, false)
            },
            "importEnergyWhByBand",
        ),
    ] {
        assert_eq!(
            summaries
                .supply(account_id, system_id, source(), broken)
                .await,
            Err(DailySummaryApiError::InvalidField(field))
        );
    }

    // With interval readings for the same day; aggregates exist on SQLite only.
    if fixture.sqlite.is_none() {
        return Ok(());
    }
    for step in 0..3 {
        let mut reading = fixture.reading(step);
        reading.generation_energy = interval(100);
        fixture.telemetry.ingest(reading).await?;
    }
    pvlog::aggregation::run_cycle(&fixture.target, fixture.management.as_ref()).await?;
    let today = "2027-01-15";
    let listed = |days: Vec<pvlog_api::DailyValueResponse>| {
        days.into_iter()
            .next()
            .ok_or("the day with readings is listed")
    };
    let computed_only = listed(
        summaries
            .list(account_id, system_id, today.to_owned(), today.to_owned())
            .await
            .map_err(failed)?,
    )?;
    assert_eq!(computed_only.basis, DayBasis::Computed);
    assert_eq!(computed_only.generation_energy_wh, Some(300));

    // Three readings do not cover the day, so an incomplete summary is used,
    // and it replaces the computed value in the totals instead of adding to it.
    let incomplete = summaries
        .supply(account_id, system_id, source(), summary(today, 500, false))
        .await
        .map_err(failed)?;
    assert_eq!(incomplete.basis, DayBasis::SuppliedIncomplete);
    assert_eq!(
        incomplete
            .computed
            .as_ref()
            .and_then(|c| c.generation_energy_wh),
        Some(300),
        "the computed value stays retrievable"
    );
    let january = |statistics: &pvlog_api::StatisticsResponse| {
        statistics
            .yearly
            .iter()
            .find(|year| year.year == 2027)
            .map(|year| (year.generation_energy_wh, year.supplied_days))
    };
    let statistics = reporting.statistics(account_id, system_id).await?;
    assert_eq!(january(&statistics), Some((Some(500), 1)));
    assert_eq!(statistics.generation_energy_wh, Some(15_000 + 900 + 500));
    Ok(())
}

/// Stores a day as the aggregation would compute it from interval readings.
async fn computed_day(
    fixture: &Fixture,
    date: &str,
    generation: i64,
    coverage_basis_points: i32,
) -> Result<(), Box<dyn Error>> {
    if let Some(account) = &fixture.sqlite {
        let mut writer = account.acquire_writer().await?;
        sqlx::query(
            "INSERT INTO system_daily_summaries (system_id,local_date,timezone,\
             generation_energy_wh,coverage_basis_points,calculated_at) VALUES (?,?,?,?,?,?)",
        )
        .bind(fixture.system_id.as_uuid().as_bytes().to_vec())
        .bind(date)
        .bind("Europe/Berlin")
        .bind(generation)
        .bind(coverage_basis_points)
        .bind(START)
        .execute(writer.connection())
        .await?;
    } else {
        use sqlx::Connection as _;
        let url = fixture.postgres_url.as_deref().ok_or("no database")?;
        let mut connection = sqlx::PgConnection::connect(url).await?;
        sqlx::query(
            "INSERT INTO telemetry.daily_summaries (account_id,system_id,local_date,timezone,\
             generation_energy_wh,coverage_basis_points,calculated_at) \
             VALUES ($1,$2,$3::date,$4,$5,$6,$7)",
        )
        .bind(fixture.account_id.as_uuid())
        .bind(fixture.system_id.as_uuid())
        .bind(date)
        .bind("Europe/Berlin")
        .bind(generation)
        .bind(coverage_basis_points)
        .bind(START)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
    }
    Ok(())
}

/// The daily yield one inverter reports, with its lifetime counter.
fn device_yield(
    date: &str,
    serial: &str,
    generation: i64,
    lifetime: i64,
    complete: bool,
) -> SuppliedDailySummary {
    SuppliedDailySummary {
        date: date.to_owned(),
        device_id: Some(serial.to_owned()),
        generation_energy_wh: Some(generation),
        generation_lifetime_wh: Some(lifetime),
        complete,
        ..SuppliedDailySummary::default()
    }
}

#[allow(clippy::too_many_lines)]
async fn device_daily_summaries(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let failed = |error: DailySummaryApiError| format!("daily summary failed: {error:?}");
    let summaries = StorageDailySummaryApi::new(fixture.target.clone(), clock());
    let reporting = StorageReportingApi::new(fixture.target.clone());
    let source = || SummarySource {
        credential_id: None,
        identity: "user:logger".to_owned(),
    };
    let (account_id, system_id) = (fixture.account_id, fixture.system_id);
    let supply = |summary: SuppliedDailySummary| {
        let summaries = &summaries;
        async move {
            summaries
                .supply(account_id, system_id, source(), summary)
                .await
        }
    };
    let (complete_day, mixed_day, sparse_day) = ("2026-07-01", "2026-07-02", "2026-07-03");
    // Readings cover the first two days well and the third hardly at all.
    computed_day(&fixture, complete_day, 9_000, 9_500).await?;
    computed_day(&fixture, mixed_day, 8_000, 9_500).await?;
    computed_day(&fixture, sparse_day, 100, 1_000).await?;

    // Complete yields of both inverters: their sum is the day's generation,
    // and what the readings add up to stays retrievable.
    supply(device_yield(complete_day, ROOF, 5_200, 10_000_000, true))
        .await
        .map_err(failed)?;
    let day = supply(device_yield(complete_day, GARAGE, 4_100, 2_000_000, true))
        .await
        .map_err(failed)?;
    assert_eq!(day.basis, DayBasis::SuppliedComplete);
    assert_eq!(day.generation_energy_wh, Some(9_300));
    assert_eq!(
        day.computed.as_ref().and_then(|c| c.generation_energy_wh),
        Some(9_000)
    );
    let sum = day.supplied.as_ref().ok_or("the sum is returned")?;
    assert_eq!(sum.generation_lifetime_wh, Some(12_000_000));
    assert!(sum.complete);
    assert_eq!(
        day.supplied_devices
            .iter()
            .map(|device| (
                device.device_id.as_str(),
                device.generation_energy_wh,
                device.generation_lifetime_wh,
                device.complete
            ))
            .collect::<Vec<_>>(),
        vec![
            (ROOF, Some(5_200), Some(10_000_000), true),
            (GARAGE, Some(4_100), Some(2_000_000), true),
        ]
    );

    // One inverter has not closed its day: the summary is not complete, and
    // the readings cover the day, so the computed value is the one reported.
    supply(device_yield(mixed_day, ROOF, 3_000, 10_003_000, true))
        .await
        .map_err(failed)?;
    let day = supply(device_yield(mixed_day, GARAGE, 2_000, 2_002_000, false))
        .await
        .map_err(failed)?;
    assert_eq!(day.basis, DayBasis::Computed);
    assert_eq!(day.generation_energy_wh, Some(8_000));
    let sum = day.supplied.as_ref().ok_or("the sum is returned")?;
    assert_eq!(
        (sum.generation_energy_wh, sum.complete),
        (Some(5_000), false)
    );

    // Without sufficient readings the incomplete summary is all there is.
    let day = supply(device_yield(sparse_day, ROOF, 700, 10_003_700, false))
        .await
        .map_err(failed)?;
    assert_eq!(day.basis, DayBasis::SuppliedIncomplete);
    assert_eq!(day.generation_energy_wh, Some(700));

    // The open day matures: the resubmission replaces the device's summary.
    let day = supply(device_yield(mixed_day, GARAGE, 4_500, 2_004_500, true))
        .await
        .map_err(failed)?;
    assert_eq!(day.basis, DayBasis::SuppliedComplete);
    assert_eq!(day.generation_energy_wh, Some(7_500));
    assert_eq!(day.supplied_devices.len(), 2, "no summary is duplicated");

    let statistics = reporting.statistics(account_id, system_id).await?;
    // This fixture has no lifetime total from readings, so the statistics
    // show only what the supplied days change against their computed values.
    assert_eq!(
        statistics.generation_energy_wh,
        Some((9_300 - 9_000) + (7_500 - 8_000) + (700 - 100)),
        "a supplied day replaces what was computed for it instead of adding to it"
    );

    // A device without a lifetime counter leaves the lifetime sum unknown.
    let day = supply(SuppliedDailySummary {
        generation_lifetime_wh: None,
        ..device_yield(sparse_day, GARAGE, 300, 0, false)
    })
    .await
    .map_err(failed)?;
    assert_eq!(day.generation_energy_wh, Some(1_000));
    assert_eq!(
        day.supplied.as_ref().and_then(|s| s.generation_lifetime_wh),
        None
    );

    // A summary of the whole system from the same source wins over the sum.
    let day = supply(summary(complete_day, 9_999, true))
        .await
        .map_err(failed)?;
    assert_eq!(day.generation_energy_wh, Some(9_999));
    assert!(day.supplied_devices.is_empty());

    assert_eq!(
        supply(device_yield(sparse_day, "not a serial", 1, 1, false)).await,
        Err(DailySummaryApiError::InvalidField("deviceId"))
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_reports_device_yields_by_precedence() -> Result<(), Box<dyn Error>> {
    device_daily_summaries(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_reports_device_yields_by_precedence_when_configured() -> Result<(), Box<dyn Error>>
{
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    device_daily_summaries(Fixture::postgres(url).await?).await
}

#[tokio::test]
async fn sqlite_includes_supplied_daily_summaries_in_statistics() -> Result<(), Box<dyn Error>> {
    daily_summaries(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_includes_supplied_daily_summaries_in_statistics_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    daily_summaries(Fixture::postgres(url).await?).await
}

#[test]
fn a_day_reports_the_complete_summary_then_covered_readings_then_the_incomplete_summary() {
    let energy = |generation| DayEnergy {
        generation_energy_wh: Some(generation),
        ..DayEnergy::default()
    };
    let computed = |coverage| {
        Some(ComputedDay {
            energy: energy(300),
            coverage_basis_points: coverage,
        })
    };
    let supplied = |complete| {
        Some(SuppliedDay {
            energy: energy(500),
            complete,
        })
    };
    let basis = |computed, supplied| effective_day(computed, supplied).map(|(_, basis)| basis);

    assert_eq!(
        basis(computed(10_000), supplied(true)),
        Some(DayBasis::SuppliedComplete)
    );
    assert_eq!(
        basis(computed(9_000), supplied(false)),
        Some(DayBasis::Computed),
        "sufficient coverage beats a summary that may still change"
    );
    assert_eq!(
        basis(computed(2_000), supplied(false)),
        Some(DayBasis::SuppliedIncomplete)
    );
    assert_eq!(basis(computed(2_000), None), Some(DayBasis::Computed));
    assert_eq!(
        basis(None, supplied(false)),
        Some(DayBasis::SuppliedIncomplete)
    );
    assert_eq!(basis(None, None), None);
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
