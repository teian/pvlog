//! Storage-backed modern analytics adapter.
//!
//! Resolves the owning account for a system, enforces account membership, and
//! executes series, statistics, and data-quality reads against that account's
//! database. Exports reuse the series read and are always produced
//! synchronously; the asynchronous export queue is not wired yet.

use std::{fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use pvlog_application::{
    AnalysisExportFormat, AnalysisExportRequest, AnalysisExportResult, Clock, DataQualityIssue,
    DataQualityPolicy, EnergyStatistics, ModernAnalyticsError, ModernAnalyticsUseCases,
    QualitySample, QueryPlanRequest, RejectedIngestion, SeriesField, SeriesQueryError,
    SeriesQueryService, StatisticsBucket, StatisticsPeriod, calculate_statistics,
    detect_data_quality,
};
use pvlog_domain::{AccountId, SystemId, UserId};
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::Row;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};

use crate::analytics_series::{AccountSeriesRepository, SeriesScope};

const MILLIS_PER_DAY: u64 = 86_400_000;
const DEFAULT_EXPECTED_INTERVAL_MILLIS: u64 = 300_000;
const SUSPECT_QUALITY_MASK: u32 = 0xFFFF;
const AGGREGATE_LAG_THRESHOLD_MILLIS: u64 = 3_600_000;

/// Serves the modern analytics endpoints from account telemetry storage.
pub struct StorageAnalyticsApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageAnalyticsApi {
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target,
            management,
            clock,
        }
    }

    /// Resolves the owning account and confirms the actor is an active member.
    async fn authorize(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<AccountId, ModernAnalyticsError> {
        let registry = self
            .management
            .system_registry(system_id)
            .await
            .map_err(|_| ModernAnalyticsError::Unavailable)?
            .ok_or(ModernAnalyticsError::NotFound)?;
        self.management
            .active_membership(registry.account_id, actor)
            .await
            .map_err(|_| ModernAnalyticsError::Unavailable)?
            .ok_or(ModernAnalyticsError::Forbidden)?;
        Ok(registry.account_id)
    }
}

#[async_trait]
impl ModernAnalyticsUseCases for StorageAnalyticsApi {
    async fn time_series(
        &self,
        actor: UserId,
        system_id: SystemId,
        request: QueryPlanRequest,
    ) -> Result<pvlog_application::SeriesQueryResult, ModernAnalyticsError> {
        let account_id = self.authorize(actor, system_id).await?;
        let scope = SeriesScope::parse(request.scope.as_deref(), request.scope_id)
            .ok_or(ModernAnalyticsError::Invalid)?;
        let repository = Arc::new(AccountSeriesRepository::new(
            self.target.clone(),
            account_id,
            scope,
            request.start_epoch_millis,
            request.end_epoch_millis,
        ));
        SeriesQueryService::new(repository)
            .query(system_id, request)
            .await
            .map_err(|error| series_error(&error))
    }

    async fn statistics(
        &self,
        actor: UserId,
        system_id: SystemId,
        period: StatisticsPeriod,
    ) -> Result<EnergyStatistics, ModernAnalyticsError> {
        let account_id = self.authorize(actor, system_id).await?;
        let summaries = self.daily_summaries(account_id, system_id, period).await?;
        if summaries.is_empty() {
            return Err(ModernAnalyticsError::NotFound);
        }
        let capacity = self.capacity_watts(account_id, system_id).await?;
        calculate_statistics(period, &summaries, capacity)
            .map_err(|_| ModernAnalyticsError::Unavailable)
    }

    async fn data_quality(
        &self,
        actor: UserId,
        system_id: SystemId,
        start_epoch_millis: i64,
        end_epoch_millis: i64,
    ) -> Result<Vec<DataQualityIssue>, ModernAnalyticsError> {
        let account_id = self.authorize(actor, system_id).await?;
        let samples = self
            .quality_samples(account_id, system_id, start_epoch_millis, end_epoch_millis)
            .await?;
        let rejected = self
            .rejected_ingestions(account_id, system_id, start_epoch_millis, end_epoch_millis)
            .await?;
        let watermark = self.aggregate_watermark(account_id, system_id).await?;
        let now = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| ModernAnalyticsError::Unavailable)?;
        detect_data_quality(
            start_epoch_millis,
            end_epoch_millis,
            &samples,
            &rejected,
            watermark,
            now,
            DataQualityPolicy {
                expected_interval_millis: DEFAULT_EXPECTED_INTERVAL_MILLIS,
                interval_tolerance_millis: DEFAULT_EXPECTED_INTERVAL_MILLIS / 2,
                suspect_quality_mask: SUSPECT_QUALITY_MASK,
                aggregate_lag_threshold_millis: AGGREGATE_LAG_THRESHOLD_MILLIS,
            },
        )
        .map_err(|_| ModernAnalyticsError::Invalid)
    }

    async fn export(
        &self,
        request: AnalysisExportRequest,
    ) -> Result<AnalysisExportResult, ModernAnalyticsError> {
        let format = request.format;
        let result = self
            .time_series(request.actor, request.system_id, request.query)
            .await?;
        let (content_type, extension, bytes) = match format {
            AnalysisExportFormat::Csv => (
                "text/csv; charset=utf-8".to_owned(),
                "csv",
                csv_bytes(&result),
            ),
            AnalysisExportFormat::Json => (
                "application/json".to_owned(),
                "json",
                serde_json::to_vec(&result).map_err(|_| ModernAnalyticsError::Unavailable)?,
            ),
        };
        Ok(AnalysisExportResult::Ready {
            content_type,
            filename: format!("analysis-export.{extension}"),
            bytes,
            modeled_metadata: None,
        })
    }
}

/// Renders one series query result as a stable long-format CSV.
fn csv_bytes(result: &pvlog_application::SeriesQueryResult) -> Vec<u8> {
    let mut out = String::from("field,timestamp_epoch_millis,value,coverage_basis_points\n");
    for series in &result.series {
        let field = field_name(series.field);
        for point in &series.points {
            let _ = writeln!(
                out,
                "{field},{},{},{}",
                point.timestamp_epoch_millis, point.value, point.coverage_basis_points
            );
        }
    }
    out.into_bytes()
}

const fn field_name(field: SeriesField) -> &'static str {
    match field {
        SeriesField::GenerationPower => "generation_power",
        SeriesField::GenerationEnergy => "generation_energy",
        SeriesField::ForecastPower => "forecast_power",
        SeriesField::ForecastEnergy => "forecast_energy",
        SeriesField::ExpectedEnergy => "expected_energy",
        SeriesField::GenerationPerformance => "generation_performance",
        SeriesField::ForecastRealization => "forecast_realization",
        SeriesField::ConsumptionPower => "consumption_power",
        SeriesField::ConsumptionEnergy => "consumption_energy",
        SeriesField::GridPower => "grid_power",
        SeriesField::BatteryPower => "battery_power",
        SeriesField::BatteryStateOfCharge => "battery_state_of_charge",
        SeriesField::Temperature => "temperature",
        SeriesField::Extended => "extended",
        SeriesField::Provenance => "provenance",
    }
}

const fn series_error(error: &SeriesQueryError) -> ModernAnalyticsError {
    match error {
        SeriesQueryError::Plan(_) => ModernAnalyticsError::Invalid,
        // Every other variant means storage returned data the planner rejects.
        _ => ModernAnalyticsError::Unavailable,
    }
}

/// Local-calendar prefix that selects the summaries a period covers.
fn period_prefix(period: StatisticsPeriod, latest_date: &str) -> String {
    match period {
        StatisticsPeriod::Daily => latest_date.to_owned(),
        StatisticsPeriod::Monthly => latest_date.chars().take(7).collect(),
        StatisticsPeriod::Yearly => latest_date.chars().take(4).collect(),
        StatisticsPeriod::Lifetime => String::new(),
    }
}

fn summary_bucket(
    generation_wh: Option<i64>,
    consumption_wh: Option<i64>,
    coverage: i64,
) -> StatisticsBucket {
    let covered = MILLIS_PER_DAY
        .saturating_mul(u64::try_from(coverage.clamp(0, 10_000)).unwrap_or_default())
        / 10_000;
    StatisticsBucket {
        generation_wh: generation_wh.and_then(|value| u64::try_from(value).ok()),
        consumption_wh: consumption_wh.and_then(|value| u64::try_from(value).ok()),
        covered_millis: covered,
        expected_millis: MILLIS_PER_DAY,
        ..StatisticsBucket::default()
    }
}

impl StorageAnalyticsApi {
    #[cfg(feature = "sqlite")]
    async fn sqlite_account(
        &self,
        management_path: &std::path::Path,
        accounts_dir: &std::path::Path,
        account_id: AccountId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, ModernAnalyticsError> {
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.to_path_buf(),
            accounts_dir.to_path_buf(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| ModernAnalyticsError::Unavailable)?
        .route(account_id)
        .await
        .map_err(|_| ModernAnalyticsError::Unavailable)
    }

    async fn daily_summaries(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        period: StatisticsPeriod,
    ) -> Result<Vec<StatisticsBucket>, ModernAnalyticsError> {
        let rows = self.summary_rows(account_id, system_id).await?;
        let Some((latest_date, _, _, _)) = rows.last() else {
            return Ok(Vec::new());
        };
        let prefix = period_prefix(period, latest_date);
        Ok(rows
            .iter()
            .filter(|(date, _, _, _)| date.starts_with(&prefix))
            .map(|(_, generation, consumption, coverage)| {
                summary_bucket(*generation, *consumption, *coverage)
            })
            .collect())
    }

    async fn summary_rows(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<(String, Option<i64>, Option<i64>, i64)>, ModernAnalyticsError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    let rows = sqlx::query(
                        "SELECT local_date,generation_energy_wh,consumption_energy_wh,\
                         coverage_basis_points FROM system_daily_summaries d WHERE system_id=? \
                         AND generation=(SELECT MAX(latest.generation) FROM system_daily_summaries latest \
                         WHERE latest.system_id=d.system_id AND latest.local_date=d.local_date) \
                         ORDER BY local_date",
                    )
                    .bind(id_blob(system_id))
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    Ok(rows
                        .iter()
                        .map(|row| {
                            (
                                row.get::<String, _>("local_date"),
                                row.try_get("generation_energy_wh").ok(),
                                row.try_get("consumption_energy_wh").ok(),
                                row.try_get("coverage_basis_points").unwrap_or_default(),
                            )
                        })
                        .collect())
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Err(ModernAnalyticsError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let mut connection = PgConnection::connect(url)
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    let rows = sqlx::query(
                        "SELECT local_date::TEXT AS local_date,generation_energy_wh,\
                         consumption_energy_wh,coverage_basis_points::BIGINT AS coverage_basis_points \
                         FROM telemetry.daily_summaries d WHERE account_id=$1 AND system_id=$2 \
                         AND generation=(SELECT MAX(latest.generation) FROM telemetry.daily_summaries latest \
                         WHERE latest.account_id=d.account_id AND latest.system_id=d.system_id \
                         AND latest.local_date=d.local_date) ORDER BY local_date",
                    )
                    .bind(account_id.as_uuid())
                    .bind(system_id.as_uuid())
                    .fetch_all(&mut connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    Ok(rows
                        .iter()
                        .map(|row| {
                            (
                                row.get::<String, _>("local_date"),
                                row.try_get("generation_energy_wh").ok(),
                                row.try_get("consumption_energy_wh").ok(),
                                row.try_get("coverage_basis_points").unwrap_or_default(),
                            )
                        })
                        .collect())
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id, system_id);
                    Err(ModernAnalyticsError::Unavailable)
                }
            }
        }
    }

    async fn capacity_watts(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Option<u64>, ModernAnalyticsError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    let total: Option<i64> = sqlx::query_scalar(
                        "SELECT SUM(p.rated_power_watts) FROM pv_strings p \
                         JOIN inverters i ON i.id=p.inverter_id WHERE i.system_id=?",
                    )
                    .bind(id_blob(system_id))
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    Ok(total
                        .and_then(|value| u64::try_from(value).ok())
                        .filter(|value| *value > 0))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Ok(None)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let mut connection = PgConnection::connect(url)
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    let total: Option<i64> = sqlx::query_scalar(
                        "SELECT SUM(p.rated_power_watts)::BIGINT FROM account_data.pv_strings p \
                         JOIN account_data.inverters i ON i.account_id=p.account_id AND i.id=p.inverter_id \
                         WHERE p.account_id=$1 AND i.system_id=$2",
                    )
                    .bind(account_id.as_uuid())
                    .bind(system_id.as_uuid())
                    .fetch_one(&mut connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    Ok(total
                        .and_then(|value| u64::try_from(value).ok())
                        .filter(|value| *value > 0))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id, system_id);
                    Ok(None)
                }
            }
        }
    }

    async fn quality_samples(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        start_epoch_millis: i64,
        end_epoch_millis: i64,
    ) -> Result<Vec<QualitySample>, ModernAnalyticsError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    let rows = sqlx::query(
                        "SELECT measured_at,source_identity,quality_flags,generation_lifetime_wh,\
                         canonical_hash FROM telemetry_hot WHERE system_id=? \
                         AND measured_at>=? AND measured_at<? ORDER BY measured_at,observation_id",
                    )
                    .bind(id_blob(system_id))
                    .bind(start_epoch_millis)
                    .bind(end_epoch_millis)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    Ok(rows
                        .iter()
                        .map(|row| QualitySample {
                            timestamp_epoch_millis: row.get("measured_at"),
                            source_reference: row.try_get("source_identity").unwrap_or_default(),
                            value_fingerprint: fingerprint(
                                row.try_get::<Vec<u8>, _>("canonical_hash").ok().as_deref(),
                            ),
                            quality_flags: row
                                .try_get::<i64, _>("quality_flags")
                                .ok()
                                .and_then(|flags| u32::try_from(flags).ok())
                                .unwrap_or_default(),
                            cumulative_energy_wh: row
                                .try_get::<i64, _>("generation_lifetime_wh")
                                .ok()
                                .and_then(|value| u64::try_from(value).ok()),
                            counter_reset_sequence: None,
                        })
                        .collect())
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Ok(Vec::new())
                }
            }
            DatabaseTarget::Postgres { .. } => Ok(Vec::new()),
        }
    }

    async fn rejected_ingestions(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        start_epoch_millis: i64,
        end_epoch_millis: i64,
    ) -> Result<Vec<RejectedIngestion>, ModernAnalyticsError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    let rows = sqlx::query(
                        "SELECT range_start,quality_kind FROM data_quality_events \
                         WHERE system_id=? AND quality_kind='rejected_ingestion' \
                         AND range_start>=? AND range_start<? ORDER BY range_start",
                    )
                    .bind(id_blob(system_id))
                    .bind(start_epoch_millis)
                    .bind(end_epoch_millis)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    Ok(rows
                        .iter()
                        .map(|row| RejectedIngestion {
                            timestamp_epoch_millis: row.get("range_start"),
                            source_reference: String::new(),
                            reason_code: row.try_get("quality_kind").unwrap_or_default(),
                        })
                        .collect())
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Ok(Vec::new())
                }
            }
            DatabaseTarget::Postgres { .. } => Ok(Vec::new()),
        }
    }

    async fn aggregate_watermark(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Option<i64>, ModernAnalyticsError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ModernAnalyticsError::Unavailable)?;
                    sqlx::query_scalar(
                        "SELECT MAX(bucket_end) FROM telemetry_rollups WHERE system_id=?",
                    )
                    .bind(id_blob(system_id))
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(|_| ModernAnalyticsError::Unavailable)
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Ok(None)
                }
            }
            DatabaseTarget::Postgres { .. } => Ok(None),
        }
    }
}

fn fingerprint(hash: Option<&[u8]>) -> [u8; 32] {
    let mut value = [0_u8; 32];
    if let Some(bytes) = hash {
        let length = bytes.len().min(32);
        value[..length].copy_from_slice(&bytes[..length]);
    }
    value
}

#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}
