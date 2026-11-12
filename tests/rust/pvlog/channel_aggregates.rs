//! Aggregates of bound measurement channels, on both databases.

use std::{error::Error, sync::Arc};

use pvlog::{
    channel_aggregation::rebuild_channel_rollups,
    channels::{StorageChannelApi, channel_repository},
};
use pvlog_api::{
    ChannelAggregateQuery, ChannelAggregateResponse, ChannelApiError, ChannelApiUseCases as _,
};
use pvlog_application::Clock;
use pvlog_domain::{
    AccountId, ChannelId, EquipmentValueProvenance, InverterId, MembershipId, ObservationId,
    StringId, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository as _, AccountRecord, ChannelBindingRecord, ChannelRegistration,
    ChannelRepository, DatabaseTarget, InverterRecord, ManagementRepository, MembershipRecord,
    PostgresAccountConfigurationRepository, PostgresManagementRepository,
    PostgresTelemetryRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SqliteManagementRepository, SqliteTelemetryRepository, StoredMeasurements, StoredObservation,
    SystemConfigurationRecord, SystemRegistryRecord, TelemetryRepository, UserRecord,
    apply_migrations,
};
use tempfile::TempDir;
use uuid::Uuid;

/// 2027-01-15 09:00 in Berlin, the start of a quarter hour.
const START: i64 = 1_800_000_000_000;
const MINUTE: i64 = 60_000;
const DAY: i64 = 24 * 60 * MINUTE;
/// Local midnight of the first day and of the year, in Berlin.
const FIRST_DAY: i64 = START - 9 * 60 * MINUTE;
const YEAR: i64 = FIRST_DAY - 14 * DAY;

/// One system with an inverter, its channel storage and its readings.
struct Fixture {
    channels: Box<dyn ChannelRepository>,
    telemetry: Box<dyn TelemetryRepository>,
    api: StorageChannelApi,
    system_id: SystemId,
    inverter: Uuid,
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
        let account = router.route(account_id).await?;
        let system_id = SystemId::new();
        let roof = inverter(system_id);
        let configuration = SqliteAccountConfigurationRepository::new(account.clone());
        configuration.save_system(&system(system_id)).await?;
        configuration.save_inverter_aggregate(&roof).await?;
        Self::assemble(
            target,
            management,
            Box::new(SqliteTelemetryRepository::new(account)),
            account_id,
            system_id,
            &roof,
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
        let roof = inverter(system_id);
        let configuration = PostgresAccountConfigurationRepository::new(url.clone(), account_id);
        configuration.save_system(&system(system_id)).await?;
        configuration.save_inverter_aggregate(&roof).await?;
        Self::assemble(
            target,
            management,
            Box::new(PostgresTelemetryRepository::new(url, account_id)),
            account_id,
            system_id,
            &roof,
            None,
        )
        .await
    }

    async fn assemble(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        telemetry: Box<dyn TelemetryRepository>,
        account_id: AccountId,
        system_id: SystemId,
        roof: &InverterRecord,
        directory: Option<TempDir>,
    ) -> Result<Self, Box<dyn Error>> {
        management
            .save_system_registry(&SystemRegistryRecord {
                system_id,
                account_id,
                created_at: START,
                updated_at: START,
            })
            .await?;
        Ok(Self {
            channels: channel_repository(&target, management.as_ref(), system_id)
                .await
                .ok_or("channel storage is unavailable")?,
            telemetry,
            api: StorageChannelApi::new(target, management, Arc::new(FixedClock)),
            system_id,
            inverter: roof.id.as_uuid(),
            _directory: directory,
        })
    }

    /// Registers a channel and binds it.
    async fn channel(
        &self,
        key: &str,
        data_type: &str,
        unit: &str,
        aggregation: &str,
        target: Option<(&str, Option<Uuid>)>,
    ) -> Result<ChannelId, Box<dyn Error>> {
        let registered = self
            .channels
            .register_channel(
                self.system_id,
                &ChannelRegistration {
                    channel_key: key.to_owned(),
                    display_name: key.to_owned(),
                    data_type: data_type.to_owned(),
                    unit: unit.to_owned(),
                    scale: 0,
                    aggregation: aggregation.to_owned(),
                },
                START,
            )
            .await?;
        if let Some((target_kind, target_id)) = target {
            self.bind(key, target_kind, target_id).await?;
        }
        Ok(registered.id)
    }

    async fn bind(
        &self,
        key: &str,
        target_kind: &str,
        target_id: Option<Uuid>,
    ) -> Result<(), Box<dyn Error>> {
        self.channels
            .bind_channel(
                self.system_id,
                key,
                &ChannelBindingRecord {
                    target_kind: target_kind.to_owned(),
                    target_id,
                    display_name: None,
                    aggregation: None,
                },
                START,
            )
            .await?
            .ok_or("the channel is known")?;
        Ok(())
    }

    /// Stores one reading with the given channel values.
    async fn reading(
        &self,
        measured_at: i64,
        received_at: i64,
        values: &[(ChannelId, i64)],
    ) -> Result<(), Box<dyn Error>> {
        let id = ObservationId::new();
        self.telemetry
            .insert_observation(&StoredObservation {
                id,
                system_id: self.system_id,
                measured_at,
                received_at,
                source_kind: "modern_api".to_owned(),
                source_identity: "aggregates-test".to_owned(),
                idempotency_identity: Some(format!("aggregates-test:{id}")),
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
        self.channels
            .write_channel_values(id, measured_at, values, received_at)
            .await?;
        Ok(())
    }

    /// Runs one aggregation pass and reports how many rows it wrote.
    async fn rebuild(&self) -> Result<u64, Box<dyn Error>> {
        Ok(rebuild_channel_rollups(self.channels.as_ref(), self.system_id, START).await?)
    }

    /// The aggregates of the channels bound to a target, as the API returns them.
    async fn aggregates(
        &self,
        target_kind: &str,
        target_id: Option<Uuid>,
        resolution: &str,
    ) -> Result<Vec<ChannelAggregateResponse>, Box<dyn Error>> {
        self.api
            .aggregates(
                self.system_id,
                ChannelAggregateQuery {
                    target_kind: target_kind.to_owned(),
                    target_id,
                    resolution: resolution.to_owned(),
                    from: YEAR,
                    to: YEAR + 366 * DAY,
                },
            )
            .await
            .map_err(|error: ChannelApiError| format!("aggregates failed: {error:?}").into())
    }
}

/// `(bucket start, value, minimum, maximum, points)` of one channel's buckets.
fn buckets(
    aggregates: &[ChannelAggregateResponse],
    key: &str,
) -> Vec<(i64, Option<i64>, i64, i64, i64)> {
    aggregates
        .iter()
        .filter(|channel| channel.channel_key == key)
        .flat_map(|channel| &channel.buckets)
        .map(|bucket| {
            (
                bucket.start_epoch_millis,
                bucket.value,
                bucket.minimum,
                bucket.maximum,
                bucket.point_count,
            )
        })
        .collect()
}

fn keys(aggregates: &[ChannelAggregateResponse]) -> Vec<&str> {
    aggregates
        .iter()
        .map(|channel| channel.channel_key.as_str())
        .collect()
}

#[allow(clippy::too_many_lines)]
async fn bound_channels_are_aggregated(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let on_inverter = Some(("inverter", Some(fixture.inverter)));
    let power = fixture
        .channel("inv.1.ac.l1.power", "integer", "W", "mean", on_inverter)
        .await?;
    let temperature = fixture
        .channel("inv.1.temperature", "integer", "m°C", "max", on_inverter)
        .await?;
    let lifetime = fixture
        .channel(
            "inv.1.energy.lifetime",
            "counter",
            "Wh",
            "counter_delta",
            on_inverter,
        )
        .await?;
    let status = fixture
        .channel("inv.1.status", "integer", "code", "last", on_inverter)
        .await?;
    let meter = fixture
        .channel(
            "meter.energy",
            "integer",
            "Wh",
            "sum",
            Some(("plant", None)),
        )
        .await?;
    let spare = fixture
        .channel("spare", "integer", "W", "mean", None)
        .await?;

    // Two readings in the first quarter hour, one in the second, one a day later.
    for (offset, watts, millidegrees, counter, code, watt_hours) in [
        (0, 1_000, 41_000, 10_000, 307, 80),
        (5 * MINUTE, 2_000, 43_500, 10_100, 307, 160),
        (15 * MINUTE, 4_000, 42_000, 10_250, 455, 320),
        (DAY, 600, -3_000, 50, 307, 40),
    ] {
        fixture
            .reading(
                START + offset,
                START + offset,
                &[
                    (power, watts),
                    (temperature, millidegrees),
                    (lifetime, counter),
                    (status, code),
                    (meter, watt_hours),
                    (spare, 9_999),
                ],
            )
            .await?;
    }
    assert!(fixture.rebuild().await? > 0);

    // The inverter's aggregates hold its four channels, each by its own kind.
    let quarters = fixture
        .aggregates("inverter", Some(fixture.inverter), "fifteen_minute")
        .await?;
    assert_eq!(
        keys(&quarters),
        vec![
            "inv.1.ac.l1.power",
            "inv.1.energy.lifetime",
            "inv.1.status",
            "inv.1.temperature"
        ]
    );
    assert_eq!(
        buckets(&quarters, "inv.1.ac.l1.power"),
        vec![
            (START, Some(1_500), 1_000, 2_000, 2),
            (START + 15 * MINUTE, Some(4_000), 4_000, 4_000, 1),
            (START + DAY, Some(600), 600, 600, 1),
        ]
    );
    let days = fixture
        .aggregates("inverter", Some(fixture.inverter), "day")
        .await?;
    assert_eq!(
        buckets(&days, "inv.1.ac.l1.power"),
        vec![
            // (1000 + 2000 + 4000) / 3, rounded.
            (FIRST_DAY, Some(2_333), 1_000, 4_000, 3),
            (FIRST_DAY + DAY, Some(600), 600, 600, 1),
        ]
    );
    assert_eq!(
        buckets(&days, "inv.1.temperature"),
        vec![
            (FIRST_DAY, Some(43_500), 41_000, 43_500, 3),
            (FIRST_DAY + DAY, Some(-3_000), -3_000, -3_000, 1),
        ]
    );
    assert_eq!(
        buckets(&days, "inv.1.status"),
        vec![
            (FIRST_DAY, Some(455), 307, 455, 3),
            (FIRST_DAY + DAY, Some(307), 307, 307, 1),
        ]
    );
    // The counter grew by 100 and 150 on the first day; its first value has
    // no predecessor. The lower value of the second day is a reset.
    assert_eq!(
        buckets(&days, "inv.1.energy.lifetime"),
        vec![
            (FIRST_DAY, Some(250), 10_000, 10_250, 3),
            (FIRST_DAY + DAY, Some(0), 50, 50, 1),
        ]
    );
    assert_eq!(
        buckets(&quarters, "inv.1.energy.lifetime")
            .iter()
            .map(|bucket| bucket.1)
            .collect::<Vec<_>>(),
        vec![Some(100), Some(150), Some(0)]
    );
    // Months and years follow the local calendar and are merged from the days.
    for resolution in ["month", "year"] {
        let coarse = fixture
            .aggregates("inverter", Some(fixture.inverter), resolution)
            .await?;
        assert_eq!(
            buckets(&coarse, "inv.1.ac.l1.power"),
            vec![(YEAR, Some(1_900), 600, 4_000, 4)],
            "{resolution}"
        );
        assert_eq!(
            buckets(&coarse, "inv.1.energy.lifetime"),
            vec![(YEAR, Some(250), 50, 10_250, 4)],
            "{resolution}"
        );
    }
    // The system's own channel is aggregated at the system, the unassigned one nowhere.
    let plant = fixture.aggregates("plant", None, "day").await?;
    assert_eq!(keys(&plant), vec!["meter.energy"]);
    assert_eq!(
        buckets(&plant, "meter.energy"),
        vec![
            (FIRST_DAY, Some(560), 80, 320, 3),
            (FIRST_DAY + DAY, Some(40), 40, 40, 1),
        ]
    );
    assert!(
        fixture
            .channels
            .channel_rollup_buckets(spare, "day", YEAR, YEAR + 366 * DAY)
            .await?
            .is_empty()
    );

    // Nothing new, nothing to do.
    assert_eq!(fixture.rebuild().await?, 0);

    // A later reading updates the buckets it falls into and no others.
    fixture
        .reading(
            START + 20 * MINUTE,
            START + 2 * DAY,
            &[(power, 6_000), (lifetime, 10_400)],
        )
        .await?;
    assert!(fixture.rebuild().await? > 0);
    let days = fixture
        .aggregates("inverter", Some(fixture.inverter), "day")
        .await?;
    assert_eq!(
        buckets(&days, "inv.1.ac.l1.power"),
        vec![
            (FIRST_DAY, Some(3_250), 1_000, 6_000, 4),
            (FIRST_DAY + DAY, Some(600), 600, 600, 1),
        ]
    );
    assert_eq!(
        buckets(&days, "inv.1.energy.lifetime")[0],
        (FIRST_DAY, Some(400), 10_000, 10_400, 4)
    );
    assert_eq!(buckets(&days, "inv.1.temperature").len(), 2);
    // A reading received in the same instant as the newest one is not missed.
    fixture
        .reading(START + 25 * MINUTE, START + 2 * DAY, &[(power, 250)])
        .await?;
    assert!(fixture.rebuild().await? > 0);
    assert_eq!(fixture.rebuild().await?, 0);
    let days = fixture
        .aggregates("inverter", Some(fixture.inverter), "day")
        .await?;
    assert_eq!(
        buckets(&days, "inv.1.ac.l1.power")[0],
        (FIRST_DAY, Some(2_650), 250, 6_000, 5)
    );

    // Rebinding takes the channel out of the inverter's aggregates at once
    // and rebuilds its whole history under the new target.
    fixture.bind("inv.1.ac.l1.power", "plant", None).await?;
    let days = fixture
        .aggregates("inverter", Some(fixture.inverter), "day")
        .await?;
    assert!(buckets(&days, "inv.1.ac.l1.power").is_empty());
    assert_eq!(buckets(&days, "inv.1.temperature").len(), 2);
    assert!(fixture.rebuild().await? > 0);
    let plant = fixture.aggregates("plant", None, "day").await?;
    assert_eq!(keys(&plant), vec!["inv.1.ac.l1.power", "meter.energy"]);
    assert_eq!(
        buckets(&plant, "inv.1.ac.l1.power"),
        vec![
            (FIRST_DAY, Some(2_650), 250, 6_000, 5),
            (FIRST_DAY + DAY, Some(600), 600, 600, 1),
        ]
    );
    assert_eq!(
        buckets(
            &fixture.aggregates("plant", None, "year").await?,
            "inv.1.ac.l1.power"
        ),
        vec![(YEAR, Some(2_308), 250, 6_000, 6)]
    );
    let days = fixture
        .aggregates("inverter", Some(fixture.inverter), "day")
        .await?;
    assert!(buckets(&days, "inv.1.ac.l1.power").is_empty());

    // An unassigned or retired channel keeps its values and has no aggregates.
    fixture
        .bind("inv.1.ac.l1.power", "unassigned", None)
        .await?;
    fixture
        .channels
        .retire_channel(fixture.system_id, "meter.energy", START)
        .await?;
    assert_eq!(fixture.rebuild().await?, 0);
    assert!(fixture.aggregates("plant", None, "day").await?.is_empty());
    assert_eq!(
        fixture
            .channels
            .channel_values(power, START, START + 3 * DAY)
            .await?
            .len(),
        6
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_aggregates_bound_channels_and_rebuilds_on_rebinding() -> Result<(), Box<dyn Error>>
{
    bound_channels_are_aggregated(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_aggregates_bound_channels_and_rebuilds_on_rebinding_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    bound_channels_are_aggregated(Fixture::postgres(url).await?).await
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
            slug: format!("aggregates-{account_id}"),
            display_name: "Aggregates".to_owned(),
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
        name: "Aggregates".to_owned(),
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

fn inverter(system_id: SystemId) -> InverterRecord {
    let inverter_id = InverterId::new();
    InverterRecord {
        id: inverter_id,
        system_id,
        name: "Roof".to_owned(),
        manufacturer: None,
        model: None,
        serial_reference: Some("1".to_owned()),
        rated_power_watts: None,
        catalog_entry_id: None,
        catalog_revision: None,
        value_provenance: EquipmentValueProvenance::Manual,
        specification_snapshot: None,
        effective_from: 0,
        effective_to: None,
        created_at: 1,
        updated_at: 1,
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

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(START).unwrap_or_else(|_| unreachable!())
    }
}
