//! Daily summaries a source supplied, and the ones computed from readings.

#[cfg(feature = "postgres")]
use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, DataSourceId, SystemId};
use serde_json::Value;
use sqlx::Row as _;
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// One day as a source summarized it, for the system or for one device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDailySummaryRecord {
    pub system_id: SystemId,
    /// Stable name of the source: its data source identifier when it has one.
    pub source_key: String,
    /// Device the summary describes; empty for the whole system.
    pub device_key: String,
    /// Local calendar date in the system's timezone, as `YYYY-MM-DD`.
    pub local_date: String,
    pub data_source_id: Option<DataSourceId>,
    pub generation_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    /// Lifetime generation counter at the end of the day.
    pub generation_lifetime_wh: Option<i64>,
    /// True once the source states that the day will not change any more.
    pub complete: bool,
    pub peak_power_watts: Option<i64>,
    /// Local time of the peak as `HH:MM`.
    pub peak_time: Option<String>,
    pub weather_condition: Option<String>,
    pub temperature_min_millidegrees_c: Option<i64>,
    pub temperature_max_millidegrees_c: Option<i64>,
    /// Imported energy in watt-hours keyed by tariff band.
    pub import_energy_by_band: Value,
    pub received_at: i64,
}

/// One day as computed from interval readings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComputedDailySummaryRecord {
    pub local_date: String,
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
    pub coverage_basis_points: i64,
}

/// Daily summaries of the systems of one account.
#[async_trait]
pub trait DailySummaryRepository: Send + Sync {
    /// Stores a supplied summary, replacing the one the same source sent
    /// earlier for the same device and date.
    async fn save_source_summary(
        &self,
        record: &SourceDailySummaryRecord,
    ) -> Result<(), AccountRepositoryError>;
    /// Lists supplied summaries with a local date in `from..=to`, oldest first.
    async fn source_summaries(
        &self,
        system_id: SystemId,
        from: &str,
        to: &str,
    ) -> Result<Vec<SourceDailySummaryRecord>, AccountRepositoryError>;
    /// Lists the newest computed summary of each local date in `from..=to`.
    async fn computed_summaries(
        &self,
        system_id: SystemId,
        from: &str,
        to: &str,
    ) -> Result<Vec<ComputedDailySummaryRecord>, AccountRepositoryError>;
}

/// `SQLite` daily summary repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteDailySummaryRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteDailySummaryRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl DailySummaryRepository for SqliteDailySummaryRepository {
    async fn save_source_summary(
        &self,
        r: &SourceDailySummaryRecord,
    ) -> Result<(), AccountRepositoryError> {
        let mut writer = self.account.acquire_writer().await?;
        sqlx::query(
            "INSERT INTO source_daily_summaries (system_id,source_key,device_key,local_date,\
             data_source_id,generation_energy_wh,export_energy_wh,consumption_energy_wh,\
             generation_lifetime_wh,complete,peak_power_watts,peak_time,weather_condition,\
             temperature_min_millidegrees_c,temperature_max_millidegrees_c,\
             import_energy_by_band_json,received_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(system_id,source_key,device_key,local_date) DO UPDATE SET \
             data_source_id=excluded.data_source_id,\
             generation_energy_wh=excluded.generation_energy_wh,\
             export_energy_wh=excluded.export_energy_wh,\
             consumption_energy_wh=excluded.consumption_energy_wh,\
             generation_lifetime_wh=excluded.generation_lifetime_wh,complete=excluded.complete,\
             peak_power_watts=excluded.peak_power_watts,peak_time=excluded.peak_time,\
             weather_condition=excluded.weather_condition,\
             temperature_min_millidegrees_c=excluded.temperature_min_millidegrees_c,\
             temperature_max_millidegrees_c=excluded.temperature_max_millidegrees_c,\
             import_energy_by_band_json=excluded.import_energy_by_band_json,\
             received_at=excluded.received_at",
        )
        .bind(r.system_id.as_uuid().as_bytes().to_vec())
        .bind(&r.source_key)
        .bind(&r.device_key)
        .bind(&r.local_date)
        .bind(r.data_source_id.map(|id| id.as_uuid().as_bytes().to_vec()))
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
        .execute(writer.connection())
        .await?;
        Ok(())
    }

    async fn source_summaries(
        &self,
        system_id: SystemId,
        from: &str,
        to: &str,
    ) -> Result<Vec<SourceDailySummaryRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let rows = sqlx::query(
            "SELECT source_key,device_key,local_date,data_source_id,generation_energy_wh,\
             export_energy_wh,consumption_energy_wh,generation_lifetime_wh,complete,\
             peak_power_watts,peak_time,weather_condition,temperature_min_millidegrees_c,\
             temperature_max_millidegrees_c,import_energy_by_band_json,received_at \
             FROM source_daily_summaries WHERE system_id=? AND local_date>=? AND local_date<=? \
             ORDER BY local_date,received_at,source_key,device_key",
        )
        .bind(system_id.as_uuid().as_bytes().to_vec())
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(SourceDailySummaryRecord {
                    system_id,
                    source_key: row.get("source_key"),
                    device_key: row.get("device_key"),
                    local_date: row.get("local_date"),
                    data_source_id: row
                        .get::<Option<Vec<u8>>, _>("data_source_id")
                        .map(|bytes| {
                            Uuid::from_slice(&bytes)
                                .ok()
                                .and_then(|id| DataSourceId::from_uuid(id).ok())
                                .ok_or(AccountRepositoryError::InvalidStoredValue)
                        })
                        .transpose()?,
                    generation_energy_wh: row.get("generation_energy_wh"),
                    export_energy_wh: row.get("export_energy_wh"),
                    consumption_energy_wh: row.get("consumption_energy_wh"),
                    generation_lifetime_wh: row.get("generation_lifetime_wh"),
                    complete: row.get("complete"),
                    peak_power_watts: row.get("peak_power_watts"),
                    peak_time: row.get("peak_time"),
                    weather_condition: row.get("weather_condition"),
                    temperature_min_millidegrees_c: row.get("temperature_min_millidegrees_c"),
                    temperature_max_millidegrees_c: row.get("temperature_max_millidegrees_c"),
                    import_energy_by_band: serde_json::from_str(
                        &row.get::<String, _>("import_energy_by_band_json"),
                    )?,
                    received_at: row.get("received_at"),
                })
            })
            .collect()
    }

    async fn computed_summaries(
        &self,
        system_id: SystemId,
        from: &str,
        to: &str,
    ) -> Result<Vec<ComputedDailySummaryRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let rows = sqlx::query(
            "SELECT local_date,generation_energy_wh,consumption_energy_wh,\
             grid_export_energy_wh,coverage_basis_points FROM system_daily_summaries d \
             WHERE system_id=? AND local_date>=? AND local_date<=? AND generation=(\
             SELECT MAX(latest.generation) FROM system_daily_summaries latest \
             WHERE latest.system_id=d.system_id AND latest.local_date=d.local_date) \
             ORDER BY local_date",
        )
        .bind(system_id.as_uuid().as_bytes().to_vec())
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .iter()
            .map(|row| ComputedDailySummaryRecord {
                local_date: row.get("local_date"),
                generation_energy_wh: row.get("generation_energy_wh"),
                consumption_energy_wh: row.get("consumption_energy_wh"),
                export_energy_wh: row.get("grid_export_energy_wh"),
                coverage_basis_points: row.get("coverage_basis_points"),
            })
            .collect())
    }
}

/// `PostgreSQL` daily summary repository scoped to one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresDailySummaryRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresDailySummaryRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresDailySummaryRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresDailySummaryRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl DailySummaryRepository for PostgresDailySummaryRepository {
    async fn save_source_summary(
        &self,
        r: &SourceDailySummaryRecord,
    ) -> Result<(), AccountRepositoryError> {
        let mut connection = crate::postgres_connection(&self.url).await?;
        sqlx::query(
            "INSERT INTO telemetry.source_daily_summaries (account_id,system_id,source_key,\
             device_key,local_date,data_source_id,generation_energy_wh,export_energy_wh,\
             consumption_energy_wh,generation_lifetime_wh,complete,peak_power_watts,peak_time,\
             weather_condition,temperature_min_millidegrees_c,temperature_max_millidegrees_c,\
             import_energy_by_band,received_at) \
             VALUES ($1,$2,$3,$4,$5::date,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) \
             ON CONFLICT (account_id,system_id,source_key,device_key,local_date) DO UPDATE SET \
             data_source_id=excluded.data_source_id,\
             generation_energy_wh=excluded.generation_energy_wh,\
             export_energy_wh=excluded.export_energy_wh,\
             consumption_energy_wh=excluded.consumption_energy_wh,\
             generation_lifetime_wh=excluded.generation_lifetime_wh,complete=excluded.complete,\
             peak_power_watts=excluded.peak_power_watts,peak_time=excluded.peak_time,\
             weather_condition=excluded.weather_condition,\
             temperature_min_millidegrees_c=excluded.temperature_min_millidegrees_c,\
             temperature_max_millidegrees_c=excluded.temperature_max_millidegrees_c,\
             import_energy_by_band=excluded.import_energy_by_band,\
             received_at=excluded.received_at",
        )
        .bind(self.account_id.as_uuid())
        .bind(r.system_id.as_uuid())
        .bind(&r.source_key)
        .bind(&r.device_key)
        .bind(&r.local_date)
        .bind(r.data_source_id.map(|id| id.as_uuid()))
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
        .execute(&mut *connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn source_summaries(
        &self,
        system_id: SystemId,
        from: &str,
        to: &str,
    ) -> Result<Vec<SourceDailySummaryRecord>, AccountRepositoryError> {
        let mut connection = crate::postgres_connection(&self.url).await?;
        let rows = sqlx::query(
            "SELECT source_key,device_key,local_date::TEXT AS local_date,data_source_id,\
             generation_energy_wh,export_energy_wh,consumption_energy_wh,generation_lifetime_wh,\
             complete,peak_power_watts,peak_time,weather_condition,\
             temperature_min_millidegrees_c,temperature_max_millidegrees_c,\
             import_energy_by_band,received_at FROM telemetry.source_daily_summaries \
             WHERE account_id=$1 AND system_id=$2 AND local_date>=$3::date \
             AND local_date<=$4::date ORDER BY local_date,received_at,source_key,device_key",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?;
        connection.close().await?;
        rows.iter()
            .map(|row| {
                Ok(SourceDailySummaryRecord {
                    system_id,
                    source_key: row.get("source_key"),
                    device_key: row.get("device_key"),
                    local_date: row.get("local_date"),
                    data_source_id: row
                        .get::<Option<Uuid>, _>("data_source_id")
                        .map(DataSourceId::from_uuid)
                        .transpose()
                        .map_err(|_| AccountRepositoryError::InvalidStoredValue)?,
                    generation_energy_wh: row.get("generation_energy_wh"),
                    export_energy_wh: row.get("export_energy_wh"),
                    consumption_energy_wh: row.get("consumption_energy_wh"),
                    generation_lifetime_wh: row.get("generation_lifetime_wh"),
                    complete: row.get("complete"),
                    peak_power_watts: row.get("peak_power_watts"),
                    peak_time: row.get("peak_time"),
                    weather_condition: row.get("weather_condition"),
                    temperature_min_millidegrees_c: row.get("temperature_min_millidegrees_c"),
                    temperature_max_millidegrees_c: row.get("temperature_max_millidegrees_c"),
                    import_energy_by_band: row.get("import_energy_by_band"),
                    received_at: row.get("received_at"),
                })
            })
            .collect()
    }

    async fn computed_summaries(
        &self,
        system_id: SystemId,
        from: &str,
        to: &str,
    ) -> Result<Vec<ComputedDailySummaryRecord>, AccountRepositoryError> {
        let mut connection = crate::postgres_connection(&self.url).await?;
        let rows = sqlx::query(
            "SELECT local_date::TEXT AS local_date,generation_energy_wh,consumption_energy_wh,\
             grid_export_energy_wh,coverage_basis_points::BIGINT AS coverage_basis_points \
             FROM telemetry.daily_summaries d WHERE account_id=$1 AND system_id=$2 \
             AND local_date>=$3::date AND local_date<=$4::date AND generation=(\
             SELECT MAX(latest.generation) FROM telemetry.daily_summaries latest \
             WHERE latest.account_id=d.account_id AND latest.system_id=d.system_id \
             AND latest.local_date=d.local_date) ORDER BY local_date",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(from)
        .bind(to)
        .fetch_all(&mut *connection)
        .await?;
        connection.close().await?;
        Ok(rows
            .iter()
            .map(|row| ComputedDailySummaryRecord {
                local_date: row.get("local_date"),
                generation_energy_wh: row.get("generation_energy_wh"),
                consumption_energy_wh: row.get("consumption_energy_wh"),
                export_energy_wh: row.get("grid_export_energy_wh"),
                coverage_basis_points: row.get("coverage_basis_points"),
            })
            .collect())
    }
}
