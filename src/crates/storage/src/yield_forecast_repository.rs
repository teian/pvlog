//! Cross-engine persistence for effective yield settings and immutable normalized weather runs.

#[cfg(feature = "postgres")]
use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{
    AccountId, EstimateRange, ForecastSettingsId, GeographicPoint, IrradiancePoint,
    MetresPerSecondMilli, MilliDegreesCelsius, NormalizedWeatherPoint, NormalizedWeatherRun,
    ProviderId, SpatialCoverage, StringId, SystemId, TimeRange, UnsignedBasisPoints, UtcTimestamp,
    WattsPerSquareMetre, WeatherDataKind, WeatherDataProvenance, WeatherDataRunId,
};
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForecastSettingsRecord {
    pub id: ForecastSettingsId,
    pub system_id: SystemId,
    pub string_id: StringId,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub model_identifier: String,
    pub model_revision: u16,
    pub soiling_loss_basis_points: u16,
    pub shading_loss_basis_points: u16,
    pub mismatch_loss_basis_points: u16,
    pub wiring_loss_basis_points: u16,
    pub unavailability_loss_basis_points: u16,
    pub calibration_basis_points: i32,
    pub configuration_digest: [u8; 32],
    pub created_at: i64,
    pub created_by: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForecastRetentionClass {
    Working,
    Issued,
    Referenced,
}

impl ForecastRetentionClass {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Issued => "issued",
            Self::Referenced => "referenced",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, YieldForecastRepositoryError> {
        match value {
            "working" => Ok(Self::Working),
            "issued" => Ok(Self::Issued),
            "referenced" => Ok(Self::Referenced),
            _ => Err(YieldForecastRepositoryError::Corrupt(
                "unknown forecast retention class",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherRunRecord {
    pub system_id: SystemId,
    pub source_run_key: String,
    pub run: NormalizedWeatherRun,
    pub retention_class: ForecastRetentionClass,
    pub retain_until: Option<i64>,
    pub referenced_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherRunInsertOutcome {
    Inserted,
    AlreadyPresent,
}

#[async_trait]
pub trait YieldForecastInputRepository: Send + Sync {
    fn account_id(&self) -> AccountId;

    async fn insert_forecast_settings(
        &self,
        record: &ForecastSettingsRecord,
    ) -> Result<(), YieldForecastRepositoryError>;

    async fn effective_forecast_settings(
        &self,
        string_id: StringId,
        at: i64,
    ) -> Result<Option<ForecastSettingsRecord>, YieldForecastRepositoryError>;

    async fn insert_weather_run(
        &self,
        record: &WeatherRunRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldForecastRepositoryError>;

    async fn weather_run(
        &self,
        id: WeatherDataRunId,
    ) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError>;

    async fn select_weather_run(
        &self,
        system_id: SystemId,
        kind: WeatherDataKind,
        range: TimeRange,
        issued_before: Option<UtcTimestamp>,
    ) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError>;

    async fn retain_weather_run(
        &self,
        id: WeatherDataRunId,
        retention_class: ForecastRetentionClass,
        retain_until: Option<i64>,
        referenced_at: Option<i64>,
    ) -> Result<bool, YieldForecastRepositoryError>;

    async fn purge_expired_weather_runs(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<u64, YieldForecastRepositoryError>;
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteYieldForecastInputRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteYieldForecastInputRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresYieldForecastInputRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresYieldForecastInputRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresYieldForecastInputRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresYieldForecastInputRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl YieldForecastInputRepository for SqliteYieldForecastInputRepository {
    fn account_id(&self) -> AccountId {
        self.account.account_id()
    }

    async fn insert_forecast_settings(
        &self,
        record: &ForecastSettingsRecord,
    ) -> Result<(), YieldForecastRepositoryError> {
        validate_settings(record)?;
        let mut writer = self.account.acquire_writer().await?;
        let overlap: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pv_string_forecast_settings \
             WHERE string_id=? AND id<>? AND effective_from < coalesce(?, 9223372036854775807) \
             AND coalesce(effective_to, 9223372036854775807) > ?",
        )
        .bind(blob(record.string_id.as_uuid()))
        .bind(blob(record.id.as_uuid()))
        .bind(record.effective_to)
        .bind(record.effective_from)
        .fetch_one(writer.connection())
        .await?;
        if overlap != 0 {
            return Err(YieldForecastRepositoryError::Conflict(
                "forecast settings overlap an existing effective period",
            ));
        }
        sqlx::query(
            "INSERT INTO pv_string_forecast_settings \
             (id,system_id,string_id,effective_from,effective_to,model_identifier,model_revision, \
              soiling_loss_basis_points,shading_loss_basis_points,mismatch_loss_basis_points, \
              wiring_loss_basis_points,unavailability_loss_basis_points,calibration_basis_points, \
              configuration_digest,created_at,created_by) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(blob(record.id.as_uuid()))
        .bind(blob(record.system_id.as_uuid()))
        .bind(blob(record.string_id.as_uuid()))
        .bind(record.effective_from)
        .bind(record.effective_to)
        .bind(&record.model_identifier)
        .bind(i64::from(record.model_revision))
        .bind(i64::from(record.soiling_loss_basis_points))
        .bind(i64::from(record.shading_loss_basis_points))
        .bind(i64::from(record.mismatch_loss_basis_points))
        .bind(i64::from(record.wiring_loss_basis_points))
        .bind(i64::from(record.unavailability_loss_basis_points))
        .bind(record.calibration_basis_points)
        .bind(record.configuration_digest.to_vec())
        .bind(record.created_at)
        .bind(record.created_by.map(blob))
        .execute(writer.connection())
        .await?;
        Ok(())
    }

    async fn effective_forecast_settings(
        &self,
        string_id: StringId,
        at: i64,
    ) -> Result<Option<ForecastSettingsRecord>, YieldForecastRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row = sqlx::query(
            "SELECT id,system_id,string_id,effective_from,effective_to,model_identifier,model_revision, \
             soiling_loss_basis_points,shading_loss_basis_points,mismatch_loss_basis_points, \
             wiring_loss_basis_points,unavailability_loss_basis_points,calibration_basis_points, \
             configuration_digest,created_at,created_by FROM pv_string_forecast_settings \
             WHERE string_id=? AND effective_from<=? AND (effective_to IS NULL OR effective_to>?) \
             ORDER BY effective_from DESC,id DESC LIMIT 1",
        )
        .bind(blob(string_id.as_uuid()))
        .bind(at)
        .bind(at)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| sqlite_settings(&row)).transpose()
    }

    async fn insert_weather_run(
        &self,
        record: &WeatherRunRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldForecastRepositoryError> {
        validate_weather_record(record)?;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        let existing = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT id FROM weather_data_runs WHERE provider_configuration_id=? AND source_run_key=?",
        )
        .bind(blob(record.run.provenance.provider_id.as_uuid()))
        .bind(&record.source_run_key)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(existing) = existing {
            transaction.rollback().await?;
            if uuid_from_blob(&existing)? == record.run.id.as_uuid() {
                return Ok(WeatherRunInsertOutcome::AlreadyPresent);
            }
            return Err(YieldForecastRepositoryError::Conflict(
                "weather source run key already belongs to another immutable run",
            ));
        }
        insert_sqlite_weather_header(&mut transaction, record).await?;
        for point in &record.run.points {
            insert_sqlite_weather_point(&mut transaction, record.run.id, point).await?;
        }
        transaction.commit().await?;
        Ok(WeatherRunInsertOutcome::Inserted)
    }

    async fn weather_run(
        &self,
        id: WeatherDataRunId,
    ) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
        let mut connection = self.account.acquire().await?;
        load_sqlite_weather_run(&mut connection, id).await
    }

    async fn select_weather_run(
        &self,
        system_id: SystemId,
        kind: WeatherDataKind,
        range: TimeRange,
        issued_before: Option<UtcTimestamp>,
    ) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
        let start = timestamp_i64(range.start)?;
        let end = timestamp_i64(range.end)?;
        let cutoff = issued_before.map(timestamp_i64).transpose()?;
        let mut connection = self.account.acquire().await?;
        let row = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT id FROM weather_data_runs WHERE system_id=? AND data_kind=? \
             AND valid_from<=? AND valid_to>=? AND (? IS NULL OR issued_at IS NULL OR issued_at<=?) \
             ORDER BY coalesce(issued_at,fetched_at) DESC,fetched_at DESC,id DESC LIMIT 1",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(weather_kind(kind))
        .bind(start)
        .bind(end)
        .bind(cutoff)
        .bind(cutoff)
        .fetch_optional(&mut *connection)
        .await?;
        match row {
            Some(id) => {
                let id = WeatherDataRunId::from_uuid(uuid_from_blob(&id)?)?;
                load_sqlite_weather_run(&mut connection, id).await
            }
            None => Ok(None),
        }
    }

    async fn retain_weather_run(
        &self,
        id: WeatherDataRunId,
        retention_class: ForecastRetentionClass,
        retain_until: Option<i64>,
        referenced_at: Option<i64>,
    ) -> Result<bool, YieldForecastRepositoryError> {
        validate_retention(retention_class, retain_until, referenced_at)?;
        let mut writer = self.account.acquire_writer().await?;
        let changed = sqlx::query(
            "UPDATE weather_data_runs SET retention_class=?,retain_until=?,referenced_at=? WHERE id=?",
        )
        .bind(retention_class.as_str())
        .bind(retain_until)
        .bind(referenced_at)
        .bind(blob(id.as_uuid()))
        .execute(writer.connection())
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    async fn purge_expired_weather_runs(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<u64, YieldForecastRepositoryError> {
        validate_retention_limit(limit)?;
        let mut writer = self.account.acquire_writer().await?;
        let changed = sqlx::query(
            "DELETE FROM weather_data_runs WHERE id IN (SELECT w.id FROM weather_data_runs w \
             WHERE w.retention_class='working' AND w.referenced_at IS NULL AND w.retain_until<=? \
             AND NOT EXISTS (SELECT 1 FROM yield_calculation_runs c WHERE c.weather_run_id=w.id) \
             ORDER BY w.retain_until,w.id LIMIT ?)",
        )
        .bind(now)
        .bind(i64::from(limit))
        .execute(writer.connection())
        .await?
        .rows_affected();
        Ok(changed)
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl YieldForecastInputRepository for PostgresYieldForecastInputRepository {
    fn account_id(&self) -> AccountId {
        self.account_id
    }

    async fn insert_forecast_settings(
        &self,
        record: &ForecastSettingsRecord,
    ) -> Result<(), YieldForecastRepositoryError> {
        validate_settings(record)?;
        let mut connection = self.connection().await?;
        let overlap: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM account_data.pv_string_forecast_settings \
             WHERE account_id=$1 AND string_id=$2 AND id<>$3 \
             AND effective_from < coalesce($4, 9223372036854775807) \
             AND coalesce(effective_to, 9223372036854775807) > $5",
        )
        .bind(self.account_id.as_uuid())
        .bind(record.string_id.as_uuid())
        .bind(record.id.as_uuid())
        .bind(record.effective_to)
        .bind(record.effective_from)
        .fetch_one(&mut connection)
        .await?;
        if overlap != 0 {
            return Err(YieldForecastRepositoryError::Conflict(
                "forecast settings overlap an existing effective period",
            ));
        }
        sqlx::query(
            "INSERT INTO account_data.pv_string_forecast_settings \
             (account_id,id,system_id,string_id,effective_from,effective_to,model_identifier,model_revision, \
              soiling_loss_basis_points,shading_loss_basis_points,mismatch_loss_basis_points, \
              wiring_loss_basis_points,unavailability_loss_basis_points,calibration_basis_points, \
              configuration_digest,created_at,created_by) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
        )
        .bind(self.account_id.as_uuid())
        .bind(record.id.as_uuid())
        .bind(record.system_id.as_uuid())
        .bind(record.string_id.as_uuid())
        .bind(record.effective_from)
        .bind(record.effective_to)
        .bind(&record.model_identifier)
        .bind(i32::from(record.model_revision))
        .bind(i32::from(record.soiling_loss_basis_points))
        .bind(i32::from(record.shading_loss_basis_points))
        .bind(i32::from(record.mismatch_loss_basis_points))
        .bind(i32::from(record.wiring_loss_basis_points))
        .bind(i32::from(record.unavailability_loss_basis_points))
        .bind(record.calibration_basis_points)
        .bind(record.configuration_digest.to_vec())
        .bind(record.created_at)
        .bind(record.created_by)
        .execute(&mut connection)
        .await?;
        Ok(())
    }

    async fn effective_forecast_settings(
        &self,
        string_id: StringId,
        at: i64,
    ) -> Result<Option<ForecastSettingsRecord>, YieldForecastRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT id,system_id,string_id,effective_from,effective_to,model_identifier,model_revision, \
             soiling_loss_basis_points,shading_loss_basis_points,mismatch_loss_basis_points, \
             wiring_loss_basis_points,unavailability_loss_basis_points,calibration_basis_points, \
             configuration_digest,created_at,created_by \
             FROM account_data.pv_string_forecast_settings \
             WHERE account_id=$1 AND string_id=$2 AND effective_from<=$3 \
             AND (effective_to IS NULL OR effective_to>$3) \
             ORDER BY effective_from DESC,id DESC LIMIT 1",
        )
        .bind(self.account_id.as_uuid())
        .bind(string_id.as_uuid())
        .bind(at)
        .fetch_optional(&mut connection)
        .await?;
        row.map(|row| postgres_settings(&row)).transpose()
    }

    async fn insert_weather_run(
        &self,
        record: &WeatherRunRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldForecastRepositoryError> {
        validate_weather_record(record)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM account_data.weather_data_runs \
             WHERE account_id=$1 AND provider_configuration_id=$2 AND source_run_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(record.run.provenance.provider_id.as_uuid())
        .bind(&record.source_run_key)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(existing) = existing {
            transaction.rollback().await?;
            if existing == record.run.id.as_uuid() {
                return Ok(WeatherRunInsertOutcome::AlreadyPresent);
            }
            return Err(YieldForecastRepositoryError::Conflict(
                "weather source run key already belongs to another immutable run",
            ));
        }
        insert_postgres_weather_header(&mut transaction, self.account_id, record).await?;
        for point in &record.run.points {
            insert_postgres_weather_point(&mut transaction, self.account_id, record.run.id, point)
                .await?;
        }
        transaction.commit().await?;
        Ok(WeatherRunInsertOutcome::Inserted)
    }

    async fn weather_run(
        &self,
        id: WeatherDataRunId,
    ) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
        let mut connection = self.connection().await?;
        load_postgres_weather_run(&mut connection, self.account_id, id).await
    }

    async fn select_weather_run(
        &self,
        system_id: SystemId,
        kind: WeatherDataKind,
        range: TimeRange,
        issued_before: Option<UtcTimestamp>,
    ) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
        let start = timestamp_i64(range.start)?;
        let end = timestamp_i64(range.end)?;
        let cutoff = issued_before.map(timestamp_i64).transpose()?;
        let mut connection = self.connection().await?;
        let id = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM account_data.weather_data_runs \
             WHERE account_id=$1 AND system_id=$2 AND data_kind=$3 \
             AND valid_from<=$4 AND valid_to>=$5 \
             AND ($6 IS NULL OR issued_at IS NULL OR issued_at<=$6) \
             ORDER BY coalesce(issued_at,fetched_at) DESC,fetched_at DESC,id DESC LIMIT 1",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(weather_kind(kind))
        .bind(start)
        .bind(end)
        .bind(cutoff)
        .fetch_optional(&mut connection)
        .await?;
        match id {
            Some(id) => {
                let id = WeatherDataRunId::from_uuid(id)?;
                load_postgres_weather_run(&mut connection, self.account_id, id).await
            }
            None => Ok(None),
        }
    }

    async fn retain_weather_run(
        &self,
        id: WeatherDataRunId,
        retention_class: ForecastRetentionClass,
        retain_until: Option<i64>,
        referenced_at: Option<i64>,
    ) -> Result<bool, YieldForecastRepositoryError> {
        validate_retention(retention_class, retain_until, referenced_at)?;
        let mut connection = self.connection().await?;
        let changed = sqlx::query(
            "UPDATE account_data.weather_data_runs SET retention_class=$1,retain_until=$2,referenced_at=$3 \
             WHERE account_id=$4 AND id=$5",
        )
        .bind(retention_class.as_str())
        .bind(retain_until)
        .bind(referenced_at)
        .bind(self.account_id.as_uuid())
        .bind(id.as_uuid())
        .execute(&mut connection)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    async fn purge_expired_weather_runs(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<u64, YieldForecastRepositoryError> {
        validate_retention_limit(limit)?;
        let mut connection = self.connection().await?;
        let changed = sqlx::query(
            "DELETE FROM account_data.weather_data_runs w WHERE w.account_id=$1 AND w.id IN ( \
             SELECT candidate.id FROM account_data.weather_data_runs candidate \
             WHERE candidate.account_id=$1 AND candidate.retention_class='working' \
             AND candidate.referenced_at IS NULL AND candidate.retain_until<=$2 \
             AND NOT EXISTS (SELECT 1 FROM account_data.yield_calculation_runs c \
                 WHERE c.account_id=$1 AND c.weather_run_id=candidate.id) \
             ORDER BY candidate.retain_until,candidate.id LIMIT $3)",
        )
        .bind(self.account_id.as_uuid())
        .bind(now)
        .bind(i64::from(limit))
        .execute(&mut connection)
        .await?
        .rows_affected();
        Ok(changed)
    }
}

#[cfg(feature = "sqlite")]
async fn insert_sqlite_weather_header(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record: &WeatherRunRecord,
) -> Result<(), YieldForecastRepositoryError> {
    let (spatial_kind, latitude, longitude, region) = spatial_columns(&record.run.spatial_coverage);
    sqlx::query(
        "INSERT INTO weather_data_runs \
         (id,system_id,provider_configuration_id,source_run_key,data_kind,issued_at,fetched_at, \
          valid_from,valid_to,resolution_seconds,spatial_kind,latitude_e6,longitude_e6,provider_region, \
          adapter,source_url,license_identifier,attribution,provenance_json,retention_class, \
          retain_until,referenced_at,created_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(blob(record.run.id.as_uuid()))
    .bind(blob(record.system_id.as_uuid()))
    .bind(blob(record.run.provenance.provider_id.as_uuid()))
    .bind(&record.source_run_key)
    .bind(weather_kind(record.run.kind))
    .bind(record.run.issued_at.map(timestamp_i64).transpose()?)
    .bind(timestamp_i64(record.run.provenance.fetched_at)?)
    .bind(timestamp_i64(record.run.valid_range.start)?)
    .bind(timestamp_i64(record.run.valid_range.end)?)
    .bind(i64::from(record.run.resolution_seconds))
    .bind(spatial_kind)
    .bind(latitude)
    .bind(longitude)
    .bind(region)
    .bind(&record.run.provenance.adapter)
    .bind(record.run.provenance.source_url.as_str())
    .bind(&record.run.provenance.license_identifier)
    .bind(&record.run.provenance.attribution)
    .bind("{}")
    .bind(record.retention_class.as_str())
    .bind(record.retain_until)
    .bind(record.referenced_at)
    .bind(record.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn insert_postgres_weather_header(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: AccountId,
    record: &WeatherRunRecord,
) -> Result<(), YieldForecastRepositoryError> {
    let (spatial_kind, latitude, longitude, region) = spatial_columns(&record.run.spatial_coverage);
    sqlx::query(
        "INSERT INTO account_data.weather_data_runs \
         (account_id,id,system_id,provider_configuration_id,source_run_key,data_kind,issued_at,fetched_at, \
          valid_from,valid_to,resolution_seconds,spatial_kind,latitude_e6,longitude_e6,provider_region, \
          adapter,source_url,license_identifier,attribution,provenance,retention_class, \
          retain_until,referenced_at,created_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)",
    )
    .bind(account_id.as_uuid())
    .bind(record.run.id.as_uuid())
    .bind(record.system_id.as_uuid())
    .bind(record.run.provenance.provider_id.as_uuid())
    .bind(&record.source_run_key)
    .bind(weather_kind(record.run.kind))
    .bind(record.run.issued_at.map(timestamp_i64).transpose()?)
    .bind(timestamp_i64(record.run.provenance.fetched_at)?)
    .bind(timestamp_i64(record.run.valid_range.start)?)
    .bind(timestamp_i64(record.run.valid_range.end)?)
    .bind(i32::try_from(record.run.resolution_seconds).map_err(|_| {
        YieldForecastRepositoryError::Validation("weather resolution exceeds database range")
    })?)
    .bind(spatial_kind)
    .bind(latitude)
    .bind(longitude)
    .bind(region)
    .bind(&record.run.provenance.adapter)
    .bind(record.run.provenance.source_url.as_str())
    .bind(&record.run.provenance.license_identifier)
    .bind(&record.run.provenance.attribution)
    .bind(serde_json::json!({}))
    .bind(record.retention_class.as_str())
    .bind(record.retain_until)
    .bind(record.referenced_at)
    .bind(record.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn insert_sqlite_weather_point(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    run_id: WeatherDataRunId,
    point: &NormalizedWeatherPoint,
) -> Result<(), YieldForecastRepositoryError> {
    let values = point_values(point);
    sqlx::query(
        "INSERT INTO weather_data_points \
         (run_id,interval_start,interval_end,global_horizontal_wm2,global_horizontal_lower_wm2, \
          global_horizontal_upper_wm2,direct_normal_wm2,direct_normal_lower_wm2,direct_normal_upper_wm2, \
          diffuse_horizontal_wm2,diffuse_horizontal_lower_wm2,diffuse_horizontal_upper_wm2, \
          plane_of_array_wm2,plane_of_array_lower_wm2,plane_of_array_upper_wm2, \
          ambient_temperature_millicelsius,wind_speed_millimetres_per_second,cloud_cover_basis_points) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(blob(run_id.as_uuid()))
    .bind(timestamp_i64(point.interval.start)?)
    .bind(timestamp_i64(point.interval.end)?)
    .bind(values[0])
    .bind(values[1])
    .bind(values[2])
    .bind(values[3])
    .bind(values[4])
    .bind(values[5])
    .bind(values[6])
    .bind(values[7])
    .bind(values[8])
    .bind(values[9])
    .bind(values[10])
    .bind(values[11])
    .bind(point.ambient_temperature.map(MilliDegreesCelsius::value))
    .bind(point.wind_speed.map(MetresPerSecondMilli::value))
    .bind(point.cloud_cover.map(UnsignedBasisPoints::value))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn insert_postgres_weather_point(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: AccountId,
    run_id: WeatherDataRunId,
    point: &NormalizedWeatherPoint,
) -> Result<(), YieldForecastRepositoryError> {
    let values = postgres_point_values(point)?;
    sqlx::query::<sqlx::Postgres>(
        "INSERT INTO account_data.weather_data_points \
         (account_id,run_id,interval_start,interval_end,global_horizontal_wm2,global_horizontal_lower_wm2, \
          global_horizontal_upper_wm2,direct_normal_wm2,direct_normal_lower_wm2,direct_normal_upper_wm2, \
          diffuse_horizontal_wm2,diffuse_horizontal_lower_wm2,diffuse_horizontal_upper_wm2, \
          plane_of_array_wm2,plane_of_array_lower_wm2,plane_of_array_upper_wm2, \
          ambient_temperature_millicelsius,wind_speed_millimetres_per_second,cloud_cover_basis_points) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",
    )
    .bind(account_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(timestamp_i64(point.interval.start)?)
    .bind(timestamp_i64(point.interval.end)?)
    .bind(values[0])
    .bind(values[1])
    .bind(values[2])
    .bind(values[3])
    .bind(values[4])
    .bind(values[5])
    .bind(values[6])
    .bind(values[7])
    .bind(values[8])
    .bind(values[9])
    .bind(values[10])
    .bind(values[11])
    .bind(point.ambient_temperature.map(MilliDegreesCelsius::value))
    .bind(point.wind_speed.map(|value| i32::try_from(value.value())).transpose().map_err(|_| {
        YieldForecastRepositoryError::Validation("wind speed exceeds database range")
    })?)
    .bind(
        point
            .cloud_cover
            .map(|value| i32::from(value.value())),
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn load_sqlite_weather_run(
    connection: &mut sqlx::SqliteConnection,
    id: WeatherDataRunId,
) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
    let row = sqlx::query(
        "SELECT id,system_id,provider_configuration_id,source_run_key,data_kind,issued_at,fetched_at, \
         valid_from,valid_to,resolution_seconds,spatial_kind,latitude_e6,longitude_e6,provider_region, \
         adapter,source_url,license_identifier,attribution,retention_class,retain_until,referenced_at,created_at \
         FROM weather_data_runs WHERE id=?",
    )
    .bind(blob(id.as_uuid()))
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let points = sqlx::query(
        "SELECT interval_start,interval_end,global_horizontal_wm2,global_horizontal_lower_wm2, \
         global_horizontal_upper_wm2,direct_normal_wm2,direct_normal_lower_wm2,direct_normal_upper_wm2, \
         diffuse_horizontal_wm2,diffuse_horizontal_lower_wm2,diffuse_horizontal_upper_wm2, \
         plane_of_array_wm2,plane_of_array_lower_wm2,plane_of_array_upper_wm2, \
         ambient_temperature_millicelsius,wind_speed_millimetres_per_second,cloud_cover_basis_points \
         FROM weather_data_points WHERE run_id=? ORDER BY interval_start",
    )
    .bind(blob(id.as_uuid()))
    .fetch_all(&mut *connection)
    .await?;
    sqlite_weather_record(&row, &points)
}

#[cfg(feature = "postgres")]
async fn load_postgres_weather_run(
    connection: &mut PgConnection,
    account_id: AccountId,
    id: WeatherDataRunId,
) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
    let row = sqlx::query(
        "SELECT id,system_id,provider_configuration_id,source_run_key,data_kind,issued_at,fetched_at, \
         valid_from,valid_to,resolution_seconds,spatial_kind,latitude_e6,longitude_e6,provider_region, \
         adapter,source_url,license_identifier,attribution,retention_class,retain_until,referenced_at,created_at \
         FROM account_data.weather_data_runs WHERE account_id=$1 AND id=$2",
    )
    .bind(account_id.as_uuid())
    .bind(id.as_uuid())
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let points = sqlx::query(
        "SELECT interval_start,interval_end,global_horizontal_wm2,global_horizontal_lower_wm2, \
         global_horizontal_upper_wm2,direct_normal_wm2,direct_normal_lower_wm2,direct_normal_upper_wm2, \
         diffuse_horizontal_wm2,diffuse_horizontal_lower_wm2,diffuse_horizontal_upper_wm2, \
         plane_of_array_wm2,plane_of_array_lower_wm2,plane_of_array_upper_wm2, \
         ambient_temperature_millicelsius,wind_speed_millimetres_per_second,cloud_cover_basis_points \
         FROM account_data.weather_data_points WHERE account_id=$1 AND run_id=$2 ORDER BY interval_start",
    )
    .bind(account_id.as_uuid())
    .bind(id.as_uuid())
    .fetch_all(&mut *connection)
    .await?;
    postgres_weather_record(&row, &points)
}

#[cfg(feature = "sqlite")]
fn sqlite_settings(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ForecastSettingsRecord, YieldForecastRepositoryError> {
    Ok(ForecastSettingsRecord {
        id: ForecastSettingsId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("id")?)?)?,
        system_id: SystemId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("system_id")?)?)?,
        string_id: StringId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("string_id")?)?)?,
        effective_from: row.try_get("effective_from")?,
        effective_to: row.try_get("effective_to")?,
        model_identifier: row.try_get("model_identifier")?,
        model_revision: integer_u16(row.try_get("model_revision")?, "model revision")?,
        soiling_loss_basis_points: integer_u16(
            row.try_get("soiling_loss_basis_points")?,
            "soiling loss",
        )?,
        shading_loss_basis_points: integer_u16(
            row.try_get("shading_loss_basis_points")?,
            "shading loss",
        )?,
        mismatch_loss_basis_points: integer_u16(
            row.try_get("mismatch_loss_basis_points")?,
            "mismatch loss",
        )?,
        wiring_loss_basis_points: integer_u16(
            row.try_get("wiring_loss_basis_points")?,
            "wiring loss",
        )?,
        unavailability_loss_basis_points: integer_u16(
            row.try_get("unavailability_loss_basis_points")?,
            "unavailability loss",
        )?,
        calibration_basis_points: row.try_get("calibration_basis_points")?,
        configuration_digest: digest(&row.try_get::<Vec<u8>, _>("configuration_digest")?)?,
        created_at: row.try_get("created_at")?,
        created_by: row
            .try_get::<Option<Vec<u8>>, _>("created_by")?
            .map(|value| uuid_from_blob(&value))
            .transpose()?,
    })
}

#[cfg(feature = "postgres")]
fn postgres_settings(
    row: &sqlx::postgres::PgRow,
) -> Result<ForecastSettingsRecord, YieldForecastRepositoryError> {
    Ok(ForecastSettingsRecord {
        id: ForecastSettingsId::from_uuid(row.try_get("id")?)?,
        system_id: SystemId::from_uuid(row.try_get("system_id")?)?,
        string_id: StringId::from_uuid(row.try_get("string_id")?)?,
        effective_from: row.try_get("effective_from")?,
        effective_to: row.try_get("effective_to")?,
        model_identifier: row.try_get("model_identifier")?,
        model_revision: integer_u16(
            i64::from(row.try_get::<i32, _>("model_revision")?),
            "model revision",
        )?,
        soiling_loss_basis_points: integer_u16(
            i64::from(row.try_get::<i32, _>("soiling_loss_basis_points")?),
            "soiling loss",
        )?,
        shading_loss_basis_points: integer_u16(
            i64::from(row.try_get::<i32, _>("shading_loss_basis_points")?),
            "shading loss",
        )?,
        mismatch_loss_basis_points: integer_u16(
            i64::from(row.try_get::<i32, _>("mismatch_loss_basis_points")?),
            "mismatch loss",
        )?,
        wiring_loss_basis_points: integer_u16(
            i64::from(row.try_get::<i32, _>("wiring_loss_basis_points")?),
            "wiring loss",
        )?,
        unavailability_loss_basis_points: integer_u16(
            i64::from(row.try_get::<i32, _>("unavailability_loss_basis_points")?),
            "unavailability loss",
        )?,
        calibration_basis_points: row.try_get("calibration_basis_points")?,
        configuration_digest: digest(&row.try_get::<Vec<u8>, _>("configuration_digest")?)?,
        created_at: row.try_get("created_at")?,
        created_by: row.try_get("created_by")?,
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_weather_record(
    row: &sqlx::sqlite::SqliteRow,
    point_rows: &[sqlx::sqlite::SqliteRow],
) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
    let id = WeatherDataRunId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("id")?)?)?;
    let points = point_rows
        .iter()
        .map(sqlite_weather_point)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(weather_record_from_columns(
        id,
        SystemId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("system_id")?)?)?,
        ProviderId::from_uuid(uuid_from_blob(
            &row.try_get::<Vec<u8>, _>("provider_configuration_id")?,
        )?)?,
        row.try_get("source_run_key")?,
        row.try_get("data_kind")?,
        row.try_get("issued_at")?,
        row.try_get("fetched_at")?,
        row.try_get("valid_from")?,
        row.try_get("valid_to")?,
        row.try_get("resolution_seconds")?,
        row.try_get("spatial_kind")?,
        row.try_get("latitude_e6")?,
        row.try_get("longitude_e6")?,
        row.try_get("provider_region")?,
        row.try_get("adapter")?,
        row.try_get("source_url")?,
        row.try_get("license_identifier")?,
        row.try_get("attribution")?,
        row.try_get("retention_class")?,
        row.try_get("retain_until")?,
        row.try_get("referenced_at")?,
        row.try_get("created_at")?,
        points,
    )?))
}

#[cfg(feature = "postgres")]
fn postgres_weather_record(
    row: &sqlx::postgres::PgRow,
    point_rows: &[sqlx::postgres::PgRow],
) -> Result<Option<WeatherRunRecord>, YieldForecastRepositoryError> {
    let points = point_rows
        .iter()
        .map(postgres_weather_point)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(weather_record_from_columns(
        WeatherDataRunId::from_uuid(row.try_get("id")?)?,
        SystemId::from_uuid(row.try_get("system_id")?)?,
        ProviderId::from_uuid(row.try_get("provider_configuration_id")?)?,
        row.try_get("source_run_key")?,
        row.try_get("data_kind")?,
        row.try_get("issued_at")?,
        row.try_get("fetched_at")?,
        row.try_get("valid_from")?,
        row.try_get("valid_to")?,
        i64::from(row.try_get::<i32, _>("resolution_seconds")?),
        row.try_get("spatial_kind")?,
        row.try_get("latitude_e6")?,
        row.try_get("longitude_e6")?,
        row.try_get("provider_region")?,
        row.try_get("adapter")?,
        row.try_get("source_url")?,
        row.try_get("license_identifier")?,
        row.try_get("attribution")?,
        row.try_get("retention_class")?,
        row.try_get("retain_until")?,
        row.try_get("referenced_at")?,
        row.try_get("created_at")?,
        points,
    )?))
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn weather_record_from_columns(
    id: WeatherDataRunId,
    system_id: SystemId,
    provider_id: ProviderId,
    source_run_key: String,
    data_kind: String,
    issued_at: Option<i64>,
    fetched_at: i64,
    valid_from: i64,
    valid_to: i64,
    resolution_seconds: i64,
    spatial_kind: String,
    latitude: Option<i32>,
    longitude: Option<i32>,
    region: Option<String>,
    adapter: String,
    source_url: String,
    license_identifier: String,
    attribution: String,
    retention_class: String,
    retain_until: Option<i64>,
    referenced_at: Option<i64>,
    created_at: i64,
    points: Vec<NormalizedWeatherPoint>,
) -> Result<WeatherRunRecord, YieldForecastRepositoryError> {
    Ok(WeatherRunRecord {
        system_id,
        source_run_key,
        run: NormalizedWeatherRun {
            id,
            kind: parse_weather_kind(&data_kind)?,
            issued_at: issued_at.map(timestamp).transpose()?,
            valid_range: TimeRange::new(timestamp(valid_from)?, timestamp(valid_to)?)
                .map_err(|_| YieldForecastRepositoryError::Corrupt("invalid weather run range"))?,
            resolution_seconds: u32::try_from(resolution_seconds).map_err(|_| {
                YieldForecastRepositoryError::Corrupt("weather resolution is out of range")
            })?,
            spatial_coverage: parse_spatial(&spatial_kind, latitude, longitude, region)?,
            provenance: WeatherDataProvenance {
                provider_id,
                adapter,
                source_url: Url::parse(&source_url)?,
                license_identifier,
                attribution,
                fetched_at: timestamp(fetched_at)?,
            },
            points,
        },
        retention_class: ForecastRetentionClass::parse(&retention_class)?,
        retain_until,
        referenced_at,
        created_at,
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_weather_point(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<NormalizedWeatherPoint, YieldForecastRepositoryError> {
    weather_point_from_row(
        row.try_get("interval_start")?,
        row.try_get("interval_end")?,
        point_estimate(row, "global_horizontal")?,
        point_estimate(row, "direct_normal")?,
        point_estimate(row, "diffuse_horizontal")?,
        point_estimate(row, "plane_of_array")?,
        row.try_get("ambient_temperature_millicelsius")?,
        row.try_get::<Option<i64>, _>("wind_speed_millimetres_per_second")?,
        row.try_get::<Option<i64>, _>("cloud_cover_basis_points")?,
    )
}

#[cfg(feature = "postgres")]
fn postgres_weather_point(
    row: &sqlx::postgres::PgRow,
) -> Result<NormalizedWeatherPoint, YieldForecastRepositoryError> {
    weather_point_from_row(
        row.try_get("interval_start")?,
        row.try_get("interval_end")?,
        postgres_point_estimate(row, "global_horizontal")?,
        postgres_point_estimate(row, "direct_normal")?,
        postgres_point_estimate(row, "diffuse_horizontal")?,
        postgres_point_estimate(row, "plane_of_array")?,
        row.try_get("ambient_temperature_millicelsius")?,
        row.try_get::<Option<i32>, _>("wind_speed_millimetres_per_second")?
            .map(i64::from),
        row.try_get::<Option<i32>, _>("cloud_cover_basis_points")?
            .map(i64::from),
    )
}

#[allow(clippy::too_many_arguments)]
fn weather_point_from_row(
    interval_start: i64,
    interval_end: i64,
    global_horizontal: Option<EstimateRange<WattsPerSquareMetre>>,
    direct_normal: Option<EstimateRange<WattsPerSquareMetre>>,
    diffuse_horizontal: Option<EstimateRange<WattsPerSquareMetre>>,
    plane_of_array: Option<EstimateRange<WattsPerSquareMetre>>,
    ambient_temperature: Option<i32>,
    wind_speed: Option<i64>,
    cloud_cover: Option<i64>,
) -> Result<NormalizedWeatherPoint, YieldForecastRepositoryError> {
    Ok(NormalizedWeatherPoint {
        interval: TimeRange::new(timestamp(interval_start)?, timestamp(interval_end)?)
            .map_err(|_| YieldForecastRepositoryError::Corrupt("invalid weather point range"))?,
        irradiance: IrradiancePoint {
            global_horizontal,
            direct_normal,
            diffuse_horizontal,
            plane_of_array,
        },
        ambient_temperature: ambient_temperature.map(MilliDegreesCelsius::new),
        wind_speed: wind_speed
            .map(|value| u32::try_from(value).map(MetresPerSecondMilli::new))
            .transpose()
            .map_err(|_| YieldForecastRepositoryError::Corrupt("wind speed is out of range"))?,
        cloud_cover: cloud_cover
            .map(|value| {
                u16::try_from(value)
                    .map_err(|_| ())
                    .and_then(|value| UnsignedBasisPoints::new(value).map_err(|_| ()))
            })
            .transpose()
            .map_err(|()| YieldForecastRepositoryError::Corrupt("cloud cover is out of range"))?,
    })
}

#[cfg(feature = "sqlite")]
fn point_estimate(
    row: &sqlx::sqlite::SqliteRow,
    prefix: &str,
) -> Result<Option<EstimateRange<WattsPerSquareMetre>>, YieldForecastRepositoryError> {
    estimate_from_values(
        row.try_get::<Option<i64>, _>(format!("{prefix}_wm2").as_str())?,
        row.try_get::<Option<i64>, _>(format!("{prefix}_lower_wm2").as_str())?,
        row.try_get::<Option<i64>, _>(format!("{prefix}_upper_wm2").as_str())?,
    )
}

#[cfg(feature = "postgres")]
fn postgres_point_estimate(
    row: &sqlx::postgres::PgRow,
    prefix: &str,
) -> Result<Option<EstimateRange<WattsPerSquareMetre>>, YieldForecastRepositoryError> {
    estimate_from_values(
        row.try_get::<Option<i32>, _>(format!("{prefix}_wm2").as_str())?
            .map(i64::from),
        row.try_get::<Option<i32>, _>(format!("{prefix}_lower_wm2").as_str())?
            .map(i64::from),
        row.try_get::<Option<i32>, _>(format!("{prefix}_upper_wm2").as_str())?
            .map(i64::from),
    )
}

fn estimate_from_values(
    central: Option<i64>,
    lower: Option<i64>,
    upper: Option<i64>,
) -> Result<Option<EstimateRange<WattsPerSquareMetre>>, YieldForecastRepositoryError> {
    central
        .map(|central| {
            Ok(EstimateRange {
                central: irradiance(central)?,
                lower: lower.map(irradiance).transpose()?,
                upper: upper.map(irradiance).transpose()?,
            })
        })
        .transpose()
}

fn validate_settings(record: &ForecastSettingsRecord) -> Result<(), YieldForecastRepositoryError> {
    if record
        .effective_to
        .is_some_and(|end| end <= record.effective_from)
    {
        return Err(YieldForecastRepositoryError::Validation(
            "forecast settings effective range is empty",
        ));
    }
    if record.model_identifier.trim().is_empty() || record.model_revision == 0 {
        return Err(YieldForecastRepositoryError::Validation(
            "forecast model identifier and revision are required",
        ));
    }
    if [
        record.soiling_loss_basis_points,
        record.shading_loss_basis_points,
        record.mismatch_loss_basis_points,
        record.wiring_loss_basis_points,
        record.unavailability_loss_basis_points,
    ]
    .into_iter()
    .any(|value| value > 10_000)
        || !(-10_000..=10_000).contains(&record.calibration_basis_points)
    {
        return Err(YieldForecastRepositoryError::Validation(
            "forecast loss or calibration is outside the supported range",
        ));
    }
    Ok(())
}

fn validate_weather_record(record: &WeatherRunRecord) -> Result<(), YieldForecastRepositoryError> {
    if record.source_run_key.trim().is_empty() || record.run.resolution_seconds == 0 {
        return Err(YieldForecastRepositoryError::Validation(
            "weather source key and positive resolution are required",
        ));
    }
    if record.run.kind == WeatherDataKind::Forecast && record.run.issued_at.is_none() {
        return Err(YieldForecastRepositoryError::Validation(
            "forecast weather runs require an issue time",
        ));
    }
    let mut previous_end = None;
    for point in &record.run.points {
        if !record.run.valid_range.contains(point.interval.start)
            || point.interval.end > record.run.valid_range.end
            || previous_end.is_some_and(|end| point.interval.start < end)
        {
            return Err(YieldForecastRepositoryError::Validation(
                "weather points must be ordered, non-overlapping, and inside the run range",
            ));
        }
        if point.irradiance.plane_of_array.is_none()
            && point.irradiance.global_horizontal.is_none()
            && (point.irradiance.direct_normal.is_none()
                || point.irradiance.diffuse_horizontal.is_none())
        {
            return Err(YieldForecastRepositoryError::Validation(
                "weather point lacks a usable irradiance input",
            ));
        }
        previous_end = Some(point.interval.end);
    }
    Ok(())
}

fn validate_retention(
    retention_class: ForecastRetentionClass,
    retain_until: Option<i64>,
    referenced_at: Option<i64>,
) -> Result<(), YieldForecastRepositoryError> {
    if retention_class == ForecastRetentionClass::Referenced && referenced_at.is_none() {
        return Err(YieldForecastRepositoryError::Validation(
            "referenced weather retention requires a reference time",
        ));
    }
    if retention_class != ForecastRetentionClass::Referenced && referenced_at.is_some() {
        return Err(YieldForecastRepositoryError::Validation(
            "only referenced weather runs may carry a reference time",
        ));
    }
    if retention_class == ForecastRetentionClass::Working && retain_until.is_none() {
        return Err(YieldForecastRepositoryError::Validation(
            "working weather retention requires an expiry",
        ));
    }
    Ok(())
}

fn validate_retention_limit(limit: u32) -> Result<(), YieldForecastRepositoryError> {
    if limit == 0 || limit > 10_000 {
        Err(YieldForecastRepositoryError::Validation(
            "retention limit must be between 1 and 10000",
        ))
    } else {
        Ok(())
    }
}

fn point_values(point: &NormalizedWeatherPoint) -> [Option<i64>; 12] {
    let ghi = irradiance_values(point.irradiance.global_horizontal);
    let dni = irradiance_values(point.irradiance.direct_normal);
    let dhi = irradiance_values(point.irradiance.diffuse_horizontal);
    let poa = irradiance_values(point.irradiance.plane_of_array);
    [
        ghi[0], ghi[1], ghi[2], dni[0], dni[1], dni[2], dhi[0], dhi[1], dhi[2], poa[0], poa[1],
        poa[2],
    ]
}

#[cfg(feature = "postgres")]
fn postgres_point_values(
    point: &NormalizedWeatherPoint,
) -> Result<[Option<i32>; 12], YieldForecastRepositoryError> {
    point_values(point)
        .map(|value| {
            value.map(i32::try_from).transpose().map_err(|_| {
                YieldForecastRepositoryError::Validation("irradiance exceeds database range")
            })
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| YieldForecastRepositoryError::Corrupt("invalid irradiance column count"))
}

fn irradiance_values(value: Option<EstimateRange<WattsPerSquareMetre>>) -> [Option<i64>; 3] {
    value.map_or([None; 3], |value| {
        [
            Some(i64::from(value.central.value())),
            value.lower.map(|item| i64::from(item.value())),
            value.upper.map(|item| i64::from(item.value())),
        ]
    })
}

fn spatial_columns(
    coverage: &SpatialCoverage,
) -> (&'static str, Option<i32>, Option<i32>, Option<&str>) {
    match coverage {
        SpatialCoverage::Point(point) => (
            "point",
            Some(point.latitude_microdegrees),
            Some(point.longitude_microdegrees),
            None,
        ),
        SpatialCoverage::ProviderRegion(region) => {
            ("provider_region", None, None, Some(region.as_str()))
        }
    }
}

fn parse_spatial(
    kind: &str,
    latitude: Option<i32>,
    longitude: Option<i32>,
    region: Option<String>,
) -> Result<SpatialCoverage, YieldForecastRepositoryError> {
    match (kind, latitude, longitude, region) {
        ("point", Some(latitude), Some(longitude), None) => {
            Ok(SpatialCoverage::Point(GeographicPoint {
                latitude_microdegrees: latitude,
                longitude_microdegrees: longitude,
            }))
        }
        ("provider_region", None, None, Some(region)) => {
            Ok(SpatialCoverage::ProviderRegion(region))
        }
        _ => Err(YieldForecastRepositoryError::Corrupt(
            "invalid weather spatial coverage",
        )),
    }
}

const fn weather_kind(kind: WeatherDataKind) -> &'static str {
    match kind {
        WeatherDataKind::Forecast => "forecast",
        WeatherDataKind::Observed => "observed",
        WeatherDataKind::Reanalysis => "reanalysis",
    }
}

fn parse_weather_kind(value: &str) -> Result<WeatherDataKind, YieldForecastRepositoryError> {
    match value {
        "forecast" => Ok(WeatherDataKind::Forecast),
        "observed" => Ok(WeatherDataKind::Observed),
        "reanalysis" => Ok(WeatherDataKind::Reanalysis),
        _ => Err(YieldForecastRepositoryError::Corrupt(
            "unknown weather data kind",
        )),
    }
}

fn timestamp_i64(value: UtcTimestamp) -> Result<i64, YieldForecastRepositoryError> {
    i64::try_from(value.epoch_millis())
        .map_err(|_| YieldForecastRepositoryError::Validation("timestamp is out of range"))
}

fn timestamp(value: i64) -> Result<UtcTimestamp, YieldForecastRepositoryError> {
    UtcTimestamp::from_epoch_millis(value)
        .map_err(|_| YieldForecastRepositoryError::Corrupt("timestamp is out of range"))
}

fn irradiance(value: i64) -> Result<WattsPerSquareMetre, YieldForecastRepositoryError> {
    u32::try_from(value)
        .map(WattsPerSquareMetre::new)
        .map_err(|_| YieldForecastRepositoryError::Corrupt("irradiance is out of range"))
}

fn integer_u16(value: i64, field: &'static str) -> Result<u16, YieldForecastRepositoryError> {
    u16::try_from(value).map_err(|_| YieldForecastRepositoryError::Corrupt(field))
}

fn digest(value: &[u8]) -> Result<[u8; 32], YieldForecastRepositoryError> {
    value
        .try_into()
        .map_err(|_| YieldForecastRepositoryError::Corrupt("invalid configuration digest"))
}

fn blob(value: Uuid) -> Vec<u8> {
    value.as_bytes().to_vec()
}

fn uuid_from_blob(value: &[u8]) -> Result<Uuid, YieldForecastRepositoryError> {
    Uuid::from_slice(value)
        .map_err(|_| YieldForecastRepositoryError::Corrupt("invalid UUID storage value"))
}

#[derive(Debug, Error)]
pub enum YieldForecastRepositoryError {
    #[error("yield forecast repository query failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[cfg(feature = "sqlite")]
    #[error("yield forecast SQLite routing failed: {0}")]
    SqliteRouting(#[from] crate::SqliteRoutingError),
    #[error("yield forecast repository identifier is invalid: {0}")]
    Identifier(#[from] pvlog_domain::IdentifierError),
    #[error("yield forecast repository URL is invalid: {0}")]
    Url(#[from] url::ParseError),
    #[error("yield forecast repository input is invalid: {0}")]
    Validation(&'static str),
    #[error("yield forecast repository conflict: {0}")]
    Conflict(&'static str),
    #[error("yield forecast repository row is corrupt: {0}")]
    Corrupt(&'static str),
}
