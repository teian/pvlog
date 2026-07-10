//! Cross-engine telemetry storage schema contracts.

use std::{collections::BTreeSet, error::Error, path::Path, str::FromStr as _};

use pvlog_domain::AccountId;
use pvlog_storage::{DatabaseTarget, SqliteAccountProvisioner, apply_migrations};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_telemetry_schema_contains_every_storage_tier() -> Result<(), Box<dyn Error>> {
    let setup = SqliteSetup::new().await?;
    let mut account = setup.account_connection().await?;
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(&mut account)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for required in [
        "telemetry_hot",
        "telemetry_hot_extended_values",
        "archived_segments",
        "correction_overlays",
        "idempotency_records",
        "telemetry_rollups",
        "system_daily_summaries",
        "system_lifetime_summaries",
        "aggregation_invalidations",
        "data_quality_events",
    ] {
        assert!(
            tables.contains(required),
            "missing telemetry table {required}"
        );
    }
    account.close().await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_hot_observations_enforce_source_and_idempotency_uniqueness()
-> Result<(), Box<dyn Error>> {
    let setup = SqliteSetup::new().await?;
    let mut account = setup.account_connection().await?;
    let (system_id, channel_id) = insert_system_and_channel(&mut account).await?;
    let observation_id = insert_hot_observation(&mut account, &system_id, "request-1").await?;
    sqlx::query(
        "INSERT INTO telemetry_hot_extended_values \
         (observation_id, channel_id, integer_value) VALUES (?, ?, 750)",
    )
    .bind(&observation_id)
    .bind(&channel_id)
    .execute(&mut account)
    .await?;

    assert!(
        insert_hot_observation(&mut account, &system_id, "request-2")
            .await
            .is_err()
    );
    sqlx::query(
        "INSERT INTO idempotency_records \
         (id, principal_type, principal_id, operation, idempotency_key, request_hash, \
          response_status, response_json, created_at, expires_at) \
         VALUES (?, 'api_credential', ?, 'telemetry.create', 'key-1', ?, 201, '{}', 1, 2)",
    )
    .bind(id())
    .bind(id())
    .bind(vec![3_u8; 32])
    .execute(&mut account)
    .await?;
    account.close().await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_segments_and_corrections_enforce_integrity_and_overlay_shape()
-> Result<(), Box<dyn Error>> {
    let setup = SqliteSetup::new().await?;
    let mut account = setup.account_connection().await?;
    let (system_id, _) = insert_system_and_channel(&mut account).await?;
    let segment_id = id();
    sqlx::query(
        "INSERT INTO archived_segments \
         (id, system_id, local_date, generation, schema_version, encoding, compression, \
          range_start, range_end, point_count, field_presence, payload, compressed_length, \
          uncompressed_length, content_hash, state, created_at, verified_at) \
         VALUES (?, ?, '2026-07-10', 1, 1, 'protobuf_columnar', 'zstd', 10, 20, 1, \
                 X'01', X'01020304', 4, 3, ?, 'verified', 1, 1)",
    )
    .bind(&segment_id)
    .bind(&system_id)
    .bind(vec![5_u8; 32])
    .execute(&mut account)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO correction_overlays \
             (id, system_id, observation_id, measured_at, operation, expected_version, reason, created_at) \
             VALUES (?, ?, ?, 15, 'replace', 1, 'operator correction', 1)",
        )
        .bind(id())
        .bind(&system_id)
        .bind(id())
        .execute(&mut account)
        .await
        .is_err()
    );
    sqlx::query(
        "INSERT INTO correction_overlays \
         (id, system_id, observation_id, measured_at, segment_id, operation, expected_version, \
          replacement_json, reason, created_at) \
         VALUES (?, ?, ?, 15, ?, 'replace', 1, '{\"generationPowerWatts\":900}', \
                 'operator correction', 1)",
    )
    .bind(id())
    .bind(&system_id)
    .bind(id())
    .bind(&segment_id)
    .execute(&mut account)
    .await?;
    account.close().await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_rollups_summaries_invalidations_and_quality_are_bounded()
-> Result<(), Box<dyn Error>> {
    let setup = SqliteSetup::new().await?;
    let mut account = setup.account_connection().await?;
    let (system_id, _) = insert_system_and_channel(&mut account).await?;
    sqlx::query(
        "INSERT INTO telemetry_rollups \
         (system_id, resolution, bucket_start, bucket_end, timezone, point_count, \
          expected_count, coverage_basis_points, calculated_at) \
         VALUES (?, 'hour', 0, 3600000, 'Europe/Berlin', 12, 12, 10000, 1)",
    )
    .bind(&system_id)
    .execute(&mut account)
    .await?;
    sqlx::query(
        "INSERT INTO system_daily_summaries \
         (system_id, local_date, timezone, coverage_basis_points, calculated_at) \
         VALUES (?, '2026-07-10', 'Europe/Berlin', 10000, 1)",
    )
    .bind(&system_id)
    .execute(&mut account)
    .await?;
    sqlx::query(
        "INSERT INTO aggregation_invalidations \
         (id, system_id, range_start, range_end, reason, required_generation, state, created_at) \
         VALUES (?, ?, 0, 3600000, 'late_data', 2, 'pending', 1)",
    )
    .bind(id())
    .bind(&system_id)
    .execute(&mut account)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO data_quality_events \
             (id, system_id, quality_kind, severity, range_start, range_end, state, detected_at) \
             VALUES (?, ?, 'missing_interval', 'warning', 20, 10, 'open', 1)",
        )
        .bind(id())
        .bind(&system_id)
        .execute(&mut account)
        .await
        .is_err()
    );
    account.close().await?;
    Ok(())
}

#[tokio::test]
async fn postgres_telemetry_tables_use_account_owned_primary_keys_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    let target = DatabaseTarget::Postgres { url: url.clone() };
    apply_migrations(&target).await?;
    let mut connection = PgConnection::connect(&url).await?;
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'telemetry' ORDER BY table_name",
    )
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for required in [
        "hot_observations",
        "archived_segments",
        "correction_overlays",
        "idempotency_records",
        "rollups",
        "daily_summaries",
        "lifetime_summaries",
        "aggregation_invalidations",
        "data_quality_events",
    ] {
        assert!(
            tables.contains(required),
            "missing PostgreSQL telemetry table {required}"
        );
    }
    let primary_key_columns: Vec<String> = sqlx::query_scalar(
        "SELECT attribute.attname \
         FROM pg_index AS index \
         JOIN pg_class AS relation ON relation.oid = index.indrelid \
         JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace \
         JOIN pg_attribute AS attribute ON attribute.attrelid = relation.oid \
              AND attribute.attnum = ANY(index.indkey) \
         WHERE namespace.nspname = 'telemetry' AND relation.relname = 'hot_observations' \
               AND index.indisprimary ORDER BY attribute.attnum",
    )
    .fetch_all(&mut connection)
    .await?;
    assert!(
        primary_key_columns
            .iter()
            .any(|column| column == "account_id")
    );
    connection.close().await?;
    Ok(())
}

struct SqliteSetup {
    _directory: TempDir,
    account_path: std::path::PathBuf,
}

impl SqliteSetup {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        let target = DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        };
        apply_migrations(&target).await?;
        let account_id = AccountId::new();
        let mut management = sqlite_connection(&management_path).await?;
        sqlx::query(
            "INSERT INTO accounts \
             (id, slug, display_name, status, created_at, updated_at) \
             VALUES (?, 'telemetry-test', 'Telemetry Test', 'provisioning', 1, 1)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .execute(&mut management)
        .await?;
        management.close().await?;
        let provisioner = SqliteAccountProvisioner::new(management_path, accounts_dir.clone());
        let result = provisioner.provision(account_id).await?;
        Ok(Self {
            _directory: directory,
            account_path: accounts_dir.join(result.opaque_locator),
        })
    }

    async fn account_connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        sqlite_connection(&self.account_path).await
    }
}

async fn insert_system_and_channel(
    connection: &mut SqliteConnection,
) -> Result<(Vec<u8>, Vec<u8>), sqlx::Error> {
    let system_id = id();
    sqlx::query(
        "INSERT INTO systems \
         (id, name, timezone, status_interval_seconds, power_calculation_mode, \
          net_calculation_mode, created_at, updated_at) \
         VALUES (?, 'Home PV', 'Europe/Berlin', 300, 'reported', 'separate_flows', 1, 1)",
    )
    .bind(&system_id)
    .execute(&mut *connection)
    .await?;
    let channel_id = id();
    sqlx::query(
        "INSERT INTO channel_definitions \
         (id, system_id, channel_key, display_name, data_type, unit, scale, effective_from, \
          created_at, updated_at) \
         VALUES (?, ?, 'irradiance', 'Irradiance', 'integer', 'W/m2', 0, 0, 1, 1)",
    )
    .bind(&channel_id)
    .bind(&system_id)
    .execute(connection)
    .await?;
    Ok((system_id, channel_id))
}

async fn insert_hot_observation(
    connection: &mut SqliteConnection,
    system_id: &[u8],
    idempotency_identity: &str,
) -> Result<Vec<u8>, sqlx::Error> {
    let observation_id = id();
    sqlx::query(
        "INSERT INTO telemetry_hot \
         (observation_id, system_id, measured_at, received_at, source_kind, source_identity, \
          idempotency_identity, generation_power_watts, canonical_hash) \
         VALUES (?, ?, 1000, 1001, 'modern_api', 'uploader-1', ?, 800, ?)",
    )
    .bind(&observation_id)
    .bind(system_id)
    .bind(idempotency_identity)
    .bind(vec![1_u8; 32])
    .execute(connection)
    .await?;
    Ok(observation_id)
}

async fn sqlite_connection(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(path.to_string_lossy().as_ref())?
        .create_if_missing(true)
        .foreign_keys(true);
    SqliteConnection::connect_with(&options).await
}

fn id() -> Vec<u8> {
    Uuid::now_v7().as_bytes().to_vec()
}
