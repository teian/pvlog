//! Account-storage adapter for the unified ingestion of a logger.
//!
//! Everything a logger sends is identified by what it describes: a reading by
//! its source and time, an energy sample by its device and time, a daily
//! summary by its source, device and date. Storing an item again changes
//! nothing; storing it with other values replaces what was stored. The marks
//! record how far each source has delivered.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};

use async_trait::async_trait;
use pvlog_domain::{AccountId, ChannelId, DataSourceId, SystemId};
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;
use crate::{SourceDailySummaryRecord, StoredObservation, TelemetryRepositoryError};

/// Source kind every reading of the ingestion API is stored under.
pub const INGESTION_SOURCE_KIND: &str = "modern_api";

/// Value of the provenance member `origin` of a reading that was built from
/// archive energy samples because no live measurement exists for its time.
pub const ENERGY_SAMPLE_ORIGIN: &str = "energy_samples";

/// Rows written by one multi-row statement.
const ROWS_PER_STATEMENT: usize = 500;

/// One reading to store together with the values of its measurement channels.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationUpsert {
    pub observation: StoredObservation,
    /// Scaled values of the measurement channels of this reading.
    pub channel_values: Vec<(ChannelId, i64)>,
    /// True for a reading that only fills a gap between live measurements. It
    /// replaces an earlier gap filler, but never a reading measured live.
    pub fills_gap: bool,
}

/// What became of a reading that was handed to [`IngestRepository::upsert_observations`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpsertOutcome {
    /// No reading of the source existed for the time.
    Inserted,
    /// The stored reading has the same content.
    Unchanged,
    /// The stored reading had other content and now has this one.
    Replaced,
    /// A live reading is stored for the time and the gap filler was not needed.
    Kept,
}

/// The lifetime counters the devices of a source reported at one time.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceCounters {
    pub measured_at: i64,
    /// Counter in watt-hours by the channel key it was stored under.
    pub lifetime_wh: BTreeMap<String, i64>,
}

/// A stored reading of a source together with the values of its measurement
/// channels, as needed to derive its interval energy again.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredReading {
    pub observation: StoredObservation,
    /// Scaled values of the measurement channels of the reading.
    pub channel_values: Vec<(ChannelId, i64)>,
    /// Lifetime counters of the devices, in watt-hours by the channel key they
    /// were stored under.
    pub device_lifetime_wh: BTreeMap<String, i64>,
}

/// One archive energy sample of a device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnergySampleRecord {
    pub system_id: SystemId,
    /// The device as the source names it; for an inverter its serial.
    pub device_key: String,
    pub measured_at: i64,
    pub energy_lifetime_wh: Option<i64>,
    pub power_watts: Option<i64>,
    pub data_source_id: Option<DataSourceId>,
    pub received_at: i64,
}

/// Which energy samples of a system to read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnergySampleFilter<'a> {
    /// Only samples of this device.
    pub device_key: Option<&'a str>,
    /// Only samples measured at or after this instant.
    pub from: Option<i64>,
    /// Only samples measured before this instant.
    pub to: Option<i64>,
    /// Largest number of samples to return.
    pub limit: u32,
}

/// How far a source has delivered for one device, or for the system when
/// `device_key` is empty. A member that is `None` leaves the stored mark as it is.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IngestionMarkRecord {
    pub device_key: String,
    pub measured_at: Option<i64>,
    pub sample_at: Option<i64>,
    /// Local date of the latest daily summary, as `YYYY-MM-DD`.
    pub summary_date: Option<String>,
    pub event_at: Option<i64>,
}

/// Persistence of what a logger delivers through the ingestion API.
#[async_trait]
pub trait IngestRepository: Send + Sync {
    /// Stores readings by source and time. All readings must belong to one
    /// system and one source. Either all of them are stored or none.
    async fn upsert_observations(
        &self,
        items: &[ObservationUpsert],
    ) -> Result<Vec<UpsertOutcome>, TelemetryRepositoryError>;
    /// Finds the lifetime counters the devices of a source reported last
    /// before `before`: those of its newest earlier reading that carries any.
    async fn device_counters_before(
        &self,
        system_id: SystemId,
        source_identity: &str,
        before: i64,
    ) -> Result<Option<DeviceCounters>, TelemetryRepositoryError>;
    /// Reads the readings of a source measured from `from` through `to` and
    /// the first one after `to`, oldest first.
    async fn readings_through(
        &self,
        system_id: SystemId,
        source_identity: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<StoredReading>, TelemetryRepositoryError>;
    /// Stores energy samples by device and time and returns how many were new
    /// or changed. Either all samples are stored or none.
    async fn save_energy_samples(
        &self,
        samples: &[EnergySampleRecord],
    ) -> Result<u64, TelemetryRepositoryError>;
    /// Reads the energy samples of a system, oldest first.
    async fn energy_samples(
        &self,
        system_id: SystemId,
        filter: EnergySampleFilter<'_>,
    ) -> Result<Vec<EnergySampleRecord>, TelemetryRepositoryError>;
    /// Stores daily summaries by source, device and date and returns how many
    /// were new or changed. Either all summaries are stored or none.
    async fn save_daily_summaries(
        &self,
        summaries: &[SourceDailySummaryRecord],
    ) -> Result<u64, TelemetryRepositoryError>;
    /// Moves the marks of a source forward; a mark never moves back.
    async fn advance_marks(
        &self,
        system_id: SystemId,
        source_key: &str,
        marks: &[IngestionMarkRecord],
    ) -> Result<(), TelemetryRepositoryError>;
    /// Reads the marks of a source, the system's first and then by device.
    async fn marks(
        &self,
        system_id: SystemId,
        source_key: &str,
    ) -> Result<Vec<IngestionMarkRecord>, TelemetryRepositoryError>;
}

/// What is stored for a reading of the same source and time.
struct ExistingObservation {
    id: Uuid,
    canonical_hash: Vec<u8>,
    fills_gap: bool,
}

/// What to do with a reading, given what is stored for its time.
enum Step {
    Insert,
    Replace(Uuid),
    Skip(UpsertOutcome),
}

fn step(item: &ObservationUpsert, existing: Option<&ExistingObservation>) -> Step {
    match existing {
        None => Step::Insert,
        Some(existing) if existing.canonical_hash == item.observation.canonical_hash => {
            Step::Skip(UpsertOutcome::Unchanged)
        }
        Some(existing) if item.fills_gap && !existing.fills_gap => Step::Skip(UpsertOutcome::Kept),
        Some(existing) => Step::Replace(existing.id),
    }
}

/// Checks that the readings can be stored as one batch of one source.
fn validate(items: &[ObservationUpsert]) -> Result<(), TelemetryRepositoryError> {
    let invalid = || TelemetryRepositoryError::InvalidRecord("observation");
    let Some(first) = items.first().map(|item| &item.observation) else {
        return Ok(());
    };
    if items.iter().any(|item| {
        let o = &item.observation;
        o.system_id != first.system_id
            || o.source_identity != first.source_identity
            || o.source_kind != INGESTION_SOURCE_KIND
            || o.source_identity.is_empty()
            || o.version <= 0
            || !(0..=65_535).contains(&o.quality_flags)
    }) {
        return Err(invalid());
    }
    Ok(())
}

/// The first and last measurement time of a batch.
fn time_range(items: &[ObservationUpsert]) -> (i64, i64) {
    let times = || items.iter().map(|item| item.observation.measured_at);
    (
        times().min().unwrap_or_default(),
        times().max().unwrap_or_default(),
    )
}

/// Joins stored readings with their channel values, given as
/// `(observation, channel, channel key, value)`.
fn stored_readings(
    observations: Vec<StoredObservation>,
    values: Vec<(Uuid, ChannelId, String, i64)>,
) -> Vec<StoredReading> {
    let mut by_observation: HashMap<Uuid, StoredReading> = HashMap::new();
    let order: Vec<Uuid> = observations.iter().map(|o| o.id.as_uuid()).collect();
    for observation in observations {
        by_observation.insert(
            observation.id.as_uuid(),
            StoredReading {
                observation,
                channel_values: Vec::new(),
                device_lifetime_wh: BTreeMap::new(),
            },
        );
    }
    for (observation, channel, key, value) in values {
        if let Some(reading) = by_observation.get_mut(&observation) {
            reading.channel_values.push((channel, value));
            if is_device_lifetime_key(&key) {
                reading.device_lifetime_wh.insert(key, value);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|id| by_observation.remove(&id))
        .collect()
}

/// Whether a channel key is the lifetime energy counter of a device, as the
/// pattern `inv.%.energy.lifetime` of the queries matches it.
fn is_device_lifetime_key(key: &str) -> bool {
    key.strip_prefix("inv.")
        .and_then(|rest| rest.strip_suffix(".energy.lifetime"))
        .is_some_and(|serial| !serial.is_empty())
}

/// The channels a batch wrote, each with whether any written value is not zero.
fn touched_channels(
    items: &[ObservationUpsert],
    outcomes: &[UpsertOutcome],
) -> Vec<(ChannelId, bool)> {
    let mut touched: BTreeMap<ChannelId, bool> = BTreeMap::new();
    for (item, outcome) in items.iter().zip(outcomes) {
        if matches!(outcome, UpsertOutcome::Inserted | UpsertOutcome::Replaced) {
            for (channel, value) in &item.channel_values {
                *touched.entry(*channel).or_default() |= *value != 0;
            }
        }
    }
    touched.into_iter().collect()
}

/// `SQLite` ingestion repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteIngestRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteIngestRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
async fn sqlite_update_hot(
    connection: &mut sqlx::SqliteConnection,
    id: Uuid,
    o: &StoredObservation,
) -> Result<(), TelemetryRepositoryError> {
    sqlx::query(
        "UPDATE telemetry_hot SET received_at=?,quality_flags=?,generation_power_watts=?,\
         generation_energy_wh=?,consumption_power_watts=?,consumption_energy_wh=?,\
         provenance_json=?,canonical_hash=?,version=version+1,data_source_id=?,\
         generation_lifetime_wh=?,consumption_lifetime_wh=?,grid_import_power_watts=?,\
         grid_import_energy_wh=?,grid_export_power_watts=?,grid_export_energy_wh=?,\
         net_grid_power_watts=?,battery_power_watts=?,battery_energy_wh=?,\
         battery_state_basis_points=?,temperature_millidegrees_c=?,voltage_millivolts=?,\
         message=? WHERE observation_id=?",
    )
    .bind(o.received_at)
    .bind(o.quality_flags)
    .bind(o.generation_power_watts)
    .bind(o.generation_energy_wh)
    .bind(o.consumption_power_watts)
    .bind(o.consumption_energy_wh)
    .bind(serde_json::to_string(&o.provenance)?)
    .bind(o.canonical_hash.as_slice())
    .bind(o.data_source_id.map(|id| blob(id.as_uuid())))
    .bind(o.measurements.generation_lifetime_wh)
    .bind(o.measurements.consumption_lifetime_wh)
    .bind(o.measurements.grid_import_power_watts)
    .bind(o.measurements.grid_import_energy_wh)
    .bind(o.measurements.grid_export_power_watts)
    .bind(o.measurements.grid_export_energy_wh)
    .bind(o.measurements.net_grid_power_watts)
    .bind(o.measurements.battery_power_watts)
    .bind(o.measurements.battery_energy_wh)
    .bind(o.measurements.battery_state_basis_points)
    .bind(o.measurements.temperature_millidegrees_c)
    .bind(o.measurements.voltage_millivolts)
    .bind(&o.measurements.message)
    .bind(blob(id))
    .execute(connection)
    .await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn sqlite_write_values(
    connection: &mut sqlx::SqliteConnection,
    observation_id: Uuid,
    values: &[(ChannelId, i64)],
) -> Result<(), TelemetryRepositoryError> {
    for rows in values.chunks(ROWS_PER_STATEMENT) {
        let mut statement = sqlx::QueryBuilder::new(
            "INSERT INTO telemetry_hot_extended_values (observation_id,channel_id,integer_value) ",
        );
        statement.push_values(rows, |mut row, (channel, value)| {
            row.push_bind(blob(observation_id))
                .push_bind(blob(channel.as_uuid()))
                .push_bind(*value);
        });
        statement.build().execute(&mut *connection).await?;
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl IngestRepository for SqliteIngestRepository {
    async fn upsert_observations(
        &self,
        items: &[ObservationUpsert],
    ) -> Result<Vec<UpsertOutcome>, TelemetryRepositoryError> {
        use sqlx::Connection as _;
        validate(items)?;
        let Some(first) = items.first().map(|item| &item.observation) else {
            return Ok(Vec::new());
        };
        let (from, to) = time_range(items);
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        let existing: HashMap<i64, ExistingObservation> = sqlx::query(
            "SELECT measured_at,observation_id,canonical_hash,\
             json_extract(provenance_json,'$.origin') AS origin FROM telemetry_hot \
             WHERE system_id=? AND source_kind=? AND source_identity=? \
             AND measured_at>=? AND measured_at<=?",
        )
        .bind(blob(first.system_id.as_uuid()))
        .bind(INGESTION_SOURCE_KIND)
        .bind(&first.source_identity)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *transaction)
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.get("measured_at"),
                ExistingObservation {
                    id: Uuid::from_slice(&row.get::<Vec<u8>, _>("observation_id"))
                        .map_err(|_| TelemetryRepositoryError::InvalidRecord("observation"))?,
                    canonical_hash: row.get("canonical_hash"),
                    fills_gap: row.get::<Option<String>, _>("origin").as_deref()
                        == Some(ENERGY_SAMPLE_ORIGIN),
                },
            ))
        })
        .collect::<Result<_, TelemetryRepositoryError>>()?;
        let mut outcomes = Vec::with_capacity(items.len());
        for item in items {
            let o = &item.observation;
            let (id, outcome) = match step(item, existing.get(&o.measured_at)) {
                Step::Skip(outcome) => {
                    outcomes.push(outcome);
                    continue;
                }
                Step::Insert => {
                    let inserted =
                        crate::telemetry_repository::sqlite_insert_hot(&mut transaction, o).await?;
                    if inserted.rows_affected() != 1 {
                        return Err(TelemetryRepositoryError::ConstraintConflict);
                    }
                    (o.id.as_uuid(), UpsertOutcome::Inserted)
                }
                Step::Replace(id) => {
                    sqlite_update_hot(&mut transaction, id, o).await?;
                    sqlx::query("DELETE FROM telemetry_hot_extended_values WHERE observation_id=?")
                        .bind(blob(id))
                        .execute(&mut *transaction)
                        .await?;
                    (id, UpsertOutcome::Replaced)
                }
            };
            sqlite_write_values(&mut transaction, id, &item.channel_values).await?;
            outcomes.push(outcome);
        }
        for (channel, nonzero) in touched_channels(items, &outcomes) {
            sqlx::query(
                "UPDATE channel_definitions SET last_value_at=?1,first_nonzero_at=\
                 COALESCE(first_nonzero_at,CASE WHEN ?2 THEN ?1 END) WHERE id=?3",
            )
            .bind(first.received_at)
            .bind(nonzero)
            .bind(blob(channel.as_uuid()))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(outcomes)
    }

    async fn device_counters_before(
        &self,
        system_id: SystemId,
        source_identity: &str,
        before: i64,
    ) -> Result<Option<DeviceCounters>, TelemetryRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let Some(measured_at) = sqlx::query_scalar::<_, i64>(
            "SELECT h.measured_at FROM telemetry_hot h WHERE h.system_id=? AND h.source_kind=? \
             AND h.source_identity=? AND h.measured_at<? AND EXISTS (\
               SELECT 1 FROM telemetry_hot_extended_values v \
               JOIN channel_definitions c ON c.id=v.channel_id \
               WHERE v.observation_id=h.observation_id \
               AND c.channel_key LIKE 'inv.%.energy.lifetime') \
             ORDER BY h.measured_at DESC LIMIT 1",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(before)
        .fetch_optional(&mut *connection)
        .await?
        else {
            return Ok(None);
        };
        let lifetime_wh = sqlx::query(
            "SELECT c.channel_key AS channel_key,v.integer_value AS value FROM telemetry_hot h \
             JOIN telemetry_hot_extended_values v ON v.observation_id=h.observation_id \
             JOIN channel_definitions c ON c.id=v.channel_id \
             WHERE h.system_id=? AND h.source_kind=? AND h.source_identity=? AND h.measured_at=? \
             AND c.channel_key LIKE 'inv.%.energy.lifetime'",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(measured_at)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| (row.get("channel_key"), row.get("value")))
        .collect();
        Ok(Some(DeviceCounters {
            measured_at,
            lifetime_wh,
        }))
    }

    async fn readings_through(
        &self,
        system_id: SystemId,
        source_identity: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<StoredReading>, TelemetryRepositoryError> {
        // The readings of the range and the first one after it.
        const RANGE: &str = "h.system_id=?1 AND h.source_kind=?2 AND h.source_identity=?3 \
             AND h.measured_at>=?4 AND h.measured_at<=(SELECT COALESCE(MIN(n.measured_at),?5) \
             FROM telemetry_hot n WHERE n.system_id=?1 AND n.source_kind=?2 \
             AND n.source_identity=?3 AND n.measured_at>?5)";
        let mut connection = self.account.acquire().await?;
        let observations = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT h.observation_id,h.system_id,h.measured_at,h.received_at,h.source_kind,\
             h.source_identity,h.idempotency_identity,h.quality_flags,h.generation_power_watts,\
             h.generation_energy_wh,h.consumption_power_watts,h.consumption_energy_wh,\
             h.provenance_json,h.canonical_hash,h.version,h.data_source_id,\
             h.generation_lifetime_wh,h.consumption_lifetime_wh,h.grid_import_power_watts,\
             h.grid_import_energy_wh,h.grid_export_power_watts,h.grid_export_energy_wh,\
             h.net_grid_power_watts,h.battery_power_watts,h.battery_energy_wh,\
             h.battery_state_basis_points,h.temperature_millidegrees_c,h.voltage_millivolts,\
             h.message FROM telemetry_hot h WHERE {RANGE} ORDER BY h.measured_at"
        )))
        .bind(blob(system_id.as_uuid()))
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(crate::telemetry_repository::sqlite_observation)
        .collect::<Result<Vec<_>, _>>()?;
        if observations.is_empty() {
            return Ok(Vec::new());
        }
        let invalid = || TelemetryRepositoryError::InvalidRecord("channel value");
        let values = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT v.observation_id AS observation_id,v.channel_id AS channel_id,\
             c.channel_key AS channel_key,v.integer_value AS value FROM telemetry_hot h \
             JOIN telemetry_hot_extended_values v ON v.observation_id=h.observation_id \
             JOIN channel_definitions c ON c.id=v.channel_id \
             WHERE {RANGE} AND v.integer_value IS NOT NULL"
        )))
        .bind(blob(system_id.as_uuid()))
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| {
            Ok((
                Uuid::from_slice(&row.get::<Vec<u8>, _>("observation_id"))
                    .map_err(|_| invalid())?,
                Uuid::from_slice(&row.get::<Vec<u8>, _>("channel_id"))
                    .ok()
                    .and_then(|id| ChannelId::from_uuid(id).ok())
                    .ok_or_else(invalid)?,
                row.get("channel_key"),
                row.get("value"),
            ))
        })
        .collect::<Result<Vec<_>, TelemetryRepositoryError>>()?;
        Ok(stored_readings(observations, values))
    }

    async fn save_energy_samples(
        &self,
        samples: &[EnergySampleRecord],
    ) -> Result<u64, TelemetryRepositoryError> {
        use sqlx::Connection as _;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        let mut stored = 0;
        for rows in samples.chunks(ROWS_PER_STATEMENT) {
            let mut statement = sqlx::QueryBuilder::new(
                "INSERT INTO device_energy_samples (system_id,device_key,measured_at,\
                 energy_lifetime_wh,power_watts,data_source_id,received_at) ",
            );
            statement.push_values(rows, |mut row, sample| {
                row.push_bind(blob(sample.system_id.as_uuid()))
                    .push_bind(&sample.device_key)
                    .push_bind(sample.measured_at)
                    .push_bind(sample.energy_lifetime_wh)
                    .push_bind(sample.power_watts)
                    .push_bind(sample.data_source_id.map(|id| blob(id.as_uuid())))
                    .push_bind(sample.received_at);
            });
            statement.push(
                " ON CONFLICT(system_id,device_key,measured_at) DO UPDATE SET \
                 energy_lifetime_wh=excluded.energy_lifetime_wh,power_watts=excluded.power_watts,\
                 data_source_id=excluded.data_source_id,received_at=excluded.received_at \
                 WHERE device_energy_samples.energy_lifetime_wh IS NOT excluded.energy_lifetime_wh \
                 OR device_energy_samples.power_watts IS NOT excluded.power_watts",
            );
            stored += statement
                .build()
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        }
        transaction.commit().await?;
        Ok(stored)
    }

    async fn energy_samples(
        &self,
        system_id: SystemId,
        filter: EnergySampleFilter<'_>,
    ) -> Result<Vec<EnergySampleRecord>, TelemetryRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT device_key,measured_at,energy_lifetime_wh,power_watts,data_source_id,\
             received_at FROM device_energy_samples WHERE system_id=?1 \
             AND (?2 IS NULL OR device_key=?2) AND measured_at>=?3 AND measured_at<?4 \
             ORDER BY measured_at,device_key LIMIT ?5",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(filter.device_key)
        .bind(filter.from.unwrap_or(i64::MIN))
        .bind(filter.to.unwrap_or(i64::MAX))
        .bind(i64::from(filter.limit))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| {
            Ok(EnergySampleRecord {
                system_id,
                device_key: row.get("device_key"),
                measured_at: row.get("measured_at"),
                energy_lifetime_wh: row.get("energy_lifetime_wh"),
                power_watts: row.get("power_watts"),
                data_source_id: row
                    .get::<Option<Vec<u8>>, _>("data_source_id")
                    .map(|bytes| {
                        Uuid::from_slice(&bytes)
                            .ok()
                            .and_then(|id| DataSourceId::from_uuid(id).ok())
                            .ok_or(TelemetryRepositoryError::InvalidRecord("energy sample"))
                    })
                    .transpose()?,
                received_at: row.get("received_at"),
            })
        })
        .collect()
    }

    async fn save_daily_summaries(
        &self,
        summaries: &[SourceDailySummaryRecord],
    ) -> Result<u64, TelemetryRepositoryError> {
        use sqlx::Connection as _;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        let mut stored = 0;
        for r in summaries {
            stored += sqlx::query(
                "INSERT INTO source_daily_summaries AS s (system_id,source_key,device_key,\
                 local_date,data_source_id,generation_energy_wh,export_energy_wh,\
                 consumption_energy_wh,generation_lifetime_wh,complete,peak_power_watts,\
                 peak_time,weather_condition,temperature_min_millidegrees_c,\
                 temperature_max_millidegrees_c,import_energy_by_band_json,received_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
                 ON CONFLICT(system_id,source_key,device_key,local_date) DO UPDATE SET \
                 data_source_id=excluded.data_source_id,\
                 generation_energy_wh=excluded.generation_energy_wh,\
                 export_energy_wh=excluded.export_energy_wh,\
                 consumption_energy_wh=excluded.consumption_energy_wh,\
                 generation_lifetime_wh=excluded.generation_lifetime_wh,\
                 complete=excluded.complete,peak_power_watts=excluded.peak_power_watts,\
                 peak_time=excluded.peak_time,weather_condition=excluded.weather_condition,\
                 temperature_min_millidegrees_c=excluded.temperature_min_millidegrees_c,\
                 temperature_max_millidegrees_c=excluded.temperature_max_millidegrees_c,\
                 import_energy_by_band_json=excluded.import_energy_by_band_json,\
                 received_at=excluded.received_at \
                 WHERE s.generation_energy_wh IS NOT excluded.generation_energy_wh \
                 OR s.export_energy_wh IS NOT excluded.export_energy_wh \
                 OR s.consumption_energy_wh IS NOT excluded.consumption_energy_wh \
                 OR s.generation_lifetime_wh IS NOT excluded.generation_lifetime_wh \
                 OR s.complete IS NOT excluded.complete \
                 OR s.peak_power_watts IS NOT excluded.peak_power_watts \
                 OR s.peak_time IS NOT excluded.peak_time \
                 OR s.weather_condition IS NOT excluded.weather_condition \
                 OR s.temperature_min_millidegrees_c IS NOT excluded.temperature_min_millidegrees_c \
                 OR s.temperature_max_millidegrees_c IS NOT excluded.temperature_max_millidegrees_c \
                 OR s.import_energy_by_band_json IS NOT excluded.import_energy_by_band_json",
            )
            .bind(blob(r.system_id.as_uuid()))
            .bind(&r.source_key)
            .bind(&r.device_key)
            .bind(&r.local_date)
            .bind(r.data_source_id.map(|id| blob(id.as_uuid())))
            .bind(r.generation_energy_wh)
            .bind(r.export_energy_wh)
            .bind(r.consumption_energy_wh)
            .bind(r.generation_lifetime_wh)
            .bind(r.complete)
            .bind(r.peak_power_watts)
            .bind(&r.peak_time)
            .bind(&r.weather_condition)
            .bind(r.temperature_min_millidegrees_c)
            .bind(r.temperature_max_millidegrees_c)
            .bind(serde_json::to_string(&r.import_energy_by_band)?)
            .bind(r.received_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        }
        transaction.commit().await?;
        Ok(stored)
    }

    async fn advance_marks(
        &self,
        system_id: SystemId,
        source_key: &str,
        marks: &[IngestionMarkRecord],
    ) -> Result<(), TelemetryRepositoryError> {
        use sqlx::Connection as _;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        for mark in marks {
            // MAX of two values is NULL when one of them is; COALESCE keeps
            // the side that exists.
            sqlx::query(
                "INSERT INTO ingestion_marks (system_id,source_key,device_key,measured_at,\
                 sample_at,summary_date,event_at) VALUES (?,?,?,?,?,?,?) \
                 ON CONFLICT(system_id,source_key,device_key) DO UPDATE SET \
                 measured_at=MAX(COALESCE(measured_at,excluded.measured_at),\
                   COALESCE(excluded.measured_at,measured_at)),\
                 sample_at=MAX(COALESCE(sample_at,excluded.sample_at),\
                   COALESCE(excluded.sample_at,sample_at)),\
                 summary_date=MAX(COALESCE(summary_date,excluded.summary_date),\
                   COALESCE(excluded.summary_date,summary_date)),\
                 event_at=MAX(COALESCE(event_at,excluded.event_at),\
                   COALESCE(excluded.event_at,event_at))",
            )
            .bind(blob(system_id.as_uuid()))
            .bind(source_key)
            .bind(&mark.device_key)
            .bind(mark.measured_at)
            .bind(mark.sample_at)
            .bind(mark.summary_date.as_deref())
            .bind(mark.event_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn marks(
        &self,
        system_id: SystemId,
        source_key: &str,
    ) -> Result<Vec<IngestionMarkRecord>, TelemetryRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(sqlx::query(
            "SELECT device_key,measured_at,sample_at,summary_date,event_at FROM ingestion_marks \
             WHERE system_id=? AND source_key=? ORDER BY device_key",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(source_key)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(mark)
        .collect())
    }
}

/// Reads a mark from a row; both databases use the same column names.
fn mark<'r, R>(row: &'r R) -> IngestionMarkRecord
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    Option<String>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    Option<i64>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    IngestionMarkRecord {
        device_key: row.get("device_key"),
        measured_at: row.get("measured_at"),
        sample_at: row.get("sample_at"),
        summary_date: row.get("summary_date"),
        event_at: row.get("event_at"),
    }
}

/// `PostgreSQL` ingestion repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresIngestRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresIngestRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresIngestRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresIngestRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "postgres")]
async fn pg_update_hot(
    connection: &mut PgConnection,
    account_id: AccountId,
    id: Uuid,
    o: &StoredObservation,
) -> Result<(), TelemetryRepositoryError> {
    sqlx::query(
        "UPDATE telemetry.hot_observations SET received_at=$1,quality_flags=$2,\
         generation_power_watts=$3,generation_energy_wh=$4,consumption_power_watts=$5,\
         consumption_energy_wh=$6,provenance=$7,canonical_hash=$8,version=version+1,\
         data_source_id=$9,generation_lifetime_wh=$10,consumption_lifetime_wh=$11,\
         grid_import_power_watts=$12,grid_import_energy_wh=$13,grid_export_power_watts=$14,\
         grid_export_energy_wh=$15,net_grid_power_watts=$16,battery_power_watts=$17,\
         battery_energy_wh=$18,battery_state_basis_points=$19,temperature_millidegrees_c=$20,\
         voltage_millivolts=$21,message=$22 \
         WHERE account_id=$23 AND observation_id=$24 AND measured_at=$25",
    )
    .bind(o.received_at)
    .bind(o.quality_flags)
    .bind(o.generation_power_watts)
    .bind(o.generation_energy_wh)
    .bind(o.consumption_power_watts)
    .bind(o.consumption_energy_wh)
    .bind(&o.provenance)
    .bind(o.canonical_hash.as_slice())
    .bind(o.data_source_id.map(DataSourceId::as_uuid))
    .bind(o.measurements.generation_lifetime_wh)
    .bind(o.measurements.consumption_lifetime_wh)
    .bind(o.measurements.grid_import_power_watts)
    .bind(o.measurements.grid_import_energy_wh)
    .bind(o.measurements.grid_export_power_watts)
    .bind(o.measurements.grid_export_energy_wh)
    .bind(o.measurements.net_grid_power_watts)
    .bind(o.measurements.battery_power_watts)
    .bind(o.measurements.battery_energy_wh)
    .bind(o.measurements.battery_state_basis_points)
    .bind(o.measurements.temperature_millidegrees_c)
    .bind(o.measurements.voltage_millivolts)
    .bind(&o.measurements.message)
    .bind(account_id.as_uuid())
    .bind(id)
    .bind(o.measured_at)
    .execute(connection)
    .await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn pg_write_values(
    connection: &mut PgConnection,
    account_id: AccountId,
    observation_id: Uuid,
    measured_at: i64,
    values: &[(ChannelId, i64)],
) -> Result<(), TelemetryRepositoryError> {
    for rows in values.chunks(ROWS_PER_STATEMENT) {
        let mut statement = sqlx::QueryBuilder::new(
            "INSERT INTO telemetry.hot_extended_values \
             (account_id,observation_id,measured_at,channel_id,integer_value) ",
        );
        statement.push_values(rows, |mut row, (channel, value)| {
            row.push_bind(account_id.as_uuid())
                .push_bind(observation_id)
                .push_bind(measured_at)
                .push_bind(channel.as_uuid())
                .push_bind(*value);
        });
        statement.build().execute(&mut *connection).await?;
    }
    Ok(())
}

#[cfg(feature = "postgres")]
#[async_trait]
impl IngestRepository for PostgresIngestRepository {
    #[allow(clippy::too_many_lines)]
    async fn upsert_observations(
        &self,
        items: &[ObservationUpsert],
    ) -> Result<Vec<UpsertOutcome>, TelemetryRepositoryError> {
        validate(items)?;
        let Some(first) = items.first().map(|item| &item.observation) else {
            return Ok(Vec::new());
        };
        let (from, to) = time_range(items);
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        // History reaches further back than the months the table is prepared
        // for, so the months of this batch are created when they are missing.
        sqlx::query(
            "SELECT telemetry.ensure_monthly_partitions('telemetry.hot_observations'::REGCLASS,\
             'hot_observations',to_timestamp($1::DOUBLE PRECISION/1000),\
             to_timestamp(($2::DOUBLE PRECISION+1)/1000))",
        )
        .bind(from)
        .bind(to)
        .execute(&mut *transaction)
        .await?;
        let existing: HashMap<i64, ExistingObservation> = sqlx::query(
            "SELECT measured_at,observation_id,canonical_hash,provenance->>'origin' AS origin \
             FROM telemetry.hot_observations WHERE account_id=$1 AND system_id=$2 \
             AND source_kind=$3 AND source_identity=$4 AND measured_at>=$5 AND measured_at<=$6",
        )
        .bind(self.account_id.as_uuid())
        .bind(first.system_id.as_uuid())
        .bind(INGESTION_SOURCE_KIND)
        .bind(&first.source_identity)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *transaction)
        .await?
        .iter()
        .map(|row| {
            (
                row.get("measured_at"),
                ExistingObservation {
                    id: row.get("observation_id"),
                    canonical_hash: row.get("canonical_hash"),
                    fills_gap: row.get::<Option<String>, _>("origin").as_deref()
                        == Some(ENERGY_SAMPLE_ORIGIN),
                },
            )
        })
        .collect();
        let mut outcomes = Vec::with_capacity(items.len());
        for item in items {
            let o = &item.observation;
            let (id, outcome) = match step(item, existing.get(&o.measured_at)) {
                Step::Skip(outcome) => {
                    outcomes.push(outcome);
                    continue;
                }
                Step::Insert => {
                    let inserted = crate::telemetry_repository::pg_insert_hot(
                        &mut transaction,
                        self.account_id,
                        o,
                    )
                    .await?;
                    if inserted.rows_affected() != 1 {
                        return Err(TelemetryRepositoryError::ConstraintConflict);
                    }
                    (o.id.as_uuid(), UpsertOutcome::Inserted)
                }
                Step::Replace(id) => {
                    pg_update_hot(&mut transaction, self.account_id, id, o).await?;
                    sqlx::query(
                        "DELETE FROM telemetry.hot_extended_values \
                         WHERE account_id=$1 AND observation_id=$2",
                    )
                    .bind(self.account_id.as_uuid())
                    .bind(id)
                    .execute(&mut *transaction)
                    .await?;
                    (id, UpsertOutcome::Replaced)
                }
            };
            pg_write_values(
                &mut transaction,
                self.account_id,
                id,
                o.measured_at,
                &item.channel_values,
            )
            .await?;
            outcomes.push(outcome);
        }
        for (channel, nonzero) in touched_channels(items, &outcomes) {
            sqlx::query(
                "UPDATE account_data.channel_definitions SET last_value_at=$1,first_nonzero_at=\
                 COALESCE(first_nonzero_at,CASE WHEN $2 THEN $1 END) \
                 WHERE account_id=$3 AND id=$4",
            )
            .bind(first.received_at)
            .bind(nonzero)
            .bind(self.account_id.as_uuid())
            .bind(channel.as_uuid())
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(outcomes)
    }

    async fn device_counters_before(
        &self,
        system_id: SystemId,
        source_identity: &str,
        before: i64,
    ) -> Result<Option<DeviceCounters>, TelemetryRepositoryError> {
        let mut connection = self.connection().await?;
        let measured_at = sqlx::query_scalar::<_, i64>(
            "SELECT h.measured_at FROM telemetry.hot_observations h WHERE h.account_id=$1 \
             AND h.system_id=$2 AND h.source_kind=$3 AND h.source_identity=$4 \
             AND h.measured_at<$5 AND EXISTS (\
               SELECT 1 FROM telemetry.hot_extended_values v \
               JOIN account_data.channel_definitions c \
                 ON c.account_id=v.account_id AND c.id=v.channel_id \
               WHERE v.account_id=h.account_id AND v.observation_id=h.observation_id \
               AND c.channel_key LIKE 'inv.%.energy.lifetime') \
             ORDER BY h.measured_at DESC LIMIT 1",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(before)
        .fetch_optional(&mut connection)
        .await?;
        let Some(measured_at) = measured_at else {
            connection.close().await?;
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT c.channel_key AS channel_key,v.integer_value AS value \
             FROM telemetry.hot_observations h \
             JOIN telemetry.hot_extended_values v \
               ON v.account_id=h.account_id AND v.observation_id=h.observation_id \
             JOIN account_data.channel_definitions c \
               ON c.account_id=v.account_id AND c.id=v.channel_id \
             WHERE h.account_id=$1 AND h.system_id=$2 AND h.source_kind=$3 \
             AND h.source_identity=$4 AND h.measured_at=$5 \
             AND c.channel_key LIKE 'inv.%.energy.lifetime'",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(measured_at)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        Ok(Some(DeviceCounters {
            measured_at,
            lifetime_wh: rows
                .iter()
                .map(|row| (row.get("channel_key"), row.get("value")))
                .collect(),
        }))
    }

    async fn readings_through(
        &self,
        system_id: SystemId,
        source_identity: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<StoredReading>, TelemetryRepositoryError> {
        // The readings of the range and the first one after it.
        const RANGE: &str = "h.account_id=$1 AND h.system_id=$2 AND h.source_kind=$3 \
             AND h.source_identity=$4 AND h.measured_at>=$5 \
             AND h.measured_at<=(SELECT COALESCE(MIN(n.measured_at),$6) \
             FROM telemetry.hot_observations n WHERE n.account_id=$1 AND n.system_id=$2 \
             AND n.source_kind=$3 AND n.source_identity=$4 AND n.measured_at>$6)";
        let mut connection = self.connection().await?;
        let observations = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT h.observation_id,h.system_id,h.measured_at,h.received_at,h.source_kind,\
             h.source_identity,h.idempotency_identity,h.quality_flags,h.generation_power_watts,\
             h.generation_energy_wh,h.consumption_power_watts,h.consumption_energy_wh,\
             h.provenance,h.canonical_hash,h.version,h.data_source_id,\
             h.generation_lifetime_wh,h.consumption_lifetime_wh,h.grid_import_power_watts,\
             h.grid_import_energy_wh,h.grid_export_power_watts,h.grid_export_energy_wh,\
             h.net_grid_power_watts,h.battery_power_watts,h.battery_energy_wh,\
             h.battery_state_basis_points,h.temperature_millidegrees_c,h.voltage_millivolts,\
             h.message FROM telemetry.hot_observations h WHERE {RANGE} ORDER BY h.measured_at"
        )))
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(from)
        .bind(to)
        .fetch_all(&mut connection)
        .await?
        .iter()
        .map(crate::telemetry_repository::pg_observation)
        .collect::<Result<Vec<_>, _>>()?;
        if observations.is_empty() {
            connection.close().await?;
            return Ok(Vec::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT v.observation_id AS observation_id,v.channel_id AS channel_id,\
             c.channel_key AS channel_key,v.integer_value AS value \
             FROM telemetry.hot_observations h \
             JOIN telemetry.hot_extended_values v \
               ON v.account_id=h.account_id AND v.observation_id=h.observation_id \
             JOIN account_data.channel_definitions c \
               ON c.account_id=v.account_id AND c.id=v.channel_id \
             WHERE {RANGE} AND v.integer_value IS NOT NULL"
        )))
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(INGESTION_SOURCE_KIND)
        .bind(source_identity)
        .bind(from)
        .bind(to)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        let values = rows
            .iter()
            .map(|row| {
                Ok((
                    row.get("observation_id"),
                    ChannelId::from_uuid(row.get("channel_id"))
                        .map_err(|_| TelemetryRepositoryError::InvalidRecord("channel value"))?,
                    row.get("channel_key"),
                    row.get("value"),
                ))
            })
            .collect::<Result<Vec<_>, TelemetryRepositoryError>>()?;
        Ok(stored_readings(observations, values))
    }

    async fn save_energy_samples(
        &self,
        samples: &[EnergySampleRecord],
    ) -> Result<u64, TelemetryRepositoryError> {
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        let mut stored = 0;
        for rows in samples.chunks(ROWS_PER_STATEMENT) {
            let mut statement = sqlx::QueryBuilder::new(
                "INSERT INTO telemetry.device_energy_samples AS s (account_id,system_id,\
                 device_key,measured_at,energy_lifetime_wh,power_watts,data_source_id,\
                 received_at) ",
            );
            statement.push_values(rows, |mut row, sample| {
                row.push_bind(self.account_id.as_uuid())
                    .push_bind(sample.system_id.as_uuid())
                    .push_bind(&sample.device_key)
                    .push_bind(sample.measured_at)
                    .push_bind(sample.energy_lifetime_wh)
                    .push_bind(sample.power_watts)
                    .push_bind(sample.data_source_id.map(DataSourceId::as_uuid))
                    .push_bind(sample.received_at);
            });
            statement.push(
                " ON CONFLICT(account_id,system_id,device_key,measured_at) DO UPDATE SET \
                 energy_lifetime_wh=excluded.energy_lifetime_wh,power_watts=excluded.power_watts,\
                 data_source_id=excluded.data_source_id,received_at=excluded.received_at \
                 WHERE s.energy_lifetime_wh IS DISTINCT FROM excluded.energy_lifetime_wh \
                 OR s.power_watts IS DISTINCT FROM excluded.power_watts",
            );
            stored += statement
                .build()
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(stored)
    }

    async fn energy_samples(
        &self,
        system_id: SystemId,
        filter: EnergySampleFilter<'_>,
    ) -> Result<Vec<EnergySampleRecord>, TelemetryRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT device_key,measured_at,energy_lifetime_wh,power_watts,data_source_id,\
             received_at FROM telemetry.device_energy_samples WHERE account_id=$1 \
             AND system_id=$2 AND ($3::TEXT IS NULL OR device_key=$3) AND measured_at>=$4 \
             AND measured_at<$5 ORDER BY measured_at,device_key LIMIT $6",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(filter.device_key)
        .bind(filter.from.unwrap_or(i64::MIN))
        .bind(filter.to.unwrap_or(i64::MAX))
        .bind(i64::from(filter.limit))
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter()
            .map(|row| {
                Ok(EnergySampleRecord {
                    system_id,
                    device_key: row.get("device_key"),
                    measured_at: row.get("measured_at"),
                    energy_lifetime_wh: row.get("energy_lifetime_wh"),
                    power_watts: row.get("power_watts"),
                    data_source_id: row
                        .get::<Option<Uuid>, _>("data_source_id")
                        .map(DataSourceId::from_uuid)
                        .transpose()
                        .map_err(|_| TelemetryRepositoryError::InvalidRecord("energy sample"))?,
                    received_at: row.get("received_at"),
                })
            })
            .collect()
    }

    async fn save_daily_summaries(
        &self,
        summaries: &[SourceDailySummaryRecord],
    ) -> Result<u64, TelemetryRepositoryError> {
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        let mut stored = 0;
        for r in summaries {
            stored += sqlx::query(
                "INSERT INTO telemetry.source_daily_summaries AS s (account_id,system_id,\
                 source_key,device_key,local_date,data_source_id,generation_energy_wh,\
                 export_energy_wh,consumption_energy_wh,generation_lifetime_wh,complete,\
                 peak_power_watts,peak_time,weather_condition,temperature_min_millidegrees_c,\
                 temperature_max_millidegrees_c,import_energy_by_band,received_at) \
                 VALUES ($1,$2,$3,$4,$5::date,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) \
                 ON CONFLICT (account_id,system_id,source_key,device_key,local_date) \
                 DO UPDATE SET data_source_id=excluded.data_source_id,\
                 generation_energy_wh=excluded.generation_energy_wh,\
                 export_energy_wh=excluded.export_energy_wh,\
                 consumption_energy_wh=excluded.consumption_energy_wh,\
                 generation_lifetime_wh=excluded.generation_lifetime_wh,\
                 complete=excluded.complete,peak_power_watts=excluded.peak_power_watts,\
                 peak_time=excluded.peak_time,weather_condition=excluded.weather_condition,\
                 temperature_min_millidegrees_c=excluded.temperature_min_millidegrees_c,\
                 temperature_max_millidegrees_c=excluded.temperature_max_millidegrees_c,\
                 import_energy_by_band=excluded.import_energy_by_band,\
                 received_at=excluded.received_at \
                 WHERE s.generation_energy_wh IS DISTINCT FROM excluded.generation_energy_wh \
                 OR s.export_energy_wh IS DISTINCT FROM excluded.export_energy_wh \
                 OR s.consumption_energy_wh IS DISTINCT FROM excluded.consumption_energy_wh \
                 OR s.generation_lifetime_wh IS DISTINCT FROM excluded.generation_lifetime_wh \
                 OR s.complete IS DISTINCT FROM excluded.complete \
                 OR s.peak_power_watts IS DISTINCT FROM excluded.peak_power_watts \
                 OR s.peak_time IS DISTINCT FROM excluded.peak_time \
                 OR s.weather_condition IS DISTINCT FROM excluded.weather_condition \
                 OR s.temperature_min_millidegrees_c \
                   IS DISTINCT FROM excluded.temperature_min_millidegrees_c \
                 OR s.temperature_max_millidegrees_c \
                   IS DISTINCT FROM excluded.temperature_max_millidegrees_c \
                 OR s.import_energy_by_band IS DISTINCT FROM excluded.import_energy_by_band",
            )
            .bind(self.account_id.as_uuid())
            .bind(r.system_id.as_uuid())
            .bind(&r.source_key)
            .bind(&r.device_key)
            .bind(&r.local_date)
            .bind(r.data_source_id.map(DataSourceId::as_uuid))
            .bind(r.generation_energy_wh)
            .bind(r.export_energy_wh)
            .bind(r.consumption_energy_wh)
            .bind(r.generation_lifetime_wh)
            .bind(r.complete)
            .bind(r.peak_power_watts)
            .bind(&r.peak_time)
            .bind(&r.weather_condition)
            .bind(r.temperature_min_millidegrees_c)
            .bind(r.temperature_max_millidegrees_c)
            .bind(&r.import_energy_by_band)
            .bind(r.received_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(stored)
    }

    async fn advance_marks(
        &self,
        system_id: SystemId,
        source_key: &str,
        marks: &[IngestionMarkRecord],
    ) -> Result<(), TelemetryRepositoryError> {
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        for mark in marks {
            // GREATEST ignores a side that is NULL.
            sqlx::query(
                "INSERT INTO telemetry.ingestion_marks AS m (account_id,system_id,source_key,\
                 device_key,measured_at,sample_at,summary_date,event_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT (account_id,system_id,source_key,device_key) DO UPDATE SET \
                 measured_at=GREATEST(m.measured_at,excluded.measured_at),\
                 sample_at=GREATEST(m.sample_at,excluded.sample_at),\
                 summary_date=GREATEST(m.summary_date,excluded.summary_date),\
                 event_at=GREATEST(m.event_at,excluded.event_at)",
            )
            .bind(self.account_id.as_uuid())
            .bind(system_id.as_uuid())
            .bind(source_key)
            .bind(&mark.device_key)
            .bind(mark.measured_at)
            .bind(mark.sample_at)
            .bind(mark.summary_date.as_deref())
            .bind(mark.event_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn marks(
        &self,
        system_id: SystemId,
        source_key: &str,
    ) -> Result<Vec<IngestionMarkRecord>, TelemetryRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT device_key,measured_at,sample_at,summary_date,event_at \
             FROM telemetry.ingestion_marks WHERE account_id=$1 AND system_id=$2 \
             AND source_key=$3 ORDER BY device_key",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(source_key)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        Ok(rows.iter().map(mark).collect())
    }
}
