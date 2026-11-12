//! Storage-backed equipment discovery.
//!
//! Ingestion records a device whose channels follow the device key scheme and
//! whose serial matches no inverter. This module lists those proposals with
//! the DC inputs that have reported power, and turns an adopted proposal into
//! an inverter with its strings and channel bindings.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{
    AdoptProposalInput, AdoptedEquipmentResponse, AdoptedStringResponse, DiscoveryApiError,
    DiscoveryApiUseCases, DiscoveryProposalResponse,
};
use pvlog_application::{Clock, DeviceChannelTarget, device_channel};
use pvlog_domain::{AccountId, EquipmentValueProvenance, InverterId, StringId, SystemId};
use pvlog_storage::{
    AccountConfigurationRepository, AccountRepositoryError, ChannelBindingRecord, ChannelRecord,
    DatabaseTarget, DiscoveryProposalRecord, DiscoveryRepository, InverterRecord,
    ManagementRepository, PvStringRecord,
};
use uuid::Uuid;

/// The account-scoped repositories discovery works with.
struct Repositories {
    discovery: Box<dyn DiscoveryRepository>,
    configuration: Box<dyn AccountConfigurationRepository>,
}

/// Opens the discovery and configuration repositories of an account.
async fn repositories(target: &DatabaseTarget, account_id: AccountId) -> Option<Repositories> {
    match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => {
            #[cfg(feature = "sqlite")]
            {
                let account = pvlog_storage::SqliteAccountPoolRouter::new(
                    management_path.clone(),
                    accounts_dir.clone(),
                    pvlog_storage::SqliteAccountPoolConfig::default(),
                )
                .ok()?
                .route(account_id)
                .await
                .ok()?;
                Some(Repositories {
                    discovery: Box::new(pvlog_storage::SqliteDiscoveryRepository::new(
                        account.clone(),
                    )),
                    configuration: Box::new(
                        pvlog_storage::SqliteAccountConfigurationRepository::new(account),
                    ),
                })
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = (management_path, accounts_dir, account_id);
                None
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Some(Repositories {
                    discovery: Box::new(pvlog_storage::PostgresDiscoveryRepository::new(
                        url.clone(),
                        account_id,
                    )),
                    configuration: Box::new(
                        pvlog_storage::PostgresAccountConfigurationRepository::new(
                            url.clone(),
                            account_id,
                        ),
                    ),
                })
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = (url, account_id);
                None
            }
        }
    }
}

/// Opens the discovery repository of the account that owns a system.
///
/// Returns `None` when the system is unknown or its storage cannot be reached.
pub async fn discovery_repository(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
    system_id: SystemId,
) -> Option<Box<dyn DiscoveryRepository>> {
    let account_id = management
        .system_registry(system_id)
        .await
        .ok()??
        .account_id;
    Some(repositories(target, account_id).await?.discovery)
}

/// Serves the equipment discovery endpoints from account storage.
pub struct StorageDiscoveryApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageDiscoveryApi {
    /// Creates the discovery API over the given database target, management repository, and clock.
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target,
            management,
            clock,
        }
    }

    /// Opens the repositories of the system's account; an unregistered system is `NotFound`.
    async fn repositories(&self, system_id: SystemId) -> Result<Repositories, DiscoveryApiError> {
        let account_id = self
            .management
            .system_registry(system_id)
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?
            .ok_or(DiscoveryApiError::NotFound)?
            .account_id;
        repositories(&self.target, account_id)
            .await
            .ok_or(DiscoveryApiError::Unavailable)
    }

    /// Lists the active channels of a system.
    async fn channels(&self, system_id: SystemId) -> Result<Vec<ChannelRecord>, DiscoveryApiError> {
        crate::channels::channel_repository(&self.target, self.management.as_ref(), system_id)
            .await
            .ok_or(DiscoveryApiError::Unavailable)?
            .channels(system_id)
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)
    }

    /// Returns the clock's current time in epoch milliseconds.
    fn now(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }

    /// Reads one proposal of a system; an unknown one is `NotFound`.
    async fn proposal(
        repositories: &Repositories,
        system_id: SystemId,
        proposal_id: Uuid,
    ) -> Result<DiscoveryProposalRecord, DiscoveryApiError> {
        repositories
            .discovery
            .discovery_proposals(system_id)
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?
            .into_iter()
            .find(|proposal| proposal.id == proposal_id)
            .ok_or(DiscoveryApiError::NotFound)
    }

    /// Binds the unassigned channels of a device to its new equipment and
    /// returns how many were bound. A tracker's channels go to the string
    /// created for it, every other channel to the inverter.
    async fn bind_device_channels(
        &self,
        system_id: SystemId,
        device: &DeviceChannels<'_>,
        inverter_id: InverterId,
        strings: &[(Option<u8>, PvStringRecord)],
        now: i64,
    ) -> Result<u64, DiscoveryApiError> {
        let channel_repository =
            crate::channels::channel_repository(&self.target, self.management.as_ref(), system_id)
                .await
                .ok_or(DiscoveryApiError::Unavailable)?;
        let mut bound_channels = 0;
        for (channel, target) in &device.channels {
            if channel.target_kind != "unassigned" {
                continue;
            }
            let string = match target {
                DeviceChannelTarget::Tracker { tracker, .. } => strings
                    .iter()
                    .find(|(string_tracker, _)| *string_tracker == Some(*tracker))
                    .map(|(_, string)| string.id.as_uuid()),
                DeviceChannelTarget::Inverter { .. } => None,
            };
            channel_repository
                .bind_channel(
                    system_id,
                    &channel.channel_key,
                    &ChannelBindingRecord {
                        target_kind: if string.is_some() {
                            "string"
                        } else {
                            "inverter"
                        }
                        .to_owned(),
                        target_id: Some(string.unwrap_or_else(|| inverter_id.as_uuid())),
                        display_name: None,
                        aggregation: None,
                    },
                    now,
                )
                .await
                .map_err(|_| DiscoveryApiError::Unavailable)?;
            bound_channels += 1;
        }
        Ok(bound_channels)
    }

    /// Stores a decision that is only allowed from one state.
    async fn decide(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
        from: &str,
        to: &str,
    ) -> Result<(), DiscoveryApiError> {
        let repositories = self.repositories(system_id).await?;
        let proposal = Self::proposal(&repositories, system_id, proposal_id).await?;
        if proposal.state != from {
            return Err(DiscoveryApiError::Conflict(if from == "open" {
                "proposal_not_open"
            } else {
                "proposal_not_dismissed"
            }));
        }
        repositories
            .discovery
            .decide_discovery_proposal(system_id, proposal_id, to, None, self.now())
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?;
        Ok(())
    }
}

/// What the channels of one device say about it.
struct DeviceChannels<'a> {
    /// Every channel whose key names the device.
    channels: Vec<(&'a ChannelRecord, DeviceChannelTarget<'a>)>,
    /// Trackers the device has a channel for.
    observed: BTreeSet<u8>,
    /// Trackers whose power channel has reported something other than zero.
    reporting: BTreeSet<u8>,
}

/// Collects the channels that belong to the device with a serial.
fn device_channels<'a>(channels: &'a [ChannelRecord], serial: &str) -> DeviceChannels<'a> {
    let mut device = DeviceChannels {
        channels: Vec::new(),
        observed: BTreeSet::new(),
        reporting: BTreeSet::new(),
    };
    for channel in channels {
        let Some(parsed) = device_channel(&channel.channel_key) else {
            continue;
        };
        if parsed.target.serial() != serial {
            continue;
        }
        if let DeviceChannelTarget::Tracker { tracker, .. } = parsed.target {
            device.observed.insert(tracker);
            if parsed.tracker_power && channel.first_nonzero_at.is_some() {
                device.reporting.insert(tracker);
            }
        }
        device.channels.push((channel, parsed.target));
    }
    device
}

/// Builds the inverter an adoption creates, with each string next to the
/// tracker it was requested for.
fn adopted_equipment(
    system_id: SystemId,
    proposal: &DiscoveryProposalRecord,
    input: AdoptProposalInput,
    now: i64,
) -> (InverterRecord, Vec<(Option<u8>, PvStringRecord)>) {
    let inverter_id = InverterId::new();
    // The equipment is in effect since the device was first seen, so the
    // readings it already delivered fall into its period.
    let effective_from = proposal.first_seen_at.min(now);
    let strings: Vec<(Option<u8>, PvStringRecord)> = input
        .strings
        .into_iter()
        .enumerate()
        .map(|(index, string)| {
            let name = string.name.unwrap_or_else(|| {
                string.tracker.map_or_else(
                    || format!("String {}", index + 1),
                    |tracker| format!("MPPT {tracker}"),
                )
            });
            (
                string.tracker,
                PvStringRecord {
                    id: StringId::new(),
                    inverter_id,
                    name,
                    // The capacity describes the array as a whole until the
                    // owner names its modules.
                    panel_count: 1,
                    panel_manufacturer: None,
                    panel_model: None,
                    rated_power_watts: string.capacity_watts,
                    module_catalog_entry_id: None,
                    module_catalog_revision: None,
                    value_provenance: EquipmentValueProvenance::Manual,
                    module_specification_snapshot: None,
                    module_peak_power_watts: Some(string.capacity_watts),
                    total_peak_power_watts: Some(string.capacity_watts),
                    orientation_degrees: string.orientation_degrees,
                    tilt_degrees: string.tilt_degrees,
                    temperature_coefficient_ppm_per_celsius: None,
                    shading: Vec::new(),
                    effective_from,
                    effective_to: None,
                    created_at: now,
                    updated_at: now,
                },
            )
        })
        .collect();
    let record = InverterRecord {
        id: inverter_id,
        system_id,
        name: input
            .name
            .or_else(|| proposal.reported_name.clone())
            .unwrap_or_else(|| proposal.device_serial.clone()),
        manufacturer: None,
        model: proposal.model.clone(),
        serial_reference: Some(proposal.device_serial.clone()),
        rated_power_watts: None,
        catalog_entry_id: None,
        catalog_revision: None,
        value_provenance: EquipmentValueProvenance::Manual,
        specification_snapshot: None,
        effective_from,
        effective_to: None,
        created_at: now,
        updated_at: now,
        strings: strings.iter().map(|(_, string)| string.clone()).collect(),
    };
    (record, strings)
}

#[async_trait]
impl DiscoveryApiUseCases for StorageDiscoveryApi {
    /// Lists the proposals whose device still matches no inverter. An input
    /// whose power has always been zero or missing is not among the trackers.
    async fn list(
        &self,
        system_id: SystemId,
        include_dismissed: bool,
    ) -> Result<Vec<DiscoveryProposalResponse>, DiscoveryApiError> {
        let repositories = self.repositories(system_id).await?;
        let proposals = repositories
            .discovery
            .discovery_proposals(system_id)
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?;
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let serials = repositories
            .discovery
            .inverter_serials(system_id, self.now())
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?;
        let channels = self.channels(system_id).await?;
        Ok(proposals
            .into_iter()
            .filter(|proposal| {
                (proposal.state == "open" || (include_dismissed && proposal.state == "dismissed"))
                    && !serials.contains(&proposal.device_serial)
            })
            .map(|proposal| {
                let device = device_channels(&channels, &proposal.device_serial);
                DiscoveryProposalResponse {
                    id: proposal.id,
                    trackers: device.reporting.into_iter().collect(),
                    channel_count: u64::try_from(device.channels.len()).unwrap_or(u64::MAX),
                    device_serial: proposal.device_serial,
                    reported_name: proposal.reported_name,
                    model: proposal.model,
                    firmware: proposal.firmware,
                    state: proposal.state,
                    first_seen_at_epoch_millis: proposal.first_seen_at,
                    last_seen_at_epoch_millis: proposal.last_seen_at,
                }
            })
            .collect())
    }

    /// Creates the inverter with the requested strings, binds the device's
    /// unassigned channels and marks the proposal as adopted.
    ///
    /// A tracker's channels go to the string created for it; every other
    /// channel of the device goes to the inverter. A channel that was bound by
    /// hand keeps its binding.
    async fn adopt(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
        input: AdoptProposalInput,
    ) -> Result<AdoptedEquipmentResponse, DiscoveryApiError> {
        let now = self.now();
        let repositories = self.repositories(system_id).await?;
        let proposal = Self::proposal(&repositories, system_id, proposal_id).await?;
        if proposal.state != "open" {
            return Err(DiscoveryApiError::Conflict("proposal_not_open"));
        }
        if repositories
            .discovery
            .inverter_serials(system_id, now)
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?
            .contains(&proposal.device_serial)
        {
            return Err(DiscoveryApiError::Conflict("inverter_exists"));
        }
        let channels = self.channels(system_id).await?;
        let device = device_channels(&channels, &proposal.device_serial);
        if input
            .strings
            .iter()
            .filter_map(|string| string.tracker)
            .any(|tracker| !device.observed.contains(&tracker))
        {
            return Err(DiscoveryApiError::InvalidInput("strings.tracker"));
        }

        let (record, strings) = adopted_equipment(system_id, &proposal, input, now);
        let inverter_id = record.id;
        repositories
            .configuration
            .save_inverter_aggregate(&record)
            .await
            .map_err(|error| match error {
                AccountRepositoryError::InvalidRecord(_)
                | AccountRepositoryError::InvalidEffectiveRange => {
                    DiscoveryApiError::InvalidInput("strings")
                }
                _ => DiscoveryApiError::Unavailable,
            })?;

        let bound_channels = self
            .bind_device_channels(system_id, &device, inverter_id, &strings, now)
            .await?;
        repositories
            .discovery
            .decide_discovery_proposal(
                system_id,
                proposal_id,
                "adopted",
                Some(inverter_id.as_uuid()),
                now,
            )
            .await
            .map_err(|_| DiscoveryApiError::Unavailable)?;
        Ok(AdoptedEquipmentResponse {
            inverter_id: inverter_id.as_uuid(),
            strings: strings
                .into_iter()
                .map(|(tracker, string)| AdoptedStringResponse {
                    string_id: string.id.as_uuid(),
                    tracker,
                    name: string.name,
                })
                .collect(),
            bound_channels,
        })
    }

    /// Dismisses an open proposal so the device is not proposed again.
    async fn dismiss(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
    ) -> Result<(), DiscoveryApiError> {
        self.decide(system_id, proposal_id, "open", "dismissed")
            .await
    }

    /// Opens a dismissed proposal again.
    async fn reset(&self, system_id: SystemId, proposal_id: Uuid) -> Result<(), DiscoveryApiError> {
        self.decide(system_id, proposal_id, "dismissed", "open")
            .await
    }
}
