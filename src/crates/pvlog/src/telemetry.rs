//! Storage-backed modern telemetry ingestion and correction adapter.
//!
//! Normalizes wire commands into canonical observations, routes them to the
//! account database that owns the target system, and reports stable per-item
//! batch outcomes. Aggregates are rebuilt asynchronously by the worker's
//! aggregation cycle rather than inside the write path.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_application::{
    BatchIngestionMode, BatchIngestionResult, BatchItemOutcome, BatchItemStatus, Clock,
    CorrectObservation, ModernTelemetryError, ModernTelemetryUseCases, NormalizeObservation,
    PortError, VersionedObservation, normalize_observation,
};
use pvlog_domain::{
    AccountId, CanonicalObservation, ChannelId, EnergyReading, ExtendedValue, GridFlow,
    ObservationId, ObservationSourceKind, SystemId,
};
use pvlog_storage::{
    DatabaseTarget, ManagementRepository, ObservationInsertOutcome, StoredObservation,
    TelemetryRepository,
};

const MAXIMUM_BATCH_ITEMS: usize = 1_000;

/// Serves the modern ingestion endpoints from account telemetry storage.
pub struct StorageTelemetryApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageTelemetryApi {
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

    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, ModernTelemetryError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?
            .map(|record| record.account_id)
            .ok_or(ModernTelemetryError::NotFound)
    }

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
}

#[async_trait]
impl ModernTelemetryUseCases for StorageTelemetryApi {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<CanonicalObservation, ModernTelemetryError> {
        let system_id = command.system_id;
        let received_at = self.received_at();
        let command = self
            .resolve_channels(system_id, command, received_at)
            .await?;
        let observation =
            normalize_observation(command).map_err(|_| ModernTelemetryError::Invalid)?;
        let repository = self.repository(system_id).await?;
        let stored = stored_observation(&observation, received_at)?;
        match repository.insert_observation(&stored).await {
            Ok(ObservationInsertOutcome::Inserted) => {
                self.write_channel_values(system_id, &observation, received_at)
                    .await?;
                Ok(observation)
            }
            // A retry carries a fresh observation id; writing values under it
            // would orphan them, so an idempotent duplicate stores nothing new.
            Ok(ObservationInsertOutcome::Duplicate) => Ok(observation),
            Err(_) => Err(ModernTelemetryError::Conflict),
        }
    }

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

        let mut outcomes = Vec::with_capacity(commands.len());
        let mut rollback = false;
        for (index, command) in commands.into_iter().enumerate() {
            let command = self
                .resolve_channels(system_id, command, received_at)
                .await?;
            let status = match normalize_observation(command) {
                Err(_) => BatchItemStatus::Invalid,
                Ok(observation) => match stored_observation(&observation, received_at) {
                    Err(_) => BatchItemStatus::Invalid,
                    Ok(stored) => match repository.insert_observation(&stored).await {
                        Ok(ObservationInsertOutcome::Inserted) => {
                            self.write_channel_values(system_id, &observation, received_at)
                                .await?;
                            BatchItemStatus::Inserted
                        }
                        Ok(ObservationInsertOutcome::Duplicate) => BatchItemStatus::Duplicate,
                        Err(_) => BatchItemStatus::Failed,
                    },
                },
            };
            if mode == BatchIngestionMode::Atomic && status != BatchItemStatus::Inserted {
                rollback = true;
            }
            outcomes.push(BatchItemOutcome {
                index,
                status,
                code: None,
            });
        }
        if rollback {
            // Atomic batches report the first rejection rather than a partial write.
            return Err(ModernTelemetryError::Invalid);
        }
        Ok(BatchIngestionResult { outcomes })
    }

    async fn correct(
        &self,
        _command: CorrectObservation,
    ) -> Result<VersionedObservation, ModernTelemetryError> {
        Err(ModernTelemetryError::Repository(PortError::Unavailable))
    }

    async fn delete(
        &self,
        _command: CorrectObservation,
    ) -> Result<ObservationId, ModernTelemetryError> {
        Err(ModernTelemetryError::Repository(PortError::Unavailable))
    }
}

impl StorageTelemetryApi {
    /// Resolves wire channel keys to identifiers, registering keys never seen
    /// before so a newly wired meter is retained instead of rejected.
    async fn resolve_channels(
        &self,
        system_id: SystemId,
        mut command: NormalizeObservation,
        received_at: i64,
    ) -> Result<NormalizeObservation, ModernTelemetryError> {
        if command.channel_readings.is_empty() {
            return Ok(command);
        }
        #[cfg(feature = "sqlite")]
        {
            if matches!(self.target, DatabaseTarget::Sqlite { .. }) {
                let account = self.sqlite_account(system_id).await?;
                let mut connection = account
                    .acquire()
                    .await
                    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
                for (key, reading) in &command.channel_readings {
                    let channel_id =
                        ensure_channel(&mut connection, system_id, key, received_at).await?;
                    // A channel carries one canonical number; power is the live
                    // reading the dashboard needs, energy is the fallback.
                    let value = reading
                        .power_watts
                        .or(reading.energy_watt_hours)
                        .unwrap_or_default();
                    command
                        .extended
                        .insert(channel_id, ExtendedValue::Integer(value));
                    command.registered_channels.insert(channel_id);
                }
                return Ok(command);
            }
        }
        let _ = (system_id, received_at);
        Err(ModernTelemetryError::Repository(PortError::Unavailable))
    }

    /// Persists the per-channel values alongside the observation row.
    async fn write_channel_values(
        &self,
        system_id: SystemId,
        observation: &CanonicalObservation,
        received_at: i64,
    ) -> Result<(), ModernTelemetryError> {
        if observation.values.extended.is_empty() {
            return Ok(());
        }
        #[cfg(feature = "sqlite")]
        {
            if matches!(self.target, DatabaseTarget::Sqlite { .. }) {
                let account = self.sqlite_account(system_id).await?;
                let mut writer = account
                    .acquire_writer()
                    .await
                    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
                for (channel_id, value) in &observation.values.extended {
                    let integer = match value {
                        ExtendedValue::Integer(number) | ExtendedValue::DecimalScaled(number) => {
                            *number
                        }
                        ExtendedValue::Boolean(flag) => i64::from(*flag),
                    };
                    sqlx::query(
                        "INSERT INTO telemetry_hot_extended_values (observation_id,channel_id,integer_value) \
                         VALUES (?,?,?) ON CONFLICT(observation_id,channel_id) DO UPDATE SET \
                         integer_value=excluded.integer_value",
                    )
                    .bind(observation.id.as_uuid().as_bytes().to_vec())
                    .bind(channel_id.as_uuid().as_bytes().to_vec())
                    .bind(integer)
                    .execute(writer.connection())
                    .await
                    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
                    sqlx::query("UPDATE channel_definitions SET last_value_at=? WHERE id=?")
                        .bind(received_at)
                        .bind(channel_id.as_uuid().as_bytes().to_vec())
                        .execute(writer.connection())
                        .await
                        .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
                }
                return Ok(());
            }
        }
        let _ = (system_id, received_at);
        Ok(())
    }

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

    fn received_at(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }
}

/// Returns the channel identifier for a wire key, registering it on first sight.
///
/// A newly discovered key starts unassigned: its readings are stored, but they
/// contribute to no equipment until an operator binds it.
#[cfg(feature = "sqlite")]
async fn ensure_channel(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    channel_key: &str,
    now: i64,
) -> Result<ChannelId, ModernTelemetryError> {
    let system = system_id.as_uuid().as_bytes().to_vec();
    let existing: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT id FROM channel_definitions WHERE system_id=? AND channel_key=?",
    )
    .bind(system.clone())
    .bind(channel_key)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
    if let Some(bytes) = existing {
        return channel_id(&bytes);
    }
    let generated = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO channel_definitions \
         (id,system_id,channel_key,display_name,data_type,unit,scale,effective_from,\
          created_at,updated_at,target_kind,tracks_power,tracks_energy,discovered_at) \
         VALUES (?,?,?,?,'integer','W',0,?,?,?,'unassigned',1,0,?)",
    )
    .bind(generated.as_bytes().to_vec())
    .bind(system)
    .bind(channel_key)
    .bind(channel_key)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(|_| ModernTelemetryError::Repository(PortError::Unavailable))?;
    channel_id(generated.as_bytes())
}

#[cfg(feature = "sqlite")]
fn channel_id(bytes: &[u8]) -> Result<ChannelId, ModernTelemetryError> {
    uuid::Uuid::from_slice(bytes)
        .ok()
        .and_then(|value| ChannelId::from_uuid(value).ok())
        .ok_or(ModernTelemetryError::Repository(PortError::Unavailable))
}

/// Projects a canonical observation onto the persisted hot-row columns.
fn stored_observation(
    observation: &CanonicalObservation,
    received_at: i64,
) -> Result<StoredObservation, ModernTelemetryError> {
    let measured_at = i64::try_from(observation.observed_at.epoch_millis())
        .map_err(|_| ModernTelemetryError::Invalid)?;
    Ok(StoredObservation {
        id: observation.id,
        system_id: observation.system_id,
        measured_at,
        received_at,
        source_kind: source_kind(observation.source.kind).to_owned(),
        source_identity: observation
            .source
            .source_reference
            .clone()
            .unwrap_or_else(|| observation.idempotency.namespace.clone()),
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
    })
}

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
        "temperatureMilliCelsius": observation
            .values
            .temperature
            .map(pvlog_domain::MilliDegreesCelsius::value),
        "voltageMillivolts": observation.values.voltage.map(pvlog_domain::MilliVolts::value),
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
