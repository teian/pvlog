//! Measurement channel storage contract, run against both databases.

use std::error::Error;

use pvlog_domain::{AccountId, ChannelId, ObservationId, SystemId};
use pvlog_storage::{
    AccountConfigurationRepository, ChannelRecord, ChannelRegistration, ChannelRepository,
    DatabaseTarget, PostgresAccountConfigurationRepository, PostgresChannelRepository,
    PostgresTelemetryRepository, RoutedSqliteAccount, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SqliteChannelRepository, SqliteTelemetryRepository, StoredMeasurements, StoredObservation,
    SystemConfigurationRecord, TelemetryRepository, apply_migrations,
};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

const START: i64 = 1_800_000_000_000;
const STEP: i64 = 300_000;

/// Direct access to the account's tables, for what no repository offers.
enum Raw {
    Sqlite(RoutedSqliteAccount),
    Postgres { url: String, account_id: AccountId },
}

/// One account with one system on one database.
struct Store {
    channels: Box<dyn ChannelRepository>,
    configuration: Box<dyn AccountConfigurationRepository>,
    telemetry: Box<dyn TelemetryRepository>,
    raw: Raw,
    system_id: SystemId,
    _directory: Option<TempDir>,
}

impl Store {
    async fn sqlite() -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        apply_migrations(&DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        })
        .await?;
        let account_id = AccountId::new();
        let mut management = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&management_path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await?;
        sqlx::query(
            "INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) \
             VALUES (?,?,'Channels','provisioning',1,1)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(format!("channels-{account_id}"))
        .execute(&mut management)
        .await?;
        management.close().await?;
        SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone())
            .provision(account_id)
            .await?;
        let router = SqliteAccountPoolRouter::new(
            management_path,
            accounts_dir,
            SqliteAccountPoolConfig::default(),
        )?;
        let account = router.route(account_id).await?;
        let store = Self {
            channels: Box::new(SqliteChannelRepository::new(account.clone())),
            configuration: Box::new(SqliteAccountConfigurationRepository::new(account.clone())),
            telemetry: Box::new(SqliteTelemetryRepository::new(account.clone())),
            raw: Raw::Sqlite(account),
            system_id: SystemId::new(),
            _directory: Some(directory),
        };
        store
            .configuration
            .save_system(&system(store.system_id))
            .await?;
        Ok(store)
    }

    async fn postgres(url: String) -> Result<Self, Box<dyn Error>> {
        apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
        let account_id = AccountId::new();
        let mut connection = PgConnection::connect(&url).await?;
        sqlx::query(
            "INSERT INTO management.accounts \
             (id,slug,display_name,status,created_at,updated_at) \
             VALUES ($1,$2,'Channels','active',1,1)",
        )
        .bind(account_id.as_uuid())
        .bind(format!("channels-{account_id}"))
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        let store = Self {
            channels: Box::new(PostgresChannelRepository::new(url.clone(), account_id)),
            configuration: Box::new(PostgresAccountConfigurationRepository::new(
                url.clone(),
                account_id,
            )),
            telemetry: Box::new(PostgresTelemetryRepository::new(url.clone(), account_id)),
            raw: Raw::Postgres { url, account_id },
            system_id: SystemId::new(),
            _directory: None,
        };
        store
            .configuration
            .save_system(&system(store.system_id))
            .await?;
        Ok(store)
    }

    /// Stores a reading at `step` intervals after the start and returns its identity.
    async fn reading(&self, step: i64) -> Result<(ObservationId, i64), Box<dyn Error>> {
        let id = ObservationId::new();
        let measured_at = START + step * STEP;
        self.telemetry
            .insert_observation(&StoredObservation {
                id,
                system_id: self.system_id,
                measured_at,
                received_at: measured_at,
                source_kind: "modern_api".to_owned(),
                source_identity: "channels-test".to_owned(),
                idempotency_identity: Some(format!("channels-test:{id}")),
                quality_flags: 0,
                generation_power_watts: None,
                generation_energy_wh: None,
                consumption_power_watts: None,
                consumption_energy_wh: None,
                provenance: serde_json::json!({}),
                canonical_hash: *blake3::hash(id.as_uuid().as_bytes()).as_bytes(),
                version: 1,
                data_source_id: None,
                measurements: StoredMeasurements::default(),
            })
            .await?;
        Ok((id, measured_at))
    }

    /// Stores one value per given channel in a new reading at `step`.
    async fn values(&self, step: i64, values: &[(ChannelId, i64)]) -> Result<(), Box<dyn Error>> {
        let (id, measured_at) = self.reading(step).await?;
        self.channels
            .write_channel_values(id, measured_at, values, measured_at)
            .await?;
        Ok(())
    }

    /// The channel stored under a key.
    async fn channel(&self, key: &str) -> Result<ChannelRecord, Box<dyn Error>> {
        self.channels
            .channel(self.system_id, key)
            .await?
            .ok_or_else(|| format!("channel {key} is missing").into())
    }

    /// The stored values of a channel over the first `steps` intervals.
    async fn stored(&self, id: ChannelId, steps: i64) -> Result<Vec<i64>, Box<dyn Error>> {
        Ok(self
            .channels
            .channel_values(id, START, START + steps * STEP)
            .await?
            .into_iter()
            .map(|value| value.value)
            .collect())
    }

    /// Inserts a channel the way the version before typed channels did.
    async fn legacy_channel(
        &self,
        key: &str,
        tracks_power: bool,
        tracks_energy: bool,
    ) -> Result<(), Box<dyn Error>> {
        let id = Uuid::now_v7();
        match &self.raw {
            Raw::Sqlite(account) => {
                let mut writer = account.acquire_writer().await?;
                sqlx::query(
                    "INSERT INTO channel_definitions \
                     (id,system_id,channel_key,display_name,data_type,unit,scale,effective_from,\
                      created_at,updated_at,target_kind,tracks_power,tracks_energy,discovered_at) \
                     VALUES (?,?,?,?,'integer','W',0,1,1,1,'unassigned',?,?,1)",
                )
                .bind(id.as_bytes().to_vec())
                .bind(self.system_id.as_uuid().as_bytes().to_vec())
                .bind(key)
                .bind(key)
                .bind(i64::from(tracks_power))
                .bind(i64::from(tracks_energy))
                .execute(writer.connection())
                .await?;
            }
            Raw::Postgres { url, account_id } => {
                let mut connection = PgConnection::connect(url).await?;
                sqlx::query(
                    "INSERT INTO account_data.channel_definitions \
                     (account_id,id,system_id,channel_key,display_name,data_type,unit,scale,\
                      effective_from,created_at,updated_at,target_kind,tracks_power,\
                      tracks_energy,discovered_at) \
                     VALUES ($1,$2,$3,$4,$4,'integer','W',0,1,1,1,'unassigned',$5,$6,1)",
                )
                .bind(account_id.as_uuid())
                .bind(id)
                .bind(self.system_id.as_uuid())
                .bind(key)
                .bind(tracks_power)
                .bind(tracks_energy)
                .execute(&mut connection)
                .await?;
                connection.close().await?;
            }
        }
        Ok(())
    }

    /// Applies the data statements of the typed-channel migration to this account.
    async fn migrate_channel_definitions(&self) -> Result<(), Box<dyn Error>> {
        match &self.raw {
            Raw::Sqlite(account) => {
                let mut writer = account.acquire_writer().await?;
                for statement in data_statements(include_str!(
                    "../../../src/crates/storage/migrations/sqlite-account/0016_typed_channels.sql"
                )) {
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .execute(writer.connection())
                        .await?;
                }
            }
            Raw::Postgres { url, account_id } => {
                let mut connection = PgConnection::connect(url).await?;
                for statement in data_statements(include_str!(
                    "../../../src/crates/storage/migrations/postgres/0019_typed_channels.sql"
                )) {
                    // Other tests share this database; only this account's
                    // channels are rewritten.
                    sqlx::query(sqlx::AssertSqlSafe(format!(
                        "{statement} WHERE account_id=$1"
                    )))
                    .bind(account_id.as_uuid())
                    .execute(&mut connection)
                    .await?;
                }
                connection.close().await?;
            }
        }
        Ok(())
    }
}

/// The statements of a migration that rewrite rows.
fn data_statements(migration: &str) -> Vec<String> {
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

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Channels".to_owned(),
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

fn registration(key: &str, data_type: &str, unit: &str, scale: i32) -> ChannelRegistration {
    ChannelRegistration {
        channel_key: key.to_owned(),
        display_name: key.to_owned(),
        data_type: data_type.to_owned(),
        unit: unit.to_owned(),
        scale,
        aggregation: "mean".to_owned(),
    }
}

/// Channels registered before typed channels existed keep their values and
/// become integer, scale 0, unit W with the aggregation their flags meant.
async fn legacy_channels_survive_the_migration(store: Store) -> Result<(), Box<dyn Error>> {
    store.legacy_channel("mppt-1", true, false).await?;
    store.legacy_channel("meter-energy", false, true).await?;
    store.legacy_channel("spare", false, false).await?;
    let power = store.channel("mppt-1").await?;
    let energy = store.channel("meter-energy").await?;
    for (step, watts, watt_hours) in [(0, 1_850, 154), (1, 0, 0), (2, -12, 7)] {
        store
            .values(step, &[(power.id, watts), (energy.id, watt_hours)])
            .await?;
    }

    store.migrate_channel_definitions().await?;

    for (key, aggregation) in [
        ("mppt-1", "mean"),
        ("meter-energy", "sum"),
        ("spare", "last"),
    ] {
        let migrated = store.channel(key).await?;
        assert_eq!(migrated.data_type, "integer", "{key}");
        assert_eq!(migrated.scale, 0, "{key}");
        assert_eq!(migrated.unit, "W", "{key}");
        assert_eq!(migrated.aggregation, aggregation, "{key}");
        assert_eq!(migrated.target_kind, "unassigned", "{key}");
    }
    assert_eq!(store.stored(power.id, 3).await?, vec![1_850, 0, -12]);
    assert_eq!(store.stored(energy.id, 3).await?, vec![154, 0, 7]);
    assert_eq!(store.channel("mppt-1").await?.last_value, Some(-12));
    Ok(())
}

#[tokio::test]
async fn sqlite_legacy_channels_survive_the_migration() -> Result<(), Box<dyn Error>> {
    legacy_channels_survive_the_migration(Store::sqlite().await?).await
}

#[tokio::test]
async fn postgres_legacy_channels_survive_the_migration_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    legacy_channels_survive_the_migration(Store::postgres(url).await?).await
}

/// A decimal channel returns what was stored at its scale.
async fn typed_channel_round_trip(store: Store) -> Result<(), Box<dyn Error>> {
    let registered = store
        .channels
        .register_channel(
            store.system_id,
            &registration("dc.voltage", "decimal", "V", 2),
            START,
        )
        .await?;
    assert_eq!(registered.data_type, "decimal");
    assert_eq!(registered.unit, "V");
    assert_eq!(registered.scale, 2);
    assert_eq!(registered.target_kind, "unassigned");
    assert_eq!(registered.discovered_at, Some(START));
    assert_eq!(registered.last_value, None);

    // A key keeps the definition it was first registered with.
    let again = store
        .channels
        .register_channel(
            store.system_id,
            &registration("dc.voltage", "integer", "W", 0),
            START + 1,
        )
        .await?;
    assert_eq!(again, registered);

    store.values(0, &[(registered.id, 38_412)]).await?;
    let stored = store.channel("dc.voltage").await?;
    assert_eq!(stored.last_value, Some(38_412));
    assert_eq!(stored.last_value_at, Some(START));
    assert_eq!(
        store.channels.channels(store.system_id).await?,
        vec![stored]
    );

    assert!(
        store
            .channels
            .register_channel(store.system_id, &registration("bad", "text", "V", 0), START)
            .await
            .is_err(),
        "an unknown data type is refused"
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_typed_channel_round_trip() -> Result<(), Box<dyn Error>> {
    typed_channel_round_trip(Store::sqlite().await?).await
}

#[tokio::test]
async fn postgres_typed_channel_round_trip_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    typed_channel_round_trip(Store::postgres(url).await?).await
}
