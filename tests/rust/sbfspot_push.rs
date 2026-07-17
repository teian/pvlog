#![allow(clippy::expect_used)]

use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use pvlog_sbfspot_push::{
    Checkpoint, CheckpointStore, PvlogClient, PvlogClientConfig, Reading, SbfspotSource,
};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::tempdir;
use url::Url;

#[tokio::test]
async fn aggregates_inverters_and_converts_cumulative_energy() {
    let directory = tempdir().expect("temp directory");
    let database = directory.path().join("SBFspot.db");
    create_sbfspot_fixture(&database).await;

    let source = SbfspotSource::open(&database).await.expect("open source");
    let readings = source.read_after(1000, 100).await.expect("read source");

    assert_eq!(readings.len(), 1);
    let reading = &readings[0];
    assert_eq!(reading.timestamp, 1300);
    assert_eq!(reading.observed_at_epoch_millis, 1_300_000);
    assert_eq!(reading.generation_power_watts, Some(1_100));
    assert_eq!(reading.generation_energy_wh, Some(200));
    assert_eq!(reading.consumption_power_watts, Some(320));
    assert_eq!(reading.consumption_energy_wh, Some(50));
    assert_eq!(reading.voltage_millivolts, Some(231_000));
    assert_eq!(reading.temperature_millidegrees_celsius, Some(36_000));
    assert_eq!(reading.source_reference, "sbfspot:daydata:1300");
}

#[tokio::test]
async fn sends_the_canonical_batch_contract_with_the_ingestion_header() {
    let captured = Arc::new(Mutex::new(None));
    let router = Router::new()
        .route(
            "/api/v1/systems/system-1/observations/batch",
            post(capture_batch),
        )
        .with_state(Arc::clone(&captured));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let address = listener.local_addr().expect("listener address");
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let client = PvlogClient::new(PvlogClientConfig {
        base_url: Url::parse(&format!("http://{address}")).expect("base URL"),
        system_id: "system-1".to_owned(),
        api_key: "secret-key".to_owned(),
        request_timeout: Duration::from_secs(2),
        maximum_attempts: 1,
    })
    .expect("client");

    client.send(&[sample_reading()]).await.expect("send batch");
    let request = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("captured request");
    assert_eq!(request["header"], "Bearer secret-key");
    assert_eq!(request["body"]["mode"], "atomic");
    assert_eq!(
        request["body"]["items"][0]["idempotencyKey"],
        "sbfspot-daydata-1300"
    );
    assert_eq!(
        request["body"]["items"][0]["observedAtEpochMillis"],
        1_300_000
    );
    server.abort();
}

#[tokio::test]
async fn checkpoint_round_trips_atomically() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("nested/checkpoint.json");
    let store = CheckpointStore::new(path);
    assert_eq!(store.load().await.expect("missing checkpoint"), None);
    store
        .save(Checkpoint {
            last_timestamp: 1300,
        })
        .await
        .expect("save checkpoint");
    assert_eq!(
        store.load().await.expect("load checkpoint"),
        Some(Checkpoint {
            last_timestamp: 1300
        })
    );
}

async fn capture_batch(
    State(captured): State<Arc<Mutex<Option<Value>>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    *captured.lock().expect("capture lock") = Some(json!({ "header": header, "body": body }));
    Json(json!({ "outcomes": [{ "index": 0, "status": "inserted", "code": null }] }))
}

fn sample_reading() -> Reading {
    Reading {
        timestamp: 1300,
        observed_at_epoch_millis: 1_300_000,
        generation_power_watts: Some(1_100),
        generation_energy_wh: Some(200),
        consumption_power_watts: Some(320),
        consumption_energy_wh: Some(50),
        voltage_millivolts: Some(231_000),
        temperature_millidegrees_celsius: Some(36_000),
        source_reference: "sbfspot:daydata:1300".to_owned(),
    }
}

async fn create_sbfspot_fixture(path: &Path) {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("create SQLite fixture");
    for statement in [
        "CREATE TABLE DayData (TimeStamp INTEGER NOT NULL, Serial INTEGER NOT NULL, TotalYield INTEGER, Power INTEGER, PVoutput INTEGER, PRIMARY KEY (TimeStamp, Serial))",
        "CREATE TABLE Consumption (TimeStamp INTEGER PRIMARY KEY, EnergyUsed INTEGER, PowerUsed INTEGER)",
        "CREATE TABLE SpotData (TimeStamp INTEGER NOT NULL, Serial INTEGER NOT NULL, Uac1 REAL, Temperature REAL, PRIMARY KEY (TimeStamp, Serial))",
        "INSERT INTO DayData VALUES (1000, 1, 1000, 400, 0), (1000, 2, 2000, 500, 0), (1300, 1, 1100, 500, 0), (1300, 2, 2100, 600, 0)",
        "INSERT INTO Consumption VALUES (1000, 500, 300), (1300, 550, 320)",
        "INSERT INTO SpotData VALUES (1300, 1, 230.0, 35.0), (1300, 2, 232.0, 37.0)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("fixture statement");
    }
    pool.close().await;
}
