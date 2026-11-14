//! Reads the facts a system's setup status and first-data check are derived from.

use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, DataSourceId, ObservationId, SystemId};
use sqlx::Row as _;
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// Stored facts about one system at one instant.
// Each flag is an independent fact, not a state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetupFactsRecord {
    pub has_location: bool,
    pub has_declared_capacity: bool,
    pub strings: u64,
    /// Data sources that were not removed.
    pub data_sources: u64,
    /// Whether any real reading is stored; test readings live elsewhere.
    pub has_reading: bool,
    /// Receive time of the most recently accepted reading.
    pub last_received_at: Option<i64>,
    /// Active channels that received a value and are bound to nothing.
    pub unassigned_channels: u64,
    pub has_effective_tariff: bool,
    pub status_interval_seconds: u32,
}

/// The most recently received reading of a system with where it came from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatestReadingRecord {
    pub observation_id: ObservationId,
    pub measured_at: i64,
    pub received_at: i64,
    pub generation_power_watts: Option<i64>,
    pub generation_energy_wh: Option<i64>,
    pub consumption_power_watts: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    /// Short text the source attached to the reading.
    pub message: Option<String>,
    pub data_source_id: Option<DataSourceId>,
    /// Name of the data source, also when it was removed since.
    pub data_source_name: Option<String>,
}

/// Read access to setup facts within one account.
#[async_trait]
pub trait SetupRepository: Send + Sync {
    /// Returns the facts of a system at `now`, or `None` when it does not exist.
    async fn setup_facts(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Option<SetupFactsRecord>, AccountRepositoryError>;
    /// Returns the most recently received real reading of a system.
    async fn latest_reading(
        &self,
        system_id: SystemId,
    ) -> Result<Option<LatestReadingRecord>, AccountRepositoryError>;
}

fn unsigned(value: i64) -> Result<u64, AccountRepositoryError> {
    u64::try_from(value).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

fn interval(value: i64) -> Result<u32, AccountRepositoryError> {
    u32::try_from(value).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

/// `SQLite` setup repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteSetupRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteSetupRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl SetupRepository for SqliteSetupRepository {
    async fn setup_facts(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Option<SetupFactsRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row = sqlx::query(
            "SELECT s.latitude_e6 IS NOT NULL AS has_location,\
             s.declared_capacity_watts IS NOT NULL AS has_declared_capacity,\
             s.status_interval_seconds,\
             (SELECT COUNT(*) FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
              WHERE i.system_id=s.id) AS strings,\
             (SELECT COUNT(*) FROM data_sources d \
              WHERE d.system_id=s.id AND d.deleted_at IS NULL) AS data_sources,\
             (SELECT MAX(h.received_at) FROM telemetry_hot h WHERE h.system_id=s.id) \
              AS last_received_at,\
             (SELECT COUNT(*) FROM channel_definitions c WHERE c.system_id=s.id \
              AND c.lifecycle='active' AND c.target_kind='unassigned' \
              AND c.last_value_at IS NOT NULL) AS unassigned_channels,\
             EXISTS (SELECT 1 FROM tariffs t WHERE t.system_id=s.id AND t.effective_from<=?1 \
              AND (t.effective_to IS NULL OR t.effective_to>?1)) AS has_effective_tariff \
             FROM systems s WHERE s.id=?2",
        )
        .bind(now)
        .bind(system_id.as_uuid().as_bytes().to_vec())
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            let last_received_at: Option<i64> = row.get("last_received_at");
            Ok(SetupFactsRecord {
                has_location: row.get::<i64, _>("has_location") != 0,
                has_declared_capacity: row.get::<i64, _>("has_declared_capacity") != 0,
                strings: unsigned(row.get("strings"))?,
                data_sources: unsigned(row.get("data_sources"))?,
                has_reading: last_received_at.is_some(),
                last_received_at,
                unassigned_channels: unsigned(row.get("unassigned_channels"))?,
                has_effective_tariff: row.get::<i64, _>("has_effective_tariff") != 0,
                status_interval_seconds: interval(row.get("status_interval_seconds"))?,
            })
        })
        .transpose()
    }

    async fn latest_reading(
        &self,
        system_id: SystemId,
    ) -> Result<Option<LatestReadingRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row = sqlx::query(
            "SELECT h.observation_id,h.measured_at,h.received_at,h.generation_power_watts,\
             h.generation_energy_wh,h.consumption_power_watts,h.consumption_energy_wh,h.message,\
             h.data_source_id,d.name AS data_source_name FROM telemetry_hot h \
             LEFT JOIN data_sources d ON d.id=h.data_source_id WHERE h.system_id=? \
             ORDER BY h.received_at DESC,h.measured_at DESC LIMIT 1",
        )
        .bind(system_id.as_uuid().as_bytes().to_vec())
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            let uuid = |bytes: Vec<u8>| {
                Uuid::from_slice(&bytes).map_err(|_| AccountRepositoryError::InvalidStoredValue)
            };
            Ok(LatestReadingRecord {
                observation_id: ObservationId::from_uuid(uuid(row.get("observation_id"))?)
                    .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
                measured_at: row.get("measured_at"),
                received_at: row.get("received_at"),
                generation_power_watts: row.get("generation_power_watts"),
                generation_energy_wh: row.get("generation_energy_wh"),
                consumption_power_watts: row.get("consumption_power_watts"),
                consumption_energy_wh: row.get("consumption_energy_wh"),
                message: row.get("message"),
                data_source_id: row
                    .get::<Option<Vec<u8>>, _>("data_source_id")
                    .map(|bytes| {
                        DataSourceId::from_uuid(uuid(bytes)?)
                            .map_err(|_| AccountRepositoryError::InvalidStoredValue)
                    })
                    .transpose()?,
                data_source_name: row.get("data_source_name"),
            })
        })
        .transpose()
    }
}

/// `PostgreSQL` setup repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresSetupRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresSetupRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresSetupRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresSetupRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl SetupRepository for PostgresSetupRepository {
    async fn setup_facts(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Option<SetupFactsRecord>, AccountRepositoryError> {
        let mut connection = crate::postgres_connection(&self.url).await?;
        let row = sqlx::query(
            "SELECT s.latitude_e6 IS NOT NULL AS has_location,\
             s.declared_capacity_watts IS NOT NULL AS has_declared_capacity,\
             s.status_interval_seconds::BIGINT AS status_interval_seconds,\
             (SELECT COUNT(*) FROM account_data.pv_strings p JOIN account_data.inverters i \
              ON i.account_id=p.account_id AND i.id=p.inverter_id \
              WHERE i.account_id=s.account_id AND i.system_id=s.id) AS strings,\
             (SELECT COUNT(*) FROM account_data.data_sources d WHERE d.account_id=s.account_id \
              AND d.system_id=s.id AND d.deleted_at IS NULL) AS data_sources,\
             (SELECT MAX(h.received_at) FROM telemetry.hot_observations h \
              WHERE h.account_id=s.account_id AND h.system_id=s.id) AS last_received_at,\
             (SELECT COUNT(*) FROM account_data.channel_definitions c \
              WHERE c.account_id=s.account_id AND c.system_id=s.id AND c.lifecycle='active' \
              AND c.target_kind='unassigned' AND c.last_value_at IS NOT NULL) \
              AS unassigned_channels,\
             EXISTS (SELECT 1 FROM account_data.tariffs t WHERE t.account_id=s.account_id \
              AND t.system_id=s.id AND t.effective_from<=$3 \
              AND (t.effective_to IS NULL OR t.effective_to>$3)) AS has_effective_tariff \
             FROM account_data.systems s WHERE s.account_id=$1 AND s.id=$2",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(now)
        .fetch_optional(&mut *connection)
        .await?;
        connection.close().await?;
        row.map(|row| {
            let last_received_at: Option<i64> = row.get("last_received_at");
            Ok(SetupFactsRecord {
                has_location: row.get("has_location"),
                has_declared_capacity: row.get("has_declared_capacity"),
                strings: unsigned(row.get("strings"))?,
                data_sources: unsigned(row.get("data_sources"))?,
                has_reading: last_received_at.is_some(),
                last_received_at,
                unassigned_channels: unsigned(row.get("unassigned_channels"))?,
                has_effective_tariff: row.get("has_effective_tariff"),
                status_interval_seconds: interval(row.get("status_interval_seconds"))?,
            })
        })
        .transpose()
    }

    async fn latest_reading(
        &self,
        system_id: SystemId,
    ) -> Result<Option<LatestReadingRecord>, AccountRepositoryError> {
        let mut connection = crate::postgres_connection(&self.url).await?;
        let row = sqlx::query(
            "SELECT h.observation_id,h.measured_at,h.received_at,h.generation_power_watts,\
             h.generation_energy_wh,h.consumption_power_watts,h.consumption_energy_wh,h.message,\
             h.data_source_id,d.name AS data_source_name FROM telemetry.hot_observations h \
             LEFT JOIN account_data.data_sources d \
              ON d.account_id=h.account_id AND d.id=h.data_source_id \
             WHERE h.account_id=$1 AND h.system_id=$2 \
             ORDER BY h.received_at DESC,h.measured_at DESC LIMIT 1",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_optional(&mut *connection)
        .await?;
        connection.close().await?;
        row.map(|row| {
            let invalid = |_| AccountRepositoryError::InvalidStoredValue;
            Ok(LatestReadingRecord {
                observation_id: ObservationId::from_uuid(row.get("observation_id"))
                    .map_err(invalid)?,
                measured_at: row.get("measured_at"),
                received_at: row.get("received_at"),
                generation_power_watts: row.get("generation_power_watts"),
                generation_energy_wh: row.get("generation_energy_wh"),
                consumption_power_watts: row.get("consumption_power_watts"),
                consumption_energy_wh: row.get("consumption_energy_wh"),
                message: row.get("message"),
                data_source_id: row
                    .get::<Option<Uuid>, _>("data_source_id")
                    .map(DataSourceId::from_uuid)
                    .transpose()
                    .map_err(invalid)?,
                data_source_name: row.get("data_source_name"),
            })
        })
        .transpose()
    }
}
