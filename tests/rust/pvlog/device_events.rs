//! Device events are stored once and listed per system and per inverter, on both databases.

use std::{error::Error, sync::Arc};

use pvlog::device_events::StorageDeviceEventApi;
use pvlog_api::{
    DeviceEventApiError, DeviceEventApiUseCases as _, DeviceEventInput, DeviceEventQuery,
    DeviceEventResponse, DeviceEventsAccepted,
};
use pvlog_application::Clock;
use pvlog_domain::{
    AccountId, ApiCredentialId, DataSourceId, EquipmentValueProvenance, InverterId, MembershipId,
    StringId, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository, AccountRecord, DataSourceRecord, DataSourceRepository,
    DatabaseTarget, InverterRecord, ManagementRepository, MembershipRecord,
    PostgresAccountConfigurationRepository, PostgresDataSourceRepository,
    PostgresManagementRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SqliteDataSourceRepository, SqliteManagementRepository, SystemConfigurationRecord,
    SystemRegistryRecord, UserRecord, apply_migrations,
};
use tempfile::TempDir;
use uuid::Uuid;

const START: i64 = 1_800_000_000_000;
const ROOF: &str = "2000123456";
const GARAGE: &str = "3000654321";

/// One account with its event service and the repositories a test prepares it with.
struct Fixture {
    events: StorageDeviceEventApi,
    configuration: Box<dyn AccountConfigurationRepository>,
    data_sources: Box<dyn DataSourceRepository>,
    management: Arc<dyn ManagementRepository>,
    account_id: AccountId,
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
        Ok(Self {
            events: StorageDeviceEventApi::new(target, management.clone(), Arc::new(FixedClock)),
            configuration: Box::new(SqliteAccountConfigurationRepository::new(account.clone())),
            data_sources: Box::new(SqliteDataSourceRepository::new(account)),
            management,
            account_id,
            _directory: Some(directory),
        })
    }

    async fn postgres(url: String) -> Result<Self, Box<dyn Error>> {
        let target = DatabaseTarget::Postgres { url: url.clone() };
        apply_migrations(&target).await?;
        let management: Arc<dyn ManagementRepository> =
            Arc::new(PostgresManagementRepository::new(url.clone()));
        let account_id = seed_account(management.as_ref(), "active").await?;
        Ok(Self {
            events: StorageDeviceEventApi::new(target, management.clone(), Arc::new(FixedClock)),
            configuration: Box::new(PostgresAccountConfigurationRepository::new(
                url.clone(),
                account_id,
            )),
            data_sources: Box::new(PostgresDataSourceRepository::new(url, account_id)),
            management,
            account_id,
            _directory: None,
        })
    }

    /// Creates a system of the account.
    async fn system(&self) -> Result<SystemId, Box<dyn Error>> {
        let system_id = SystemId::new();
        self.configuration
            .save_system(&SystemConfigurationRecord {
                id: system_id,
                name: "Events".to_owned(),
                description: String::new(),
                timezone: "UTC".to_owned(),
                visibility: "private".to_owned(),
                lifecycle: "active".to_owned(),
                status_interval_seconds: 300,
                power_calculation_mode: "reported".to_owned(),
                net_calculation_mode: "separate_flows".to_owned(),
                created_at: START,
                updated_at: START,
            })
            .await?;
        self.management
            .save_system_registry(&SystemRegistryRecord {
                system_id,
                account_id: self.account_id,
                created_at: START,
                updated_at: START,
            })
            .await?;
        Ok(system_id)
    }

    /// Creates an inverter of a system and returns its identifier.
    async fn inverter(
        &self,
        system_id: SystemId,
        serial: Option<&str>,
    ) -> Result<Uuid, Box<dyn Error>> {
        let inverter_id = InverterId::new();
        self.configuration
            .save_inverter_aggregate(&InverterRecord {
                id: inverter_id,
                system_id,
                name: "Inverter".to_owned(),
                manufacturer: None,
                model: None,
                serial_reference: serial.map(ToOwned::to_owned),
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
            })
            .await?;
        Ok(inverter_id.as_uuid())
    }

    async fn list(
        &self,
        system_id: SystemId,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, Box<dyn Error>> {
        self.events.list(system_id, query).await.map_err(failed)
    }
}

fn failed(error: DeviceEventApiError) -> Box<dyn Error> {
    format!("device event request failed: {error:?}").into()
}

fn event(
    device: &str,
    event_id: Option<&str>,
    minutes: i64,
    code: Option<i64>,
    message: Option<&str>,
) -> DeviceEventInput {
    DeviceEventInput {
        device_id: device.to_owned(),
        event_id: event_id.map(ToOwned::to_owned),
        occurred_at_epoch_millis: START + minutes * 60_000,
        code,
        message: message.map(ToOwned::to_owned),
    }
}

/// `(device, event id, code)` of listed events, in the order listed.
fn listed(events: &[DeviceEventResponse]) -> Vec<(&str, Option<&str>, Option<i64>)> {
    events
        .iter()
        .map(|event| {
            (
                event.device_id.as_str(),
                event.event_id.as_deref(),
                event.code,
            )
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
async fn device_events_are_stored_once(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let system_id = fixture.system().await?;
    let roof = fixture.inverter(system_id, Some(ROOF)).await?;
    let unnamed = fixture.inverter(system_id, None).await?;

    // One upload: two events of the roof inverter, one with the source's own
    // identifier and one without, and an event of a device that is no inverter.
    let upload = vec![
        event(
            ROOF,
            Some("8812"),
            0,
            Some(3_501),
            Some("Insulation failure"),
        ),
        event(ROOF, None, 5, Some(104), None),
        event(GARAGE, Some("17"), 10, None, Some("Grid relay opened")),
    ];
    assert_eq!(
        fixture
            .events
            .store(system_id, None, upload.clone())
            .await
            .map_err(failed)?,
        DeviceEventsAccepted {
            stored: 3,
            unchanged: 0
        }
    );

    // Uploading the same events again stores nothing: each exists once.
    assert_eq!(
        fixture
            .events
            .store(system_id, None, upload)
            .await
            .map_err(failed)?,
        DeviceEventsAccepted {
            stored: 0,
            unchanged: 3
        }
    );
    let all = fixture.list(system_id, DeviceEventQuery::default()).await?;
    assert_eq!(
        listed(&all),
        vec![
            (GARAGE, Some("17"), None),
            (ROOF, None, Some(104)),
            (ROOF, Some("8812"), Some(3_501)),
        ],
        "newest first, each once"
    );
    assert_eq!(all[2].message.as_deref(), Some("Insulation failure"));
    assert_eq!(all[2].occurred_at_epoch_millis, START);
    assert_eq!(all[2].received_at_epoch_millis, START);
    assert_eq!(all[2].data_source_id, None);

    // The source's identifier is the event, whatever else changes: the stored
    // event takes the newer content and stays one event.
    assert_eq!(
        fixture
            .events
            .store(
                system_id,
                None,
                vec![
                    event(
                        ROOF,
                        Some("8812"),
                        1,
                        Some(3_501),
                        Some("Insulation failure (cleared)")
                    ),
                    // The same identifier at another device is another event.
                    event(GARAGE, Some("8812"), 2, Some(3_501), None),
                    // Without an identifier, time and code are the event.
                    event(ROOF, None, 5, Some(105), None),
                    event(ROOF, None, 5, Some(104), None),
                ],
            )
            .await
            .map_err(failed)?,
        DeviceEventsAccepted {
            stored: 3,
            unchanged: 1
        }
    );
    let roof_events = fixture
        .list(
            system_id,
            DeviceEventQuery {
                device_id: Some(ROOF.to_owned()),
                ..DeviceEventQuery::default()
            },
        )
        .await?;
    assert_eq!(roof_events.len(), 3);
    let replaced = roof_events
        .iter()
        .find(|event| event.event_id.as_deref() == Some("8812"))
        .ok_or("the identified event is listed")?;
    assert_eq!(replaced.id, all[2].id, "it is still the same event");
    assert_eq!(
        replaced.message.as_deref(),
        Some("Insulation failure (cleared)")
    );
    assert_eq!(replaced.occurred_at_epoch_millis, START + 60_000);

    // An inverter's events are those of the device with its serial.
    let for_inverter = fixture
        .events
        .list_for_inverter(system_id, roof, DeviceEventQuery::default())
        .await
        .map_err(failed)?;
    assert_eq!(for_inverter, roof_events);
    assert_eq!(
        fixture
            .events
            .list_for_inverter(system_id, unnamed, DeviceEventQuery::default())
            .await,
        Ok(Vec::new()),
        "an inverter without a serial has no events"
    );
    assert_eq!(
        fixture
            .events
            .list_for_inverter(system_id, Uuid::now_v7(), DeviceEventQuery::default())
            .await,
        Err(DeviceEventApiError::NotFound)
    );

    // A range and a limit narrow the listing.
    let window = fixture
        .list(
            system_id,
            DeviceEventQuery {
                from: Some(START + 2 * 60_000),
                to: Some(START + 10 * 60_000),
                ..DeviceEventQuery::default()
            },
        )
        .await?;
    assert_eq!(
        listed(&window)
            .iter()
            .map(|event| event.2)
            .collect::<std::collections::BTreeSet<_>>(),
        [Some(104), Some(105), Some(3_501)].into(),
        "the end of the range is exclusive"
    );
    assert_eq!(window.len(), 3);
    let newest = fixture
        .list(
            system_id,
            DeviceEventQuery {
                limit: Some(1),
                ..DeviceEventQuery::default()
            },
        )
        .await?;
    assert_eq!(listed(&newest), vec![(GARAGE, Some("17"), None)]);

    // An event delivered through a data source's key is attributed to it.
    let credential_id = ApiCredentialId::new();
    let data_source_id = DataSourceId::new();
    fixture
        .data_sources
        .save_data_source(&DataSourceRecord {
            id: data_source_id,
            system_id,
            name: "Logger".to_owned(),
            profile: "smalog".to_owned(),
            credential_id: Some(credential_id),
            created_at: START,
            updated_at: START,
            deleted_at: None,
        })
        .await?;
    fixture
        .events
        .store(
            system_id,
            Some(credential_id),
            vec![event(ROOF, Some("9000"), 30, Some(1), None)],
        )
        .await
        .map_err(failed)?;
    assert_eq!(
        fixture
            .list(
                system_id,
                DeviceEventQuery {
                    limit: Some(1),
                    ..DeviceEventQuery::default()
                }
            )
            .await?[0]
            .data_source_id,
        Some(data_source_id.as_uuid())
    );

    // Another system has its own events, also for the same device and identifier.
    let other = fixture.system().await?;
    assert!(
        fixture
            .list(other, DeviceEventQuery::default())
            .await?
            .is_empty()
    );
    assert_eq!(
        fixture
            .events
            .store(
                other,
                None,
                vec![event(ROOF, Some("8812"), 0, Some(3_501), None)]
            )
            .await
            .map_err(failed)?
            .stored,
        1
    );
    assert_eq!(
        fixture
            .list(system_id, DeviceEventQuery::default())
            .await?
            .len(),
        6
    );
    assert_eq!(
        fixture
            .events
            .store(
                SystemId::new(),
                None,
                vec![event(ROOF, None, 0, None, None)]
            )
            .await,
        Err(DeviceEventApiError::NotFound)
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_stores_a_device_event_once_and_lists_it() -> Result<(), Box<dyn Error>> {
    device_events_are_stored_once(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_stores_a_device_event_once_and_lists_it_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    device_events_are_stored_once(Fixture::postgres(url).await?).await
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
            slug: format!("events-{account_id}"),
            display_name: "Events".to_owned(),
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

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(START).unwrap_or_else(|_| unreachable!())
    }
}
