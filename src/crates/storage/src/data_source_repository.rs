//! Account-storage adapter for the data sources that feed a system.
//!
//! A data source row lives next to the system it feeds. Its upload credential
//! lives in the management database; only the credential's identifier is kept
//! here. Removing a source keeps the row as a tombstone so the readings it
//! delivered stay attributed to it.

use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, ApiCredentialId, DataSourceId, SystemId};
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// One uploader or import feeding a system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataSourceRecord {
    pub id: DataSourceId,
    pub system_id: SystemId,
    pub name: String,
    /// Identifier of the source profile, such as `smalog`.
    pub profile: String,
    /// Upload credential issued for this source; absent once it was removed.
    pub credential_id: Option<ApiCredentialId>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Set when the source was removed; its readings stay attributed.
    pub deleted_at: Option<i64>,
}

/// What a data source has delivered, read from the stored readings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataSourceActivity {
    pub data_source_id: DataSourceId,
    /// Measurement time of the newest accepted reading.
    pub last_measured_at: i64,
    /// Receive time of the most recently accepted reading.
    pub last_received_at: i64,
    /// Readings received at or after the requested instant.
    pub recent_readings: u64,
}

/// Persistence of data sources within one account.
#[async_trait]
pub trait DataSourceRepository: Send + Sync {
    /// Inserts a data source or replaces its name, credential and removal state.
    async fn save_data_source(
        &self,
        record: &DataSourceRecord,
    ) -> Result<(), AccountRepositoryError>;
    /// Lists the data sources of a system that were not removed, oldest first.
    async fn data_sources(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<DataSourceRecord>, AccountRepositoryError>;
    /// Reads one data source of a system, including a removed one.
    async fn data_source(
        &self,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<Option<DataSourceRecord>, AccountRepositoryError>;
    /// Finds the live data source that owns a credential.
    async fn data_source_by_credential(
        &self,
        credential_id: ApiCredentialId,
    ) -> Result<Option<DataSourceRecord>, AccountRepositoryError>;
    /// Summarizes the readings each data source of a system delivered, counting
    /// those received at or after `recent_since`.
    async fn data_source_activity(
        &self,
        system_id: SystemId,
        recent_since: i64,
    ) -> Result<Vec<DataSourceActivity>, AccountRepositoryError>;
}

fn validate(record: &DataSourceRecord) -> Result<(), AccountRepositoryError> {
    if record.name.trim().is_empty() || record.profile.trim().is_empty() {
        return Err(AccountRepositoryError::InvalidRecord("data source"));
    }
    Ok(())
}

fn count(value: i64) -> Result<u64, AccountRepositoryError> {
    u64::try_from(value).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

/// `SQLite` data source repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteDataSourceRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteDataSourceRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "sqlite")]
fn sqlite_record(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<DataSourceRecord, AccountRepositoryError> {
    let id = |column: &str| -> Result<Uuid, AccountRepositoryError> {
        Uuid::from_slice(&row.get::<Vec<u8>, _>(column))
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)
    };
    Ok(DataSourceRecord {
        id: DataSourceId::from_uuid(id("id")?)
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        system_id: SystemId::from_uuid(id("system_id")?)
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        name: row.get("name"),
        profile: row.get("profile"),
        credential_id: row
            .get::<Option<Vec<u8>>, _>("credential_id")
            .map(|bytes| {
                Uuid::from_slice(&bytes)
                    .ok()
                    .and_then(|uuid| ApiCredentialId::from_uuid(uuid).ok())
                    .ok_or(AccountRepositoryError::InvalidStoredValue)
            })
            .transpose()?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.get("deleted_at"),
    })
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl DataSourceRepository for SqliteDataSourceRepository {
    async fn save_data_source(
        &self,
        record: &DataSourceRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate(record)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query(
            "INSERT INTO data_sources \
             (id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at) \
             VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,\
             credential_id=excluded.credential_id,updated_at=excluded.updated_at,\
             deleted_at=excluded.deleted_at,version=data_sources.version+1",
        )
        .bind(blob(record.id.as_uuid()))
        .bind(blob(record.system_id.as_uuid()))
        .bind(&record.name)
        .bind(&record.profile)
        .bind(record.credential_id.map(|id| blob(id.as_uuid())))
        .bind(record.created_at)
        .bind(record.updated_at)
        .bind(record.deleted_at)
        .execute(writer.connection())
        .await?;
        Ok(())
    }

    async fn data_sources(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<DataSourceRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at FROM data_sources \
             WHERE system_id=? AND deleted_at IS NULL ORDER BY created_at,id"
)
        .bind(blob(system_id.as_uuid()))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(sqlite_record)
        .collect()
    }

    async fn data_source(
        &self,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<Option<DataSourceRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at FROM data_sources WHERE system_id=? AND id=?"
)
        .bind(blob(system_id.as_uuid()))
        .bind(blob(id.as_uuid()))
        .fetch_optional(&mut *connection)
        .await?
        .as_ref()
        .map(sqlite_record)
        .transpose()
    }

    async fn data_source_by_credential(
        &self,
        credential_id: ApiCredentialId,
    ) -> Result<Option<DataSourceRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at FROM data_sources \
             WHERE credential_id=? AND deleted_at IS NULL"
)
        .bind(blob(credential_id.as_uuid()))
        .fetch_optional(&mut *connection)
        .await?
        .as_ref()
        .map(sqlite_record)
        .transpose()
    }

    async fn data_source_activity(
        &self,
        system_id: SystemId,
        recent_since: i64,
    ) -> Result<Vec<DataSourceActivity>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT data_source_id,MAX(measured_at) AS last_measured_at,\
             MAX(received_at) AS last_received_at,\
             SUM(CASE WHEN received_at>=? THEN 1 ELSE 0 END) AS recent_readings \
             FROM telemetry_hot WHERE system_id=? AND data_source_id IS NOT NULL \
             GROUP BY data_source_id",
        )
        .bind(recent_since)
        .bind(blob(system_id.as_uuid()))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| {
            let id = Uuid::from_slice(&row.get::<Vec<u8>, _>("data_source_id"))
                .ok()
                .and_then(|uuid| DataSourceId::from_uuid(uuid).ok())
                .ok_or(AccountRepositoryError::InvalidStoredValue)?;
            Ok(DataSourceActivity {
                data_source_id: id,
                last_measured_at: row.get("last_measured_at"),
                last_received_at: row.get("last_received_at"),
                recent_readings: count(row.get("recent_readings"))?,
            })
        })
        .collect()
    }
}

/// `PostgreSQL` data source repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresDataSourceRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresDataSourceRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresDataSourceRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresDataSourceRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "postgres")]
fn postgres_record(
    row: &sqlx::postgres::PgRow,
) -> Result<DataSourceRecord, AccountRepositoryError> {
    let invalid = |_| AccountRepositoryError::InvalidStoredValue;
    Ok(DataSourceRecord {
        id: DataSourceId::from_uuid(row.get("id")).map_err(invalid)?,
        system_id: SystemId::from_uuid(row.get("system_id")).map_err(invalid)?,
        name: row.get("name"),
        profile: row.get("profile"),
        credential_id: row
            .get::<Option<Uuid>, _>("credential_id")
            .map(ApiCredentialId::from_uuid)
            .transpose()
            .map_err(invalid)?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.get("deleted_at"),
    })
}

#[cfg(feature = "postgres")]
#[async_trait]
impl DataSourceRepository for PostgresDataSourceRepository {
    async fn save_data_source(
        &self,
        record: &DataSourceRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate(record)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO account_data.data_sources \
             (account_id,id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(account_id,id) DO UPDATE SET \
             name=excluded.name,credential_id=excluded.credential_id,\
             updated_at=excluded.updated_at,deleted_at=excluded.deleted_at,\
             version=account_data.data_sources.version+1",
        )
        .bind(self.account_id.as_uuid())
        .bind(record.id.as_uuid())
        .bind(record.system_id.as_uuid())
        .bind(&record.name)
        .bind(&record.profile)
        .bind(record.credential_id.map(|id| id.as_uuid()))
        .bind(record.created_at)
        .bind(record.updated_at)
        .bind(record.deleted_at)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn data_sources(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<DataSourceRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at FROM account_data.data_sources \
             WHERE account_id=$1 AND system_id=$2 AND deleted_at IS NULL ORDER BY created_at,id"
)
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(postgres_record).collect()
    }

    async fn data_source(
        &self,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<Option<DataSourceRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at FROM account_data.data_sources \
             WHERE account_id=$1 AND system_id=$2 AND id=$3"
)
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.as_ref().map(postgres_record).transpose()
    }

    async fn data_source_by_credential(
        &self,
        credential_id: ApiCredentialId,
    ) -> Result<Option<DataSourceRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT id,system_id,name,profile,credential_id,created_at,updated_at,deleted_at FROM account_data.data_sources \
             WHERE account_id=$1 AND credential_id=$2 AND deleted_at IS NULL"
)
        .bind(self.account_id.as_uuid())
        .bind(credential_id.as_uuid())
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.as_ref().map(postgres_record).transpose()
    }

    async fn data_source_activity(
        &self,
        system_id: SystemId,
        recent_since: i64,
    ) -> Result<Vec<DataSourceActivity>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT data_source_id,MAX(measured_at) AS last_measured_at,\
             MAX(received_at) AS last_received_at,\
             COUNT(*) FILTER (WHERE received_at>=$3) AS recent_readings \
             FROM telemetry.hot_observations \
             WHERE account_id=$1 AND system_id=$2 AND data_source_id IS NOT NULL \
             GROUP BY data_source_id",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(recent_since)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter()
            .map(|row| {
                Ok(DataSourceActivity {
                    data_source_id: DataSourceId::from_uuid(row.get("data_source_id"))
                        .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
                    last_measured_at: row.get("last_measured_at"),
                    last_received_at: row.get("last_received_at"),
                    recent_readings: count(row.get("recent_readings"))?,
                })
            })
            .collect()
    }
}
