//! Account-storage adapter for equipment discovery proposals.
//!
//! A proposal records a device a source reported that matches no inverter of
//! the system. It is created by ingestion and decided by the owner; storage
//! never creates equipment on its own.

use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, SystemId};
use sqlx::Row as _;
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// What a source reported about a device.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveredDeviceRecord {
    pub device_serial: String,
    pub reported_name: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
}

/// The identity a source reported for one of its devices.
///
/// A member that is `None` was not reported and leaves the stored value as it is.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReportedDeviceRecord {
    pub device_serial: String,
    /// Kind of device as the source names it; `inverter` when not stated.
    pub kind: Option<String>,
    pub family_id: Option<String>,
    pub configured_name: Option<String>,
    pub reported_name: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub transport: Option<String>,
}

/// The latest identity held for a device, with the times it was reported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredReportedDevice {
    pub identity: ReportedDeviceRecord,
    pub first_reported_at: i64,
    pub last_reported_at: i64,
}

/// A device proposed as an inverter of a system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryProposalRecord {
    pub id: Uuid,
    pub system_id: SystemId,
    pub device_serial: String,
    pub reported_name: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    /// `open`, `dismissed` or `adopted`.
    pub state: String,
    /// The inverter an adopted proposal created.
    pub inverter_id: Option<Uuid>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub decided_at: Option<i64>,
}

/// Persistence of equipment discovery proposals within one account.
#[async_trait]
pub trait DiscoveryRepository: Send + Sync {
    /// Records that a source reported a device.
    ///
    /// Nothing is stored while an inverter of the system carries the serial.
    /// Otherwise the device's proposal is created, or updated with the newer
    /// identity; a dismissed proposal stays dismissed, and an adopted one whose
    /// inverter is gone is open again.
    async fn record_discovered_device(
        &self,
        system_id: SystemId,
        device: &DiscoveredDeviceRecord,
        now: i64,
    ) -> Result<(), AccountRepositoryError>;
    /// Keeps the identity a source reported for a device.
    ///
    /// The inverter of the system that carries the serial takes the reported
    /// firmware, with `now` as the time of the change when the version differs
    /// from the one held, and the reported model unless its model comes from
    /// the equipment catalog. Returns whether an inverter was changed.
    async fn record_device_identity(
        &self,
        system_id: SystemId,
        device: &ReportedDeviceRecord,
        now: i64,
    ) -> Result<bool, AccountRepositoryError>;
    /// Lists the latest identity of every device reported for a system, by serial.
    async fn reported_devices(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<StoredReportedDevice>, AccountRepositoryError>;
    /// Lists the proposals of a system in every state, oldest first.
    async fn discovery_proposals(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<DiscoveryProposalRecord>, AccountRepositoryError>;
    /// Stores the decision on a proposal and reports whether it exists.
    /// `inverter_id` names the inverter an adoption created.
    async fn decide_discovery_proposal(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
        state: &str,
        inverter_id: Option<Uuid>,
        now: i64,
    ) -> Result<bool, AccountRepositoryError>;
    /// Lists the serials of the inverters of a system that are in effect at `now`.
    async fn inverter_serials(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Vec<String>, AccountRepositoryError>;
}

fn validate_device(device: &DiscoveredDeviceRecord) -> Result<(), AccountRepositoryError> {
    if device.device_serial.trim().is_empty() {
        return Err(AccountRepositoryError::InvalidRecord("discovered device"));
    }
    Ok(())
}

fn validate_identity(device: &ReportedDeviceRecord) -> Result<(), AccountRepositoryError> {
    let length = device.device_serial.chars().count();
    if device.device_serial.trim().is_empty() || length > 64 {
        return Err(AccountRepositoryError::InvalidRecord("reported device"));
    }
    Ok(())
}

/// Reads a reported device from a row; both databases use the same column names.
fn reported_device<'r, R>(row: &'r R) -> StoredReportedDevice
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    Option<String>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    StoredReportedDevice {
        identity: ReportedDeviceRecord {
            device_serial: row.get("device_serial"),
            kind: Some(row.get("kind")),
            family_id: row.get("family_id"),
            configured_name: row.get("configured_name"),
            reported_name: row.get("reported_name"),
            model: row.get("model"),
            firmware: row.get("firmware"),
            transport: row.get("transport"),
        },
        first_reported_at: row.get("first_reported_at"),
        last_reported_at: row.get("last_reported_at"),
    }
}

fn validate_state(state: &str) -> Result<(), AccountRepositoryError> {
    if matches!(state, "open" | "dismissed" | "adopted") {
        Ok(())
    } else {
        Err(AccountRepositoryError::InvalidRecord("discovery proposal"))
    }
}

/// `SQLite` discovery repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteDiscoveryRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteDiscoveryRepository {
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
fn sqlite_proposal(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<DiscoveryProposalRecord, AccountRepositoryError> {
    let uuid = |bytes: Vec<u8>| {
        Uuid::from_slice(&bytes).map_err(|_| AccountRepositoryError::InvalidStoredValue)
    };
    Ok(DiscoveryProposalRecord {
        id: uuid(row.get("id"))?,
        system_id: SystemId::from_uuid(uuid(row.get("system_id"))?)
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        device_serial: row.get("device_serial"),
        reported_name: row.get("reported_name"),
        model: row.get("model"),
        firmware: row.get("firmware"),
        state: row.get("state"),
        inverter_id: row
            .get::<Option<Vec<u8>>, _>("inverter_id")
            .map(uuid)
            .transpose()?,
        first_seen_at: row.get("first_seen_at"),
        last_seen_at: row.get("last_seen_at"),
        decided_at: row.get("decided_at"),
    })
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl DiscoveryRepository for SqliteDiscoveryRepository {
    async fn record_device_identity(
        &self,
        system_id: SystemId,
        device: &ReportedDeviceRecord,
        now: i64,
    ) -> Result<bool, AccountRepositoryError> {
        validate_identity(device)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query(
            "INSERT INTO reported_devices (system_id,device_serial,kind,family_id,\
             configured_name,reported_name,model,firmware,transport,first_reported_at,\
             last_reported_at) VALUES (?,?,COALESCE(?,'inverter'),?,?,?,?,?,?,?,?) \
             ON CONFLICT(system_id,device_serial) DO UPDATE SET \
             kind=COALESCE(?,kind),\
             family_id=COALESCE(excluded.family_id,family_id),\
             configured_name=COALESCE(excluded.configured_name,configured_name),\
             reported_name=COALESCE(excluded.reported_name,reported_name),\
             model=COALESCE(excluded.model,model),\
             firmware=COALESCE(excluded.firmware,firmware),\
             transport=COALESCE(excluded.transport,transport),\
             last_reported_at=excluded.last_reported_at",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(&device.device_serial)
        .bind(device.kind.as_deref())
        .bind(device.family_id.as_deref())
        .bind(device.configured_name.as_deref())
        .bind(device.reported_name.as_deref())
        .bind(device.model.as_deref())
        .bind(device.firmware.as_deref())
        .bind(device.transport.as_deref())
        .bind(now)
        .bind(now)
        .bind(device.kind.as_deref())
        .execute(writer.connection())
        .await?;
        // Every right-hand side reads the row as it was before the update.
        let changed = sqlx::query(
            "UPDATE inverters SET \
             firmware_changed_at=CASE WHEN ?1 IS NOT NULL AND firmware_version IS NOT ?1 \
               THEN ?3 ELSE firmware_changed_at END,\
             firmware_version=COALESCE(?1,firmware_version),\
             model=CASE WHEN catalog_entry_id IS NULL THEN COALESCE(?2,model) ELSE model END,\
             updated_at=?3,version=version+1 \
             WHERE system_id=?4 AND serial_reference=?5 AND effective_from<=?3 \
             AND (effective_to IS NULL OR effective_to>?3) \
             AND ((?1 IS NOT NULL AND firmware_version IS NOT ?1) \
               OR (?2 IS NOT NULL AND catalog_entry_id IS NULL AND model IS NOT ?2))",
        )
        .bind(device.firmware.as_deref())
        .bind(device.model.as_deref())
        .bind(now)
        .bind(blob(system_id.as_uuid()))
        .bind(&device.device_serial)
        .execute(writer.connection())
        .await?
        .rows_affected();
        Ok(changed > 0)
    }

    async fn reported_devices(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<StoredReportedDevice>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(sqlx::query(
            "SELECT device_serial,kind,family_id,configured_name,reported_name,model,firmware,\
             transport,first_reported_at,last_reported_at FROM reported_devices \
             WHERE system_id=? ORDER BY device_serial",
        )
        .bind(blob(system_id.as_uuid()))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(reported_device)
        .collect())
    }

    async fn record_discovered_device(
        &self,
        system_id: SystemId,
        device: &DiscoveredDeviceRecord,
        now: i64,
    ) -> Result<(), AccountRepositoryError> {
        validate_device(device)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query(
            "INSERT INTO equipment_discovery_proposals \
             (id,system_id,device_serial,reported_name,model,firmware,state,\
              first_seen_at,last_seen_at) \
             SELECT ?,?,?,?,?,?,'open',?,? WHERE NOT EXISTS (\
               SELECT 1 FROM inverters WHERE system_id=? AND serial_reference=? \
               AND effective_from<=? AND (effective_to IS NULL OR effective_to>?)) \
             ON CONFLICT(system_id,device_serial) DO UPDATE SET \
             last_seen_at=excluded.last_seen_at,\
             reported_name=COALESCE(excluded.reported_name,reported_name),\
             model=COALESCE(excluded.model,model),\
             firmware=COALESCE(excluded.firmware,firmware),\
             inverter_id=CASE WHEN state='adopted' THEN NULL ELSE inverter_id END,\
             decided_at=CASE WHEN state='adopted' THEN NULL ELSE decided_at END,\
             state=CASE WHEN state='adopted' THEN 'open' ELSE state END",
        )
        .bind(blob(Uuid::now_v7()))
        .bind(blob(system_id.as_uuid()))
        .bind(&device.device_serial)
        .bind(device.reported_name.as_deref())
        .bind(device.model.as_deref())
        .bind(device.firmware.as_deref())
        .bind(now)
        .bind(now)
        .bind(blob(system_id.as_uuid()))
        .bind(&device.device_serial)
        .bind(now)
        .bind(now)
        .execute(writer.connection())
        .await?;
        Ok(())
    }

    async fn discovery_proposals(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<DiscoveryProposalRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(
            "SELECT id,system_id,device_serial,reported_name,model,firmware,state,inverter_id,\
             first_seen_at,last_seen_at,decided_at FROM equipment_discovery_proposals \
             WHERE system_id=? ORDER BY first_seen_at,device_serial",
        )
        .bind(blob(system_id.as_uuid()))
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(sqlite_proposal)
        .collect()
    }

    async fn decide_discovery_proposal(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
        state: &str,
        inverter_id: Option<Uuid>,
        now: i64,
    ) -> Result<bool, AccountRepositoryError> {
        validate_state(state)?;
        let mut writer = self.account.acquire_writer().await?;
        Ok(sqlx::query(
            "UPDATE equipment_discovery_proposals SET state=?,inverter_id=?,\
             decided_at=CASE WHEN ?='open' THEN NULL ELSE ? END WHERE system_id=? AND id=?",
        )
        .bind(state)
        .bind(inverter_id.map(blob))
        .bind(state)
        .bind(now)
        .bind(blob(system_id.as_uuid()))
        .bind(blob(proposal_id))
        .execute(writer.connection())
        .await?
        .rows_affected()
            > 0)
    }

    async fn inverter_serials(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Vec<String>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        Ok(sqlx::query_scalar(
            "SELECT serial_reference FROM inverters WHERE system_id=? \
             AND serial_reference IS NOT NULL AND effective_from<=? \
             AND (effective_to IS NULL OR effective_to>?)",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(now)
        .bind(now)
        .fetch_all(&mut *connection)
        .await?)
    }
}

/// `PostgreSQL` discovery repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresDiscoveryRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresDiscoveryRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<crate::PostgresConnection, sqlx::Error> {
        crate::postgres_connection(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresDiscoveryRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresDiscoveryRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "postgres")]
fn postgres_proposal(
    row: &sqlx::postgres::PgRow,
) -> Result<DiscoveryProposalRecord, AccountRepositoryError> {
    Ok(DiscoveryProposalRecord {
        id: row.get("id"),
        system_id: SystemId::from_uuid(row.get("system_id"))
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        device_serial: row.get("device_serial"),
        reported_name: row.get("reported_name"),
        model: row.get("model"),
        firmware: row.get("firmware"),
        state: row.get("state"),
        inverter_id: row.get("inverter_id"),
        first_seen_at: row.get("first_seen_at"),
        last_seen_at: row.get("last_seen_at"),
        decided_at: row.get("decided_at"),
    })
}

#[cfg(feature = "postgres")]
#[async_trait]
impl DiscoveryRepository for PostgresDiscoveryRepository {
    async fn record_device_identity(
        &self,
        system_id: SystemId,
        device: &ReportedDeviceRecord,
        now: i64,
    ) -> Result<bool, AccountRepositoryError> {
        validate_identity(device)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO account_data.reported_devices AS d (account_id,system_id,device_serial,\
             kind,family_id,configured_name,reported_name,model,firmware,transport,\
             first_reported_at,last_reported_at) \
             VALUES ($1,$2,$3,COALESCE($4,'inverter'),$5,$6,$7,$8,$9,$10,$11,$11) \
             ON CONFLICT(account_id,system_id,device_serial) DO UPDATE SET \
             kind=COALESCE($4,d.kind),\
             family_id=COALESCE(excluded.family_id,d.family_id),\
             configured_name=COALESCE(excluded.configured_name,d.configured_name),\
             reported_name=COALESCE(excluded.reported_name,d.reported_name),\
             model=COALESCE(excluded.model,d.model),\
             firmware=COALESCE(excluded.firmware,d.firmware),\
             transport=COALESCE(excluded.transport,d.transport),\
             last_reported_at=excluded.last_reported_at",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(&device.device_serial)
        .bind(device.kind.as_deref())
        .bind(device.family_id.as_deref())
        .bind(device.configured_name.as_deref())
        .bind(device.reported_name.as_deref())
        .bind(device.model.as_deref())
        .bind(device.firmware.as_deref())
        .bind(device.transport.as_deref())
        .bind(now)
        .execute(&mut *connection)
        .await?;
        // Every right-hand side reads the row as it was before the update.
        let changed = sqlx::query(
            "UPDATE account_data.inverters SET \
             firmware_changed_at=CASE WHEN $1::TEXT IS NOT NULL \
               AND firmware_version IS DISTINCT FROM $1 THEN $3 ELSE firmware_changed_at END,\
             firmware_version=COALESCE($1,firmware_version),\
             model=CASE WHEN catalog_entry_id IS NULL THEN COALESCE($2,model) ELSE model END,\
             updated_at=$3,version=version+1 \
             WHERE account_id=$4 AND system_id=$5 AND serial_reference=$6 \
             AND effective_from<=$3 AND (effective_to IS NULL OR effective_to>$3) \
             AND (($1::TEXT IS NOT NULL AND firmware_version IS DISTINCT FROM $1) \
               OR ($2::TEXT IS NOT NULL AND catalog_entry_id IS NULL \
                 AND model IS DISTINCT FROM $2))",
        )
        .bind(device.firmware.as_deref())
        .bind(device.model.as_deref())
        .bind(now)
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(&device.device_serial)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        connection.close().await?;
        Ok(changed > 0)
    }

    async fn reported_devices(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<StoredReportedDevice>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT device_serial,kind,family_id,configured_name,reported_name,model,firmware,\
             transport,first_reported_at,last_reported_at FROM account_data.reported_devices \
             WHERE account_id=$1 AND system_id=$2 ORDER BY device_serial",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_all(&mut *connection)
        .await?;
        connection.close().await?;
        Ok(rows.iter().map(reported_device).collect())
    }

    async fn record_discovered_device(
        &self,
        system_id: SystemId,
        device: &DiscoveredDeviceRecord,
        now: i64,
    ) -> Result<(), AccountRepositoryError> {
        validate_device(device)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO account_data.equipment_discovery_proposals AS p \
             (account_id,id,system_id,device_serial,reported_name,model,firmware,state,\
              first_seen_at,last_seen_at) \
             SELECT $1,$2,$3,$4,$5,$6,$7,'open',$8,$8 WHERE NOT EXISTS (\
               SELECT 1 FROM account_data.inverters WHERE account_id=$1 AND system_id=$3 \
               AND serial_reference=$4 AND effective_from<=$8 \
               AND (effective_to IS NULL OR effective_to>$8)) \
             ON CONFLICT(account_id,system_id,device_serial) DO UPDATE SET \
             last_seen_at=excluded.last_seen_at,\
             reported_name=COALESCE(excluded.reported_name,p.reported_name),\
             model=COALESCE(excluded.model,p.model),\
             firmware=COALESCE(excluded.firmware,p.firmware),\
             inverter_id=CASE WHEN p.state='adopted' THEN NULL ELSE p.inverter_id END,\
             decided_at=CASE WHEN p.state='adopted' THEN NULL ELSE p.decided_at END,\
             state=CASE WHEN p.state='adopted' THEN 'open' ELSE p.state END",
        )
        .bind(self.account_id.as_uuid())
        .bind(Uuid::now_v7())
        .bind(system_id.as_uuid())
        .bind(&device.device_serial)
        .bind(device.reported_name.as_deref())
        .bind(device.model.as_deref())
        .bind(device.firmware.as_deref())
        .bind(now)
        .execute(&mut *connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn discovery_proposals(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<DiscoveryProposalRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT id,system_id,device_serial,reported_name,model,firmware,state,inverter_id,\
             first_seen_at,last_seen_at,decided_at \
             FROM account_data.equipment_discovery_proposals \
             WHERE account_id=$1 AND system_id=$2 ORDER BY first_seen_at,device_serial",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .fetch_all(&mut *connection)
        .await?;
        connection.close().await?;
        rows.iter().map(postgres_proposal).collect()
    }

    async fn decide_discovery_proposal(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
        state: &str,
        inverter_id: Option<Uuid>,
        now: i64,
    ) -> Result<bool, AccountRepositoryError> {
        validate_state(state)?;
        let mut connection = self.connection().await?;
        let updated = sqlx::query(
            "UPDATE account_data.equipment_discovery_proposals SET state=$4,inverter_id=$5,\
             decided_at=CASE WHEN $4='open' THEN NULL ELSE $6 END \
             WHERE account_id=$1 AND system_id=$2 AND id=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(proposal_id)
        .bind(state)
        .bind(inverter_id)
        .bind(now)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        connection.close().await?;
        Ok(updated > 0)
    }

    async fn inverter_serials(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Vec<String>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let serials = sqlx::query_scalar(
            "SELECT serial_reference FROM account_data.inverters \
             WHERE account_id=$1 AND system_id=$2 AND serial_reference IS NOT NULL \
             AND effective_from<=$3 AND (effective_to IS NULL OR effective_to>$3)",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(now)
        .fetch_all(&mut *connection)
        .await?;
        connection.close().await?;
        Ok(serials)
    }
}
