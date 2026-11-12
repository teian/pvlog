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
    ) -> Result<(), AccountRepositoryError>;
    /// Reads the values of a channel measured in `[from, to)`, oldest first.
    async fn channel_values(
        &self,
        channel_id: ChannelId,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelValueRecord>, AccountRepositoryError>;
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
            "c.last_value_at,c.updated_at,",
            "(SELECT v.integer_value FROM telemetry_hot_extended_values v ",
            "JOIN telemetry_hot h ON h.observation_id=v.observation_id ",
            "WHERE v.channel_id=c.id ORDER BY h.measured_at DESC LIMIT 1) AS last_value ",
            "FROM channel_definitions c ",
            $tail
        )
    };
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
         c.updated_at,NULL AS last_value FROM channel_definitions c WHERE c.system_id=?",
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
        last_value: row.get("last_value"),
        updated_at: row.get("updated_at"),
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
        // definition would orphan every value.
        Ok(sqlx::query(
            "UPDATE channel_definitions SET lifecycle='retired',updated_at=?,\
             version=version+1 WHERE system_id=? AND channel_key=?",
        )
        .bind(now)
        .bind(blob(system_id.as_uuid()))
        .bind(channel_key)
        .execute(writer.connection())
        .await?
        .rows_affected()
            > 0)
    }

    async fn write_channel_values(
        &self,
        observation_id: ObservationId,
        _measured_at: i64,
        values: &[(ChannelId, i64)],
        received_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        if values.is_empty() {
            return Ok(());
        }
        let mut writer = self.account.acquire_writer().await?;
        for (channel, value) in values {
            sqlx::query(
                "INSERT INTO telemetry_hot_extended_values \
                 (observation_id,channel_id,integer_value) VALUES (?,?,?) \
                 ON CONFLICT(observation_id,channel_id) DO UPDATE SET \
                 integer_value=excluded.integer_value",
            )
            .bind(blob(observation_id.as_uuid()))
            .bind(blob(channel.as_uuid()))
            .bind(value)
            .execute(writer.connection())
            .await?;
            sqlx::query("UPDATE channel_definitions SET last_value_at=? WHERE id=?")
                .bind(received_at)
                .bind(blob(channel.as_uuid()))
                .execute(writer.connection())
                .await?;
        }
        Ok(())
    }

    async fn channel_values(
        &self,
        channel_id: ChannelId,
        from: i64,
        to: i64,
    ) -> Result<Vec<ChannelValueRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(sqlx::query(
            "SELECT h.measured_at AS measured_at,v.integer_value AS value \
             FROM telemetry_hot_extended_values v \
             JOIN telemetry_hot h ON h.observation_id=v.observation_id \
             WHERE v.channel_id=? AND h.measured_at>=? AND h.measured_at<? \
             ORDER BY h.measured_at",
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
            "c.last_value_at,c.updated_at,",
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
        last_value: row.get("last_value"),
        updated_at: row.get("updated_at"),
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
             c.last_value_at,c.updated_at,NULL::BIGINT AS last_value \
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
             version=version+1 WHERE account_id=$1 AND system_id=$2 AND channel_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(channel_key)
        .bind(now)
        .execute(&mut connection)
        .await?
        .rows_affected();
        connection.close().await?;
        Ok(updated > 0)
    }

    async fn write_channel_values(
        &self,
        observation_id: ObservationId,
        measured_at: i64,
        values: &[(ChannelId, i64)],
        received_at: i64,
    ) -> Result<(), AccountRepositoryError> {
        if values.is_empty() {
            return Ok(());
        }
        let mut connection = self.connection().await?;
        for (channel, value) in values {
            sqlx::query(
                "INSERT INTO telemetry.hot_extended_values \
                 (account_id,observation_id,measured_at,channel_id,integer_value) \
                 VALUES ($1,$2,$3,$4,$5) \
                 ON CONFLICT(account_id,observation_id,measured_at,channel_id) DO UPDATE SET \
                 integer_value=excluded.integer_value",
            )
            .bind(self.account_id.as_uuid())
            .bind(observation_id.as_uuid())
            .bind(measured_at)
            .bind(channel.as_uuid())
            .bind(value)
            .execute(&mut connection)
            .await?;
            sqlx::query(
                "UPDATE account_data.channel_definitions SET last_value_at=$3 \
                 WHERE account_id=$1 AND id=$2",
            )
            .bind(self.account_id.as_uuid())
            .bind(channel.as_uuid())
            .bind(received_at)
            .execute(&mut connection)
            .await?;
        }
        connection.close().await?;
        Ok(())
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
}
