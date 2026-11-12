//! Canonical reading fields beyond power and interval energy, on both databases.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::{Arc, atomic::AtomicI64, atomic::Ordering},
};

use pvlog::{
    daily_summaries::StorageDailySummaryApi, reporting::StorageReportingApi,
    telemetry::StorageTelemetryApi,
};
use pvlog_api::{
    DailySummaryApiError, DailySummaryApiUseCases as _, ReportingApiUseCases as _, SummarySource,
};
use pvlog_application::{
    BatchIngestionMode, ChannelReading, Clock, ComputedDay, DayBasis, DayEnergy, DecimalValue,
    EnergyInput, EnergyUnit, ModernTelemetryError, ModernTelemetryUseCases as _,
    NormalizeObservation, ObservationSupplements, SuppliedDailySummary, SuppliedDay, effective_day,
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
    account_id: AccountId,
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
        Ok(Self {
            telemetry: StorageTelemetryApi::new(target.clone(), management.clone(), clock()),
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
