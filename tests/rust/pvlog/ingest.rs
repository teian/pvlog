//! The ingestion API of a logger, from request to stored values, on both databases.

use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use pvlog::{
    daily_summaries::StorageDailySummaryApi, device_events::StorageDeviceEventApi,
    ingest::StorageIngestApi,
};
use pvlog_api::{
    DailySummaryApiUseCases as _, DeviceEventApiUseCases as _, DeviceEventQuery, IngestApiError,
    IngestApiUseCases as _, IngestMark, IngestResponse, IngestWriter,
};
use pvlog_application::{
    Clock, DayBasis, IngestRequest, check_ingest_request, unknown_ingest_member,
    validate_ingest_request,
};
use pvlog_domain::{AccountId, MembershipId, QualityFlags, SystemId, UserId, UtcTimestamp};
use pvlog_storage::{
    AccountConfigurationRepository as _, AccountRecord, DatabaseTarget, EnergySampleFilter,
    IngestRepository, ManagementRepository, MembershipRecord,
    PostgresAccountConfigurationRepository, PostgresIngestRepository, PostgresManagementRepository,
    PostgresTelemetryRepository, SqliteAccountConfigurationRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteIngestRepository,
    SqliteManagementRepository, SqliteTelemetryRepository, StoredObservation,
    SystemConfigurationRecord, SystemRegistryRecord, TelemetryRepository, UserRecord,
    apply_migrations,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// 2027-01-15 08:00 UTC in epoch seconds, and the logger's five-minute cycle.
const START: i64 = 1_800_000_000;
const STEP: i64 = 300;
const ROOF: &str = "2000123456";
const GARAGE: &str = "3000654321";

/// One system fed through the ingestion service, with direct access to what was stored.
struct Fixture {
    ingest: StorageIngestApi,
    telemetry: Box<dyn TelemetryRepository>,
    stored: Box<dyn IngestRepository>,
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    account_id: AccountId,
    system_id: SystemId,
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
        let account = SqliteAccountPoolRouter::new(
            management_path,
            accounts_dir,
            SqliteAccountPoolConfig::default(),
        )?
        .route(account_id)
        .await?;
        let system_id = SystemId::new();
        SqliteAccountConfigurationRepository::new(account.clone())
            .save_system(&system(system_id))
            .await?;
        Self::assemble(
            target,
            management,
            Box::new(SqliteTelemetryRepository::new(account.clone())),
            Box::new(SqliteIngestRepository::new(account)),
            account_id,
            system_id,
            Some(directory),
        )
        .await
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
        Self::assemble(
            target,
            management,
            Box::new(PostgresTelemetryRepository::new(url.clone(), account_id)),
            Box::new(PostgresIngestRepository::new(url, account_id)),
            account_id,
            system_id,
            None,
        )
        .await
    }

    async fn assemble(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        telemetry: Box<dyn TelemetryRepository>,
        stored: Box<dyn IngestRepository>,
        account_id: AccountId,
        system_id: SystemId,
        directory: Option<TempDir>,
    ) -> Result<Self, Box<dyn Error>> {
        management
            .save_system_registry(&SystemRegistryRecord {
                system_id,
                account_id,
                created_at: START * 1_000,
                updated_at: START * 1_000,
            })
            .await?;
        Ok(Self {
            ingest: StorageIngestApi::new(target.clone(), management.clone(), clock()),
            telemetry,
            stored,
            target,
            management,
            account_id,
            system_id,
            _directory: directory,
        })
    }

    /// Sends a request the way the API hands it on: checked completely first.
    async fn send_as(&self, identity: &str, body: Value) -> Result<IngestResponse, Box<dyn Error>> {
        assert_eq!(unknown_ingest_member(&body), None);
        let request: IngestRequest = serde_json::from_value(body)?;
        check_ingest_request(&request).map_err(|problem| format!("{problem:?}"))?;
        assert_eq!(validate_ingest_request(&request), Vec::new());
        self.ingest
            .ingest(self.system_id, writer(identity), request)
            .await
            .map_err(failed)
    }

    async fn send(&self, body: Value) -> Result<IngestResponse, Box<dyn Error>> {
        self.send_as("user:logger", body).await
    }

    /// The stored readings between two steps of the logger's cycle.
    async fn readings(&self, from: i64, to: i64) -> Result<Vec<StoredObservation>, Box<dyn Error>> {
        Ok(self
            .telemetry
            .observations(
                self.system_id,
                (START + from * STEP) * 1_000,
                (START + to * STEP) * 1_000,
            )
            .await?)
    }

    /// The stored reading at one step.
    async fn reading(&self, step: i64) -> Result<StoredObservation, Box<dyn Error>> {
        self.readings(step, step + 1)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| format!("no reading stored at step {step}").into())
    }

    /// The value a measurement channel holds at one step, with its unit and scale.
    async fn channel_value(
        &self,
        key: &str,
        step: i64,
    ) -> Result<Option<(i64, String, i32)>, Box<dyn Error>> {
        let channels = pvlog::channels::channel_repository(
            &self.target,
            self.management.as_ref(),
            self.system_id,
        )
        .await
        .ok_or("channel storage is unavailable")?;
        let Some(channel) = channels.channel(self.system_id, key).await? else {
            return Ok(None);
        };
        let at = (START + step * STEP) * 1_000;
        Ok(channels
            .channel_values(channel.id, at, at + 1)
            .await?
            .first()
            .map(|value| (value.value, channel.unit.clone(), channel.scale)))
    }

    /// The integer a device value of the key scheme holds at one step.
    async fn device_value(
        &self,
        serial: &str,
        measurement: &str,
        step: i64,
    ) -> Result<Option<i64>, Box<dyn Error>> {
        Ok(self
            .channel_value(&format!("inv.{serial}.{measurement}"), step)
            .await?
            .map(|(value, _, _)| value))
    }
}

fn writer(identity: &str) -> IngestWriter {
    IngestWriter {
        credential_id: None,
        identity: identity.to_owned(),
    }
}

fn failed(error: IngestApiError) -> Box<dyn Error> {
    format!("ingestion failed: {error:?}").into()
}

fn flagged(reading: &StoredObservation, flag: QualityFlags) -> bool {
    u16::try_from(reading.quality_flags)
        .ok()
        .and_then(|bits| QualityFlags::from_bits(bits).ok())
        .is_some_and(|flags| flags.contains(flag))
}

/// A request of schema version 1 with the given lists.
fn request(members: Value) -> Value {
    let mut body =
        json!({ "schemaVersion": 1, "source": { "name": "smalog", "version": "0.3.0" } });
    if let (Some(body), Some(members)) = (body.as_object_mut(), members.as_object()) {
        body.extend(members.clone());
    }
    body
}

/// An inverter that reports one phase and its lifetime counter.
fn inverter(serial: &str, power: i64, lifetime: i64) -> Value {
    json!({
        "id": serial,
        "ac": [{ "phase": 1, "powerW": power }],
        "energyLifetimeWh": lifetime,
    })
}

fn measurement(step: i64, devices: &[Value]) -> Value {
    json!({ "measuredAt": START + step * STEP, "devices": devices })
}

fn counts(response: &IngestResponse) -> [(u64, u64); 4] {
    [
        (
            response.measurements.stored,
            response.measurements.unchanged,
        ),
        (
            response.energy_samples.stored,
            response.energy_samples.unchanged,
        ),
        (
            response.daily_summaries.stored,
            response.daily_summaries.unchanged,
        ),
        (response.events.stored, response.events.unchanged),
    ]
}

/// A logger's complete cycle: two inverters with three phases, sparse
/// trackers, a battery and an extra value, a daily yield and an event.
fn complete_cycle() -> Value {
    request(json!({
        "devices": [
            { "id": ROOF, "kind": "inverter", "familyId": "9305", "configuredName": "Dach",
              "reportedName": "SN: 2000123456", "model": "SB 3000TL-21",
              "firmware": "3.10.18.R", "transport": "bluetooth" },
            { "id": GARAGE, "model": "SB 1.5-1VL-40" },
        ],
        "measurements": [{
            "measuredAt": START,
            "system": { "consumptionPowerW": 420, "consumptionEnergyWh": 35 },
            "devices": [
                { "id": ROOF,
                  "ac": [
                      { "phase": 1, "powerW": 800, "currentMa": 3478, "voltageMv": 230_100 },
                      { "phase": 2, "powerW": 790, "currentMa": 3434, "voltageMv": 229_900 },
                      { "phase": 3, "powerW": 810, "currentMa": 3521, "voltageMv": 230_400 },
                  ],
                  "gridFrequencyMhz": 49_980, "gridImportPowerW": 0, "gridExportPowerW": 1_900,
                  "energyTodayWh": 4_200, "energyLifetimeWh": 10_000_000,
                  "operatingTimeS": 86_400, "feedInTimeS": 80_000,
                  "statusCode": 307, "gridRelayStatusCode": 51,
                  "temperatureMillicelsius": 41_500, "signalPermille": 730,
                  "trackers": [
                      { "number": 1, "powerW": 1_300, "currentMa": 3_400, "voltageMv": 384_120 },
                      { "number": 5, "powerW": 1_150, "currentMa": 3_050, "voltageMv": 377_000 },
                  ],
                  "battery": { "stateOfChargePermille": 815, "voltageMv": 51_200,
                               "currentMa": -4_300, "temperatureMillicelsius": 24_000 },
                  "extra": { "dc.isolation": { "value": 1.25, "unit": "MΩ" } } },
                { "id": GARAGE,
                  "ac": [{ "phase": 1, "powerW": 600 }],
                  "energyLifetimeWh": 2_000_000,
                  "trackers": [{ "number": 1, "powerW": 640 }] },
            ],
        }],
        "dailySummaries": [
            { "deviceId": ROOF, "date": "2027-01-14", "energyWh": 5_200,
              "energyLifetimeWh": 9_995_800, "complete": true },
        ],
        "events": [
            { "deviceId": ROOF, "eventId": 8812, "occurredAt": START - 60, "code": 10_100,
              "type": "Event", "category": "Info", "group": "Parameter",
              "message": "Grid voltage setpoint", "oldValue": "230.0 V",
              "newValue": "235.0 V", "userGroup": "Installer" },
        ],
    }))
}

#[allow(clippy::too_many_lines)]
async fn a_complete_cycle_is_stored(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let response = fixture.send(complete_cycle()).await?;
    assert_eq!(counts(&response), [(1, 0), (0, 0), (1, 0), (1, 0)]);
    assert_eq!(response.measurements.rejected, Vec::new());

    // Every value reads back unchanged for its device, phase, tracker or battery.
    for (serial, measurement, value) in [
        (ROOF, "ac.l1.power", 800),
        (ROOF, "ac.l2.current", 3_434),
        (ROOF, "ac.l3.voltage", 230_400),
        (ROOF, "grid.frequency", 49_980),
        (ROOF, "grid.import_power", 0),
        (ROOF, "grid.export_power", 1_900),
        (ROOF, "energy.today", 4_200),
        (ROOF, "energy.lifetime", 10_000_000),
        (ROOF, "operating_time", 86_400),
        (ROOF, "feed_in_time", 80_000),
        (ROOF, "status", 307),
        (ROOF, "grid_relay", 51),
        (ROOF, "temperature", 41_500),
        (ROOF, "signal", 730),
        (ROOF, "mppt.1.voltage", 384_120),
        (ROOF, "mppt.5.power", 1_150),
        (ROOF, "battery.soc", 815),
        (ROOF, "battery.current", -4_300),
        (ROOF, "battery.temperature", 24_000),
        (GARAGE, "ac.l1.power", 600),
        (GARAGE, "mppt.1.power", 640),
    ] {
        assert_eq!(
            fixture.device_value(serial, measurement, 0).await?,
            Some(value),
            "{serial} {measurement}"
        );
    }
    // What a device did not report is missing, not zero; tracker numbers keep their gaps.
    assert_eq!(fixture.device_value(GARAGE, "ac.l2.power", 0).await?, None);
    assert_eq!(fixture.device_value(GARAGE, "temperature", 0).await?, None);
    assert_eq!(fixture.device_value(ROOF, "mppt.2.power", 0).await?, None);
    // A value PVLog has no member for becomes a channel with its own unit.
    assert_eq!(
        fixture
            .channel_value(&format!("inv.{ROOF}.x.dc.isolation"), 0)
            .await?,
        Some((1_250, "MΩ".to_owned(), 3))
    );

    // The system's own values: stated where the logger stated them, derived otherwise.
    let system = fixture.reading(0).await?;
    assert_eq!(system.consumption_power_watts, Some(420));
    assert_eq!(system.consumption_energy_wh, Some(35));
    assert_eq!(system.generation_power_watts, Some(800 + 790 + 810 + 600));
    assert_eq!(system.measurements.generation_lifetime_wh, Some(12_000_000));
    assert_eq!(
        system.generation_energy_wh, None,
        "there is no reading before this one"
    );
    assert_eq!(system.measurements.grid_export_power_watts, Some(1_900));
    assert_eq!(system.measurements.battery_state_basis_points, Some(8_150));

    // The daily yield and the event arrive with the same request.
    let days = StorageDailySummaryApi::new(fixture.target.clone(), clock())
        .list(
            fixture.account_id,
            fixture.system_id,
            "2027-01-14".to_owned(),
            "2027-01-14".to_owned(),
        )
        .await
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].basis, DayBasis::SuppliedComplete);
    assert_eq!(days[0].generation_energy_wh, Some(5_200));
    assert_eq!(
        days[0].supplied_devices[0].generation_lifetime_wh,
        Some(9_995_800)
    );
    let events =
        StorageDeviceEventApi::new(fixture.target.clone(), fixture.management.clone(), clock())
            .list(fixture.system_id, DeviceEventQuery::default())
            .await
            .map_err(|error| format!("{error:?}"))?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id.as_deref(), Some("8812"));
    assert_eq!(events[0].occurred_at_epoch_millis, (START - 60) * 1_000);
    assert_eq!(events[0].details.old_value.as_deref(), Some("230.0 V"));
    assert_eq!(events[0].details.user_group.as_deref(), Some("Installer"));

    // The devices are known by what they said about themselves, and proposed.
    let discovery = pvlog::discovery::discovery_repository(
        &fixture.target,
        fixture.management.as_ref(),
        fixture.system_id,
    )
    .await
    .ok_or("discovery storage is unavailable")?;
    let reported = discovery.reported_devices(fixture.system_id).await?;
    assert_eq!(reported.len(), 2);
    assert_eq!(reported[0].identity.firmware.as_deref(), Some("3.10.18.R"));
    assert_eq!(reported[0].identity.family_id.as_deref(), Some("9305"));
    let proposals = discovery.discovery_proposals(fixture.system_id).await?;
    assert_eq!(
        proposals
            .iter()
            .map(|proposal| (proposal.device_serial.as_str(), proposal.model.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (ROOF, Some("SB 3000TL-21")),
            (GARAGE, Some("SB 1.5-1VL-40"))
        ]
    );

    // The same request again: every item is unchanged and nothing stored differs.
    let before = fixture.readings(-1, 1).await?;
    let again = fixture.send(complete_cycle()).await?;
    assert_eq!(counts(&again), [(0, 1), (0, 0), (0, 1), (0, 1)]);
    assert_eq!(fixture.readings(-1, 1).await?, before);
    assert_eq!(again.marks, response.marks);

    // A value that matures replaces its predecessor: the day's yield grows
    // and the measurement is corrected, each still existing once.
    let mut matured = complete_cycle();
    matured["dailySummaries"][0]["energyWh"] = json!(5_350);
    matured["measurements"][0]["devices"][1] = json!({
        "id": GARAGE, "ac": [{ "phase": 1, "powerW": 650 }], "energyLifetimeWh": 2_000_010,
    });
    let replaced = fixture.send(matured).await?;
    assert_eq!(counts(&replaced), [(1, 0), (0, 0), (1, 0), (0, 1)]);
    let readings = fixture.readings(-1, 1).await?;
    assert_eq!(readings.len(), 1);
    assert_eq!(
        readings[0].id, before[0].id,
        "the reading keeps its identity"
    );
    assert_eq!(
        readings[0].generation_power_watts,
        Some(800 + 790 + 810 + 650)
    );
    assert_eq!(
        fixture.device_value(GARAGE, "ac.l1.power", 0).await?,
        Some(650)
    );
    assert_eq!(
        fixture.device_value(GARAGE, "mppt.1.power", 0).await?,
        None,
        "a value the corrected measurement no longer carries is gone"
    );
    Ok(())
}

async fn system_values_alone_are_stored(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // A source without device detail sends counters of the system only.
    let reading = |step: i64, lifetime: i64| {
        json!({
            "measuredAt": START + step * STEP,
            "system": {
                "generationPowerW": 1_200, "generationLifetimeWh": lifetime,
                "gridImportPowerW": 0, "gridExportPowerW": 700,
                "batteryStateOfChargePermille": 505, "batteryPowerW": -300,
                "temperatureMillicelsius": 18_500, "voltageMv": 231_000, "message": "ok",
            },
        })
    };
    let response = fixture
        .send(request(
            json!({ "measurements": [reading(0, 5_000), reading(1, 5_100)] }),
        ))
        .await?;
    assert_eq!(counts(&response)[0], (2, 0));
    let first = fixture.reading(0).await?;
    assert_eq!(first.generation_power_watts, Some(1_200));
    assert_eq!(first.generation_energy_wh, None);
    assert_eq!(first.measurements.net_grid_power_watts, Some(-700));
    assert_eq!(first.measurements.battery_state_basis_points, Some(5_050));
    assert_eq!(first.measurements.battery_power_watts, Some(-300));
    assert_eq!(first.measurements.temperature_millidegrees_c, Some(18_500));
    assert_eq!(first.measurements.message.as_deref(), Some("ok"));
    // The counter of the system says what was generated in between.
    let second = fixture.reading(1).await?;
    assert_eq!(second.generation_energy_wh, Some(100));
    assert!(flagged(&second, QualityFlags::DERIVED));

    // No device was named, so there is no channel and no proposal.
    let channels = pvlog::channels::channel_repository(
        &fixture.target,
        fixture.management.as_ref(),
        fixture.system_id,
    )
    .await
    .ok_or("channel storage is unavailable")?;
    assert!(channels.channels(fixture.system_id).await?.is_empty());
    assert_eq!(response.marks.devices, Vec::new());
    assert_eq!(response.marks.system.measured_at, Some(START + STEP));

    // A later request continues from the counter that is stored.
    fixture
        .send(request(json!({ "measurements": [reading(2, 5_250)] })))
        .await?;
    assert_eq!(fixture.reading(2).await?.generation_energy_wh, Some(150));
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn system_values_are_derived_from_devices(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let both = |step: i64, roof: i64, garage: i64| {
        measurement(
            step,
            &[inverter(ROOF, 1_000, roof), inverter(GARAGE, 500, garage)],
        )
    };
    fixture
        .send(request(json!({ "measurements": [
            both(0, 10_000, 2_000),
            both(1, 10_080, 2_040),
            // The garage inverter is silent.
            measurement(2, &[inverter(ROOF, 900, 10_150)]),
            both(3, 10_230, 2_100),
            both(4, 10_300, 2_130),
        ] })))
        .await?;
    let first = fixture.reading(0).await?;
    assert_eq!(first.generation_power_watts, Some(1_500));
    assert_eq!(first.measurements.generation_lifetime_wh, Some(12_000));
    assert_eq!(first.generation_energy_wh, None);

    let second = fixture.reading(1).await?;
    assert_eq!(second.generation_energy_wh, Some(80 + 40));
    assert!(flagged(&second, QualityFlags::DERIVED));

    // One of two inverters: its power is the system's, and no energy is
    // derived from a lifetime sum that lacks a device.
    let silent = fixture.reading(2).await?;
    assert_eq!(silent.generation_power_watts, Some(900));
    assert_eq!(silent.generation_energy_wh, None);
    assert_eq!(silent.measurements.generation_lifetime_wh, None);

    // The set of reporting devices changed again, so the energy of this
    // interval is unknown too; from the next one it is derived again.
    let back = fixture.reading(3).await?;
    assert_eq!(back.measurements.generation_lifetime_wh, Some(12_330));
    assert_eq!(back.generation_energy_wh, None);
    assert_eq!(
        fixture.reading(4).await?.generation_energy_wh,
        Some(70 + 30)
    );

    // A later request continues from what is stored.
    fixture
        .send(request(json!({ "measurements": [both(5, 10_350, 2_150)] })))
        .await?;
    assert_eq!(
        fixture.reading(5).await?.generation_energy_wh,
        Some(50 + 20)
    );

    // A value the logger states for the system wins over the derived one.
    let mut stated = both(6, 10_400, 2_170);
    stated["system"] = json!({ "generationPowerW": 1_480, "generationEnergyWh": 66 });
    // An inverter without any phase power contributes nothing to the power,
    // and a counter that went back is a reset: no energy across it.
    let without_power = measurement(
        7,
        &[
            json!({ "id": ROOF, "energyLifetimeWh": 10_450, "temperatureMillicelsius": 30_000 }),
            json!({ "id": GARAGE, "energyLifetimeWh": 5 }),
        ],
    );
    let grid_and_battery = measurement(
        8,
        &[
            json!({ "id": ROOF, "gridImportPowerW": 120, "gridExportPowerW": 0,
                    "battery": { "stateOfChargePermille": 800 } }),
            json!({ "id": GARAGE, "gridImportPowerW": 30, "gridExportPowerW": 0,
                    "battery": { "stateOfChargePermille": 601 } }),
        ],
    );
    fixture
        .send(request(
            json!({ "measurements": [stated, without_power, grid_and_battery] }),
        ))
        .await?;
    let stated = fixture.reading(6).await?;
    assert_eq!(stated.generation_power_watts, Some(1_480));
    assert_eq!(stated.generation_energy_wh, Some(66));
    assert!(!flagged(&stated, QualityFlags::DERIVED));

    let reset = fixture.reading(7).await?;
    assert_eq!(
        reset.generation_power_watts, None,
        "no device reported power"
    );
    assert_eq!(reset.generation_energy_wh, None);
    assert!(flagged(&reset, QualityFlags::COUNTER_RESET));

    let summed = fixture.reading(8).await?;
    assert_eq!(summed.measurements.grid_import_power_watts, Some(150));
    assert_eq!(summed.measurements.grid_export_power_watts, Some(0));
    assert_eq!(summed.measurements.net_grid_power_watts, Some(150));
    assert_eq!(
        summed.measurements.battery_state_basis_points,
        Some(7_005),
        "two batteries count equally"
    );
    assert_eq!(summed.measurements.generation_lifetime_wh, None);
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn archive_samples_fill_gaps(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let sample = |serial: &str, step: i64, lifetime: i64, power: i64| {
        json!({ "deviceId": serial, "measuredAt": START + step * STEP,
                "energyLifetimeWh": lifetime, "powerW": power })
    };
    let live = |step: i64, roof: i64, garage: i64| {
        measurement(
            step,
            &[inverter(ROOF, 1_000, roof), inverter(GARAGE, 500, garage)],
        )
    };
    // The logger was polling at step 0 and at step 3. In between it only has
    // what the inverters kept in their own archive.
    let body = request(json!({
        "measurements": [live(0, 10_000, 2_000), live(3, 10_240, 2_120)],
        "energySamples": [
            sample(ROOF, 0, 10_000, 990),
            sample(ROOF, 1, 10_080, 960),
            sample(GARAGE, 1, 2_040, 480),
            sample(ROOF, 2, 10_160, 955),
            sample(GARAGE, 2, 2_080, 470),
        ],
    }));
    let response = fixture.send(body.clone()).await?;
    assert_eq!(counts(&response)[..2], [(2, 0), (5, 0)]);

    // The samples are stored for their inverter as they were sent.
    let samples = fixture
        .stored
        .energy_samples(
            fixture.system_id,
            EnergySampleFilter {
                device_key: Some(GARAGE),
                from: None,
                to: None,
                limit: 100,
            },
        )
        .await?;
    assert_eq!(
        samples
            .iter()
            .map(|sample| (
                sample.measured_at,
                sample.energy_lifetime_wh,
                sample.power_watts
            ))
            .collect::<Vec<_>>(),
        vec![
            ((START + STEP) * 1_000, Some(2_040), Some(480)),
            ((START + 2 * STEP) * 1_000, Some(2_080), Some(470)),
        ]
    );

    // Where no live measurement exists, the samples contribute the energy of
    // the interval, and the live measurement after them continues from there.
    let filled = fixture.reading(1).await?;
    assert_eq!(filled.generation_energy_wh, Some(80 + 40));
    assert_eq!(filled.generation_power_watts, Some(960 + 480));
    assert_eq!(filled.measurements.generation_lifetime_wh, Some(12_120));
    assert_eq!(
        fixture.device_value(ROOF, "energy.lifetime", 1).await?,
        Some(10_080)
    );
    assert_eq!(
        fixture.reading(2).await?.generation_energy_wh,
        Some(80 + 40)
    );
    assert_eq!(
        fixture.reading(3).await?.generation_energy_wh,
        Some(80 + 40)
    );
    // Where a live measurement exists, it stays what it is.
    assert_eq!(
        fixture.reading(0).await?.generation_power_watts,
        Some(1_500)
    );
    assert_eq!(fixture.readings(0, 4).await?.len(), 4);

    // History that is sent again while live data arrives duplicates nothing.
    let again = fixture.send(body).await?;
    assert_eq!(counts(&again)[..2], [(0, 2), (0, 5)]);
    assert_eq!(fixture.readings(0, 4).await?.len(), 4);

    // Samples that arrive alone, for a time that has a live measurement, do
    // not replace it; a live measurement that arrives later does replace what
    // samples filled in.
    let late = fixture
        .send(request(json!({
            "measurements": [live(1, 10_081, 2_041)],
            "energySamples": [sample(ROOF, 3, 10_240, 1), sample(GARAGE, 3, 2_120, 1)],
        })))
        .await?;
    assert_eq!(counts(&late)[..2], [(1, 0), (2, 0)]);
    assert_eq!(
        fixture.reading(3).await?.generation_power_watts,
        Some(1_500)
    );
    let replaced = fixture.reading(1).await?;
    assert_eq!(replaced.id, filled.id);
    assert_eq!(replaced.generation_power_watts, Some(1_500));
    assert_eq!(replaced.generation_energy_wh, Some(81 + 41));
    assert_eq!(fixture.readings(0, 4).await?.len(), 4);

    assert_eq!(
        late.marks.devices,
        vec![
            IngestMark {
                device_id: Some(ROOF.to_owned()),
                measured_at: Some(START + 3 * STEP),
                sample_at: Some(START + 3 * STEP),
                ..IngestMark::default()
            },
            IngestMark {
                device_id: Some(GARAGE.to_owned()),
                measured_at: Some(START + 3 * STEP),
                sample_at: Some(START + 3 * STEP),
                ..IngestMark::default()
            },
        ]
    );
    Ok(())
}

async fn the_marks_are_a_cursor(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let empty = fixture
        .ingest
        .state(fixture.system_id, writer("user:logger"))
        .await
        .map_err(failed)?;
    assert_eq!(empty.devices, Vec::new());
    assert_eq!(empty.system, IngestMark::default());

    let response = fixture
        .send(request(json!({
            "measurements": [
                measurement(4, &[inverter(ROOF, 1_000, 10_000)]),
                measurement(2, &[inverter(ROOF, 1_000, 9_900), inverter(GARAGE, 1, 1)]),
            ],
            "energySamples": [{ "deviceId": ROOF, "measuredAt": START - STEP, "powerW": 5 }],
            "dailySummaries": [
                { "deviceId": ROOF, "date": "2027-01-13", "energyWh": 1 },
                { "deviceId": ROOF, "date": "2027-01-14", "energyWh": 2, "complete": true },
                { "date": "2027-01-12", "energyWh": 9 },
            ],
            "events": [
                { "deviceId": ROOF, "occurredAt": START + 30, "code": 1 },
                { "deviceId": "logger", "occurredAt": START + 90, "message": "restarted" },
            ],
        })))
        .await?;
    let roof = IngestMark {
        device_id: Some(ROOF.to_owned()),
        measured_at: Some(START + 4 * STEP),
        sample_at: Some(START - STEP),
        summary_date: Some("2027-01-14".to_owned()),
        event_at: Some(START + 30),
    };
    assert_eq!(
        response.marks.devices,
        vec![
            roof.clone(),
            IngestMark {
                device_id: Some(GARAGE.to_owned()),
                measured_at: Some(START + 2 * STEP),
                ..IngestMark::default()
            },
            IngestMark {
                device_id: Some("logger".to_owned()),
                event_at: Some(START + 90),
                ..IngestMark::default()
            },
        ]
    );
    assert_eq!(
        response.marks.system,
        IngestMark {
            measured_at: Some(START + 4 * STEP),
            summary_date: Some("2027-01-12".to_owned()),
            ..IngestMark::default()
        }
    );

    // A logger that lost its state asks, and gets the same marks.
    let state = fixture
        .ingest
        .state(fixture.system_id, writer("user:logger"))
        .await
        .map_err(failed)?;
    assert_eq!(state, response.marks);

    // Older data moves no mark back.
    let older = fixture
        .send(request(json!({
            "measurements": [measurement(1, &[inverter(ROOF, 1_000, 9_800)])],
            "dailySummaries": [{ "deviceId": ROOF, "date": "2027-01-01", "energyWh": 1 }],
        })))
        .await?;
    assert_eq!(older.marks.devices[0], roof);

    // Another source has delivered nothing, whatever this one did.
    let other = fixture
        .ingest
        .state(fixture.system_id, writer("user:another"))
        .await
        .map_err(failed)?;
    assert_eq!(other.devices, Vec::new());
    assert!(matches!(
        fixture
            .ingest
            .state(SystemId::new(), writer("user:logger"))
            .await,
        Err(IngestApiError::NotFound)
    ));
    Ok(())
}

async fn extra_values_become_channels(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let with_extra = |step: i64, extra: Value| {
        measurement(
            step,
            &[json!({ "id": ROOF, "ac": [{ "phase": 1, "powerW": 100 }], "extra": extra })],
        )
    };
    let response = fixture
        .send(request(json!({ "measurements": [
            with_extra(0, json!({ "fan.speed": { "value": 1450, "unit": "rpm" } })),
        ] })))
        .await?;
    assert_eq!(response.measurements.rejected, Vec::new());
    let key = format!("inv.{ROOF}.x.fan.speed");
    // A whole number keeps room for the decimals a later value may have.
    assert_eq!(
        fixture.channel_value(&key, 0).await?,
        Some((1_450_000, "rpm".to_owned(), 3))
    );

    // A value that contradicts its channel is left out and named, while the
    // rest of the measurement is stored.
    let response = fixture
        .send(request(json!({ "measurements": [
            with_extra(1, json!({ "fan.speed": { "value": 24, "unit": "Hz" } })),
        ] })))
        .await?;
    assert_eq!(counts(&response)[0], (1, 0));
    assert_eq!(response.measurements.rejected.len(), 1);
    let rejected = &response.measurements.rejected[0];
    assert_eq!(
        (
            rejected.category,
            rejected.index,
            rejected.member.as_str(),
            rejected.reason
        ),
        ("measurements", 0, key.as_str(), "unit_mismatch")
    );
    assert_eq!(fixture.channel_value(&key, 1).await?, None);
    assert_eq!(
        fixture.device_value(ROOF, "ac.l1.power", 1).await?,
        Some(100)
    );
    Ok(())
}

async fn history_arrives_in_bulk(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // Years ago, and several thousand measurements in one request.
    let history = 1_500_000_000;
    let measurements: Vec<Value> = (0..5_000_i64)
        .map(|step| {
            json!({
                "measuredAt": history + step * STEP,
                "devices": [
                    { "id": ROOF, "ac": [{ "phase": 1, "powerW": 1_000 + step % 7 }],
                      "energyLifetimeWh": 1_000 + step * 80,
                      "trackers": [{ "number": 1, "powerW": 1_050 }] },
                    { "id": GARAGE, "ac": [{ "phase": 1, "powerW": 500 }],
                      "energyLifetimeWh": 500 + step * 40 },
                ],
            })
        })
        .collect();
    let body = request(json!({ "measurements": measurements }));
    let response = fixture.send(body.clone()).await?;
    assert_eq!(counts(&response)[0], (5_000, 0));
    let stored = fixture
        .telemetry
        .observations(
            fixture.system_id,
            history * 1_000,
            (history + 5_000 * STEP) * 1_000,
        )
        .await?;
    assert_eq!(stored.len(), 5_000);
    assert_eq!(stored[0].generation_energy_wh, None);
    assert!(
        stored[1..]
            .iter()
            .all(|reading| reading.generation_energy_wh == Some(120))
    );
    assert_eq!(
        response.marks.system.measured_at,
        Some(history + 4_999 * STEP)
    );
    let again = fixture.send(body).await?;
    assert_eq!(counts(&again)[0], (0, 5_000));
    Ok(())
}

macro_rules! on_both_databases {
    ($($scenario:ident => $sqlite:ident, $postgres:ident;)*) => {$(
        #[tokio::test]
        async fn $sqlite() -> Result<(), Box<dyn Error>> {
            $scenario(Fixture::sqlite().await?).await
        }

        #[tokio::test]
        async fn $postgres() -> Result<(), Box<dyn Error>> {
            let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
                return Ok(());
            };
            $scenario(Fixture::postgres(url).await?).await
        }
    )*};
}

on_both_databases! {
    a_complete_cycle_is_stored =>
        sqlite_stores_a_complete_cycle_and_replaces_what_matures,
        postgres_stores_a_complete_cycle_and_replaces_what_matures_when_configured;
    system_values_alone_are_stored =>
        sqlite_stores_measurements_of_the_system_alone,
        postgres_stores_measurements_of_the_system_alone_when_configured;
    system_values_are_derived_from_devices =>
        sqlite_derives_system_values_from_devices,
        postgres_derives_system_values_from_devices_when_configured;
    archive_samples_fill_gaps =>
        sqlite_fills_gaps_from_archive_energy_samples,
        postgres_fills_gaps_from_archive_energy_samples_when_configured;
    the_marks_are_a_cursor =>
        sqlite_returns_the_marks_a_logger_continues_from,
        postgres_returns_the_marks_a_logger_continues_from_when_configured;
    extra_values_become_channels =>
        sqlite_stores_extra_device_values_as_channels,
        postgres_stores_extra_device_values_as_channels_when_configured;
    history_arrives_in_bulk =>
        sqlite_accepts_years_old_history_in_bulk,
        postgres_accepts_years_old_history_in_bulk_when_configured;
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(SteppingClock(AtomicI64::new(START * 1_000)))
}

/// A clock that moves on with every reading, so that each request is received
/// at its own time.
struct SteppingClock(AtomicI64);

impl Clock for SteppingClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(self.0.fetch_add(1_000, Ordering::SeqCst))
            .unwrap_or_else(|_| unreachable!())
    }
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
            slug: format!("ingest-{account_id}"),
            display_name: "Ingest".to_owned(),
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
        name: "Ingest".to_owned(),
        description: String::new(),
        timezone: "Europe/Berlin".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: 300,
        power_calculation_mode: "reported".to_owned(),
        net_calculation_mode: "separate_flows".to_owned(),
        created_at: START,
        updated_at: START,
    }
}
