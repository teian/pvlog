//! Account-owned configuration repositories with shared effective-date semantics.

#[cfg(feature = "postgres")]
use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{
    AccountId, AuditEventId, ChannelId, EquipmentId, InverterId, StringId, SystemId, TariffId,
};
use serde_json::Value;
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
use thiserror::Error;
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemConfigurationRecord {
    pub id: SystemId,
    pub name: String,
    pub description: String,
    pub timezone: String,
    pub visibility: String,
    pub lifecycle: String,
    pub status_interval_seconds: i64,
    pub power_calculation_mode: String,
    pub net_calculation_mode: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquipmentRecord {
    pub id: EquipmentId,
    pub system_id: SystemId,
    pub equipment_kind: String,
    pub name: String,
    pub capacity_watts: Option<i64>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub configuration: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InverterRecord {
    pub id: InverterId,
    pub system_id: SystemId,
    pub name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_reference: Option<String>,
    pub rated_power_watts: Option<i64>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub strings: Vec<PvStringRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PvStringRecord {
    pub id: StringId,
    pub inverter_id: InverterId,
    pub name: String,
    pub panel_count: u32,
    pub panel_manufacturer: Option<String>,
    pub panel_model: Option<String>,
    pub rated_power_watts: i64,
    pub orientation_degrees: Option<u16>,
    pub tilt_degrees: Option<u8>,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TariffRecord {
    pub id: TariffId,
    pub system_id: SystemId,
    pub name: String,
    pub direction: String,
    pub currency_code: String,
    pub minor_units_per_kwh: i64,
    pub schedule: Value,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelDefinitionRecord {
    pub id: ChannelId,
    pub system_id: SystemId,
    pub channel_key: String,
    pub display_name: String,
    pub data_type: String,
    pub unit: String,
    pub scale: i32,
    pub minimum_value: Option<i64>,
    pub maximum_value: Option<i64>,
    pub lifecycle: String,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub display: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountAuditRecord {
    pub id: AuditEventId,
    pub occurred_at: i64,
    pub request_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub outcome: String,
    pub previous_event_hash: Option<[u8; 32]>,
    pub event_hash: [u8; 32],
    pub safe_metadata: Value,
}

#[async_trait]
pub trait AccountConfigurationRepository: Send + Sync {
    fn account_id(&self) -> AccountId;
    async fn save_system(
        &self,
        record: &SystemConfigurationRecord,
    ) -> Result<(), AccountRepositoryError>;
    async fn system(
        &self,
        system_id: SystemId,
    ) -> Result<Option<SystemConfigurationRecord>, AccountRepositoryError>;
    async fn save_inverter_aggregate(
        &self,
        record: &InverterRecord,
    ) -> Result<(), AccountRepositoryError>;
    async fn effective_inverters(
        &self,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<InverterRecord>, AccountRepositoryError>;
    async fn save_equipment(&self, record: &EquipmentRecord) -> Result<(), AccountRepositoryError>;
    async fn effective_equipment(
        &self,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<EquipmentRecord>, AccountRepositoryError>;
    async fn save_tariff(&self, record: &TariffRecord) -> Result<(), AccountRepositoryError>;
    async fn effective_tariff(
        &self,
        system_id: SystemId,
        direction: &str,
        at: i64,
    ) -> Result<Option<TariffRecord>, AccountRepositoryError>;
    async fn save_channel(
        &self,
        record: &ChannelDefinitionRecord,
    ) -> Result<(), AccountRepositoryError>;
    async fn effective_channel(
        &self,
        system_id: SystemId,
        channel_key: &str,
        at: i64,
    ) -> Result<Option<ChannelDefinitionRecord>, AccountRepositoryError>;
    async fn append_audit(&self, record: &AccountAuditRecord)
    -> Result<(), AccountRepositoryError>;
    async fn audit(&self, limit: u32) -> Result<Vec<AccountAuditRecord>, AccountRepositoryError>;
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteAccountConfigurationRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteAccountConfigurationRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresAccountConfigurationRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresAccountConfigurationRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresAccountConfigurationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresAccountConfigurationRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl AccountConfigurationRepository for SqliteAccountConfigurationRepository {
    fn account_id(&self) -> AccountId {
        self.account.account_id()
    }

    async fn save_system(
        &self,
        record: &SystemConfigurationRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate_system(record)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO systems (id,name,description,timezone,visibility,lifecycle,status_interval_seconds,power_calculation_mode,net_calculation_mode,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,timezone=excluded.timezone,visibility=excluded.visibility,lifecycle=excluded.lifecycle,status_interval_seconds=excluded.status_interval_seconds,power_calculation_mode=excluded.power_calculation_mode,net_calculation_mode=excluded.net_calculation_mode,updated_at=excluded.updated_at,version=version+1").bind(blob(record.id.as_uuid())).bind(&record.name).bind(&record.description).bind(&record.timezone).bind(&record.visibility).bind(&record.lifecycle).bind(record.status_interval_seconds).bind(&record.power_calculation_mode).bind(&record.net_calculation_mode).bind(record.created_at).bind(record.updated_at).execute(writer.connection()).await?;
        Ok(())
    }
    async fn system(
        &self,
        id: SystemId,
    ) -> Result<Option<SystemConfigurationRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,name,description,timezone,visibility,lifecycle,status_interval_seconds,power_calculation_mode,net_calculation_mode,created_at,updated_at FROM systems WHERE id=?").bind(blob(id.as_uuid())).fetch_optional(&mut *connection).await?;
        row.map(|row| sqlite_system(&row)).transpose()
    }
    async fn save_inverter_aggregate(
        &self,
        record: &InverterRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate_inverter(record)?;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        sqlx::query("INSERT INTO inverters (id,system_id,name,manufacturer,model,serial_reference,rated_power_watts,effective_from,effective_to,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET system_id=excluded.system_id,name=excluded.name,manufacturer=excluded.manufacturer,model=excluded.model,serial_reference=excluded.serial_reference,rated_power_watts=excluded.rated_power_watts,effective_from=excluded.effective_from,effective_to=excluded.effective_to,updated_at=excluded.updated_at,version=version+1")
            .bind(blob(record.id.as_uuid())).bind(blob(record.system_id.as_uuid())).bind(&record.name)
            .bind(&record.manufacturer).bind(&record.model).bind(&record.serial_reference)
            .bind(record.rated_power_watts).bind(record.effective_from).bind(record.effective_to)
            .bind(record.created_at).bind(record.updated_at).execute(&mut *transaction).await?;
        sqlx::query("DELETE FROM pv_strings WHERE inverter_id=?")
            .bind(blob(record.id.as_uuid()))
            .execute(&mut *transaction)
            .await?;
        for string in &record.strings {
            sqlx::query("INSERT INTO pv_strings (id,inverter_id,name,panel_count,panel_manufacturer,panel_model,rated_power_watts,orientation_degrees,tilt_degrees,effective_from,effective_to,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(blob(string.id.as_uuid())).bind(blob(string.inverter_id.as_uuid()))
                .bind(&string.name).bind(i64::from(string.panel_count)).bind(&string.panel_manufacturer)
                .bind(&string.panel_model).bind(string.rated_power_watts)
                .bind(string.orientation_degrees.map(i64::from)).bind(string.tilt_degrees.map(i64::from))
                .bind(string.effective_from).bind(string.effective_to).bind(string.created_at)
                .bind(string.updated_at).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }
    async fn effective_inverters(
        &self,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<InverterRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let rows = sqlx::query("SELECT id,system_id,name,manufacturer,model,serial_reference,rated_power_watts,effective_from,effective_to,created_at,updated_at FROM inverters WHERE system_id=? AND effective_from<=? AND (effective_to IS NULL OR effective_to>?) ORDER BY effective_from,id")
            .bind(blob(system_id.as_uuid())).bind(at).bind(at).fetch_all(&mut *connection).await?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut inverter = sqlite_inverter(row)?;
            let strings = sqlx::query("SELECT id,inverter_id,name,panel_count,panel_manufacturer,panel_model,rated_power_watts,orientation_degrees,tilt_degrees,effective_from,effective_to,created_at,updated_at FROM pv_strings WHERE inverter_id=? AND effective_from<=? AND (effective_to IS NULL OR effective_to>?) ORDER BY effective_from,id")
                .bind(blob(inverter.id.as_uuid())).bind(at).bind(at).fetch_all(&mut *connection).await?;
            inverter.strings = strings
                .iter()
                .map(sqlite_pv_string)
                .collect::<Result<_, _>>()?;
            records.push(inverter);
        }
        Ok(records)
    }
    async fn save_equipment(&self, r: &EquipmentRecord) -> Result<(), AccountRepositoryError> {
        validate_range(r.effective_from, r.effective_to)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO equipment (id,system_id,equipment_kind,name,capacity_watts,effective_from,effective_to,configuration_json,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,capacity_watts=excluded.capacity_watts,effective_from=excluded.effective_from,effective_to=excluded.effective_to,configuration_json=excluded.configuration_json,updated_at=excluded.updated_at,version=version+1").bind(blob(r.id.as_uuid())).bind(blob(r.system_id.as_uuid())).bind(&r.equipment_kind).bind(&r.name).bind(r.capacity_watts).bind(r.effective_from).bind(r.effective_to).bind(serde_json::to_string(&r.configuration)?).bind(r.created_at).bind(r.updated_at).execute(writer.connection()).await?;
        Ok(())
    }
    async fn effective_equipment(
        &self,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<EquipmentRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let rows=sqlx::query("SELECT id,system_id,equipment_kind,name,capacity_watts,effective_from,effective_to,configuration_json,created_at,updated_at FROM equipment WHERE system_id=? AND effective_from<=? AND (effective_to IS NULL OR effective_to>?) ORDER BY effective_from,id").bind(blob(system_id.as_uuid())).bind(at).bind(at).fetch_all(&mut *connection).await?;
        rows.iter().map(sqlite_equipment).collect()
    }
    async fn save_tariff(&self, r: &TariffRecord) -> Result<(), AccountRepositoryError> {
        validate_range(r.effective_from, r.effective_to)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO tariffs (id,system_id,name,direction,currency_code,minor_units_per_kwh,schedule_json,effective_from,effective_to,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,direction=excluded.direction,currency_code=excluded.currency_code,minor_units_per_kwh=excluded.minor_units_per_kwh,schedule_json=excluded.schedule_json,effective_from=excluded.effective_from,effective_to=excluded.effective_to,updated_at=excluded.updated_at,version=version+1").bind(blob(r.id.as_uuid())).bind(blob(r.system_id.as_uuid())).bind(&r.name).bind(&r.direction).bind(&r.currency_code).bind(r.minor_units_per_kwh).bind(serde_json::to_string(&r.schedule)?).bind(r.effective_from).bind(r.effective_to).bind(r.created_at).bind(r.updated_at).execute(writer.connection()).await?;
        Ok(())
    }
    async fn effective_tariff(
        &self,
        system_id: SystemId,
        direction: &str,
        at: i64,
    ) -> Result<Option<TariffRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,system_id,name,direction,currency_code,minor_units_per_kwh,schedule_json,effective_from,effective_to,created_at,updated_at FROM tariffs WHERE system_id=? AND direction=? AND effective_from<=? AND (effective_to IS NULL OR effective_to>?) ORDER BY effective_from DESC,id DESC LIMIT 1").bind(blob(system_id.as_uuid())).bind(direction).bind(at).bind(at).fetch_optional(&mut *connection).await?;
        row.map(|row| sqlite_tariff(&row)).transpose()
    }
    async fn save_channel(
        &self,
        r: &ChannelDefinitionRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate_range(r.effective_from, r.effective_to)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO channel_definitions (id,system_id,channel_key,display_name,data_type,unit,scale,minimum_value,maximum_value,lifecycle,effective_from,effective_to,display_json,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,data_type=excluded.data_type,unit=excluded.unit,scale=excluded.scale,minimum_value=excluded.minimum_value,maximum_value=excluded.maximum_value,lifecycle=excluded.lifecycle,effective_from=excluded.effective_from,effective_to=excluded.effective_to,display_json=excluded.display_json,updated_at=excluded.updated_at,version=version+1").bind(blob(r.id.as_uuid())).bind(blob(r.system_id.as_uuid())).bind(&r.channel_key).bind(&r.display_name).bind(&r.data_type).bind(&r.unit).bind(r.scale).bind(r.minimum_value).bind(r.maximum_value).bind(&r.lifecycle).bind(r.effective_from).bind(r.effective_to).bind(serde_json::to_string(&r.display)?).bind(r.created_at).bind(r.updated_at).execute(writer.connection()).await?;
        Ok(())
    }
    async fn effective_channel(
        &self,
        system_id: SystemId,
        key: &str,
        at: i64,
    ) -> Result<Option<ChannelDefinitionRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,system_id,channel_key,display_name,data_type,unit,scale,minimum_value,maximum_value,lifecycle,effective_from,effective_to,display_json,created_at,updated_at FROM channel_definitions WHERE system_id=? AND channel_key=? AND lifecycle='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>?) LIMIT 1").bind(blob(system_id.as_uuid())).bind(key).bind(at).bind(at).fetch_optional(&mut *connection).await?;
        row.map(|row| sqlite_channel(&row)).transpose()
    }
    async fn append_audit(&self, r: &AccountAuditRecord) -> Result<(), AccountRepositoryError> {
        validate_audit(r)?;
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO account_audit_events (id,occurred_at,request_id,actor_type,actor_id,action,target_type,target_id,outcome,previous_event_hash,event_hash,safe_metadata_json) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)").bind(blob(r.id.as_uuid())).bind(r.occurred_at).bind(r.request_id.map(blob)).bind(&r.actor_type).bind(r.actor_id.map(blob)).bind(&r.action).bind(&r.target_type).bind(r.target_id.map(blob)).bind(&r.outcome).bind(r.previous_event_hash.map(|h|h.to_vec())).bind(r.event_hash.as_slice()).bind(serde_json::to_string(&r.safe_metadata)?).execute(writer.connection()).await?;
        Ok(())
    }
    async fn audit(&self, limit: u32) -> Result<Vec<AccountAuditRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let rows=sqlx::query("SELECT id,occurred_at,request_id,actor_type,actor_id,action,target_type,target_id,outcome,previous_event_hash,event_hash,safe_metadata_json FROM account_audit_events ORDER BY occurred_at DESC,id DESC LIMIT ?").bind(limit).fetch_all(&mut *connection).await?;
        rows.iter().map(sqlite_audit).collect()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl AccountConfigurationRepository for PostgresAccountConfigurationRepository {
    fn account_id(&self) -> AccountId {
        self.account_id
    }
    async fn save_system(
        &self,
        r: &SystemConfigurationRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate_system(r)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO account_data.systems (account_id,id,name,description,timezone,visibility,lifecycle,status_interval_seconds,power_calculation_mode,net_calculation_mode,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,description=excluded.description,timezone=excluded.timezone,visibility=excluded.visibility,lifecycle=excluded.lifecycle,status_interval_seconds=excluded.status_interval_seconds,power_calculation_mode=excluded.power_calculation_mode,net_calculation_mode=excluded.net_calculation_mode,updated_at=excluded.updated_at,version=account_data.systems.version+1").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(&r.name).bind(&r.description).bind(&r.timezone).bind(&r.visibility).bind(&r.lifecycle).bind(r.status_interval_seconds).bind(&r.power_calculation_mode).bind(&r.net_calculation_mode).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn system(
        &self,
        id: SystemId,
    ) -> Result<Option<SystemConfigurationRecord>, AccountRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,name,description,timezone,visibility,lifecycle,status_interval_seconds,power_calculation_mode,net_calculation_mode,created_at,updated_at FROM account_data.systems WHERE account_id=$1 AND id=$2").bind(self.account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|row| pg_system(&row)).transpose()
    }
    async fn save_inverter_aggregate(
        &self,
        record: &InverterRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate_inverter(record)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        sqlx::query("INSERT INTO account_data.inverters (account_id,id,system_id,name,manufacturer,model,serial_reference,rated_power_watts,effective_from,effective_to,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(account_id,id) DO UPDATE SET system_id=excluded.system_id,name=excluded.name,manufacturer=excluded.manufacturer,model=excluded.model,serial_reference=excluded.serial_reference,rated_power_watts=excluded.rated_power_watts,effective_from=excluded.effective_from,effective_to=excluded.effective_to,updated_at=excluded.updated_at,version=account_data.inverters.version+1")
            .bind(self.account_id.as_uuid()).bind(record.id.as_uuid()).bind(record.system_id.as_uuid())
            .bind(&record.name).bind(&record.manufacturer).bind(&record.model).bind(&record.serial_reference)
            .bind(record.rated_power_watts).bind(record.effective_from).bind(record.effective_to)
            .bind(record.created_at).bind(record.updated_at).execute(&mut *transaction).await?;
        sqlx::query("DELETE FROM account_data.pv_strings WHERE account_id=$1 AND inverter_id=$2")
            .bind(self.account_id.as_uuid())
            .bind(record.id.as_uuid())
            .execute(&mut *transaction)
            .await?;
        for string in &record.strings {
            sqlx::query("INSERT INTO account_data.pv_strings (account_id,id,inverter_id,name,panel_count,panel_manufacturer,panel_model,rated_power_watts,orientation_degrees,tilt_degrees,effective_from,effective_to,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
                .bind(self.account_id.as_uuid()).bind(string.id.as_uuid()).bind(string.inverter_id.as_uuid())
                .bind(&string.name).bind(i32::try_from(string.panel_count).map_err(|_| AccountRepositoryError::InvalidRecord("PV string"))?)
                .bind(&string.panel_manufacturer).bind(&string.panel_model).bind(string.rated_power_watts)
                .bind(string.orientation_degrees.map(i32::from)).bind(string.tilt_degrees.map(i32::from))
                .bind(string.effective_from).bind(string.effective_to).bind(string.created_at)
                .bind(string.updated_at).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }
    async fn effective_inverters(
        &self,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<InverterRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query("SELECT id,system_id,name,manufacturer,model,serial_reference,rated_power_watts,effective_from,effective_to,created_at,updated_at FROM account_data.inverters WHERE account_id=$1 AND system_id=$2 AND effective_from<=$3 AND (effective_to IS NULL OR effective_to>$3) ORDER BY effective_from,id")
            .bind(self.account_id.as_uuid()).bind(system_id.as_uuid()).bind(at).fetch_all(&mut connection).await?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut inverter = pg_inverter(row)?;
            let strings = sqlx::query("SELECT id,inverter_id,name,panel_count,panel_manufacturer,panel_model,rated_power_watts,orientation_degrees,tilt_degrees,effective_from,effective_to,created_at,updated_at FROM account_data.pv_strings WHERE account_id=$1 AND inverter_id=$2 AND effective_from<=$3 AND (effective_to IS NULL OR effective_to>$3) ORDER BY effective_from,id")
                .bind(self.account_id.as_uuid()).bind(inverter.id.as_uuid()).bind(at).fetch_all(&mut connection).await?;
            inverter.strings = strings.iter().map(pg_pv_string).collect::<Result<_, _>>()?;
            records.push(inverter);
        }
        connection.close().await?;
        Ok(records)
    }
    async fn save_equipment(&self, r: &EquipmentRecord) -> Result<(), AccountRepositoryError> {
        validate_range(r.effective_from, r.effective_to)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO account_data.equipment (account_id,id,system_id,equipment_kind,name,capacity_watts,effective_from,effective_to,configuration,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,capacity_watts=excluded.capacity_watts,effective_from=excluded.effective_from,effective_to=excluded.effective_to,configuration=excluded.configuration,updated_at=excluded.updated_at,version=account_data.equipment.version+1").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(r.system_id.as_uuid()).bind(&r.equipment_kind).bind(&r.name).bind(r.capacity_watts).bind(r.effective_from).bind(r.effective_to).bind(&r.configuration).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn effective_equipment(
        &self,
        s: SystemId,
        at: i64,
    ) -> Result<Vec<EquipmentRecord>, AccountRepositoryError> {
        let mut c = self.connection().await?;
        let rows=sqlx::query("SELECT id,system_id,equipment_kind,name,capacity_watts,effective_from,effective_to,configuration,created_at,updated_at FROM account_data.equipment WHERE account_id=$1 AND system_id=$2 AND effective_from<=$3 AND (effective_to IS NULL OR effective_to>$3) ORDER BY effective_from,id").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(at).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(pg_equipment).collect()
    }
    async fn save_tariff(&self, r: &TariffRecord) -> Result<(), AccountRepositoryError> {
        validate_range(r.effective_from, r.effective_to)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO account_data.tariffs (account_id,id,system_id,name,direction,currency_code,minor_units_per_kwh,schedule,effective_from,effective_to,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,direction=excluded.direction,currency_code=excluded.currency_code,minor_units_per_kwh=excluded.minor_units_per_kwh,schedule=excluded.schedule,effective_from=excluded.effective_from,effective_to=excluded.effective_to,updated_at=excluded.updated_at,version=account_data.tariffs.version+1").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(r.system_id.as_uuid()).bind(&r.name).bind(&r.direction).bind(&r.currency_code).bind(r.minor_units_per_kwh).bind(&r.schedule).bind(r.effective_from).bind(r.effective_to).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn effective_tariff(
        &self,
        s: SystemId,
        direction: &str,
        at: i64,
    ) -> Result<Option<TariffRecord>, AccountRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,system_id,name,direction,currency_code,minor_units_per_kwh,schedule,effective_from,effective_to,created_at,updated_at FROM account_data.tariffs WHERE account_id=$1 AND system_id=$2 AND direction=$3 AND effective_from<=$4 AND (effective_to IS NULL OR effective_to>$4) ORDER BY effective_from DESC,id DESC LIMIT 1").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(direction).bind(at).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|row| pg_tariff(&row)).transpose()
    }
    async fn save_channel(
        &self,
        r: &ChannelDefinitionRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate_range(r.effective_from, r.effective_to)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO account_data.channel_definitions (account_id,id,system_id,channel_key,display_name,data_type,unit,scale,minimum_value,maximum_value,lifecycle,effective_from,effective_to,display,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT(account_id,id) DO UPDATE SET display_name=excluded.display_name,data_type=excluded.data_type,unit=excluded.unit,scale=excluded.scale,minimum_value=excluded.minimum_value,maximum_value=excluded.maximum_value,lifecycle=excluded.lifecycle,effective_from=excluded.effective_from,effective_to=excluded.effective_to,display=excluded.display,updated_at=excluded.updated_at,version=account_data.channel_definitions.version+1").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(r.system_id.as_uuid()).bind(&r.channel_key).bind(&r.display_name).bind(&r.data_type).bind(&r.unit).bind(r.scale).bind(r.minimum_value).bind(r.maximum_value).bind(&r.lifecycle).bind(r.effective_from).bind(r.effective_to).bind(&r.display).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn effective_channel(
        &self,
        s: SystemId,
        key: &str,
        at: i64,
    ) -> Result<Option<ChannelDefinitionRecord>, AccountRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,system_id,channel_key,display_name,data_type,unit,scale,minimum_value,maximum_value,lifecycle,effective_from,effective_to,display,created_at,updated_at FROM account_data.channel_definitions WHERE account_id=$1 AND system_id=$2 AND channel_key=$3 AND lifecycle='active' AND effective_from<=$4 AND (effective_to IS NULL OR effective_to>$4) LIMIT 1").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(key).bind(at).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|row| pg_channel(&row)).transpose()
    }
    async fn append_audit(&self, r: &AccountAuditRecord) -> Result<(), AccountRepositoryError> {
        validate_audit(r)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO account_data.audit_events (account_id,id,occurred_at,request_id,actor_type,actor_id,action,target_type,target_id,outcome,previous_event_hash,event_hash,safe_metadata) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(r.occurred_at).bind(r.request_id).bind(&r.actor_type).bind(r.actor_id).bind(&r.action).bind(&r.target_type).bind(r.target_id).bind(&r.outcome).bind(r.previous_event_hash.map(|h|h.to_vec())).bind(r.event_hash.as_slice()).bind(&r.safe_metadata).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn audit(&self, limit: u32) -> Result<Vec<AccountAuditRecord>, AccountRepositoryError> {
        let mut c = self.connection().await?;
        let rows=sqlx::query("SELECT id,occurred_at,request_id,actor_type,actor_id,action,target_type,target_id,outcome,previous_event_hash,event_hash,safe_metadata FROM account_data.audit_events WHERE account_id=$1 ORDER BY occurred_at DESC,id DESC LIMIT $2").bind(self.account_id.as_uuid()).bind(i64::from(limit)).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(pg_audit).collect()
    }
}

fn validate_range(from: i64, to: Option<i64>) -> Result<(), AccountRepositoryError> {
    if to.is_some_and(|to| to <= from) {
        Err(AccountRepositoryError::InvalidEffectiveRange)
    } else {
        Ok(())
    }
}
fn validate_system(r: &SystemConfigurationRecord) -> Result<(), AccountRepositoryError> {
    if r.name.trim().is_empty() || r.timezone.trim().is_empty() {
        return Err(AccountRepositoryError::InvalidRecord("system"));
    }
    Ok(())
}
fn validate_inverter(r: &InverterRecord) -> Result<(), AccountRepositoryError> {
    validate_range(r.effective_from, r.effective_to)?;
    if r.name.trim().is_empty() || r.strings.is_empty() {
        return Err(AccountRepositoryError::InvalidRecord("inverter"));
    }
    for string in &r.strings {
        validate_range(string.effective_from, string.effective_to)?;
        if string.inverter_id != r.id
            || string.name.trim().is_empty()
            || string.panel_count == 0
            || string.rated_power_watts <= 0
            || string.orientation_degrees.is_some_and(|value| value > 359)
            || string.tilt_degrees.is_some_and(|value| value > 90)
        {
            return Err(AccountRepositoryError::InvalidRecord("PV string"));
        }
    }
    Ok(())
}
fn validate_audit(r: &AccountAuditRecord) -> Result<(), AccountRepositoryError> {
    if r.action.trim().is_empty() || r.target_type.trim().is_empty() {
        return Err(AccountRepositoryError::InvalidRecord("audit"));
    }
    Ok(())
}
fn digest(v: Vec<u8>) -> Result<[u8; 32], AccountRepositoryError> {
    v.try_into()
        .map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn sqlite_id<T>(
    v: Vec<u8>,
    f: impl FnOnce(Uuid) -> Result<T, pvlog_domain::IdentifierError>,
) -> Result<T, AccountRepositoryError> {
    let id = Uuid::from_slice(&v).map_err(|_| AccountRepositoryError::InvalidStoredValue)?;
    f(id).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}
#[cfg(feature = "postgres")]
fn pg_id<T>(
    v: Uuid,
    f: impl FnOnce(Uuid) -> Result<T, pvlog_domain::IdentifierError>,
) -> Result<T, AccountRepositoryError> {
    f(v).map_err(|_| AccountRepositoryError::InvalidStoredValue)
}

#[cfg(feature = "sqlite")]
fn sqlite_system(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<SystemConfigurationRecord, AccountRepositoryError> {
    Ok(SystemConfigurationRecord {
        id: sqlite_id(r.get("id"), SystemId::from_uuid)?,
        name: r.get("name"),
        description: r.get("description"),
        timezone: r.get("timezone"),
        visibility: r.get("visibility"),
        lifecycle: r.get("lifecycle"),
        status_interval_seconds: r.get("status_interval_seconds"),
        power_calculation_mode: r.get("power_calculation_mode"),
        net_calculation_mode: r.get("net_calculation_mode"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_equipment(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<EquipmentRecord, AccountRepositoryError> {
    Ok(EquipmentRecord {
        id: sqlite_id(r.get("id"), EquipmentId::from_uuid)?,
        system_id: sqlite_id(r.get("system_id"), SystemId::from_uuid)?,
        equipment_kind: r.get("equipment_kind"),
        name: r.get("name"),
        capacity_watts: r.get("capacity_watts"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        configuration: serde_json::from_str(&r.get::<String, _>("configuration_json"))?,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_inverter(r: &sqlx::sqlite::SqliteRow) -> Result<InverterRecord, AccountRepositoryError> {
    Ok(InverterRecord {
        id: sqlite_id(r.get("id"), InverterId::from_uuid)?,
        system_id: sqlite_id(r.get("system_id"), SystemId::from_uuid)?,
        name: r.get("name"),
        manufacturer: r.get("manufacturer"),
        model: r.get("model"),
        serial_reference: r.get("serial_reference"),
        rated_power_watts: r.get("rated_power_watts"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        strings: Vec::new(),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_pv_string(r: &sqlx::sqlite::SqliteRow) -> Result<PvStringRecord, AccountRepositoryError> {
    Ok(PvStringRecord {
        id: sqlite_id(r.get("id"), StringId::from_uuid)?,
        inverter_id: sqlite_id(r.get("inverter_id"), InverterId::from_uuid)?,
        name: r.get("name"),
        panel_count: u32::try_from(r.get::<i64, _>("panel_count"))
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        panel_manufacturer: r.get("panel_manufacturer"),
        panel_model: r.get("panel_model"),
        rated_power_watts: r.get("rated_power_watts"),
        orientation_degrees: r
            .get::<Option<i64>, _>("orientation_degrees")
            .map(u16::try_from)
            .transpose()
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        tilt_degrees: r
            .get::<Option<i64>, _>("tilt_degrees")
            .map(u8::try_from)
            .transpose()
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_tariff(r: &sqlx::sqlite::SqliteRow) -> Result<TariffRecord, AccountRepositoryError> {
    Ok(TariffRecord {
        id: sqlite_id(r.get("id"), TariffId::from_uuid)?,
        system_id: sqlite_id(r.get("system_id"), SystemId::from_uuid)?,
        name: r.get("name"),
        direction: r.get("direction"),
        currency_code: r.get("currency_code"),
        minor_units_per_kwh: r.get("minor_units_per_kwh"),
        schedule: serde_json::from_str(&r.get::<String, _>("schedule_json"))?,
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_channel(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<ChannelDefinitionRecord, AccountRepositoryError> {
    Ok(ChannelDefinitionRecord {
        id: sqlite_id(r.get("id"), ChannelId::from_uuid)?,
        system_id: sqlite_id(r.get("system_id"), SystemId::from_uuid)?,
        channel_key: r.get("channel_key"),
        display_name: r.get("display_name"),
        data_type: r.get("data_type"),
        unit: r.get("unit"),
        scale: r.get("scale"),
        minimum_value: r.get("minimum_value"),
        maximum_value: r.get("maximum_value"),
        lifecycle: r.get("lifecycle"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        display: serde_json::from_str(&r.get::<String, _>("display_json"))?,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_audit(r: &sqlx::sqlite::SqliteRow) -> Result<AccountAuditRecord, AccountRepositoryError> {
    Ok(AccountAuditRecord {
        id: sqlite_id(r.get("id"), AuditEventId::from_uuid)?,
        occurred_at: r.get("occurred_at"),
        request_id: r
            .get::<Option<Vec<u8>>, _>("request_id")
            .map(|v| Uuid::from_slice(&v).map_err(|_| AccountRepositoryError::InvalidStoredValue))
            .transpose()?,
        actor_type: r.get("actor_type"),
        actor_id: r
            .get::<Option<Vec<u8>>, _>("actor_id")
            .map(|v| Uuid::from_slice(&v).map_err(|_| AccountRepositoryError::InvalidStoredValue))
            .transpose()?,
        action: r.get("action"),
        target_type: r.get("target_type"),
        target_id: r
            .get::<Option<Vec<u8>>, _>("target_id")
            .map(|v| Uuid::from_slice(&v).map_err(|_| AccountRepositoryError::InvalidStoredValue))
            .transpose()?,
        outcome: r.get("outcome"),
        previous_event_hash: r
            .get::<Option<Vec<u8>>, _>("previous_event_hash")
            .map(digest)
            .transpose()?,
        event_hash: digest(r.get("event_hash"))?,
        safe_metadata: serde_json::from_str(&r.get::<String, _>("safe_metadata_json"))?,
    })
}

#[cfg(feature = "postgres")]
fn pg_system(
    r: &sqlx::postgres::PgRow,
) -> Result<SystemConfigurationRecord, AccountRepositoryError> {
    Ok(SystemConfigurationRecord {
        id: pg_id(r.get("id"), SystemId::from_uuid)?,
        name: r.get("name"),
        description: r.get("description"),
        timezone: r.get("timezone"),
        visibility: r.get("visibility"),
        lifecycle: r.get("lifecycle"),
        status_interval_seconds: i64::from(r.get::<i32, _>("status_interval_seconds")),
        power_calculation_mode: r.get("power_calculation_mode"),
        net_calculation_mode: r.get("net_calculation_mode"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn pg_equipment(r: &sqlx::postgres::PgRow) -> Result<EquipmentRecord, AccountRepositoryError> {
    Ok(EquipmentRecord {
        id: pg_id(r.get("id"), EquipmentId::from_uuid)?,
        system_id: pg_id(r.get("system_id"), SystemId::from_uuid)?,
        equipment_kind: r.get("equipment_kind"),
        name: r.get("name"),
        capacity_watts: r.get("capacity_watts"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        configuration: r.get("configuration"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn pg_inverter(r: &sqlx::postgres::PgRow) -> Result<InverterRecord, AccountRepositoryError> {
    Ok(InverterRecord {
        id: pg_id(r.get("id"), InverterId::from_uuid)?,
        system_id: pg_id(r.get("system_id"), SystemId::from_uuid)?,
        name: r.get("name"),
        manufacturer: r.get("manufacturer"),
        model: r.get("model"),
        serial_reference: r.get("serial_reference"),
        rated_power_watts: r.get("rated_power_watts"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        strings: Vec::new(),
    })
}
#[cfg(feature = "postgres")]
fn pg_pv_string(r: &sqlx::postgres::PgRow) -> Result<PvStringRecord, AccountRepositoryError> {
    Ok(PvStringRecord {
        id: pg_id(r.get("id"), StringId::from_uuid)?,
        inverter_id: pg_id(r.get("inverter_id"), InverterId::from_uuid)?,
        name: r.get("name"),
        panel_count: u32::try_from(r.get::<i32, _>("panel_count"))
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        panel_manufacturer: r.get("panel_manufacturer"),
        panel_model: r.get("panel_model"),
        rated_power_watts: r.get("rated_power_watts"),
        orientation_degrees: r
            .get::<Option<i32>, _>("orientation_degrees")
            .map(u16::try_from)
            .transpose()
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        tilt_degrees: r
            .get::<Option<i32>, _>("tilt_degrees")
            .map(u8::try_from)
            .transpose()
            .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn pg_tariff(r: &sqlx::postgres::PgRow) -> Result<TariffRecord, AccountRepositoryError> {
    Ok(TariffRecord {
        id: pg_id(r.get("id"), TariffId::from_uuid)?,
        system_id: pg_id(r.get("system_id"), SystemId::from_uuid)?,
        name: r.get("name"),
        direction: r.get("direction"),
        currency_code: r.get("currency_code"),
        minor_units_per_kwh: r.get("minor_units_per_kwh"),
        schedule: r.get("schedule"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn pg_channel(
    r: &sqlx::postgres::PgRow,
) -> Result<ChannelDefinitionRecord, AccountRepositoryError> {
    Ok(ChannelDefinitionRecord {
        id: pg_id(r.get("id"), ChannelId::from_uuid)?,
        system_id: pg_id(r.get("system_id"), SystemId::from_uuid)?,
        channel_key: r.get("channel_key"),
        display_name: r.get("display_name"),
        data_type: r.get("data_type"),
        unit: r.get("unit"),
        scale: r.get("scale"),
        minimum_value: r.get("minimum_value"),
        maximum_value: r.get("maximum_value"),
        lifecycle: r.get("lifecycle"),
        effective_from: r.get("effective_from"),
        effective_to: r.get("effective_to"),
        display: r.get("display"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn pg_audit(r: &sqlx::postgres::PgRow) -> Result<AccountAuditRecord, AccountRepositoryError> {
    Ok(AccountAuditRecord {
        id: pg_id(r.get("id"), AuditEventId::from_uuid)?,
        occurred_at: r.get("occurred_at"),
        request_id: r.get("request_id"),
        actor_type: r.get("actor_type"),
        actor_id: r.get("actor_id"),
        action: r.get("action"),
        target_type: r.get("target_type"),
        target_id: r.get("target_id"),
        outcome: r.get("outcome"),
        previous_event_hash: r
            .get::<Option<Vec<u8>>, _>("previous_event_hash")
            .map(digest)
            .transpose()?,
        event_hash: digest(r.get("event_hash"))?,
        safe_metadata: r.get("safe_metadata"),
    })
}

#[derive(Debug, Error)]
pub enum AccountRepositoryError {
    #[error("account database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    Routing(#[from] crate::SqliteRoutingError),
    #[error("account JSON value is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid {0} account record")]
    InvalidRecord(&'static str),
    #[error("effective_to must be greater than effective_from")]
    InvalidEffectiveRange,
    #[error("account storage contains an invalid value")]
    InvalidStoredValue,
}
