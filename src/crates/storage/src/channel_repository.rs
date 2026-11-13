//! Account-storage adapter for measurement channels and their values.
//!
//! A channel is a named value series a source delivers for one system. Its
//! definition says what the stored integer means: `scale` counts the decimal
//! places, so a value of 12.34 on a channel of scale 2 is stored as 1234.
//! Binding a channel to equipment never rewrites its values; it only changes
//! where they are attributed.

use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, ChannelId, ObservationId, SystemId};
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// One measurement channel with its definition, binding and newest value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelRecord {
    pub id: ChannelId,
    pub system_id: SystemId,
    pub channel_key: String,
    pub display_name: String,
    /// `integer`, `decimal`, `boolean` or `counter`.
    pub data_type: String,
    pub unit: String,
    /// Decimal places of the stored integer.
    pub scale: i32,
    /// `mean`, `sum`, `min`, `max`, `last` or `counter_delta`.
    pub aggregation: String,
    /// `unassigned`, `plant`, `inverter` or `string`.
    pub target_kind: String,
    /// Inverter or string the channel is bound to.
    pub target_id: Option<Uuid>,
    /// `active` or `retired`.
    pub lifecycle: String,
    pub discovered_at: Option<i64>,
    pub last_value_at: Option<i64>,
    /// When the channel first received a value other than zero.
    pub first_nonzero_at: Option<i64>,
    /// Newest stored value by measurement time, as a scaled integer.
    pub last_value: Option<i64>,
    pub updated_at: i64,
}

/// Definition a channel is registered with when its key is first seen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelRegistration {
    pub channel_key: String,
    pub display_name: String,
    pub data_type: String,
    pub unit: String,
    pub scale: i32,
    pub aggregation: String,
}

/// Binding to store for a channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelBindingRecord {
    pub target_kind: String,
    pub target_id: Option<Uuid>,
    /// New display name; the stored one is kept when absent.
    pub display_name: Option<String>,
    /// New aggregation kind; the stored one is kept when absent.
    pub aggregation: Option<String>,
}

/// One stored value of a channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelValueRecord {
    pub measured_at: i64,
    /// Scaled integer at the channel's scale.
    pub value: i64,
}

/// The channel values of one stored reading.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingChannelValues {
    pub observation_id: ObservationId,
    pub measured_at: i64,
    /// Each channel with its scaled integer value.
    pub values: Vec<(ChannelId, i64)>,
}

/// Each channel of the readings once, with whether any of its values is not zero.
fn touched_channels(readings: &[ReadingChannelValues]) -> Vec<(ChannelId, bool)> {
    let mut touched: std::collections::BTreeMap<ChannelId, bool> =
        std::collections::BTreeMap::new();
    for (channel, value) in readings.iter().flat_map(|reading| &reading.values) {
        *touched.entry(*channel).or_default() |= *value != 0;
    }
    touched.into_iter().collect()
}

/// Aggregate of one channel over one local-calendar bucket.
///
/// The sums are kept so that a coarser bucket can be merged from finer ones.
/// What the channel reports for the period follows from its aggregation kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelRollupRecord {
    pub channel_id: ChannelId,
    /// `fifteen_minute`, `hour`, `day`, `month` or `year`.
    pub resolution: String,
    pub bucket_start: i64,
    pub bucket_end: i64,
    pub point_count: i64,
    pub value_sum: i64,
    pub value_min: i64,
    pub value_max: i64,
    pub value_first: i64,
    pub value_last: i64,
    /// Growth of a counter within the bucket; absent for other channels and
    /// for a bucket whose first value has no predecessor.
    pub counter_delta: Option<i64>,
}

/// A bound channel with values its aggregates do not cover yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingChannelRollup {
    /// The channel, without its newest value.
    pub channel: ChannelRecord,
    /// Measurement time of the oldest value not yet aggregated.
    pub first_measured_at: i64,
    /// Measurement time of the newest value not yet aggregated.
    pub last_measured_at: i64,
    /// Receive time of the newest reading that carries a value of the channel.
    pub last_received_at: i64,
    /// Values of the channel received at exactly `last_received_at`.
    pub last_received_values: i64,
}

/// Persistence of measurement channels within one account.
#[async_trait]
pub trait ChannelRepository: Send + Sync {
    /// Returns the channel stored under each key, in the order given, and
    /// registers the keys that are new with the given definition. An existing
    /// channel keeps its definition. The newest value is not read.
    async fn register_channels(
        &self,
        system_id: SystemId,
        registrations: &[ChannelRegistration],
        now: i64,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError>;
    /// Returns the channel stored under the key, registering it when it is new.
    async fn register_channel(
        &self,
        system_id: SystemId,
        registration: &ChannelRegistration,
        now: i64,
    ) -> Result<ChannelRecord, AccountRepositoryError> {
        self.register_channels(system_id, std::slice::from_ref(registration), now)
            .await?
            .pop()
            .ok_or(AccountRepositoryError::InvalidStoredValue)
    }
    /// Lists the active channels of a system, unassigned ones first.
    async fn channels(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError>;
    /// Reads one channel by its key, including a retired one.
    async fn channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError>;
    /// Stores a binding and drops the aggregates built under the previous one.
    ///
    /// Returns `None` for an unknown key.
    ///
    /// # Errors
    ///
    /// Returns `InvalidRecord("channel target")` when the target is not
    /// equipment of the same system.
    async fn bind_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        binding: &ChannelBindingRecord,
        now: i64,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError>;
    /// Retires a channel and keeps its values; reports whether the key was known.
    async fn retire_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        now: i64,
    ) -> Result<bool, AccountRepositoryError>;
    /// Stores the channel values of one stored reading. Writing the same
    /// reading again replaces its values.
    async fn write_channel_values(
        &self,
        observation_id: ObservationId,
        measured_at: i64,
        values: &[(ChannelId, i64)],
        received_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        self.write_readings_values(
            &[ReadingChannelValues {
                observation_id,
                measured_at,
                values: values.to_vec(),
            }],
            received_at,
        )
        .await
    }
    /// Stores the channel values of several stored readings at once, in one
    /// transaction. Writing the same reading again replaces its values.
    async fn write_readings_values(
        &self,
        readings: &[ReadingChannelValues],
        received_at: i64,
    ) -> Result<(), AccountRepositoryError>;
    /// Reads the values of a channel measured in `[from, to)`, oldest first.
    async fn channel_values(
        &self,
        channel_id: ChannelId,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelValueRecord>, AccountRepositoryError>;
    /// Reads the values of several channels of one system measured in
    /// `[from, to)` in one pass, oldest first, each with its channel.
    async fn system_channel_values(
        &self,
        system_id: SystemId,
        channel_ids: &[ChannelId],
        from: i64,
        to: i64,
    ) -> Result<Vec<(ChannelId, ChannelValueRecord)>, AccountRepositoryError>;
    /// Reads the timezone of a system, in which its aggregates are bucketed.
    async fn system_timezone(
        &self,
        system_id: SystemId,
    ) -> Result<Option<String>, AccountRepositoryError>;
    /// Reads the newest value of a channel measured before `before`.
    async fn channel_value_before(
        &self,
        channel_id: ChannelId,
        before: i64,
    ) -> Result<Option<ChannelValueRecord>, AccountRepositoryError>;
    /// Lists the active, bound channels of a system that received values
    /// their aggregates do not cover yet.
    async fn pending_channel_rollups(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<PendingChannelRollup>, AccountRepositoryError>;
    /// Replaces the aggregates of a channel at the given resolutions whose
    /// bucket starts in `[from, to)`. They are stored under the channel's
    /// binding; an unassigned channel only loses its aggregates.
    async fn replace_channel_rollups(
        &self,
        channel: &ChannelRecord,
        resolutions: &[&'static str],
        from: i64,
        to: i64,
        rollups: &[ChannelRollupRecord],
        calculated_at: i64,
    ) -> Result<(), AccountRepositoryError>;
    /// Reads the aggregates of a channel at one resolution whose bucket
    /// starts in `[from, to)`, oldest first.
    async fn channel_rollup_buckets(
        &self,
        channel_id: ChannelId,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelRollupRecord>, AccountRepositoryError>;
    /// Records that the aggregates of a channel cover every value received up
    /// to `received_at`, of which `values` arrived at that very instant.
    async fn mark_channel_rollups_built(
        &self,
        channel_id: ChannelId,
        received_at: i64,
        values: i64,
    ) -> Result<(), AccountRepositoryError>;
    /// Reads the aggregates of every channel bound to one target at one
    /// resolution whose bucket starts in `[from, to)`, by channel and time.
    /// `target_id` is absent for the plant.
    async fn target_channel_rollups(
        &self,
        system_id: SystemId,
        target_kind: &str,
        target_id: Option<Uuid>,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelRollupRecord>, AccountRepositoryError>;
}

const DATA_TYPES: [&str; 4] = ["integer", "decimal", "boolean", "counter"];
const AGGREGATIONS: [&str; 6] = ["mean", "sum", "min", "max", "last", "counter_delta"];
const TARGET_KINDS: [&str; 4] = ["unassigned", "plant", "inverter", "string"];

fn validate_registration(registration: &ChannelRegistration) -> Result<(), AccountRepositoryError> {
    if registration.channel_key.trim().is_empty()
        || !DATA_TYPES.contains(&registration.data_type.as_str())
        || !AGGREGATIONS.contains(&registration.aggregation.as_str())
        || !(0..=9).contains(&registration.scale)
    {
        return Err(AccountRepositoryError::InvalidRecord("channel"));
    }
    Ok(())
}

fn validate_binding(binding: &ChannelBindingRecord) -> Result<(), AccountRepositoryError> {
    let needs_target = matches!(binding.target_kind.as_str(), "inverter" | "string");
    if !TARGET_KINDS.contains(&binding.target_kind.as_str())
        || needs_target != binding.target_id.is_some()
        || binding
            .aggregation
            .as_deref()
            .is_some_and(|kind| !AGGREGATIONS.contains(&kind))
    {
        return Err(AccountRepositoryError::InvalidRecord("channel binding"));
    }
    Ok(())
}

/// Picks the stored channel of each registration, in the order given.
fn registered(
    registrations: &[ChannelRegistration],
    known: &[ChannelRecord],
) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
    registrations
        .iter()
        .map(|registration| {
            known
                .iter()
                .find(|channel| channel.channel_key == registration.channel_key)
                .cloned()
                .ok_or(AccountRepositoryError::InvalidStoredValue)
        })
        .collect()
}

fn channel_id(value: Uuid) -> Result<ChannelId, AccountRepositoryError> {
    ChannelId::from_uuid(value).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

/// `SQLite` channel repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteChannelRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteChannelRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

/// Builds the statement that reads channels with their newest value.
#[cfg(feature = "sqlite")]
macro_rules! sqlite_channel_select {
    ($tail:literal) => {
        concat!(
            "SELECT c.id,c.system_id,c.channel_key,c.display_name,c.data_type,",
            "c.unit,c.scale,c.aggregation,c.target_kind,c.target_id,c.lifecycle,c.discovered_at,",
            "c.last_value_at,c.first_nonzero_at,c.updated_at,",
            // Walking the system's readings newest first and probing each
            // for the channel stops at the newest value; going through the
            // channel's values instead would sort all of them.
            "(SELECT v.integer_value FROM telemetry_hot h ",
            "INDEXED BY telemetry_hot_system_time_idx ",
            "CROSS JOIN telemetry_hot_extended_values v ",
            "ON v.observation_id=h.observation_id AND v.channel_id=c.id ",
            "WHERE h.system_id=c.system_id ORDER BY h.measured_at DESC LIMIT 1) AS last_value ",
            "FROM channel_definitions c ",
            $tail
        )
    };
}

/// Removes the aggregates of the channel stored under a key.
#[cfg(feature = "sqlite")]
async fn sqlite_drop_rollups(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    channel_key: &str,
) -> Result<(), AccountRepositoryError> {
    sqlx::query(
        "DELETE FROM channel_rollups WHERE channel_id IN \
         (SELECT id FROM channel_definitions WHERE system_id=? AND channel_key=?)",
    )
    .bind(blob(system_id.as_uuid()))
    .bind(channel_key)
    .execute(connection)
    .await?;
    Ok(())
}

/// Reads every channel definition of a system without its newest value.
#[cfg(feature = "sqlite")]
async fn sqlite_definitions(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
    sqlx::query(
        "SELECT c.id,c.system_id,c.channel_key,c.display_name,c.data_type,c.unit,c.scale,\
         c.aggregation,c.target_kind,c.target_id,c.lifecycle,c.discovered_at,c.last_value_at,\
         c.first_nonzero_at,c.updated_at,NULL AS last_value \
         FROM channel_definitions c WHERE c.system_id=?",
    )
    .bind(blob(system_id.as_uuid()))
    .fetch_all(connection)
    .await?
    .iter()
    .map(sqlite_channel)
    .collect()
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
fn sqlite_uuid(bytes: &[u8]) -> Result<Uuid, AccountRepositoryError> {
    Uuid::from_slice(bytes).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

#[cfg(feature = "sqlite")]
fn sqlite_channel(row: &sqlx::sqlite::SqliteRow) -> Result<ChannelRecord, AccountRepositoryError> {
    Ok(ChannelRecord {
        id: channel_id(sqlite_uuid(&row.get::<Vec<u8>, _>("id"))?)?,
        system_id: SystemId::from_uuid(sqlite_uuid(&row.get::<Vec<u8>, _>("system_id"))?)
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        channel_key: row.get("channel_key"),
        display_name: row.get("display_name"),
        data_type: row.get("data_type"),
        unit: row.get("unit"),
        scale: i32::try_from(row.get::<i64, _>("scale"))
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        aggregation: row.get("aggregation"),
        target_kind: row.get("target_kind"),
        target_id: row
            .get::<Option<Vec<u8>>, _>("target_id")
            .map(|bytes| sqlite_uuid(&bytes))
            .transpose()?,
        lifecycle: row.get("lifecycle"),
        discovered_at: row.get("discovered_at"),
        last_value_at: row.get("last_value_at"),
        first_nonzero_at: row.get("first_nonzero_at"),
        last_value: row.get("last_value"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_rollup(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ChannelRollupRecord, AccountRepositoryError> {
    Ok(ChannelRollupRecord {
        channel_id: channel_id(sqlite_uuid(&row.get::<Vec<u8>, _>("channel_id"))?)?,
        resolution: row.get("resolution"),
        bucket_start: row.get("bucket_start"),
        bucket_end: row.get("bucket_end"),
        point_count: row.get("point_count"),
        value_sum: row.get("value_sum"),
        value_min: row.get("value_min"),
        value_max: row.get("value_max"),
        value_first: row.get("value_first"),
        value_last: row.get("value_last"),
        counter_delta: row.get("counter_delta"),
    })
}

#[cfg(feature = "sqlite")]
async fn sqlite_channel_by_key(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    channel_key: &str,
) -> Result<Option<ChannelRecord>, AccountRepositoryError> {
    sqlx::query(sqlite_channel_select!(
        "WHERE c.system_id=? AND c.channel_key=?"
    ))
    .bind(blob(system_id.as_uuid()))
    .bind(channel_key)
    .fetch_optional(connection)
    .await?
    .as_ref()
    .map(sqlite_channel)
    .transpose()
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl ChannelRepository for SqliteChannelRepository {
    async fn register_channels(
        &self,
        system_id: SystemId,
        registrations: &[ChannelRegistration],
        now: i64,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
        let known = {
            let mut connection = self.account.acquire().await?;
            sqlite_definitions(&mut connection, system_id).await?
        };
        let missing: Vec<&ChannelRegistration> = registrations
            .iter()
            .filter(|registration| {
                !known
                    .iter()
                    .any(|channel| channel.channel_key == registration.channel_key)
            })
            .collect();
        if missing.is_empty() {
            return registered(registrations, &known);
        }
        let mut writer = self.account.acquire_writer().await?;
        for registration in missing {
            validate_registration(registration)?;
            sqlx::query(
                "INSERT INTO channel_definitions \
                 (id,system_id,channel_key,display_name,data_type,unit,scale,aggregation,\
                  effective_from,created_at,updated_at,target_kind,discovered_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,'unassigned',?) \
                 ON CONFLICT(system_id,channel_key) DO NOTHING",
            )
            .bind(blob(Uuid::now_v7()))
            .bind(blob(system_id.as_uuid()))
            .bind(&registration.channel_key)
            .bind(&registration.display_name)
            .bind(&registration.data_type)
            .bind(&registration.unit)
            .bind(registration.scale)
            .bind(&registration.aggregation)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(writer.connection())
            .await?;
        }
        let known = sqlite_definitions(writer.connection(), system_id).await?;
        registered(registrations, &known)
    }

    async fn channels(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(sqlite_channel_select!(
            "WHERE c.system_id=? AND c.lifecycle='active' \
             ORDER BY c.target_kind='unassigned' DESC,c.channel_key"
        ))
        .bind(blob(system_id.as_uuid()))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(sqlite_channel)
        .collect()
    }

    async fn channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlite_channel_by_key(&mut connection, system_id, channel_key).await
    }

    async fn bind_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        binding: &ChannelBindingRecord,
        now: i64,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError> {
        validate_binding(binding)?;
        let mut writer = self.account.acquire_writer().await?;
        if let Some(target_id) = binding.target_id {
            let statement = if binding.target_kind == "inverter" {
                "SELECT 1 FROM inverters WHERE id=? AND system_id=?"
            } else {
                "SELECT 1 FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
                 WHERE p.id=? AND i.system_id=?"
            };
            let known: Option<i64> = sqlx::query_scalar(statement)
                .bind(blob(target_id))
                .bind(blob(system_id.as_uuid()))
                .fetch_optional(writer.connection())
                .await?;
            if known.is_none() {
                return Err(AccountRepositoryError::InvalidRecord("channel target"));
            }
        }
        let updated = sqlx::query(
            "UPDATE channel_definitions SET target_kind=?,target_id=?,\
             display_name=COALESCE(?,display_name),aggregation=COALESCE(?,aggregation),\
             rollup_watermark=NULL,rollup_watermark_values=NULL,\
             updated_at=?,version=version+1 WHERE system_id=? AND channel_key=?",
        )
        .bind(&binding.target_kind)
        .bind(binding.target_id.map(blob))
        .bind(binding.display_name.as_deref())
        .bind(binding.aggregation.as_deref())
        .bind(now)
        .bind(blob(system_id.as_uuid()))
        .bind(channel_key)
        .execute(writer.connection())
        .await?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        // Per-string and per-inverter rollups follow the binding they were
        // built under. They are dropped so that the aggregation cycle, which
        // notices the newer binding, rebuilds them against the current one.
        sqlx::query(
            "DELETE FROM telemetry_rollups \
             WHERE system_id=? AND scope_kind IN ('string','inverter')",
        )
        .bind(blob(system_id.as_uuid()))
        .execute(writer.connection())
        .await?;
        sqlite_drop_rollups(writer.connection(), system_id, channel_key).await?;
        sqlite_channel_by_key(writer.connection(), system_id, channel_key).await
    }

    async fn retire_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        now: i64,
    ) -> Result<bool, AccountRepositoryError> {
        let mut writer = self.account.acquire_writer().await?;
        // Retiring keeps the stored values and their channel row; deleting the
        // definition would orphan every value. Its aggregates go.
        let retired = sqlx::query(
            "UPDATE channel_definitions SET lifecycle='retired',updated_at=?,\
             rollup_watermark=NULL,rollup_watermark_values=NULL,\
             version=version+1 WHERE system_id=? AND channel_key=?",
        )
        .bind(now)
        .bind(blob(system_id.as_uuid()))
        .bind(channel_key)
        .execute(writer.connection())
        .await?
        .rows_affected()
            > 0;
        sqlite_drop_rollups(writer.connection(), system_id, channel_key).await?;
        Ok(retired)
    }

    async fn write_readings_values(
        &self,
        readings: &[ReadingChannelValues],
        received_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        // Three values a row stays well below the bound parameters SQLite allows.
        const ROWS_PER_STATEMENT: usize = 300;
        let rows: Vec<(ObservationId, ChannelId, i64)> = readings
            .iter()
            .flat_map(|reading| {
                reading
                    .values
                    .iter()
                    .map(|(channel, value)| (reading.observation_id, *channel, *value))
            })
            .collect();
        if rows.is_empty() {
            return Ok(());
        }
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        for chunk in rows.chunks(ROWS_PER_STATEMENT) {
            let mut statement = sqlx::QueryBuilder::new(
                "INSERT INTO telemetry_hot_extended_values \
                 (observation_id,channel_id,integer_value) ",
            );
            statement.push_values(chunk, |mut row, (observation, channel, value)| {
                row.push_bind(blob(observation.as_uuid()))
                    .push_bind(blob(channel.as_uuid()))
                    .push_bind(*value);
            });
            statement.push(
                " ON CONFLICT(observation_id,channel_id) DO UPDATE SET \
                 integer_value=excluded.integer_value",
            );
            statement.build().execute(&mut *transaction).await?;
        }
        for (channel, nonzero) in touched_channels(readings) {
            sqlx::query(
                "UPDATE channel_definitions SET last_value_at=?1,first_nonzero_at=\
                 COALESCE(first_nonzero_at,CASE WHEN ?2 THEN ?1 END) WHERE id=?3",
            )
            .bind(received_at)
            .bind(nonzero)
            .bind(blob(channel.as_uuid()))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn system_channel_values(
        &self,
        system_id: SystemId,
        channel_ids: &[ChannelId],
        from: i64,
        to: i64,
    ) -> Result<Vec<(ChannelId, ChannelValueRecord)>, AccountRepositoryError> {
        if channel_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut connection = self.account.acquire().await?;
        // The system's readings of the range, each with the values of the
        // asked channels it carries.
        let mut statement = sqlx::QueryBuilder::new(
            "SELECT v.channel_id AS channel_id,h.measured_at AS measured_at,\
             v.integer_value AS value FROM telemetry_hot h \
             CROSS JOIN telemetry_hot_extended_values v ON v.observation_id=h.observation_id \
             WHERE h.system_id=",
        );
        statement
            .push_bind(blob(system_id.as_uuid()))
            .push(" AND h.measured_at>=")
            .push_bind(from)
            .push(" AND h.measured_at<")
            .push_bind(to)
            .push(" AND v.channel_id IN (");
        let mut ids = statement.separated(",");
        for channel_id in channel_ids {
            ids.push_bind(blob(channel_id.as_uuid()));
        }
        statement.push(") ORDER BY h.measured_at");
        statement
            .build()
            .fetch_all(&mut *connection)
            .await?
            .iter()
            .map(|row| {
                Ok((
                    channel_id(sqlite_uuid(&row.get::<Vec<u8>, _>("channel_id"))?)?,
                    ChannelValueRecord {
                        measured_at: row.get("measured_at"),
                        value: row.get("value"),
                    },
                ))
            })
            .collect()
    }

    async fn channel_values(
        &self,
        channel_id: ChannelId,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelValueRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(sqlx::query(
            // Through the system's readings of the range rather than every
            // value of the channel, which would be read to keep a few.
            "SELECT h.measured_at AS measured_at,v.integer_value AS value \
             FROM telemetry_hot h CROSS JOIN telemetry_hot_extended_values v \
             ON v.observation_id=h.observation_id AND v.channel_id=?1 \
             WHERE h.system_id=(SELECT system_id FROM channel_definitions WHERE id=?1) \
             AND h.measured_at>=?2 AND h.measured_at<?3 ORDER BY h.measured_at",
        )
        .bind(blob(channel_id.as_uuid()))
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| ChannelValueRecord {
            measured_at: row.get("measured_at"),
            value: row.get("value"),
        })
        .collect())
    }

    async fn system_timezone(
        &self,
        system_id: SystemId,
    ) -> Result<Option<String>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(
            sqlx::query_scalar("SELECT timezone FROM systems WHERE id=?")
                .bind(blob(system_id.as_uuid()))
                .fetch_optional(&mut *connection)
                .await?,
        )
    }

    async fn channel_value_before(
        &self,
        channel_id: ChannelId,
        before: i64,
    ) -> Result<Option<ChannelValueRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(sqlx::query(
            "SELECT h.measured_at AS measured_at,v.integer_value AS value \
             FROM telemetry_hot h CROSS JOIN telemetry_hot_extended_values v \
             ON v.observation_id=h.observation_id AND v.channel_id=?1 \
             WHERE h.system_id=(SELECT system_id FROM channel_definitions WHERE id=?1) \
             AND h.measured_at<?2 ORDER BY h.measured_at DESC LIMIT 1",
        )
        .bind(blob(channel_id.as_uuid()))
        .bind(before)
        .fetch_optional(&mut *connection)
        .await?
        .map(|row| ChannelValueRecord {
            measured_at: row.get("measured_at"),
            value: row.get("value"),
        }))
    }

    async fn pending_channel_rollups(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<PendingChannelRollup>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            // Only channels that received a value since their watermark are
            // looked at: the time of their newest value tells, and a value
            // received at the very watermark is told by counting those. Their
            // values are read from the readings received since the oldest of
            // their watermarks, through the receive-time index, rather than
            // every value ever stored. A channel without a watermark is built
            // over the whole time the system has readings, which the
            // measurement-time index tells without reading a value.
            "WITH bound AS MATERIALIZED (\
               SELECT c.id,c.rollup_watermark,c.rollup_watermark_values \
               FROM channel_definitions c \
               WHERE c.system_id=?1 AND c.lifecycle='active' AND c.target_kind<>'unassigned' \
               AND c.rollup_watermark IS NOT NULL AND (c.last_value_at>c.rollup_watermark \
                 OR (c.last_value_at=c.rollup_watermark AND (\
                   SELECT COUNT(*) FROM telemetry_hot h INDEXED BY telemetry_hot_received_idx \
                   CROSS JOIN telemetry_hot_extended_values v \
                   ON v.observation_id=h.observation_id AND v.channel_id=c.id \
                   WHERE h.received_at=c.rollup_watermark AND h.system_id=?1\
                 )<>c.rollup_watermark_values))),\
             pending AS (\
               SELECT v.channel_id AS channel_id,MIN(h.measured_at) AS first_measured_at,\
               MAX(h.measured_at) AS last_measured_at,MAX(h.received_at) AS last_received_at,\
               COUNT(*) AS value_count \
               FROM telemetry_hot h INDEXED BY telemetry_hot_received_idx \
               CROSS JOIN telemetry_hot_extended_values v ON v.observation_id=h.observation_id \
               CROSS JOIN bound b ON b.id=v.channel_id \
               WHERE h.received_at>=(SELECT COALESCE(MIN(rollup_watermark),\
                 9223372036854775807) FROM bound) \
               AND h.system_id=?1 AND h.received_at>=b.rollup_watermark \
               GROUP BY v.channel_id \
               UNION ALL \
               SELECT c.id,\
               (SELECT MIN(measured_at) FROM telemetry_hot WHERE system_id=?1),\
               (SELECT MAX(measured_at) FROM telemetry_hot WHERE system_id=?1),\
               c.last_value_at,NULL FROM channel_definitions c \
               WHERE c.system_id=?1 AND c.lifecycle='active' AND c.target_kind<>'unassigned' \
               AND c.rollup_watermark IS NULL AND c.last_value_at IS NOT NULL) \
             SELECT c.id,c.system_id,c.channel_key,c.display_name,c.data_type,c.unit,c.scale,\
             c.aggregation,c.target_kind,c.target_id,c.lifecycle,c.discovered_at,\
             c.last_value_at,c.first_nonzero_at,c.updated_at,NULL AS last_value,\
             p.first_measured_at,p.last_measured_at,p.last_received_at,\
             (SELECT COUNT(*) FROM telemetry_hot h \
              CROSS JOIN telemetry_hot_extended_values v \
              ON v.observation_id=h.observation_id AND v.channel_id=c.id \
              WHERE h.received_at=p.last_received_at AND h.system_id=?1) \
              AS last_received_values \
             FROM pending p JOIN channel_definitions c ON c.id=p.channel_id \
             WHERE p.first_measured_at IS NOT NULL AND (c.rollup_watermark IS NULL \
             OR p.last_received_at>c.rollup_watermark \
             OR p.value_count<>c.rollup_watermark_values) ORDER BY c.channel_key",
        )
        .bind(blob(system_id.as_uuid()))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| {
            Ok(PendingChannelRollup {
                channel: sqlite_channel(row)?,
                first_measured_at: row.get("first_measured_at"),
                last_measured_at: row.get("last_measured_at"),
                last_received_at: row.get("last_received_at"),
                last_received_values: row.get("last_received_values"),
            })
        })
        .collect()
    }

    async fn replace_channel_rollups(
        &self,
        channel: &ChannelRecord,
        resolutions: &[&'static str],
        from: i64,
        to: i64,
        rollups: &[ChannelRollupRecord],
        calculated_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        use sqlx::Connection as _;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        for resolution in resolutions {
            sqlx::query(
                "DELETE FROM channel_rollups WHERE channel_id=? AND resolution=? \
                 AND bucket_start>=? AND bucket_start<?",
            )
            .bind(blob(channel.id.as_uuid()))
            .bind(*resolution)
            .bind(from)
            .bind(to)
            .execute(&mut *transaction)
            .await?;
        }
        if channel.target_kind != "unassigned" {
            // 15 bound values a row stays well below the parameters SQLite allows.
            for chunk in rollups.chunks(500) {
                let mut statement = sqlx::QueryBuilder::new(
                    "INSERT INTO channel_rollups (system_id,channel_id,target_kind,target_id,\
                     resolution,bucket_start,bucket_end,point_count,value_sum,value_min,\
                     value_max,value_first,value_last,counter_delta,calculated_at) ",
                );
                statement.push_values(chunk, |mut row, rollup| {
                    row.push_bind(blob(channel.system_id.as_uuid()))
                        .push_bind(blob(channel.id.as_uuid()))
                        .push_bind(channel.target_kind.clone())
                        .push_bind(channel.target_id.map(blob))
                        .push_bind(rollup.resolution.clone())
                        .push_bind(rollup.bucket_start)
                        .push_bind(rollup.bucket_end)
                        .push_bind(rollup.point_count)
                        .push_bind(rollup.value_sum)
                        .push_bind(rollup.value_min)
                        .push_bind(rollup.value_max)
                        .push_bind(rollup.value_first)
                        .push_bind(rollup.value_last)
                        .push_bind(rollup.counter_delta)
                        .push_bind(calculated_at);
                });
                statement.build().execute(&mut *transaction).await?;
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn channel_rollup_buckets(
        &self,
        channel_id: ChannelId,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelRollupRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT channel_id,resolution,bucket_start,bucket_end,point_count,value_sum,\
             value_min,value_max,value_first,value_last,counter_delta FROM channel_rollups \
             WHERE channel_id=? AND resolution=? AND bucket_start>=? AND bucket_start<? \
             ORDER BY bucket_start",
        )
        .bind(blob(channel_id.as_uuid()))
        .bind(resolution)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(sqlite_rollup)
        .collect()
    }

    async fn mark_channel_rollups_built(
        &self,
        channel_id: ChannelId,
        received_at: i64,
        values: i64,
    ) -> Result<(), AccountRepositoryError> {
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query(
            "UPDATE channel_definitions SET rollup_watermark=?,rollup_watermark_values=? \
             WHERE id=?",
        )
        .bind(received_at)
        .bind(values)
        .bind(blob(channel_id.as_uuid()))
        .execute(writer.connection())
        .await?;
        Ok(())
    }

    async fn target_channel_rollups(
        &self,
        system_id: SystemId,
        target_kind: &str,
        target_id: Option<Uuid>,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelRollupRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT channel_id,resolution,bucket_start,bucket_end,point_count,value_sum,\
             value_min,value_max,value_first,value_last,counter_delta FROM channel_rollups \
             WHERE system_id=? AND target_kind=? AND target_id IS ? AND resolution=? \
             AND bucket_start>=? AND bucket_start<? ORDER BY channel_id,bucket_start",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(target_kind)
        .bind(target_id.map(blob))
        .bind(resolution)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(sqlite_rollup)
        .collect()
    }
}

/// `PostgreSQL` channel repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresChannelRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresChannelRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresChannelRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresChannelRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

/// Builds the statement that reads channels with their newest value.
#[cfg(feature = "postgres")]
macro_rules! postgres_channel_select {
    ($tail:literal) => {
        concat!(
            "SELECT c.id,c.system_id,c.channel_key,c.display_name,c.data_type,",
            "c.unit,c.scale,c.aggregation,c.target_kind,c.target_id,c.lifecycle,c.discovered_at,",
            "c.last_value_at,c.first_nonzero_at,c.updated_at,",
            "(SELECT v.integer_value FROM telemetry.hot_extended_values v ",
            "WHERE v.account_id=c.account_id AND v.channel_id=c.id ",
            "ORDER BY v.measured_at DESC LIMIT 1) AS last_value ",
            "FROM account_data.channel_definitions c ",
            $tail
        )
    };
}

#[cfg(feature = "postgres")]
fn postgres_channel(row: &sqlx::postgres::PgRow) -> Result<ChannelRecord, AccountRepositoryError> {
    Ok(ChannelRecord {
        id: channel_id(row.get("id"))?,
        system_id: SystemId::from_uuid(row.get("system_id"))
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        channel_key: row.get("channel_key"),
        display_name: row.get("display_name"),
        data_type: row.get("data_type"),
        unit: row.get("unit"),
        scale: row.get("scale"),
        aggregation: row.get("aggregation"),
        target_kind: row.get("target_kind"),
        target_id: row.get("target_id"),
        lifecycle: row.get("lifecycle"),
        discovered_at: row.get("discovered_at"),
        last_value_at: row.get("last_value_at"),
        first_nonzero_at: row.get("first_nonzero_at"),
        last_value: row.get("last_value"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn postgres_rollup(
    row: &sqlx::postgres::PgRow,
) -> Result<ChannelRollupRecord, AccountRepositoryError> {
    Ok(ChannelRollupRecord {
        channel_id: channel_id(row.get("channel_id"))?,
        resolution: row.get("resolution"),
        bucket_start: row.get("bucket_start"),
        bucket_end: row.get("bucket_end"),
        point_count: row.get("point_count"),
        value_sum: row.get("value_sum"),
        value_min: row.get("value_min"),
        value_max: row.get("value_max"),
        value_first: row.get("value_first"),
        value_last: row.get("value_last"),
        counter_delta: row.get("counter_delta"),
    })
}

#[cfg(feature = "postgres")]
impl PostgresChannelRepository {
    /// Reads every channel definition of a system without its newest value.
    async fn definitions(
        &self,
        connection: &mut PgConnection,
        system_id: SystemId,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
        sqlx::query(
            "SELECT c.id,c.system_id,c.channel_key,c.display_name,c.data_type,c.unit,c.scale,\
             c.aggregation,c.target_kind,c.target_id,c.lifecycle,c.discovered_at,\
             c.last_value_at,c.first_nonzero_at,c.updated_at,NULL::BIGINT AS last_value \
             FROM account_data.channel_definitions c WHERE c.account_id=$1 AND c.system_id=$2",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_all(connection)
        .await?
        .iter()
        .map(postgres_channel)
        .collect()
    }

    /// Removes the aggregates of the channel stored under a key.
    async fn drop_rollups(
        &self,
        connection: &mut PgConnection,
        system_id: SystemId,
        channel_key: &str,
    ) -> Result<(), AccountRepositoryError> {
        sqlx::query(
            "DELETE FROM telemetry.channel_rollups r USING account_data.channel_definitions c \
             WHERE r.account_id=c.account_id AND r.channel_id=c.id \
             AND c.account_id=$1 AND c.system_id=$2 AND c.channel_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(channel_key)
        .execute(connection)
        .await?;
        Ok(())
    }

    async fn channel_by_key(
        &self,
        connection: &mut PgConnection,
        system_id: SystemId,
        channel_key: &str,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError> {
        sqlx::query(postgres_channel_select!(
            "WHERE c.account_id=$1 AND c.system_id=$2 AND c.channel_key=$3"
        ))
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(channel_key)
        .fetch_optional(connection)
        .await?
        .as_ref()
        .map(postgres_channel)
        .transpose()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl ChannelRepository for PostgresChannelRepository {
    async fn register_channels(
        &self,
        system_id: SystemId,
        registrations: &[ChannelRegistration],
        now: i64,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let mut known = self.definitions(&mut connection, system_id).await?;
        let mut inserted = false;
        for registration in registrations {
            if known
                .iter()
                .any(|channel| channel.channel_key == registration.channel_key)
            {
                continue;
            }
            validate_registration(registration)?;
            sqlx::query(
                "INSERT INTO account_data.channel_definitions \
                 (account_id,id,system_id,channel_key,display_name,data_type,unit,scale,\
                  aggregation,effective_from,created_at,updated_at,target_kind,discovered_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$10,'unassigned',$10) \
                 ON CONFLICT(account_id,system_id,channel_key) DO NOTHING",
            )
            .bind(self.account_id.as_uuid())
            .bind(Uuid::now_v7())
            .bind(system_id.as_uuid())
            .bind(&registration.channel_key)
            .bind(&registration.display_name)
            .bind(&registration.data_type)
            .bind(&registration.unit)
            .bind(registration.scale)
            .bind(&registration.aggregation)
            .bind(now)
            .execute(&mut connection)
            .await?;
            inserted = true;
        }
        if inserted {
            known = self.definitions(&mut connection, system_id).await?;
        }
        connection.close().await?;
        registered(registrations, &known)
    }

    async fn channels(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<ChannelRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(postgres_channel_select!(
            "WHERE c.account_id=$1 AND c.system_id=$2 \
             AND c.lifecycle='active' ORDER BY (c.target_kind='unassigned') DESC,c.channel_key"
        ))
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(postgres_channel).collect()
    }

    async fn channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let stored = self
            .channel_by_key(&mut connection, system_id, channel_key)
            .await?;
        connection.close().await?;
        Ok(stored)
    }

    async fn bind_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        binding: &ChannelBindingRecord,
        now: i64,
    ) -> Result<Option<ChannelRecord>, AccountRepositoryError> {
        validate_binding(binding)?;
        let mut connection = self.connection().await?;
        if let Some(target_id) = binding.target_id {
            let statement = if binding.target_kind == "inverter" {
                "SELECT 1 FROM account_data.inverters \
                 WHERE account_id=$1 AND id=$2 AND system_id=$3"
            } else {
                "SELECT 1 FROM account_data.pv_strings p \
                 JOIN account_data.inverters i \
                   ON i.account_id=p.account_id AND i.id=p.inverter_id \
                 WHERE p.account_id=$1 AND p.id=$2 AND i.system_id=$3"
            };
            let known: Option<i32> = sqlx::query_scalar(statement)
                .bind(self.account_id.as_uuid())
                .bind(target_id)
                .bind(system_id.as_uuid())
                .fetch_optional(&mut connection)
                .await?;
            if known.is_none() {
                connection.close().await?;
                return Err(AccountRepositoryError::InvalidRecord("channel target"));
            }
        }
        let updated = sqlx::query(
            "UPDATE account_data.channel_definitions SET target_kind=$4,target_id=$5,\
             display_name=COALESCE($6,display_name),aggregation=COALESCE($7,aggregation),\
             rollup_watermark=NULL,rollup_watermark_values=NULL,\
             updated_at=$8,version=version+1 \
             WHERE account_id=$1 AND system_id=$2 AND channel_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(channel_key)
        .bind(&binding.target_kind)
        .bind(binding.target_id)
        .bind(binding.display_name.as_deref())
        .bind(binding.aggregation.as_deref())
        .bind(now)
        .execute(&mut connection)
        .await?;
        if updated.rows_affected() == 0 {
            connection.close().await?;
            return Ok(None);
        }
        // Per-string and per-inverter rollups follow the binding they were
        // built under, so they are dropped and rebuilt against the current one.
        sqlx::query(
            "DELETE FROM telemetry.rollups WHERE account_id=$1 AND system_id=$2 \
             AND scope_kind IN ('string','inverter')",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .execute(&mut connection)
        .await?;
        self.drop_rollups(&mut connection, system_id, channel_key)
            .await?;
        let stored = self
            .channel_by_key(&mut connection, system_id, channel_key)
            .await?;
        connection.close().await?;
        Ok(stored)
    }

    async fn retire_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        now: i64,
    ) -> Result<bool, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let updated = sqlx::query(
            "UPDATE account_data.channel_definitions SET lifecycle='retired',updated_at=$4,\
             rollup_watermark=NULL,rollup_watermark_values=NULL,\
             version=version+1 WHERE account_id=$1 AND system_id=$2 AND channel_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(channel_key)
        .bind(now)
        .execute(&mut connection)
        .await?
        .rows_affected();
        self.drop_rollups(&mut connection, system_id, channel_key)
            .await?;
        connection.close().await?;
        Ok(updated > 0)
    }

    async fn write_readings_values(
        &self,
        readings: &[ReadingChannelValues],
        received_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        if readings.iter().all(|reading| reading.values.is_empty()) {
            return Ok(());
        }
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        for reading in readings {
            for (channel, value) in &reading.values {
                sqlx::query(
                    "INSERT INTO telemetry.hot_extended_values \
                     (account_id,observation_id,measured_at,channel_id,integer_value) \
                     VALUES ($1,$2,$3,$4,$5) \
                     ON CONFLICT(account_id,observation_id,measured_at,channel_id) DO UPDATE SET \
                     integer_value=excluded.integer_value",
                )
                .bind(self.account_id.as_uuid())
                .bind(reading.observation_id.as_uuid())
                .bind(reading.measured_at)
                .bind(channel.as_uuid())
                .bind(value)
                .execute(&mut *transaction)
                .await?;
            }
        }
        for (channel, nonzero) in touched_channels(readings) {
            sqlx::query(
                "UPDATE account_data.channel_definitions SET last_value_at=$3,\
                 first_nonzero_at=COALESCE(first_nonzero_at,CASE WHEN $4 THEN $3 END) \
                 WHERE account_id=$1 AND id=$2",
            )
            .bind(self.account_id.as_uuid())
            .bind(channel.as_uuid())
            .bind(received_at)
            .bind(nonzero)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn system_channel_values(
        &self,
        system_id: SystemId,
        channel_ids: &[ChannelId],
        from: i64,
        to: i64,
    ) -> Result<Vec<(ChannelId, ChannelValueRecord)>, AccountRepositoryError> {
        if channel_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = channel_ids.iter().map(|id| id.as_uuid()).collect();
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT v.channel_id,v.measured_at,v.integer_value AS value \
             FROM telemetry.hot_extended_values v \
             JOIN account_data.channel_definitions c \
               ON c.account_id=v.account_id AND c.id=v.channel_id \
             WHERE v.account_id=$1 AND c.system_id=$2 AND v.channel_id=ANY($3) \
             AND v.measured_at>=$4 AND v.measured_at<$5 ORDER BY v.measured_at",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(&ids)
        .bind(from)
        .bind(to)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter()
            .map(|row| {
                Ok((
                    channel_id(row.get("channel_id"))?,
                    ChannelValueRecord {
                        measured_at: row.get("measured_at"),
                        value: row.get("value"),
                    },
                ))
            })
            .collect()
    }

    async fn channel_values(
        &self,
        channel_id: ChannelId,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelValueRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT measured_at,integer_value AS value FROM telemetry.hot_extended_values \
             WHERE account_id=$1 AND channel_id=$2 AND measured_at>=$3 AND measured_at<$4 \
             ORDER BY measured_at",
        )
        .bind(self.account_id.as_uuid())
        .bind(channel_id.as_uuid())
        .bind(from)
        .bind(to)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        Ok(rows
            .iter()
            .map(|row| ChannelValueRecord {
                measured_at: row.get("measured_at"),
                value: row.get("value"),
            })
            .collect())
    }

    async fn system_timezone(
        &self,
        system_id: SystemId,
    ) -> Result<Option<String>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let timezone = sqlx::query_scalar(
            "SELECT timezone FROM account_data.systems WHERE account_id=$1 AND id=$2",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        Ok(timezone)
    }

    async fn channel_value_before(
        &self,
        channel_id: ChannelId,
        before: i64,
    ) -> Result<Option<ChannelValueRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT measured_at,integer_value AS value FROM telemetry.hot_extended_values \
             WHERE account_id=$1 AND channel_id=$2 AND measured_at<$3 \
             ORDER BY measured_at DESC LIMIT 1",
        )
        .bind(self.account_id.as_uuid())
        .bind(channel_id.as_uuid())
        .bind(before)
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        Ok(row.map(|row| ChannelValueRecord {
            measured_at: row.get("measured_at"),
            value: row.get("value"),
        }))
    }

    async fn pending_channel_rollups(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<PendingChannelRollup>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "WITH pending AS (\
               SELECT v.channel_id AS channel_id,MIN(v.measured_at) AS first_measured_at,\
               MAX(v.measured_at) AS last_measured_at,MAX(h.received_at) AS last_received_at,\
               COUNT(*) AS value_count \
               FROM account_data.channel_definitions c \
               JOIN telemetry.hot_extended_values v \
                 ON v.account_id=c.account_id AND v.channel_id=c.id \
               JOIN telemetry.hot_observations h ON h.account_id=v.account_id \
                 AND h.observation_id=v.observation_id AND h.measured_at=v.measured_at \
               WHERE c.account_id=$1 AND c.system_id=$2 AND c.lifecycle='active' \
               AND c.target_kind<>'unassigned' \
               AND (c.rollup_watermark IS NULL OR h.received_at>=c.rollup_watermark) \
               GROUP BY v.channel_id) \
             SELECT c.id,c.system_id,c.channel_key,c.display_name,c.data_type,c.unit,c.scale,\
             c.aggregation,c.target_kind,c.target_id,c.lifecycle,c.discovered_at,\
             c.last_value_at,c.first_nonzero_at,c.updated_at,NULL::BIGINT AS last_value,\
             p.first_measured_at,p.last_measured_at,p.last_received_at,\
             (SELECT COUNT(*) FROM telemetry.hot_extended_values v \
              JOIN telemetry.hot_observations h ON h.account_id=v.account_id \
                AND h.observation_id=v.observation_id AND h.measured_at=v.measured_at \
              WHERE v.account_id=c.account_id AND v.channel_id=c.id \
              AND h.received_at=p.last_received_at) AS last_received_values \
             FROM pending p JOIN account_data.channel_definitions c \
               ON c.account_id=$1 AND c.id=p.channel_id \
             WHERE c.rollup_watermark IS NULL OR p.last_received_at>c.rollup_watermark \
             OR p.value_count<>c.rollup_watermark_values ORDER BY c.channel_key",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter()
            .map(|row| {
                Ok(PendingChannelRollup {
                    channel: postgres_channel(row)?,
                    first_measured_at: row.get("first_measured_at"),
                    last_measured_at: row.get("last_measured_at"),
                    last_received_at: row.get("last_received_at"),
                    last_received_values: row.get("last_received_values"),
                })
            })
            .collect()
    }

    async fn replace_channel_rollups(
        &self,
        channel: &ChannelRecord,
        resolutions: &[&'static str],
        from: i64,
        to: i64,
        rollups: &[ChannelRollupRecord],
        calculated_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        for resolution in resolutions {
            sqlx::query(
                "DELETE FROM telemetry.channel_rollups WHERE account_id=$1 AND channel_id=$2 \
                 AND resolution=$3 AND bucket_start>=$4 AND bucket_start<$5",
            )
            .bind(self.account_id.as_uuid())
            .bind(channel.id.as_uuid())
            .bind(*resolution)
            .bind(from)
            .bind(to)
            .execute(&mut *transaction)
            .await?;
        }
        if channel.target_kind != "unassigned" {
            for rollup in rollups {
                sqlx::query(
                    "INSERT INTO telemetry.channel_rollups (account_id,system_id,channel_id,\
                     target_kind,target_id,resolution,bucket_start,bucket_end,point_count,\
                     value_sum,value_min,value_max,value_first,value_last,counter_delta,\
                     calculated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
                )
                .bind(self.account_id.as_uuid())
                .bind(channel.system_id.as_uuid())
                .bind(channel.id.as_uuid())
                .bind(&channel.target_kind)
                .bind(channel.target_id)
                .bind(&rollup.resolution)
                .bind(rollup.bucket_start)
                .bind(rollup.bucket_end)
                .bind(rollup.point_count)
                .bind(rollup.value_sum)
                .bind(rollup.value_min)
                .bind(rollup.value_max)
                .bind(rollup.value_first)
                .bind(rollup.value_last)
                .bind(rollup.counter_delta)
                .bind(calculated_at)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn channel_rollup_buckets(
        &self,
        channel_id: ChannelId,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelRollupRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT channel_id,resolution,bucket_start,bucket_end,point_count,value_sum,\
             value_min,value_max,value_first,value_last,counter_delta \
             FROM telemetry.channel_rollups WHERE account_id=$1 AND channel_id=$2 \
             AND resolution=$3 AND bucket_start>=$4 AND bucket_start<$5 ORDER BY bucket_start",
        )
        .bind(self.account_id.as_uuid())
        .bind(channel_id.as_uuid())
        .bind(resolution)
        .bind(from)
        .bind(to)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(postgres_rollup).collect()
    }

    async fn mark_channel_rollups_built(
        &self,
        channel_id: ChannelId,
        received_at: i64,
        values: i64,
    ) -> Result<(), AccountRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query(
            "UPDATE account_data.channel_definitions SET rollup_watermark=$3,\
             rollup_watermark_values=$4 WHERE account_id=$1 AND id=$2",
        )
        .bind(self.account_id.as_uuid())
        .bind(channel_id.as_uuid())
        .bind(received_at)
        .bind(values)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn target_channel_rollups(
        &self,
        system_id: SystemId,
        target_kind: &str,
        target_id: Option<Uuid>,
        resolution: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelRollupRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT channel_id,resolution,bucket_start,bucket_end,point_count,value_sum,\
             value_min,value_max,value_first,value_last,counter_delta \
             FROM telemetry.channel_rollups WHERE account_id=$1 AND system_id=$2 \
             AND target_kind=$3 AND target_id IS NOT DISTINCT FROM $4 AND resolution=$5 \
             AND bucket_start>=$6 AND bucket_start<$7 ORDER BY channel_id,bucket_start",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(target_kind)
        .bind(target_id)
        .bind(resolution)
        .bind(from)
        .bind(to)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(postgres_rollup).collect()
    }
}
