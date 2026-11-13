#![allow(clippy::expect_used)]

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Uri},
    routing::post,
};
use pvlog_application::{device_channel, render_configuration, source_profile};
use pvlog_sbfspot_push::{
    ChannelValue, Checkpoint, CheckpointStore, DeviceIdentity, EventPosition, PushConfig,
    PushSummary, PvlogClient, PvlogClientConfig, Reading, SbfspotSource, SourceGroup, push_pending,
};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::tempdir;
use url::Url;

const ROOF: i64 = 2_000_123_456;
const GARAGE: i64 = 3_000_654_321;

/// The tables of an `SBFspot` database, as `SBFspot` creates them.
const SCHEMA: &[&str] = &[
    "CREATE TABLE DayData (TimeStamp datetime NOT NULL, Serial int NOT NULL, TotalYield int8, \
     Power int8, PVoutput int, PRIMARY KEY (TimeStamp, Serial))",
    "CREATE TABLE Consumption (TimeStamp datetime NOT NULL, EnergyUsed int, PowerUsed int, \
     PRIMARY KEY (TimeStamp))",
    "CREATE TABLE Inverters (Serial int NOT NULL, Name varchar(32), Type varchar(32), \
     SW_Version varchar(32), TimeStamp datetime, TotalPac int, EToday int8, ETotal int8, \
     OperatingTime double, FeedInTime double, Status varchar(10), GridRelay varchar(10), \
     Temperature float, PRIMARY KEY (Serial))",
    "CREATE TABLE SpotData (TimeStamp datetime NOT NULL, Serial int NOT NULL, Pdc1 int, Pdc2 int, \
     Idc1 float, Idc2 float, Udc1 float, Udc2 float, Pac1 int, Pac2 int, Pac3 int, Iac1 float, \
     Iac2 float, Iac3 float, Uac1 float, Uac2 float, Uac3 float, EToday int8, ETotal int8, \
     Frequency float, OperatingTime double, FeedInTime double, BT_Signal float, \
     Status varchar(10), GridRelay varchar(10), Temperature float, \
     PRIMARY KEY (TimeStamp, Serial))",
    "CREATE TABLE EventData (EntryID int, TimeStamp datetime NOT NULL, Serial int NOT NULL, \
     SusyID int, EventCode int, EventType varchar(32), Category varchar(32), \
     EventGroup varchar(32), Tag varchar(200), OldValue varchar(32), NewValue varchar(32), \
     UserGroup varchar(10), PRIMARY KEY (Serial, EntryID))",
];

/// Two inverters over two archive times: a three-phase one with two DC
/// inputs on the roof and a single-phase one with one input on the garage.
const TWO_INVERTERS: &[&str] = &[
    "INSERT INTO DayData VALUES (1000, 2000123456, 10000000, 2300, 0), \
     (1000, 3000654321, 2000000, 580, 0), (1300, 2000123456, 10000200, 2400, 0), \
     (1300, 3000654321, 2000050, 600, 0)",
    "INSERT INTO Consumption VALUES (1000, 500, 300), (1300, 550, 320)",
    "INSERT INTO Inverters (Serial, Name, Type, SW_Version) VALUES \
     (2000123456, 'SN: 2000123456', 'STP 6000TL-20', '3.10.18.R'), \
     (3000654321, 'SN: 3000654321', 'SB 1.5-1VL-40', '2.4.6.R')",
    // The roof inverter was read 20 seconds before the archive time and again
    // far outside its window; the garage inverter 10 seconds after it.
    "INSERT INTO SpotData VALUES \
     (1280, 2000123456, 1300, 1100, 3.4, 2.918, 384.12, 377.0, 800, 790, 810, 3.478, 3.434, \
      3.521, 230.1, 229.9, 230.4, 4200, 10000195, 49.98, 24000.5, 22222.25, 0, 'OK', 'Closed', 41.5), \
     (1600, 2000123456, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 'Fault', 'Open', 1), \
     (1310, 3000654321, 600, 0, 1.7, 0, 376.5, 0, 600, 0, 0, 2.609, 0, 0, 230.0, 0, 0, 1500, \
      2000050, 49.981, 11111.0, 10000.0, 73.0, 'Warning', 'Closed', 35.0)",
    "INSERT INTO EventData VALUES \
     (8812, 1100, 2000123456, 9360, 10100, 'Event', 'Info', 'Parameter', 'Grid voltage setpoint', \
      '230.0 V', '235.0 V', 'Installer'), \
     (17, 1200, 3000654321, 9303, 104, NULL, NULL, NULL, '', NULL, NULL, NULL)",
];

async fn database(path: &Path, statements: &[&[&'static str]]) -> SqlitePool {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("create SQLite fixture");
    for statement in statements.iter().copied().flatten() {
        sqlx::query(*statement)
            .execute(&pool)
            .await
            .expect("fixture statement");
    }
    pool
}

fn values(reading: &Reading, serial: i64) -> BTreeMap<&str, i64> {
    let prefix = format!("inv.{serial}.");
    reading
        .channels
        .iter()
        .filter_map(|(key, value)| Some((key.strip_prefix(&prefix)?, value.value)))
        .collect()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn two_inverters_are_delivered_with_their_own_values_and_the_totals_of_the_system() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("SBFspot.db");
    database(&path, &[SCHEMA, TWO_INVERTERS])
        .await
        .close()
        .await;

    let source = SbfspotSource::open(&path).await.expect("open source");
    assert_eq!(source.unavailable_groups(), Vec::new());
    let readings = source.read_after(1000, 100).await.expect("read source");
    assert_eq!(readings.len(), 1);
    let reading = &readings[0];

    // The system's totals stay in the first-class fields.
    assert_eq!(reading.timestamp, 1300);
    assert_eq!(reading.observed_at_epoch_millis, 1_300_000);
    assert_eq!(reading.generation_power_watts, Some(3_000));
    assert_eq!(reading.generation_energy_wh, Some(250));
    assert_eq!(reading.consumption_power_watts, Some(320));
    assert_eq!(reading.consumption_energy_wh, Some(50));
    assert_eq!(reading.voltage_millivolts, Some(230_050));
    assert_eq!(reading.temperature_millidegrees_celsius, Some(38_250));
    assert_eq!(reading.source_reference, "sbfspot:daydata:1300");

    // Each inverter's values, in the integer units of the key scheme.
    assert_eq!(
        values(reading, ROOF),
        BTreeMap::from([
            ("ac.l1.power", 800),
            ("ac.l1.current", 3_478),
            ("ac.l1.voltage", 230_100),
            ("ac.l2.power", 790),
            ("ac.l2.current", 3_434),
            ("ac.l2.voltage", 229_900),
            ("ac.l3.power", 810),
            ("ac.l3.current", 3_521),
            ("ac.l3.voltage", 230_400),
            ("mppt.1.power", 1_300),
            ("mppt.1.current", 3_400),
            ("mppt.1.voltage", 384_120),
            ("mppt.2.power", 1_100),
            ("mppt.2.current", 2_918),
            ("mppt.2.voltage", 377_000),
            ("energy.today", 4_200),
            ("energy.lifetime", 10_000_200),
            ("grid.frequency", 49_980),
            ("operating_time", 86_401_800),
            ("feed_in_time", 80_000_100),
            ("signal", 0),
            ("status", 307),
            ("grid_relay", 51),
            ("temperature", 41_500),
        ])
    );
    // A phase and an input the inverter does not have are absent, not zero.
    assert_eq!(
        values(reading, GARAGE),
        BTreeMap::from([
            ("ac.l1.power", 600),
            ("ac.l1.current", 2_609),
            ("ac.l1.voltage", 230_000),
            ("mppt.1.power", 600),
            ("mppt.1.current", 1_700),
            ("mppt.1.voltage", 376_500),
            ("energy.today", 1_500),
            ("energy.lifetime", 2_000_050),
            ("grid.frequency", 49_981),
            ("operating_time", 39_999_600),
            ("feed_in_time", 36_000_000),
            ("signal", 730),
            ("status", 455),
            ("grid_relay", 51),
            ("temperature", 35_000),
        ])
    );

    // What the inverters and their inputs report adds up to the system's totals.
    let total = |suffix: &str| -> i64 {
        reading
            .channels
            .iter()
            .filter(|(key, _)| key.ends_with(suffix))
            .map(|(_, value)| value.value)
            .sum()
    };
    assert_eq!(
        Some(total(".power") - total("mppt.1.power") - total("mppt.2.power")),
        reading.generation_power_watts
    );
    assert_eq!(
        Some(total("mppt.1.power") + total("mppt.2.power")),
        reading.generation_power_watts
    );
    assert_eq!(total("energy.lifetime"), 10_000_200 + 2_000_050);

    // Every key is one the SBFspot profile defines, so PVLog types it and a
    // later move to smalog keeps the channels.
    let profile = source_profile("sbfspot").expect("sbfspot profile");
    for key in reading.channels.keys() {
        let definition = device_channel(key)
            .expect("a key of the device key scheme")
            .definition;
        assert!(profile.channel_keys.contains(definition), "{key}");
    }

    // Who the inverters are travels with the first reading of a page.
    assert_eq!(
        reading.devices,
        vec![
            DeviceIdentity {
                id: ROOF.to_string(),
                reported_name: Some("SN: 2000123456".to_owned()),
                model: Some("STP 6000TL-20".to_owned()),
                firmware: Some("3.10.18.R".to_owned()),
            },
            DeviceIdentity {
                id: GARAGE.to_string(),
                reported_name: Some("SN: 3000654321".to_owned()),
                model: Some("SB 1.5-1VL-40".to_owned()),
                firmware: Some("2.4.6.R".to_owned()),
            },
        ]
    );
    let page = source.read_after(0, 100).await.expect("read source");
    assert_eq!(page.len(), 2);
    assert_eq!(
        page[0].generation_energy_wh, None,
        "nothing was stored before it"
    );
    assert!(page[1].devices.is_empty());
    assert_eq!(
        values(&page[0], ROOF),
        BTreeMap::from([("energy.lifetime", 10_000_000)]),
        "no spot value lies within reach of the first archive time"
    );
}

#[tokio::test]
async fn the_values_that_exist_are_delivered_when_optional_tables_are_absent() {
    let directory = tempdir().expect("temp directory");
    let archive_only = directory.path().join("archive.db");
    database(
        &archive_only,
        &[
            &SCHEMA[..1],
            &["INSERT INTO DayData VALUES (1000, 1, 1000, 400, 0), (1300, 1, 1100, 500, 0)"],
        ],
    )
    .await
    .close()
    .await;
    let source = SbfspotSource::open(&archive_only)
        .await
        .expect("open source");
    assert_eq!(
        source.unavailable_groups(),
        vec![
            SourceGroup::SpotValues,
            SourceGroup::Consumption,
            SourceGroup::InverterIdentity,
            SourceGroup::Events,
        ]
    );
    let readings = source.read_after(1000, 100).await.expect("read source");
    assert_eq!(readings[0].generation_power_watts, Some(500));
    assert_eq!(readings[0].generation_energy_wh, Some(100));
    assert_eq!(readings[0].consumption_power_watts, None);
    assert_eq!(readings[0].voltage_millivolts, None);
    assert_eq!(
        values(&readings[0], 1),
        BTreeMap::from([("energy.lifetime", 1_100)])
    );
    assert!(readings[0].devices.is_empty());
    assert!(
        source
            .read_events_after(EventPosition::default(), 10)
            .await
            .expect("read events")
            .is_empty()
    );
    // The summary of a pass names the groups that could not be delivered.
    let summary = push_pending(
        &source,
        &client("http://127.0.0.1:9"),
        &PushConfig {
            batch_size: 100,
            checkpoint_path: directory.path().join("state.json"),
            initial_timestamp: 0,
            dry_run: true,
        },
    )
    .await
    .expect("dry run");
    assert_eq!(
        serde_json::to_value(&summary).expect("summary")["unavailable"],
        json!(["spot_values", "consumption", "inverter_identity", "events"])
    );

    // An older database whose SpotData lacks most columns delivers those it has.
    let narrow = directory.path().join("narrow.db");
    database(
        &narrow,
        &[&[
            SCHEMA[0],
            "CREATE TABLE SpotData (TimeStamp datetime NOT NULL, Serial int NOT NULL, Uac1 float, \
             Temperature float, PRIMARY KEY (TimeStamp, Serial))",
            "INSERT INTO DayData VALUES (1300, 1, 1100, 500, 0)",
            "INSERT INTO SpotData VALUES (1300, 1, 231.0, 36.0)",
        ]],
    )
    .await
    .close()
    .await;
    let source = SbfspotSource::open(&narrow).await.expect("open source");
    let readings = source.read_after(0, 100).await.expect("read source");
    assert_eq!(
        values(&readings[0], 1),
        BTreeMap::from([
            ("ac.l1.voltage", 231_000),
            ("energy.lifetime", 1_100),
            ("temperature", 36_000),
        ])
    );
    assert_eq!(readings[0].voltage_millivolts, Some(231_000));
    assert_eq!(readings[0].temperature_millidegrees_celsius, Some(36_000));
}

#[tokio::test]
async fn no_energy_is_derived_across_a_replaced_inverter() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("SBFspot.db");
    // Inverter 2 fails after the second archive time and inverter 3 takes its place.
    database(
        &path,
        &[
            &SCHEMA[..1],
            &["INSERT INTO DayData VALUES \
               (1000, 1, 10000, 400, 0), (1000, 2, 5000, 300, 0), \
               (1300, 1, 10040, 400, 0), (1300, 2, 5030, 300, 0), \
               (1600, 1, 10080, 400, 0), \
               (1900, 1, 10120, 400, 0), (1900, 3, 7, 310, 0), \
               (2200, 1, 10160, 400, 0), (2200, 3, 38, 310, 0), \
               (2500, 1, 10200, 400, 0), (2500, 3, 5, 310, 0)"],
        ],
    )
    .await
    .close()
    .await;
    let source = SbfspotSource::open(&path).await.expect("open source");
    let readings = source.read_after(0, 100).await.expect("read source");
    assert_eq!(
        readings
            .iter()
            .map(|reading| (reading.timestamp, reading.generation_energy_wh))
            .collect::<Vec<_>>(),
        vec![
            (1000, None),
            (1300, Some(40 + 30)),
            // One serial is silent, then another one appears: the difference
            // of the sums would be energy nobody generated.
            (1600, None),
            (1900, None),
            (2200, Some(40 + 31)),
            // A counter that went back is a reset.
            (2500, None),
        ]
    );
    assert_eq!(readings[3].generation_power_watts, Some(710));
    // The new serial is delivered under its own keys, from which PVLog
    // proposes it as an inverter.
    assert_eq!(
        values(&readings[3], 3),
        BTreeMap::from([("energy.lifetime", 7)])
    );
    assert!(values(&readings[3], 2).is_empty());

    // The same holds when the change falls between two pages.
    let page = source.read_after(1600, 2).await.expect("read source");
    assert_eq!(
        page.iter()
            .map(|reading| reading.generation_energy_wh)
            .collect::<Vec<_>>(),
        vec![None, Some(40 + 31)]
    );
}

/// What a stand-in for `PVLog` received: the path, the key and the body of each request.
type Requests = Arc<Mutex<Vec<(String, String, Value)>>>;

async fn capture(
    State(requests): State<Requests>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let key = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let outcomes: Vec<Value> = body["items"]
        .as_array()
        .map(|items| {
            (0..items.len())
                .map(|index| json!({ "index": index, "status": "inserted", "code": null }))
                .collect()
        })
        .unwrap_or_default();
    requests
        .lock()
        .expect("requests lock")
        .push((uri.path().to_owned(), key, body));
    Json(json!({ "outcomes": outcomes, "stored": 0, "unchanged": 0 }))
}

/// Starts a stand-in for `PVLog` and returns its address.
async fn server(requests: &Requests) -> String {
    let router = Router::new()
        .route("/api/v1/systems/system-1/observations/batch", post(capture))
        .route("/api/v1/systems/system-1/device-events", post(capture))
        .with_state(Arc::clone(requests));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let address = listener.local_addr().expect("listener address");
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("http://{address}")
}

fn client(base_url: &str) -> PvlogClient {
    PvlogClient::new(PvlogClientConfig {
        base_url: Url::parse(base_url).expect("base URL"),
        system_id: "system-1".to_owned(),
        api_key: "secret-key".to_owned(),
        request_timeout: Duration::from_secs(2),
        maximum_attempts: 1,
    })
    .expect("client")
}

#[tokio::test]
async fn a_pass_uploads_readings_and_events_and_a_second_pass_uploads_nothing_again() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("SBFspot.db");
    let pool = database(&path, &[SCHEMA, TWO_INVERTERS]).await;
    let source = SbfspotSource::open(&path).await.expect("open source");
    let requests = Requests::default();
    let client = client(&server(&requests).await);
    let config = PushConfig {
        batch_size: 500,
        checkpoint_path: directory.path().join("state.json"),
        initial_timestamp: 0,
        dry_run: false,
    };

    let summary = push_pending(&source, &client, &config)
        .await
        .expect("first pass");
    assert_eq!(
        summary,
        PushSummary {
            batches: 1,
            observations: 2,
            last_timestamp: Some(1300),
            events: 2,
            unavailable: Vec::new(),
        }
    );
    let sent = std::mem::take(&mut *requests.lock().expect("requests lock"));
    assert_eq!(sent.len(), 2);
    let (path_of_batch, key, batch) = &sent[0];
    assert_eq!(path_of_batch, "/api/v1/systems/system-1/observations/batch");
    assert_eq!(key, "Bearer secret-key");
    assert_eq!(batch["mode"], "atomic");
    let item = &batch["items"][1];
    assert_eq!(item["idempotencyKey"], "sbfspot-daydata-1300");
    assert_eq!(item["observedAtEpochMillis"], 1_300_000);
    assert_eq!(item["generationPowerWatts"], 3_000);
    assert_eq!(
        item["channels"]["inv.2000123456.mppt.2.voltage"],
        json!({ "value": 377_000 })
    );
    assert_eq!(
        item["channels"]["inv.3000654321.status"],
        json!({ "value": 455 })
    );
    assert!(item.get("devices").is_none());
    assert_eq!(batch["items"][0]["devices"][1]["firmware"], "2.4.6.R");

    let (path_of_events, _, events) = &sent[1];
    assert_eq!(path_of_events, "/api/v1/systems/system-1/device-events");
    assert_eq!(
        events["events"],
        json!([
            { "deviceId": "2000123456", "eventId": "8812", "occurredAtEpochMillis": 1_100_000,
              "code": 10_100, "type": "Event", "category": "Info", "group": "Parameter",
              "message": "Grid voltage setpoint", "oldValue": "230.0 V", "newValue": "235.0 V",
              "userGroup": "Installer" },
            { "deviceId": "3000654321", "eventId": "17", "occurredAtEpochMillis": 1_200_000,
              "code": 104 },
        ])
    );

    // Nothing is new: a second pass sends neither a reading nor an event.
    let summary = push_pending(&source, &client, &config)
        .await
        .expect("second pass");
    assert_eq!((summary.observations, summary.events), (0, 0));
    assert!(requests.lock().expect("requests lock").is_empty());

    // A new event, even one with an earlier entry number, is uploaded alone.
    sqlx::query(
        "INSERT INTO EventData (EntryID, TimeStamp, Serial, EventCode) VALUES (9, 1250, 2000123456, 3501)",
    )
    .execute(&pool)
    .await
    .expect("new event");
    let summary = push_pending(&source, &client, &config)
        .await
        .expect("third pass");
    assert_eq!((summary.observations, summary.events), (0, 1));
    let sent = requests.lock().expect("requests lock").clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].2["events"][0]["eventId"], "9");
    assert_eq!(
        CheckpointStore::new(config.checkpoint_path.clone())
            .load()
            .await
            .expect("load checkpoint"),
        Some(Checkpoint {
            last_timestamp: 1300,
            last_event: Some(EventPosition {
                timestamp: 1250,
                serial: ROOF,
                entry_id: 9,
            }),
        })
    );
    pool.close().await;
}

#[tokio::test]
async fn a_reading_is_serialized_as_the_batch_contract_expects() {
    let requests = Requests::default();
    let client = client(&server(&requests).await);
    client.send(&[sample_reading()]).await.expect("send batch");
    let sent = requests.lock().expect("requests lock").clone();
    assert_eq!(
        sent[0].2,
        json!({
            "mode": "atomic",
            "items": [{
                "idempotencyKey": "sbfspot-daydata-1300",
                "observedAtEpochMillis": 1_300_000,
                "generationPowerWatts": 1_100,
                "generationEnergyWh": 200,
                "sourceReference": "sbfspot:daydata:1300",
                "channels": { "inv.1.ac.l1.power": { "value": 1_100 } },
            }],
        })
    );
}

#[tokio::test]
async fn checkpoint_round_trips_atomically() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("nested/checkpoint.json");
    let store = CheckpointStore::new(path.clone());
    assert_eq!(store.load().await.expect("missing checkpoint"), None);
    let checkpoint = Checkpoint {
        last_timestamp: 1300,
        last_event: Some(EventPosition {
            timestamp: 1200,
            serial: GARAGE,
            entry_id: 17,
        }),
    };
    store.save(checkpoint).await.expect("save checkpoint");
    assert_eq!(
        store.load().await.expect("load checkpoint"),
        Some(checkpoint)
    );

    // A checkpoint written before events were uploaded is still read.
    tokio::fs::write(&path, br#"{"lastTimestamp":1300}"#)
        .await
        .expect("write old checkpoint");
    assert_eq!(
        store.load().await.expect("load old checkpoint"),
        Some(Checkpoint {
            last_timestamp: 1300,
            last_event: None,
        })
    );
}

#[tokio::test]
async fn the_generated_configuration_runs_the_uploader_in_dry_run_mode() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("SBFspot.db");
    database(&path, &[SCHEMA, TWO_INVERTERS])
        .await
        .close()
        .await;

    // The environment PVLog shows for a new SBFspot data source, with only
    // the path of the local database adjusted.
    let profile = source_profile("sbfspot").expect("sbfspot profile");
    let environment = render_configuration(
        profile,
        "https://pvlog.example",
        "01900000-0000-7000-8000-000000000000",
        "pvlog_generated_key",
    )
    .remove(0)
    .content;
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_pvlog-sbfspot-push"));
    command.env_clear();
    for line in environment.lines() {
        let (name, value) = line.split_once('=').expect("an assignment per line");
        command.env(name, value);
    }
    let output = command
        .env("SBFSPOT_DATABASE", &path)
        .env("SBFSPOT_PUSH_STATE", directory.path().join("state.json"))
        .args(["once", "--dry-run"])
        .output()
        .expect("run the uploader");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).expect("a summary on stdout");
    assert_eq!(
        summary,
        json!({
            "batches": 1, "observations": 2, "lastTimestamp": 1300, "events": 2,
            "unavailable": [],
        })
    );
    assert!(
        !directory.path().join("state.json").exists(),
        "a dry run sends nothing and saves no position"
    );
}

fn sample_reading() -> Reading {
    Reading {
        timestamp: 1300,
        observed_at_epoch_millis: 1_300_000,
        generation_power_watts: Some(1_100),
        generation_energy_wh: Some(200),
        consumption_power_watts: None,
        consumption_energy_wh: None,
        voltage_millivolts: None,
        temperature_millidegrees_celsius: None,
        source_reference: "sbfspot:daydata:1300".to_owned(),
        channels: BTreeMap::from([(
            "inv.1.ac.l1.power".to_owned(),
            ChannelValue { value: 1_100 },
        )]),
        devices: Vec::new(),
    }
}

#[tokio::test]
async fn text_that_older_sbfspot_versions_stored_in_latin_1_is_read() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("SBFspot.db");
    // `Ger\xE4t` is "Gerät" in Latin-1; newer versions store UTF-8.
    database(
        &path,
        &[
            SCHEMA,
            &[
                "INSERT INTO DayData VALUES (1000, 2000123456, 10000000, 2300, 0)",
                "INSERT INTO EventData VALUES \
                 (1, 900, 2000123456, 9360, 10100, 'Event', 'Info', \
                  CAST(X'476572E474' AS TEXT), 'Grid', NULL, NULL, NULL), \
                 (2, 950, 2000123456, 9360, 10100, 'Event', 'Info', 'Gerät', 'Grid', \
                  NULL, NULL, NULL)",
                "INSERT INTO SpotData (TimeStamp, Serial, Pac1, Status, GridRelay) VALUES \
                 (1000, 2000123456, 2300, CAST(X'4F4B' AS TEXT), 'Closed')",
            ],
        ],
    )
    .await
    .close()
    .await;

    let source = SbfspotSource::open(&path).await.expect("open source");
    let events = source
        .read_events_after(
            EventPosition {
                timestamp: 0,
                serial: 0,
                entry_id: 0,
            },
            10,
        )
        .await
        .expect("read events");
    let groups: Vec<_> = events.iter().map(|event| event.group.as_deref()).collect();
    assert_eq!(groups, vec![Some("Gerät"), Some("Gerät")]);
    let readings = source.read_after(0, 10).await.expect("read readings");
    assert_eq!(values(&readings[0], ROOF).get("status"), Some(&307));
}
