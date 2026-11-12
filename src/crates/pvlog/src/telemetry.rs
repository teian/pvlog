//! Storage-backed modern telemetry ingestion and correction adapter.
//!
//! Normalizes wire commands into canonical observations, routes them to the
//! account database that owns the target system, and reports stable per-item
//! batch outcomes. Aggregates are rebuilt asynchronously by the worker's
//! aggregation cycle rather than inside the write path.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_application::{
    BatchIngestionMode, BatchIngestionResult, BatchItemOutcome, BatchItemStatus,
    ChannelAggregation, ChannelTyping, ChannelValueType, Clock, CorrectObservation,
    CounterBaseline, IngestedObservation, IngestionNormalizationError, ModernTelemetryError,
    ModernTelemetryUseCases, NormalizeObservation, PortError, RejectedChannel,
    VersionedObservation, channel_typing_for, derive_counter_energy, normalize_observation,
    scaled_channel_value,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, CanonicalObservation, ChannelId, DataSourceId, EnergyReading,
    ExtendedValue, GridFlow, ObservationId, ObservationSourceKind, SystemId,
};
use pvlog_storage::{
    ChannelRecord, ChannelRegistration, ChannelRepository, DataSourceRepository, DatabaseTarget,
    ManagementRepository, ObservationInsertOutcome, StoredMeasurements, StoredObservation,
    TelemetryRepository, TelemetryRepositoryError,
};

const MAXIMUM_BATCH_ITEMS: usize = 1_000;

/// Serves the modern ingestion endpoints from account telemetry storage.
pub struct StorageTelemetryApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageTelemetryApi {
    /// Creates the telemetry API over the given storage target, management registry and clock.
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

    /// Looks up the account that owns a system, failing with `NotFound` for unknown systems and a repository error when the registry cannot be read.
    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, ModernTelemetryError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?
            .map(|record| record.account_id)
            .ok_or(ModernTelemetryError::NotFound)
    }

    /// Opens the telemetry repository of the system's account for the configured storage target.
    async fn repository(
        &self,
        system_id: SystemId,
    ) -> Result<Box<dyn TelemetryRepository>, ModernTelemetryError> {
        let account_id = self.account_for(system_id).await?;
        match &self.target {
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
                    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?
                    .route(account_id)
                    .await
                    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
                    Ok(Box::new(pvlog_storage::SqliteTelemetryRepository::new(
                        account,
                    )))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir);
                    Err(ModernTelemetryError::Repository(PortError::Unavailable))
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Ok(Box::new(pvlog_storage::PostgresTelemetryRepository::new(
                        url.clone(),
                        account_id,
                    )))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = url;
                    Err(ModernTelemetryError::Repository(PortError::Unavailable))
                }
            }
        }
    }

    /// Opens the data source repository of the system's account.
    async fn data_source_repository(
        &self,
        system_id: SystemId,
    ) -> Result<Box<dyn DataSourceRepository>, ModernTelemetryError> {
        let unavailable = || ModernTelemetryError::Repository(PortError::Unavailable);
        match &self.target {
            DatabaseTarget::Sqlite { .. } => {
                #[cfg(feature = "sqlite")]
                {
                    Ok(Box::new(pvlog_storage::SqliteDataSourceRepository::new(
                        self.sqlite_account(system_id).await?,
                    )))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    Err(unavailable())
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let _ = unavailable;
                    Ok(Box::new(pvlog_storage::PostgresDataSourceRepository::new(
                        url.clone(),
                        self.account_for(system_id).await?,
                    )))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = url;
                    Err(unavailable())
                }
            }
        }
    }

    /// Opens the measurement channel repository of the system's account.
    async fn channel_repository(
        &self,
        system_id: SystemId,
    ) -> Result<Box<dyn ChannelRepository>, ModernTelemetryError> {
        crate::channels::channel_repository(&self.target, self.management.as_ref(), system_id)
            .await
            .ok_or(ModernTelemetryError::Repository(PortError::Unavailable))
    }

    /// Keeps a test reading where it cannot be mistaken for telemetry.
    async fn store_test_reading(
        &self,
        system_id: SystemId,
        data_source_id: Option<DataSourceId>,
        observation: &CanonicalObservation,
        received_at: i64,
    ) -> Result<(), ModernTelemetryError> {
        let unavailable = || ModernTelemetryError::Repository(PortError::Unavailable);
        self.data_source_repository(system_id)
            .await?
            .save_test_reading(&pvlog_storage::TestReadingRecord {
                id: observation.id.as_uuid(),
                system_id,
                data_source_id,
                measured_at: i64::try_from(observation.observed_at.epoch_millis())
                    .map_err(|_| ModernTelemetryError::Invalid)?,
                received_at,
                values: serde_json::to_value(&observation.values).map_err(|_| unavailable())?,
            })
            .await
            .map_err(|_| unavailable())
    }

    /// Finds the data source a credential belongs to, so its readings are
    /// attributed to it. A reading from a signed-in user or from a credential
    /// that belongs to no data source has none.
    async fn data_source_for(
        &self,
        system_id: SystemId,
        credential_id: Option<ApiCredentialId>,
    ) -> Result<Option<DataSourceId>, ModernTelemetryError> {
        let Some(credential_id) = credential_id else {
            return Ok(None);
        };
        let unavailable = || ModernTelemetryError::Repository(PortError::Unavailable);
        let repository = self.data_source_repository(system_id).await?;
        Ok(repository
            .data_source_by_credential(credential_id)
            .await
            .map_err(|_| unavailable())?
            // A credential is restricted to its system; a source of another
            // system must never be credited.
            .filter(|record| record.system_id == system_id)
            .map(|record| record.id))
    }
}

#[async_trait]
impl ModernTelemetryUseCases for StorageTelemetryApi {
    /// Normalizes and stores one observation with its channel values; an idempotent duplicate stores nothing new and a storage failure is reported as a conflict.
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<IngestedObservation, ModernTelemetryError> {
        let system_id = command.system_id;
        let received_at = self.received_at();
        let data_source = self
            .data_source_for(system_id, command.credential_id)
            .await?;
        if command.test {
            // Checked like a real reading, but kept apart: it registers no
            // channel and enters no total.
            let observation = normalize_observation(command).map_err(invalid)?;
            self.store_test_reading(system_id, data_source, &observation, received_at)
                .await?;
            return Ok(observation.into());
        }
        let channels = self.channel_repository(system_id).await?;
        let (command, rejected_channels) =
            resolve_channels(channels.as_ref(), system_id, command, received_at).await?;
        let mut observation = normalize_observation(command).map_err(invalid)?;
        let repository = self.repository(system_id).await?;
        apply_counters(repository.as_ref(), &mut observation, data_source, &[]).await?;
        let stored = stored_observation(&observation, received_at, data_source)?;
        match repository.insert_observation(&stored).await {
            Ok(ObservationInsertOutcome::Inserted) => {
                write_channel_values(channels.as_ref(), &observation, received_at).await?;
            }
            // A retry carries a fresh observation id; writing values under it
            // would orphan them, so an idempotent duplicate stores nothing new.
            Ok(ObservationInsertOutcome::Duplicate) => {}
            Err(_) => return Err(ModernTelemetryError::Conflict),
        }
        Ok(IngestedObservation {
            observation,
            rejected_channels,
        })
    }

    /// Ingests a single-system batch of up to the maximum item count, reporting a per-item status; in atomic mode any rejected or non-inserted item fails the batch as invalid and nothing is stored.
    async fn ingest_batch(
        &self,
        commands: Vec<NormalizeObservation>,
        mode: BatchIngestionMode,
    ) -> Result<BatchIngestionResult, ModernTelemetryError> {
        if commands.is_empty() || commands.len() > MAXIMUM_BATCH_ITEMS {
            return Err(ModernTelemetryError::Invalid);
        }
        let Some(system_id) = commands.first().map(|command| command.system_id) else {
            return Err(ModernTelemetryError::Invalid);
        };
        if commands
            .iter()
            .any(|command| command.system_id != system_id)
        {
            return Err(ModernTelemetryError::Invalid);
        }
        let repository = self.repository(system_id).await?;
        let received_at = self.received_at();
        // One request is made with one credential, so the source is the same
        // for every item.
        let data_source = self
            .data_source_for(
                system_id,
                commands.first().and_then(|command| command.credential_id),
            )
            .await?;

        if mode == BatchIngestionMode::Atomic {
            return self
                .ingest_atomic(
                    system_id,
                    commands,
                    received_at,
                    data_source,
                    repository.as_ref(),
                )
                .await;
        }

        let channels = self.channel_repository(system_id).await?;
        let mut outcomes = Vec::with_capacity(commands.len());
        for (index, command) in commands.into_iter().enumerate() {
            let (command, rejected_channels) =
                resolve_channels(channels.as_ref(), system_id, command, received_at).await?;
            let status = match normalize_observation(command) {
                Err(_) => BatchItemStatus::Invalid,
                Ok(mut observation) => {
                    apply_counters(repository.as_ref(), &mut observation, data_source, &[]).await?;
                    match stored_observation(&observation, received_at, data_source) {
                        Err(_) => BatchItemStatus::Invalid,
                        Ok(stored) => match repository.insert_observation(&stored).await {
                            Ok(ObservationInsertOutcome::Inserted) => {
                                write_channel_values(channels.as_ref(), &observation, received_at)
                                    .await?;
                                BatchItemStatus::Inserted
                            }
                            Ok(ObservationInsertOutcome::Duplicate) => BatchItemStatus::Duplicate,
                            Err(_) => BatchItemStatus::Failed,
                        },
                    }
                }
            };
            outcomes.push(BatchItemOutcome {
                index,
                status,
                code: None,
                rejected_channels,
            });
        }
        Ok(BatchIngestionResult { outcomes })
    }

    /// Not supported by this storage-backed implementation; always fails with a repository `Unavailable` error.
    async fn correct(
        &self,
        _command: CorrectObservation,
    ) -> Result<VersionedObservation, ModernTelemetryError> {
        Err(ModernTelemetryError::Repository(PortError::Unavailable))
    }

    /// Not supported by this storage-backed implementation; always fails with a repository `Unavailable` error.
    async fn delete(
        &self,
        _command: CorrectObservation,
    ) -> Result<ObservationId, ModernTelemetryError> {
        Err(ModernTelemetryError::Repository(PortError::Unavailable))
    }
}

impl StorageTelemetryApi {
    /// Validates every item first, then stores them in one transaction so an atomic
    /// batch is either written completely or not at all.
    async fn ingest_atomic(
        &self,
        system_id: SystemId,
        commands: Vec<NormalizeObservation>,
        received_at: i64,
        data_source: Option<DataSourceId>,
        repository: &dyn TelemetryRepository,
    ) -> Result<BatchIngestionResult, ModernTelemetryError> {
        let channels = self.channel_repository(system_id).await?;
        let mut observations = Vec::with_capacity(commands.len());
        let mut rejected = Vec::with_capacity(commands.len());
        let mut stored = Vec::with_capacity(commands.len());
        for command in commands {
            let (command, rejected_channels) =
                resolve_channels(channels.as_ref(), system_id, command, received_at).await?;
            rejected.push(rejected_channels);
            let mut observation = normalize_observation(command).map_err(invalid)?;
            // Nothing of this batch is stored yet, so an earlier item is the
            // predecessor of a later one only through this list.
            apply_counters(repository, &mut observation, data_source, &stored).await?;
            stored.push(
                stored_observation(&observation, received_at, data_source)
                    .map_err(|_| ModernTelemetryError::Invalid)?,
            );
            observations.push(observation);
        }
        let outcomes = repository
            .insert_observations_atomic(&stored)
            .await
            .map_err(|error| match error {
                TelemetryRepositoryError::InvalidRecord(_)
                | TelemetryRepositoryError::UniquenessConflict
                | TelemetryRepositoryError::ConstraintConflict => ModernTelemetryError::Invalid,
                _ => ModernTelemetryError::Repository(PortError::Unavailable),
            })?;
        if outcomes
            .iter()
            .any(|outcome| *outcome != ObservationInsertOutcome::Inserted)
        {
            return Err(ModernTelemetryError::Invalid);
        }
        for observation in &observations {
            write_channel_values(channels.as_ref(), observation, received_at).await?;
        }
        Ok(BatchIngestionResult {
            outcomes: rejected
                .into_iter()
                .enumerate()
                .map(|(index, rejected_channels)| BatchItemOutcome {
                    index,
                    status: BatchItemStatus::Inserted,
                    code: None,
                    rejected_channels,
                })
                .collect(),
        })
    }
}

impl StorageTelemetryApi {
    /// Opens the routed `SQLite` account database for a system, failing when the target is not `SQLite` or routing fails.
    #[cfg(feature = "sqlite")]
    async fn sqlite_account(
        &self,
        system_id: SystemId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, ModernTelemetryError> {
        let DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } = &self.target
        else {
            return Err(ModernTelemetryError::Repository(PortError::Unavailable));
        };
        let account_id = self.account_for(system_id).await?;
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.clone(),
            accounts_dir.clone(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?
        .route(account_id)
        .await
        .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))
    }

    /// Returns the current clock time as epoch milliseconds, or zero if it does not fit in an `i64`.
    fn received_at(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }
}

/// Resolves wire channel keys to channels and their values to scaled integers.
///
/// A key never seen before is registered, so a newly wired meter is retained
/// instead of rejected. A value that contradicts its channel's definition is
/// left out and reported; the rest of the reading goes on.
async fn resolve_channels(
    repository: &dyn ChannelRepository,
    system_id: SystemId,
    mut command: NormalizeObservation,
    received_at: i64,
) -> Result<(NormalizeObservation, Vec<RejectedChannel>), ModernTelemetryError> {
    if command.channel_readings.is_empty() {
        return Ok((command, Vec::new()));
    }
    let unavailable = || ModernTelemetryError::Repository(PortError::Unavailable);
    let registrations: Vec<ChannelRegistration> = command
        .channel_readings
        .iter()
        .map(|(key, reading)| {
            let typing = channel_typing_for(key, reading);
            ChannelRegistration {
                channel_key: key.clone(),
                display_name: key.clone(),
                data_type: typing.value_type.name().to_owned(),
                unit: typing.unit,
                scale: i32::from(typing.scale),
                aggregation: typing.aggregation.name().to_owned(),
            }
        })
        .collect();
    let channels = repository
        .register_channels(system_id, &registrations, received_at)
        .await
        .map_err(|_| unavailable())?;
    let mut rejected = Vec::new();
    for ((key, reading), channel) in command.channel_readings.iter().zip(&channels) {
        let typing = channel_typing(channel).ok_or_else(unavailable)?;
        match scaled_channel_value(&typing, reading) {
            Ok(value) => {
                let value = if typing.scale == 0 {
                    ExtendedValue::Integer(value)
                } else {
                    ExtendedValue::DecimalScaled(value)
                };
                command.extended.insert(channel.id, value);
                command.registered_channels.insert(channel.id);
            }
            Err(error) => rejected.push(RejectedChannel {
                channel_key: key.clone(),
                reason: error.code(),
            }),
        }
    }
    Ok((command, rejected))
}

/// Reads the stored definition of a channel.
fn channel_typing(channel: &ChannelRecord) -> Option<ChannelTyping> {
    Some(ChannelTyping {
        value_type: ChannelValueType::from_name(&channel.data_type)?,
        unit: channel.unit.clone(),
        scale: u8::try_from(channel.scale).ok()?,
        aggregation: ChannelAggregation::from_name(&channel.aggregation)?,
    })
}

/// Persists the per-channel values alongside the observation row.
async fn write_channel_values(
    repository: &dyn ChannelRepository,
    observation: &CanonicalObservation,
    received_at: i64,
) -> Result<(), ModernTelemetryError> {
    if observation.values.extended.is_empty() {
        return Ok(());
    }
    let values: Vec<(ChannelId, i64)> = observation
        .values
        .extended
        .iter()
        .map(|(channel, value)| {
            (
                *channel,
                match value {
                    ExtendedValue::Integer(number) | ExtendedValue::DecimalScaled(number) => {
                        *number
                    }
                    ExtendedValue::Boolean(flag) => i64::from(*flag),
                },
            )
        })
        .collect();
    let measured_at = i64::try_from(observation.observed_at.epoch_millis())
        .map_err(|_| ModernTelemetryError::Invalid)?;
    repository
        .write_channel_values(observation.id, measured_at, &values, received_at)
        .await
        .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))
}

/// Projects a canonical observation onto the persisted hot-row columns.
fn stored_observation(
    observation: &CanonicalObservation,
    received_at: i64,
    data_source_id: Option<DataSourceId>,
) -> Result<StoredObservation, ModernTelemetryError> {
    let measured_at = i64::try_from(observation.observed_at.epoch_millis())
        .map_err(|_| ModernTelemetryError::Invalid)?;
    let grid_import = observation
        .values
        .grid_import_power
        .map(pvlog_domain::Watts::value);
    let grid_export = observation
        .values
        .grid_export_power
        .map(pvlog_domain::Watts::value);
    Ok(StoredObservation {
        id: observation.id,
        system_id: observation.system_id,
        measured_at,
        received_at,
        source_kind: source_kind(observation.source.kind).to_owned(),
        source_identity: source_identity(observation),
        idempotency_identity: Some(format!(
            "{}:{}",
            observation.idempotency.namespace, observation.idempotency.key
        )),
        quality_flags: i32::from(observation.quality.bits()),
        generation_power_watts: observation
            .values
            .generation_power
            .map(pvlog_domain::Watts::value),
        generation_energy_wh: observation.values.generation_energy.map(energy_value),
        consumption_power_watts: observation
            .values
            .consumption_power
            .map(pvlog_domain::Watts::value),
        consumption_energy_wh: observation.values.consumption_energy.map(energy_value),
        provenance: provenance(observation),
        canonical_hash: observation.idempotency.payload_hash,
        version: 1,
        data_source_id,
        measurements: StoredMeasurements {
            generation_lifetime_wh: observation
                .values
                .generation_lifetime
                .map(pvlog_domain::WattHours::value),
            consumption_lifetime_wh: observation
                .values
                .consumption_lifetime
                .map(pvlog_domain::WattHours::value),
            grid_import_power_watts: grid_import,
            grid_export_power_watts: grid_export,
            grid_import_energy_wh: observation
                .values
                .grid_import_energy
                .map(pvlog_domain::WattHours::value),
            grid_export_energy_wh: observation
                .values
                .grid_export_energy
                .map(pvlog_domain::WattHours::value),
            // The net flow is only known when both directions were reported.
            net_grid_power_watts: grid_import.zip(grid_export).map(|(i, e)| i - e),
            temperature_millidegrees_c: observation
                .values
                .temperature
                .map(|value| i64::from(value.value())),
            voltage_millivolts: observation
                .values
                .voltage
                .map(|value| i64::from(value.value())),
            message: observation.values.message.clone(),
            ..StoredMeasurements::default()
        },
    })
}

/// Maps a normalization failure onto the telemetry error, keeping the field it names.
fn invalid(error: IngestionNormalizationError) -> ModernTelemetryError {
    match error {
        IngestionNormalizationError::InvalidField(field) => {
            ModernTelemetryError::InvalidField(field)
        }
        _ => ModernTelemetryError::Invalid,
    }
}

/// The identity a reading is stored under when it has no data source.
fn source_identity(observation: &CanonicalObservation) -> String {
    observation
        .source
        .source_reference
        .clone()
        .unwrap_or_else(|| observation.idempotency.namespace.clone())
}

/// Derives interval energy from the lifetime counters of a reading.
///
/// The predecessor is the newest earlier counter of the same source, whether
/// it is already stored or still waiting in `pending` to be stored together
/// with this reading.
async fn apply_counters(
    repository: &dyn TelemetryRepository,
    observation: &mut CanonicalObservation,
    data_source: Option<DataSourceId>,
    pending: &[StoredObservation],
) -> Result<(), ModernTelemetryError> {
    if observation.values.generation_lifetime.is_none()
        && observation.values.consumption_lifetime.is_none()
    {
        return Ok(());
    }
    let measured_at = i64::try_from(observation.observed_at.epoch_millis())
        .map_err(|_| ModernTelemetryError::Invalid)?;
    let identity = source_identity(observation);
    let stored = repository
        .latest_counters(observation.system_id, data_source, &identity, measured_at)
        .await
        .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
    let same_source = |candidate: &&StoredObservation| {
        candidate.measured_at < measured_at
            && candidate.data_source_id == data_source
            && (data_source.is_some() || candidate.source_identity == identity)
    };
    let newest = |stored: Option<(i64, i64)>, counter: fn(&StoredObservation) -> Option<i64>| {
        pending
            .iter()
            .filter(same_source)
            .filter_map(|candidate| Some((candidate.measured_at, counter(candidate)?)))
            .chain(stored)
            .max_by_key(|(at, _)| *at)
            .map(|(_, value)| value)
    };
    derive_counter_energy(
        observation,
        CounterBaseline {
            generation_lifetime_wh: newest(stored.generation, |o| {
                o.measurements.generation_lifetime_wh
            }),
            consumption_lifetime_wh: newest(stored.consumption, |o| {
                o.measurements.consumption_lifetime_wh
            }),
        },
    );
    Ok(())
}

/// Returns the numeric value of an energy reading, whether interval or cumulative total.
const fn energy_value(reading: EnergyReading) -> i64 {
    match reading {
        EnergyReading::Interval(value) | EnergyReading::Cumulative { total: value, .. } => {
            value.value()
        }
    }
}

/// Retains the measurements the hot row cannot express as dedicated columns.
fn provenance(observation: &CanonicalObservation) -> serde_json::Value {
    let grid = observation.values.grid.map(|flow| match flow {
        GridFlow::Net { power, .. } => serde_json::json!({ "netPowerWatts": power.value() }),
        GridFlow::Split {
            import_power,
            export_power,
        } => serde_json::json!({
            "importPowerWatts": import_power.value(),
            "exportPowerWatts": export_power.value(),
        }),
    });
    serde_json::json!({
        "sourceKind": source_kind(observation.source.kind),
        "grid": grid,
    })
}

/// Maps the ingestion boundary category onto the persisted source enumeration.
const fn source_kind(kind: ObservationSourceKind) -> &'static str {
    match kind {
        ObservationSourceKind::ModernApi | ObservationSourceKind::CompatibilityApi => "modern_api",
        ObservationSourceKind::Import => "import",
        ObservationSourceKind::PollingConnector => "provider",
        ObservationSourceKind::Manual => "correction",
        ObservationSourceKind::Derived => "system",
    }
}
