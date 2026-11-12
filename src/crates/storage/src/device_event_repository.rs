//! Account-storage adapter for the events devices report.
//!
//! An event belongs to a device, named as its source names it, and is
//! identified within that device by its event key. Storing an event that is
//! already stored changes nothing unless its content differs.

use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, DataSourceId, SystemId};
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// What a device says about an event beyond its code and message.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceEventDetails {
    pub event_type: Option<String>,
    /// Severity category, such as an error or a warning.
    pub category: Option<String>,
    pub group: Option<String>,
    /// Value a changed setting had before the event.
    pub old_value: Option<String>,
    /// Value a changed setting has since the event.
    pub new_value: Option<String>,
    /// User group that caused the event, such as an installer.
    pub user_group: Option<String>,
}

impl DeviceEventDetails {
    fn texts(&self) -> [&Option<String>; 6] {
        [
            &self.event_type,
            &self.category,
            &self.group,
            &self.old_value,
            &self.new_value,
            &self.user_group,
        ]
    }
}

/// Reads the details of an event from a row; both databases use the same column names.
fn details<'r, R>(row: &'r R) -> DeviceEventDetails
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    Option<String>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    DeviceEventDetails {
        event_type: row.get("event_type"),
        category: row.get("category"),
        group: row.get("event_group"),
        old_value: row.get("old_value"),
        new_value: row.get("new_value"),
        user_group: row.get("user_group"),
    }
}

/// One event a device reported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceEventRecord {
    pub id: Uuid,
    pub system_id: SystemId,
    /// The device as the source names it; for an inverter its serial.
    pub device_key: String,
    /// Identity of the event within its device.
    pub event_key: String,
    /// Data source the event was delivered through, when it is known.
    pub data_source_id: Option<DataSourceId>,
    pub occurred_at: i64,
    pub code: Option<i64>,
    pub message: Option<String>,
    pub details: DeviceEventDetails,
    pub received_at: i64,
}

/// Which events of a system to read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceEventFilter<'a> {
    /// Only events of this device.
    pub device_key: Option<&'a str>,
    /// Only events that occurred at or after this instant.
    pub from: Option<i64>,
    /// Only events that occurred before this instant.
    pub to: Option<i64>,
    /// Largest number of events to return.
    pub limit: u32,
}

/// Persistence of device events within one account.
#[async_trait]
pub trait DeviceEventRepository: Send + Sync {
    /// Stores events and returns how many were new or changed. An event whose
    /// device and event key are already stored with the same content is left
    /// as it is; one with different content replaces the stored one. Either
    /// all events are stored or none.
    async fn save_device_events(
        &self,
        events: &[DeviceEventRecord],
    ) -> Result<u64, AccountRepositoryError>;
    /// Reads the events of a system, newest first.
    async fn device_events(
        &self,
        system_id: SystemId,
        filter: DeviceEventFilter<'_>,
    ) -> Result<Vec<DeviceEventRecord>, AccountRepositoryError>;
    /// Reads the serial of an inverter of a system. The outer value is absent
    /// for an unknown inverter, the inner one for an inverter without a serial.
    async fn inverter_serial(
        &self,
        system_id: SystemId,
        inverter_id: Uuid,
    ) -> Result<Option<Option<String>>, AccountRepositoryError>;
}

fn validate(event: &DeviceEventRecord) -> Result<(), AccountRepositoryError> {
    let characters = |text: &str| text.chars().count();
    if !(1..=64).contains(&characters(&event.device_key))
        || !(1..=160).contains(&characters(&event.event_key))
        || event
            .message
            .as_ref()
            .is_some_and(|message| characters(message) > 500)
        || event
            .details
            .texts()
            .into_iter()
            .flatten()
            .any(|text| characters(text) > 500)
    {
        return Err(AccountRepositoryError::InvalidRecord("device event"));
    }
    Ok(())
}

/// `SQLite` device event repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteDeviceEventRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteDeviceEventRepository {
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
#[async_trait]
impl DeviceEventRepository for SqliteDeviceEventRepository {
    async fn save_device_events(
        &self,
        events: &[DeviceEventRecord],
    ) -> Result<u64, AccountRepositoryError> {
        use sqlx::Connection as _;
        events.iter().try_for_each(validate)?;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        let mut stored = 0;
        for event in events {
            stored += sqlx::query(
                "INSERT INTO device_events (id,system_id,device_key,event_key,data_source_id,\
                 occurred_at,code,message,received_at,event_type,category,event_group,\
                 old_value,new_value,user_group) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
                 ON CONFLICT(system_id,device_key,event_key) DO UPDATE SET \
                 occurred_at=excluded.occurred_at,code=excluded.code,message=excluded.message,\
                 data_source_id=excluded.data_source_id,received_at=excluded.received_at,\
                 event_type=excluded.event_type,category=excluded.category,\
                 event_group=excluded.event_group,old_value=excluded.old_value,\
                 new_value=excluded.new_value,user_group=excluded.user_group \
                 WHERE device_events.occurred_at IS NOT excluded.occurred_at \
                 OR device_events.code IS NOT excluded.code \
                 OR device_events.message IS NOT excluded.message \
                 OR device_events.event_type IS NOT excluded.event_type \
                 OR device_events.category IS NOT excluded.category \
                 OR device_events.event_group IS NOT excluded.event_group \
                 OR device_events.old_value IS NOT excluded.old_value \
                 OR device_events.new_value IS NOT excluded.new_value \
                 OR device_events.user_group IS NOT excluded.user_group",
            )
            .bind(blob(event.id))
            .bind(blob(event.system_id.as_uuid()))
            .bind(&event.device_key)
            .bind(&event.event_key)
            .bind(event.data_source_id.map(|id| blob(id.as_uuid())))
            .bind(event.occurred_at)
            .bind(event.code)
            .bind(event.message.as_deref())
            .bind(event.received_at)
            .bind(event.details.event_type.as_deref())
            .bind(event.details.category.as_deref())
            .bind(event.details.group.as_deref())
            .bind(event.details.old_value.as_deref())
            .bind(event.details.new_value.as_deref())
            .bind(event.details.user_group.as_deref())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        }
        transaction.commit().await?;
        Ok(stored)
    }

    async fn device_events(
        &self,
        system_id: SystemId,
        filter: DeviceEventFilter<'_>,
    ) -> Result<Vec<DeviceEventRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT id,device_key,event_key,data_source_id,occurred_at,code,message,received_at,\
             event_type,category,event_group,old_value,new_value,user_group \
             FROM device_events WHERE system_id=?1 AND (?2 IS NULL OR device_key=?2) \
             AND occurred_at>=?3 AND occurred_at<?4 \
             ORDER BY occurred_at DESC,id DESC LIMIT ?5",
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
            let uuid = |bytes: Vec<u8>| {
                Uuid::from_slice(&bytes).map_err(|_| AccountRepositoryError::InvalidStoredValue)
            };
            Ok(DeviceEventRecord {
                id: uuid(row.get("id"))?,
                system_id,
                device_key: row.get("device_key"),
                event_key: row.get("event_key"),
                data_source_id: row
                    .get::<Option<Vec<u8>>, _>("data_source_id")
                    .map(|bytes| {
                        DataSourceId::from_uuid(uuid(bytes)?)
                            .map_err(|_| AccountRepositoryError::InvalidStoredValue)
                    })
                    .transpose()?,
                occurred_at: row.get("occurred_at"),
                code: row.get("code"),
                message: row.get("message"),
                details: details(row),
                received_at: row.get("received_at"),
            })
        })
        .collect()
    }

    async fn inverter_serial(
        &self,
        system_id: SystemId,
        inverter_id: Uuid,
    ) -> Result<Option<Option<String>>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(
            sqlx::query("SELECT serial_reference FROM inverters WHERE id=? AND system_id=?")
                .bind(blob(inverter_id))
                .bind(blob(system_id.as_uuid()))
                .fetch_optional(&mut *connection)
                .await?
                .map(|row| row.get("serial_reference")),
        )
    }
}

/// `PostgreSQL` device event repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresDeviceEventRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresDeviceEventRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresDeviceEventRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresDeviceEventRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl DeviceEventRepository for PostgresDeviceEventRepository {
    async fn save_device_events(
        &self,
        events: &[DeviceEventRecord],
    ) -> Result<u64, AccountRepositoryError> {
        events.iter().try_for_each(validate)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        let mut stored = 0;
        for event in events {
            stored += sqlx::query(
                "INSERT INTO telemetry.device_events AS e (account_id,id,system_id,device_key,\
                 event_key,data_source_id,occurred_at,code,message,received_at,event_type,\
                 category,event_group,old_value,new_value,user_group) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) \
                 ON CONFLICT(account_id,system_id,device_key,event_key) DO UPDATE SET \
                 occurred_at=excluded.occurred_at,code=excluded.code,message=excluded.message,\
                 data_source_id=excluded.data_source_id,received_at=excluded.received_at,\
                 event_type=excluded.event_type,category=excluded.category,\
                 event_group=excluded.event_group,old_value=excluded.old_value,\
                 new_value=excluded.new_value,user_group=excluded.user_group \
                 WHERE e.occurred_at IS DISTINCT FROM excluded.occurred_at \
                 OR e.code IS DISTINCT FROM excluded.code \
                 OR e.message IS DISTINCT FROM excluded.message \
                 OR e.event_type IS DISTINCT FROM excluded.event_type \
                 OR e.category IS DISTINCT FROM excluded.category \
                 OR e.event_group IS DISTINCT FROM excluded.event_group \
                 OR e.old_value IS DISTINCT FROM excluded.old_value \
                 OR e.new_value IS DISTINCT FROM excluded.new_value \
                 OR e.user_group IS DISTINCT FROM excluded.user_group",
            )
            .bind(self.account_id.as_uuid())
            .bind(event.id)
            .bind(event.system_id.as_uuid())
            .bind(&event.device_key)
            .bind(&event.event_key)
            .bind(event.data_source_id.map(|id| id.as_uuid()))
            .bind(event.occurred_at)
            .bind(event.code)
            .bind(event.message.as_deref())
            .bind(event.received_at)
            .bind(event.details.event_type.as_deref())
            .bind(event.details.category.as_deref())
            .bind(event.details.group.as_deref())
            .bind(event.details.old_value.as_deref())
            .bind(event.details.new_value.as_deref())
            .bind(event.details.user_group.as_deref())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(stored)
    }

    async fn device_events(
        &self,
        system_id: SystemId,
        filter: DeviceEventFilter<'_>,
    ) -> Result<Vec<DeviceEventRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT id,device_key,event_key,data_source_id,occurred_at,code,message,received_at,\
             event_type,category,event_group,old_value,new_value,user_group \
             FROM telemetry.device_events WHERE account_id=$1 AND system_id=$2 \
             AND ($3::TEXT IS NULL OR device_key=$3) AND occurred_at>=$4 AND occurred_at<$5 \
             ORDER BY occurred_at DESC,id DESC LIMIT $6",
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
                Ok(DeviceEventRecord {
                    id: row.get("id"),
                    system_id,
                    device_key: row.get("device_key"),
                    event_key: row.get("event_key"),
                    data_source_id: row
                        .get::<Option<Uuid>, _>("data_source_id")
                        .map(DataSourceId::from_uuid)
                        .transpose()
                        .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
                    occurred_at: row.get("occurred_at"),
                    code: row.get("code"),
                    message: row.get("message"),
                    details: details(row),
                    received_at: row.get("received_at"),
                })
            })
            .collect()
    }

    async fn inverter_serial(
        &self,
        system_id: SystemId,
        inverter_id: Uuid,
    ) -> Result<Option<Option<String>>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT serial_reference FROM account_data.inverters \
             WHERE account_id=$1 AND id=$2 AND system_id=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(inverter_id)
        .bind(system_id.as_uuid())
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        Ok(row.map(|row| row.get("serial_reference")))
    }
}
