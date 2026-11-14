//! Equipment discovery proposals from reported devices, on both databases.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::Arc,
};

use pvlog::{
    channels::channel_repository, discovery::StorageDiscoveryApi, telemetry::StorageTelemetryApi,
};
use pvlog_api::{
    AdoptProposalInput, AdoptedStringInput, DiscoveryApiError, DiscoveryApiUseCases as _,
    DiscoveryProposalResponse,
};
use pvlog_application::{
    ChannelReading, Clock, DecimalValue, ModernTelemetryUseCases as _, NormalizeObservation,
    ObservationSupplements,
};
use pvlog_domain::{
    AccountId, BatteryFlowState, InverterId, MembershipId, ObservationSource,
    ObservationSourceKind, QualityFlags, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository, AccountRecord, ChannelBindingRecord, ChannelRepository,
    DatabaseTarget, ManagementRepository, MembershipRecord, PostgresAccountConfigurationRepository,
    PostgresManagementRepository, SqliteAccountConfigurationRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteManagementRepository,
    SystemConfigurationRecord, SystemRegistryRecord, UserRecord, apply_migrations,
};
use tempfile::TempDir;
use uuid::Uuid;

const START: i64 = 1_800_000_000_000;
const STEP: i64 = 300_000;
const ROOF: &str = "2000123456";
const GARAGE: &str = "3000654321";

/// One system without equipment, fed through the ingestion service.
struct Fixture {
    telemetry: StorageTelemetryApi,
    discovery: StorageDiscoveryApi,
    channels: Box<dyn ChannelRepository>,
    configuration: Box<dyn AccountConfigurationRepository>,
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
        let router = SqliteAccountPoolRouter::new(
            management_path,
            accounts_dir,
            SqliteAccountPoolConfig::default(),
        )?;
        let configuration =
            SqliteAccountConfigurationRepository::new(router.route(account_id).await?);
        Self::assemble(
            target,
            management,
            Box::new(configuration),
            account_id,
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
        let configuration = PostgresAccountConfigurationRepository::new(url, account_id);
        Self::assemble(
            target,
            management,
            Box::new(configuration),
            account_id,
            None,
        )
        .await
    }

    async fn assemble(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        configuration: Box<dyn AccountConfigurationRepository>,
        account_id: AccountId,
        directory: Option<TempDir>,
    ) -> Result<Self, Box<dyn Error>> {
        let system_id = SystemId::new();
        configuration.save_system(&system(system_id)).await?;
        management
            .save_system_registry(&SystemRegistryRecord {
                system_id,
                account_id,
                created_at: START,
                updated_at: START,
            })
            .await?;
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        Ok(Self {
            telemetry: StorageTelemetryApi::new(target.clone(), management.clone(), clock.clone()),
            channels: channel_repository(&target, management.as_ref(), system_id)
                .await
                .ok_or("channel storage is unavailable")?,
            discovery: StorageDiscoveryApi::new(target, management, clock),
            configuration,
            system_id,
            _directory: directory,
        })
    }

    /// Ingests a reading at `step` with integer channel values.
    async fn reading(&self, step: i64, channels: &[(String, i64)]) -> Result<(), Box<dyn Error>> {
        let measured_at = START + step * STEP;
        let observed_at = UtcTimestamp::from_epoch_millis(measured_at)?;
        let accepted = self
            .telemetry
            .ingest(NormalizeObservation {
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
                channel_readings: channels
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.clone(),
                            ChannelReading {
                                value: Some(DecimalValue::integer(*value)),
                                ..ChannelReading::default()
                            },
                        )
                    })
                    .collect(),
                supplements: ObservationSupplements::default(),
                source: ObservationSource {
                    kind: ObservationSourceKind::ModernApi,
                    source_reference: Some("logger".to_owned()),
                },
                idempotency_namespace: "discovery-test".to_owned(),
                idempotency_key: measured_at.to_string(),
                quality: QualityFlags::NONE,
            })
            .await?;
        assert_eq!(accepted.rejected_channels, Vec::new());
        Ok(())
    }

    async fn proposals(
        &self,
        include_dismissed: bool,
    ) -> Result<Vec<DiscoveryProposalResponse>, Box<dyn Error>> {
        self.discovery
            .list(self.system_id, include_dismissed)
            .await
            .map_err(failed)
    }

    /// Where a channel is bound: its target kind and target.
    async fn binding(&self, key: &str) -> Result<(String, Option<Uuid>), Box<dyn Error>> {
        let channel = self
            .channels
            .channel(self.system_id, key)
            .await?
            .ok_or_else(|| format!("channel {key} is missing"))?;
        Ok((channel.target_kind, channel.target_id))
    }
}

fn failed(error: DiscoveryApiError) -> Box<dyn Error> {
    format!("discovery request failed: {error:?}").into()
}

fn key(serial: &str, measurement: &str) -> String {
    format!("inv.{serial}.{measurement}")
}

fn string(tracker: Option<u8>, name: Option<&str>, capacity_watts: i64) -> AdoptedStringInput {
    AdoptedStringInput {
        tracker,
        name: name.map(ToOwned::to_owned),
        capacity_watts,
        orientation_degrees: Some(180),
        tilt_degrees: Some(30),
    }
}

#[allow(clippy::too_many_lines)]
async fn discovered_devices_are_proposed(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let system_id = fixture.system_id;
    assert_eq!(
        fixture.proposals(true).await?,
        [] as [pvlog_api::DiscoveryProposalResponse; 0]
    );

    // A logger reports an inverter nobody configured: three DC inputs, of
    // which the second delivers nothing, and tracker numbers with a gap.
    let roof = |measurement: &str| key(ROOF, measurement);
    fixture
        .reading(
            0,
            &[
                (roof("ac.l1.power"), 1_500),
                (roof("status"), 307),
                (roof("mppt.1.power"), 800),
                (roof("mppt.1.voltage"), 384_120),
                (roof("mppt.2.power"), 0),
                (roof("mppt.5.power"), 700),
                ("meter".to_owned(), 230),
            ],
        )
        .await?;
    let proposals = fixture.proposals(false).await?;
    assert_eq!(proposals.len(), 1);
    let proposal = &proposals[0];
    assert_eq!(proposal.device_serial, ROOF);
    assert_eq!(proposal.state, "open");
    assert_eq!(
        proposal.trackers,
        vec![1, 5],
        "tracker 2 never reported power"
    );
    assert_eq!(proposal.channel_count, 6);
    assert_eq!(proposal.first_seen_at_epoch_millis, START);
    assert!(
        fixture
            .configuration
            .effective_inverters(system_id, START)
            .await?
            .is_empty(),
        "a proposal creates no equipment"
    );

    // More readings change nothing about it; a second device gets its own.
    fixture
        .reading(
            1,
            &[
                (roof("mppt.2.power"), 0),
                (key(GARAGE, "ac.l1.power"), 400),
                (key(GARAGE, "mppt.1.power"), 420),
            ],
        )
        .await?;
    let proposals = fixture.proposals(false).await?;
    assert_eq!(
        proposals
            .iter()
            .map(|proposal| (proposal.device_serial.as_str(), proposal.trackers.clone()))
            .collect::<Vec<_>>(),
        vec![(ROOF, vec![1, 5]), (GARAGE, vec![1])]
    );
    let (roof_proposal, garage_proposal) = (proposals[0].id, proposals[1].id);

    // A dismissed device is not proposed again, whatever it reports, until
    // the dismissal is reset.
    fixture
        .discovery
        .dismiss(system_id, garage_proposal)
        .await
        .map_err(failed)?;
    fixture
        .reading(2, &[(key(GARAGE, "mppt.2.power"), 300)])
        .await?;
    assert_eq!(fixture.proposals(false).await?.len(), 1);
    let with_dismissed = fixture.proposals(true).await?;
    assert_eq!(with_dismissed.len(), 2);
    assert_eq!(with_dismissed[1].state, "dismissed");
    assert_eq!(with_dismissed[1].trackers, vec![1, 2]);
    assert_eq!(
        fixture.discovery.dismiss(system_id, garage_proposal).await,
        Err(DiscoveryApiError::Conflict("proposal_not_open"))
    );
    assert_eq!(
        fixture.discovery.reset(system_id, roof_proposal).await,
        Err(DiscoveryApiError::Conflict("proposal_not_dismissed"))
    );
    fixture
        .discovery
        .reset(system_id, garage_proposal)
        .await
        .map_err(failed)?;
    assert_eq!(fixture.proposals(false).await?.len(), 2);

    // A string can only be created for a tracker the device has.
    assert_eq!(
        fixture
            .discovery
            .adopt(
                system_id,
                roof_proposal,
                AdoptProposalInput {
                    name: None,
                    strings: vec![string(Some(9), None, 4_000)],
                },
            )
            .await,
        Err(DiscoveryApiError::InvalidInput("strings.tracker"))
    );
    assert_eq!(
        fixture
            .discovery
            .adopt(
                system_id,
                Uuid::now_v7(),
                AdoptProposalInput {
                    name: None,
                    strings: vec![string(Some(1), None, 4_000)],
                },
            )
            .await,
        Err(DiscoveryApiError::NotFound)
    );

    // A channel the owner bound by hand keeps its binding through adoption.
    fixture
        .channels
        .bind_channel(
            system_id,
            &roof("status"),
            &ChannelBindingRecord {
                target_kind: "plant".to_owned(),
                target_id: None,
                display_name: None,
                aggregation: None,
            },
            START,
        )
        .await?;

    // Adopting creates the inverter with one string per reporting tracker,
    // which keep their numbers, and binds the device's channels.
    let adopted = fixture
        .discovery
        .adopt(
            system_id,
            roof_proposal,
            AdoptProposalInput {
                name: Some("Roof inverter".to_owned()),
                strings: vec![
                    string(Some(1), Some("East"), 4_400),
                    string(Some(5), None, 3_000),
                ],
            },
        )
        .await
        .map_err(failed)?;
    assert_eq!(adopted.bound_channels, 5);
    assert_eq!(
        adopted
            .strings
            .iter()
            .map(|string| (string.tracker, string.name.as_str()))
            .collect::<Vec<_>>(),
        vec![(Some(1), "East"), (Some(5), "MPPT 5")]
    );
    let inverters = fixture
        .configuration
        .effective_inverters(system_id, START)
        .await?;
    assert_eq!(inverters.len(), 1);
    let inverter = &inverters[0];
    assert_eq!(inverter.id.as_uuid(), adopted.inverter_id);
    assert_eq!(inverter.name, "Roof inverter");
    assert_eq!(inverter.serial_reference.as_deref(), Some(ROOF));
    assert_eq!(
        inverter
            .strings
            .iter()
            .map(|string| (string.name.as_str(), string.rated_power_watts))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([("East", 4_400), ("MPPT 5", 3_000)])
    );
    let (east, fifth) = (adopted.strings[0].string_id, adopted.strings[1].string_id);
    let on_inverter = ("inverter".to_owned(), Some(adopted.inverter_id));
    for (measurement, expected) in [
        ("mppt.1.power", ("string".to_owned(), Some(east))),
        ("mppt.1.voltage", ("string".to_owned(), Some(east))),
        ("mppt.5.power", ("string".to_owned(), Some(fifth))),
        // An input without a string of its own belongs to the inverter.
        ("mppt.2.power", on_inverter.clone()),
        ("ac.l1.power", on_inverter.clone()),
        ("status", ("plant".to_owned(), None)),
    ] {
        assert_eq!(
            fixture.binding(&roof(measurement)).await?,
            expected,
            "{measurement}"
        );
    }
    assert_eq!(
        fixture.binding("meter").await?,
        ("unassigned".to_owned(), None)
    );

    // The device is equipment now: it is no longer proposed, also not when it
    // reports something new, and it cannot be adopted twice.
    fixture.reading(3, &[(roof("temperature"), 41_000)]).await?;
    assert_eq!(
        fixture
            .proposals(true)
            .await?
            .iter()
            .map(|proposal| proposal.device_serial.as_str())
            .collect::<Vec<_>>(),
        vec![GARAGE]
    );
    assert_eq!(
        fixture
            .discovery
            .adopt(
                system_id,
                roof_proposal,
                AdoptProposalInput {
                    name: None,
                    strings: vec![string(Some(1), None, 4_000)],
                },
            )
            .await,
        Err(DiscoveryApiError::Conflict("proposal_not_open"))
    );

    // Once its inverter is removed, a device that reports again is proposed again.
    assert!(
        fixture
            .configuration
            .delete_inverter_aggregate(system_id, InverterId::from_uuid(adopted.inverter_id)?)
            .await?
    );
    fixture.reading(4, &[(roof("signal"), 870)]).await?;
    let reopened = fixture.proposals(false).await?;
    assert_eq!(
        reopened
            .iter()
            .map(|proposal| (proposal.device_serial.as_str(), proposal.state.as_str()))
            .collect::<Vec<_>>(),
        vec![(ROOF, "open"), (GARAGE, "open")]
    );
    assert_eq!(reopened[0].id, roof_proposal);

    // A device without a tracker can be adopted with a string of its own.
    let adopted = fixture
        .discovery
        .adopt(
            system_id,
            garage_proposal,
            AdoptProposalInput {
                name: None,
                strings: vec![string(None, None, 2_000)],
            },
        )
        .await
        .map_err(failed)?;
    assert_eq!(adopted.strings[0].name, "String 1");
    assert_eq!(
        fixture.binding(&key(GARAGE, "mppt.1.power")).await?,
        ("inverter".to_owned(), Some(adopted.inverter_id))
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_proposes_adopts_and_dismisses_discovered_devices() -> Result<(), Box<dyn Error>> {
    discovered_devices_are_proposed(Fixture::sqlite().await?).await
}

#[tokio::test]
async fn postgres_proposes_adopts_and_dismisses_discovered_devices_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    discovered_devices_are_proposed(Fixture::postgres(url).await?).await
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
            slug: format!("discovery-{account_id}"),
            display_name: "Discovery".to_owned(),
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
        name: "Discovery".to_owned(),
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

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(START).unwrap_or_else(|_| unreachable!())
    }
}
