//! An aggregate series must equal the sum of the strings it covers.
//!
//! This is the rule the dashboard design states outright: a system or inverter
//! view is the true sum of its member strings' readings, never a separately
//! modelled curve. It is asserted here rather than only observed by eye,
//! because a change to the channel join or the scope filter would otherwise
//! shift totals silently.

use std::{collections::BTreeSet, error::Error};

use pvlog::analytics_series::{AccountSeriesRepository, SeriesScope};
use pvlog_application::{
    QueryPlanRequest, QueryResolution, RequestedResolution, SeriesField, SeriesQueryRepository,
    plan_query,
};
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::{DatabaseTarget, SqliteAccountProvisioner, apply_migrations};
use sqlx::{ConnectOptions as _, Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

/// Readings for two strings at three instants, chosen so every total differs.
const EAST: [i64; 3] = [3_000, 3_100, 3_200];
const WEST: [i64; 3] = [2_000, 2_050, 2_100];
const FIRST_INSTANT: i64 = 1_800_000_000_000;
const STEP_MILLIS: i64 = 300_000;

#[tokio::test]
async fn an_aggregate_series_equals_the_sum_of_its_strings() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;

    let east = fixture
        .read(SeriesScope::String(fixture.east_string))
        .await?;
    let west = fixture
        .read(SeriesScope::String(fixture.west_string))
        .await?;
    let inverter = fixture
        .read(SeriesScope::Inverter(fixture.inverter))
        .await?;
    let system = fixture.read(SeriesScope::System).await?;

    assert_eq!(east, EAST.to_vec(), "string reads its own channel only");
    assert_eq!(west, WEST.to_vec(), "string reads its own channel only");

    let expected: Vec<i64> = EAST
        .iter()
        .zip(WEST.iter())
        .map(|(east, west)| east + west)
        .collect();
    assert_eq!(inverter, expected, "inverter is the sum of its strings");
    assert_eq!(system, expected, "system is the sum of its strings");
    Ok(())
}

#[tokio::test]
async fn an_unbound_channel_contributes_to_no_scope() -> Result<(), Box<dyn Error>> {
    let mut fixture = Fixture::new().await?;
    // A meter nobody has assigned yet is retained but must not inflate a total.
    fixture.add_unbound_channel(9_999).await?;

    let system = fixture.read(SeriesScope::System).await?;
    let expected: Vec<i64> = EAST
        .iter()
        .zip(WEST.iter())
        .map(|(east, west)| east + west)
        .collect();
    assert_eq!(system, expected, "unassigned readings stay out of the sum");
    Ok(())
}

/// A system created from a name alone has no strings to sum, so its series is
/// what the uploader reported for the plant.
#[tokio::test]
async fn a_system_without_equipment_reports_its_plant_readings() -> Result<(), Box<dyn Error>> {
    let mut fixture = Fixture::new().await?;
    let bare = fixture
        .add_system_without_equipment(&[1_500, 1_600])
        .await?;
    fixture.system_id = bare;

    let system = fixture.read(SeriesScope::System).await?;
    assert_eq!(system, vec![1_500, 1_600]);
    Ok(())
}

struct Fixture {
    _directory: TempDir,
    target: DatabaseTarget,
    account_id: AccountId,
    system_id: SystemId,
    inverter: Uuid,
    east_string: Uuid,
    west_string: Uuid,
    account_path: std::path::PathBuf,
}

impl Fixture {
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
        let mut management = connect(&management_path).await?;
        sqlx::query(
            "INSERT INTO accounts (id, slug, display_name, status, created_at, updated_at) \
             VALUES (?, 'scope-test', 'Scope Test', 'provisioning', 1, 1)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .execute(&mut management)
        .await?;
        management.close().await?;

        let provisioner = SqliteAccountProvisioner::new(management_path, accounts_dir.clone());
        let database = provisioner.provision(account_id).await?;
        let account_path = accounts_dir.join(database.opaque_locator);

        let system_id = SystemId::new();
        let inverter = Uuid::now_v7();
        let east_string = Uuid::now_v7();
        let west_string = Uuid::now_v7();

        let mut account = connect(&account_path).await?;
        seed(&mut account, system_id, inverter, east_string, west_string).await?;
        account.close().await?;

        Ok(Self {
            _directory: directory,
            target,
            account_id,
            system_id,
            inverter,
            east_string,
            west_string,
            account_path,
        })
    }

    /// Adds a channel bound to nothing, with a value large enough to be obvious.
    async fn add_unbound_channel(&mut self, value: i64) -> Result<(), Box<dyn Error>> {
        let mut account = connect(&self.account_path).await?;
        let channel = Uuid::now_v7();
        insert_channel(&mut account, self.system_id, channel, "spare", None).await?;
        for index in 0..EAST.len() {
            let observation = observation_id(index);
            insert_value(&mut account, observation, channel, value).await?;
        }
        account.close().await?;
        Ok(())
    }

    /// Adds a second system that has plant readings but no inverter or string.
    async fn add_system_without_equipment(
        &mut self,
        generation_watts: &[i64],
    ) -> Result<SystemId, Box<dyn Error>> {
        let system_id = SystemId::new();
        let mut account = connect(&self.account_path).await?;
        sqlx::query(
            "INSERT INTO systems (id, name, timezone, status_interval_seconds, \
             power_calculation_mode, net_calculation_mode, created_at, updated_at) \
             VALUES (?, 'Name only', 'Europe/Berlin', 300, 'reported', 'separate_flows', 1, 1)",
        )
        .bind(system_id.as_uuid().as_bytes().as_slice())
        .execute(&mut account)
        .await?;
        for (index, watts) in generation_watts.iter().enumerate() {
            let measured_at =
                FIRST_INSTANT + STEP_MILLIS * i64::try_from(index).unwrap_or_default();
            sqlx::query(
                "INSERT INTO telemetry_hot (observation_id, system_id, measured_at, received_at, \
                 source_kind, source_identity, quality_flags, generation_power_watts, \
                 canonical_hash) VALUES (?, ?, ?, ?, 'modern_api', 'bare', 0, ?, zeroblob(32))",
            )
            .bind(Uuid::now_v7().as_bytes().as_slice())
            .bind(system_id.as_uuid().as_bytes().as_slice())
            .bind(measured_at)
            .bind(measured_at)
            .bind(watts)
            .execute(&mut account)
            .await?;
        }
        account.close().await?;
        Ok(system_id)
    }

    async fn read(&self, scope: SeriesScope) -> Result<Vec<i64>, Box<dyn Error>> {
        let end = FIRST_INSTANT + STEP_MILLIS * 10;
        let repository = AccountSeriesRepository::new(
            self.target.clone(),
            self.account_id,
            scope,
            FIRST_INSTANT,
            end,
        );
        let plan = plan_query(QueryPlanRequest {
            start_epoch_millis: FIRST_INSTANT,
            end_epoch_millis: end,
            requested_resolution: RequestedResolution::Raw,
            fields: BTreeSet::from([SeriesField::GenerationPower]),
            timezone: "Europe/Berlin".to_owned(),
            maximum_points: 1_000,
            expected_raw_interval_millis: STEP_MILLIS.unsigned_abs(),
            hot_data_start_epoch_millis: i64::MIN,
            available_rollups: BTreeSet::from([QueryResolution::Hourly]),
            scope: None,
            scope_id: None,
        })?;
        let series = repository.execute_plan(self.system_id, &plan).await?;
        Ok(series
            .first()
            .map_or_default(|entry| entry.points.iter().map(|point| point.value).collect()))
    }
}

async fn connect(path: &std::path::Path) -> Result<SqliteConnection, sqlx::Error> {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .connect()
        .await
}

fn observation_id(index: usize) -> Uuid {
    let mut bytes = [0_u8; 16];
    bytes[15] = u8::try_from(index + 1).unwrap_or(1);
    Uuid::from_bytes(bytes)
}

async fn seed(
    account: &mut SqliteConnection,
    system_id: SystemId,
    inverter: Uuid,
    east_string: Uuid,
    west_string: Uuid,
) -> Result<(), Box<dyn Error>> {
    sqlx::query(
        "INSERT INTO systems (id, name, timezone, status_interval_seconds, \
         power_calculation_mode, net_calculation_mode, created_at, updated_at) \
         VALUES (?, 'Scope PV', 'Europe/Berlin', 300, 'reported', 'separate_flows', 1, 1)",
    )
    .bind(system_id.as_uuid().as_bytes().as_slice())
    .execute(&mut *account)
    .await?;
    sqlx::query(
        "INSERT INTO inverters (id, system_id, name, effective_from, created_at, updated_at) \
         VALUES (?, ?, 'Inverter', 0, 1, 1)",
    )
    .bind(inverter.as_bytes().as_slice())
    .bind(system_id.as_uuid().as_bytes().as_slice())
    .execute(&mut *account)
    .await?;
    for (id, name) in [(east_string, "East"), (west_string, "West")] {
        sqlx::query(
            "INSERT INTO pv_strings (id, inverter_id, name, panel_count, rated_power_watts, \
             effective_from, created_at, updated_at) VALUES (?, ?, ?, 10, 4000, 0, 1, 1)",
        )
        .bind(id.as_bytes().as_slice())
        .bind(inverter.as_bytes().as_slice())
        .bind(name)
        .execute(&mut *account)
        .await?;
    }

    let east_channel = Uuid::now_v7();
    let west_channel = Uuid::now_v7();
    insert_channel(
        account,
        system_id,
        east_channel,
        "mppt-1",
        Some(east_string),
    )
    .await?;
    insert_channel(
        account,
        system_id,
        west_channel,
        "mppt-2",
        Some(west_string),
    )
    .await?;

    for (index, (east, west)) in EAST.iter().zip(WEST.iter()).enumerate() {
        let observation = observation_id(index);
        let measured_at = FIRST_INSTANT + STEP_MILLIS * i64::try_from(index).unwrap_or_default();
        sqlx::query(
            "INSERT INTO telemetry_hot (observation_id, system_id, measured_at, received_at, \
             source_kind, source_identity, quality_flags, canonical_hash) \
             VALUES (?, ?, ?, ?, 'modern_api', 'scope-test', 0, zeroblob(32))",
        )
        .bind(observation.as_bytes().as_slice())
        .bind(system_id.as_uuid().as_bytes().as_slice())
        .bind(measured_at)
        .bind(measured_at)
        .execute(&mut *account)
        .await?;
        insert_value(account, observation, east_channel, *east).await?;
        insert_value(account, observation, west_channel, *west).await?;
    }
    Ok(())
}

async fn insert_channel(
    account: &mut SqliteConnection,
    system_id: SystemId,
    channel: Uuid,
    key: &str,
    target: Option<Uuid>,
) -> Result<(), Box<dyn Error>> {
    sqlx::query(
        "INSERT INTO channel_definitions (id, system_id, channel_key, display_name, data_type, \
         unit, scale, effective_from, created_at, updated_at, target_kind, target_id) \
         VALUES (?, ?, ?, ?, 'integer', 'W', 0, 0, 1, 1, ?, ?)",
    )
    .bind(channel.as_bytes().as_slice())
    .bind(system_id.as_uuid().as_bytes().as_slice())
    .bind(key)
    .bind(key)
    .bind(if target.is_some() {
        "string"
    } else {
        "unassigned"
    })
    .bind(target.map(|value| value.as_bytes().to_vec()))
    .execute(account)
    .await?;
    Ok(())
}

async fn insert_value(
    account: &mut SqliteConnection,
    observation: Uuid,
    channel: Uuid,
    value: i64,
) -> Result<(), Box<dyn Error>> {
    sqlx::query(
        "INSERT INTO telemetry_hot_extended_values (observation_id, channel_id, integer_value) \
         VALUES (?, ?, ?)",
    )
    .bind(observation.as_bytes().as_slice())
    .bind(channel.as_bytes().as_slice())
    .bind(value)
    .execute(account)
    .await?;
    Ok(())
}
