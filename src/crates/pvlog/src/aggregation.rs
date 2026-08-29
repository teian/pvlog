//! Incremental rebuild of telemetry rollups and calendar summaries.
//!
//! Ingestion writes hot observations only. This cycle turns them into the
//! aggregates that the reporting and analytics endpoints read.
//!
//! Work is discovered without a queue: for every system, observations received
//! after the newest rollup's `calculated_at` mark the measured range that needs
//! rebuilding. Buckets are resolved in the system's own timezone, so daily,
//! monthly, and yearly windows follow the local calendar across DST changes.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike as _, NaiveDate, TimeZone as _, Timelike as _, Utc};
use chrono_tz::Tz;
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(feature = "sqlite")]
use sqlx::Row;
use thiserror::Error;

const RESOLUTIONS: [Resolution; 5] = [
    Resolution::FifteenMinutes,
    Resolution::Hour,
    Resolution::Day,
    Resolution::Month,
    Resolution::Year,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resolution {
    FifteenMinutes,
    Hour,
    Day,
    Month,
    Year,
}

impl Resolution {
    const fn name(self) -> &'static str {
        match self {
            Self::FifteenMinutes => "fifteen_minute",
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

/// One accepted observation reduced to the columns aggregates need.
#[derive(Clone, Copy, Debug)]
struct Sample {
    measured_at: i64,
    generation_power_watts: Option<i64>,
    generation_energy_wh: Option<i64>,
    consumption_energy_wh: Option<i64>,
    quality_flags: i64,
}

/// Accumulated state for one local-calendar bucket.
#[derive(Clone, Debug, Default)]
struct Bucket {
    start: i64,
    end: i64,
    generation_energy_wh: i64,
    consumption_energy_wh: i64,
    minimum_watts: Option<i64>,
    maximum_watts: Option<i64>,
    first_watts: Option<i64>,
    last_watts: Option<i64>,
    points: i64,
    quality_flags: i64,
}

impl Bucket {
    fn accept(&mut self, sample: Sample) {
        self.points += 1;
        self.generation_energy_wh += sample.generation_energy_wh.unwrap_or_default();
        self.consumption_energy_wh += sample.consumption_energy_wh.unwrap_or_default();
        self.quality_flags |= sample.quality_flags;
        if let Some(watts) = sample.generation_power_watts {
            self.minimum_watts = Some(self.minimum_watts.map_or(watts, |value| value.min(watts)));
            self.maximum_watts = Some(self.maximum_watts.map_or(watts, |value| value.max(watts)));
            self.first_watts = self.first_watts.or(Some(watts));
            self.last_watts = Some(watts);
        }
    }

    /// Expected sample count for the bucket at the system's reporting cadence.
    fn expected(&self, interval_millis: i64) -> i64 {
        ((self.end - self.start) / interval_millis.max(1)).max(self.points)
    }

    fn coverage(&self, interval_millis: i64) -> i32 {
        let expected = self.expected(interval_millis);
        if expected <= 0 {
            return 0;
        }
        i32::try_from((self.points * 10_000) / expected)
            .unwrap_or(10_000)
            .clamp(0, 10_000)
    }
}

/// Rebuilds every account's stale aggregates once.
///
/// # Errors
///
/// Returns an error when management routing or account storage is unavailable.
pub async fn run_cycle(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
) -> Result<AggregationReport, AggregationError> {
    let mut report = AggregationReport::default();
    let DatabaseTarget::Sqlite {
        management_path,
        accounts_dir,
    } = target
    else {
        // The PostgreSQL aggregation path is not implemented yet.
        return Ok(report);
    };
    #[cfg(not(feature = "sqlite"))]
    {
        let _ = (management_path, accounts_dir, management);
        return Ok(report);
    }
    #[cfg(feature = "sqlite")]
    {
        for account_id in account_ids(management_path).await? {
            let account = pvlog_storage::SqliteAccountPoolRouter::new(
                management_path.clone(),
                accounts_dir.clone(),
                pvlog_storage::SqliteAccountPoolConfig::default(),
            )
            .map_err(|_| AggregationError::Unavailable)?
            .route(account_id)
            .await
            .map_err(|_| AggregationError::Unavailable)?;
            for system_id in management
                .systems_for_account(account_id)
                .await
                .map_err(|_| AggregationError::Unavailable)?
            {
                report.merge(rebuild_system(&account, system_id).await?);
            }
        }
        Ok(report)
    }
}

/// Counts produced by one aggregation cycle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AggregationReport {
    pub systems_rebuilt: u64,
    pub rollups_written: u64,
    pub daily_summaries_written: u64,
}

impl AggregationReport {
    fn merge(&mut self, other: Self) {
        self.systems_rebuilt += other.systems_rebuilt;
        self.rollups_written += other.rollups_written;
        self.daily_summaries_written += other.daily_summaries_written;
    }
}

/// Lists every provisioned account database in the management registry.
#[cfg(feature = "sqlite")]
async fn account_ids(
    management_path: &std::path::Path,
) -> Result<Vec<AccountId>, AggregationError> {
    use sqlx::ConnectOptions as _;
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(management_path)
        .read_only(true)
        .connect()
        .await
        .map_err(|_| AggregationError::Unavailable)?;
    let rows = sqlx::query(
        "SELECT account_id FROM account_database_registry \
         WHERE lifecycle_state='active' ORDER BY account_id",
    )
    .fetch_all(&mut connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    rows.iter()
        .map(|row| {
            let bytes: Vec<u8> = row.get("account_id");
            <[u8; 16]>::try_from(bytes.as_slice())
                .ok()
                .map(uuid::Uuid::from_bytes)
                .and_then(|value| AccountId::from_uuid(value).ok())
                .ok_or(AggregationError::Unavailable)
        })
        .collect()
}

#[cfg(feature = "sqlite")]
async fn rebuild_system(
    account: &pvlog_storage::RoutedSqliteAccount,
    system_id: SystemId,
) -> Result<AggregationReport, AggregationError> {
    let mut connection = account
        .acquire()
        .await
        .map_err(|_| AggregationError::Unavailable)?;
    let blob = system_id.as_uuid().as_bytes().to_vec();

    let Some(system) =
        sqlx::query("SELECT timezone,status_interval_seconds FROM systems WHERE id=?")
            .bind(blob.clone())
            .fetch_optional(&mut *connection)
            .await
            .map_err(|_| AggregationError::Unavailable)?
    else {
        return Ok(AggregationReport::default());
    };
    let timezone: String = system.get("timezone");
    let zone: Tz = timezone.parse().map_err(|_| AggregationError::Timezone)?;
    let interval_millis: i64 = system
        .get::<i64, _>("status_interval_seconds")
        .saturating_mul(1_000);

    let watermark: Option<i64> =
        sqlx::query_scalar(
            "SELECT MAX(calculated_at) FROM telemetry_rollups WHERE system_id=? AND scope_kind='system'",
        )
            .bind(blob.clone())
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| AggregationError::Unavailable)?;
    let pending: Option<(Option<i64>, Option<i64>)> = sqlx::query(
        "SELECT MIN(measured_at) AS first, MAX(measured_at) AS last FROM telemetry_hot \
         WHERE system_id=? AND received_at>?",
    )
    .bind(blob.clone())
    .bind(watermark.unwrap_or(i64::MIN))
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?
    .map(|row| (row.try_get("first").ok(), row.try_get("last").ok()));
    let (Some(Some(first)), Some(Some(last))) = (
        pending.map(|(first, _)| first),
        pending.map(|(_, last)| last),
    ) else {
        return Ok(AggregationReport::default());
    };

    // Yearly buckets are the coarsest, so widening to whole local years covers
    // every finer bucket the touched observations belong to.
    let start = year_start(zone, first)?;
    let end = year_start(zone, last)?
        .checked_add(1)
        .ok_or(AggregationError::Timezone)?;
    let end = next_year_start(zone, end)?;

    let rows = sqlx::query(
        "SELECT measured_at,generation_power_watts,generation_energy_wh,consumption_energy_wh,\
         quality_flags FROM telemetry_hot WHERE system_id=? AND measured_at>=? AND measured_at<? \
         ORDER BY measured_at",
    )
    .bind(blob.clone())
    .bind(start)
    .bind(end)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    if rows.is_empty() {
        return Ok(AggregationReport::default());
    }
    let samples: Vec<Sample> = rows
        .iter()
        .map(|row| Sample {
            measured_at: row.get("measured_at"),
            generation_power_watts: row.try_get("generation_power_watts").ok(),
            generation_energy_wh: row.try_get("generation_energy_wh").ok(),
            consumption_energy_wh: row.try_get("consumption_energy_wh").ok(),
            quality_flags: row.try_get("quality_flags").unwrap_or_default(),
        })
        .collect();

    let calculated_at = Utc::now().timestamp_millis();
    let mut report = AggregationReport {
        systems_rebuilt: 1,
        ..AggregationReport::default()
    };

    let written = write_aggregates(
        &mut connection,
        &AggregateContext {
            blob: blob.clone(),
            timezone: timezone.clone(),
            zone,
            interval_millis,
            calculated_at,
        },
        &samples,
    )
    .await?;
    report.merge(written);

    rebuild_lifetime(&mut connection, &blob, calculated_at).await?;
    Ok(report)
}

/// Everything the aggregate writers need beyond the samples themselves.
#[cfg(feature = "sqlite")]
struct AggregateContext {
    blob: Vec<u8>,
    timezone: String,
    zone: Tz,
    interval_millis: i64,
    calculated_at: i64,
}

/// Writes every resolution's rollups plus the daily summaries they imply.
#[cfg(feature = "sqlite")]
async fn write_aggregates(
    connection: &mut sqlx::SqliteConnection,
    context: &AggregateContext,
    samples: &[Sample],
) -> Result<AggregationReport, AggregationError> {
    let mut report = AggregationReport::default();
    let mut daily: BTreeMap<String, Bucket> = BTreeMap::new();
    for resolution in RESOLUTIONS {
        let buckets = bucket_samples(context.zone, resolution, samples)?;
        for (label, bucket) in &buckets {
            write_rollup(&mut *connection, context, resolution, bucket).await?;
            report.rollups_written += 1;
            if resolution == Resolution::Day {
                daily.insert(label.clone(), bucket.clone());
            }
        }
    }
    for (local_date, bucket) in &daily {
        write_daily_summary(&mut *connection, context, local_date, bucket).await?;
        report.daily_summaries_written += 1;
    }
    Ok(report)
}

#[cfg(feature = "sqlite")]
async fn write_rollup(
    connection: &mut sqlx::SqliteConnection,
    context: &AggregateContext,
    resolution: Resolution,
    bucket: &Bucket,
) -> Result<(), AggregationError> {
    sqlx::query(
        "INSERT INTO telemetry_rollups (system_id,scope_kind,scope_id,resolution,\
         bucket_start,bucket_end,timezone,generation,source_generation,point_count,expected_count,\
         generation_energy_sum_wh,consumption_energy_sum_wh,generation_power_min_watts,\
         generation_power_max_watts,generation_power_first_watts,generation_power_last_watts,\
         quality_flags,coverage_basis_points,calculated_at) \
         VALUES (?,?,?,?,?,?,?,1,1,?,?,?,?,?,?,?,?,?,?,?) \
         ON CONFLICT(system_id,scope_kind,scope_id,resolution,bucket_start,generation) DO UPDATE SET \
         bucket_end=excluded.bucket_end,point_count=excluded.point_count,\
         expected_count=excluded.expected_count,\
         generation_energy_sum_wh=excluded.generation_energy_sum_wh,\
         consumption_energy_sum_wh=excluded.consumption_energy_sum_wh,\
         generation_power_min_watts=excluded.generation_power_min_watts,\
         generation_power_max_watts=excluded.generation_power_max_watts,\
         generation_power_first_watts=excluded.generation_power_first_watts,\
         generation_power_last_watts=excluded.generation_power_last_watts,\
         quality_flags=excluded.quality_flags,\
         coverage_basis_points=excluded.coverage_basis_points,\
         calculated_at=excluded.calculated_at",
    )
    .bind(context.blob.clone())
    // Aggregates currently describe the system as a whole; per-string buckets
    // follow once channel-mapped readings exist.
    .bind("system")
    .bind(context.blob.clone())
    .bind(resolution.name())
    .bind(bucket.start)
    .bind(bucket.end)
    .bind(&context.timezone)
    .bind(bucket.points)
    .bind(bucket.expected(context.interval_millis))
    .bind(bucket.generation_energy_wh)
    .bind(bucket.consumption_energy_wh)
    .bind(bucket.minimum_watts)
    .bind(bucket.maximum_watts)
    .bind(bucket.first_watts)
    .bind(bucket.last_watts)
    .bind(bucket.quality_flags)
    .bind(bucket.coverage(context.interval_millis))
    .bind(context.calculated_at)
    .execute(connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
async fn write_daily_summary(
    connection: &mut sqlx::SqliteConnection,
    context: &AggregateContext,
    local_date: &str,
    bucket: &Bucket,
) -> Result<(), AggregationError> {
    sqlx::query(
        "INSERT INTO system_daily_summaries (system_id,local_date,timezone,generation,\
         generation_energy_wh,consumption_energy_wh,peak_generation_power_watts,\
         coverage_basis_points,quality_flags,calculated_at) VALUES (?,?,?,1,?,?,?,?,?,?) \
         ON CONFLICT(system_id,local_date,generation) DO UPDATE SET \
         generation_energy_wh=excluded.generation_energy_wh,\
         consumption_energy_wh=excluded.consumption_energy_wh,\
         peak_generation_power_watts=excluded.peak_generation_power_watts,\
         coverage_basis_points=excluded.coverage_basis_points,\
         quality_flags=excluded.quality_flags,calculated_at=excluded.calculated_at",
    )
    .bind(context.blob.clone())
    .bind(local_date)
    .bind(&context.timezone)
    .bind(bucket.generation_energy_wh)
    .bind(bucket.consumption_energy_wh)
    .bind(bucket.maximum_watts)
    .bind(bucket.coverage(context.interval_millis))
    .bind(bucket.quality_flags)
    .bind(context.calculated_at)
    .execute(connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    Ok(())
}

/// Recomputes the lifetime summary from every stored daily summary.
#[cfg(feature = "sqlite")]
async fn rebuild_lifetime(
    connection: &mut sqlx::SqliteConnection,
    blob: &[u8],
    calculated_at: i64,
) -> Result<(), AggregationError> {
    let Some(totals) = sqlx::query(
        "SELECT SUM(generation_energy_wh) AS generation,SUM(consumption_energy_wh) AS consumption,\
         AVG(coverage_basis_points) AS coverage FROM system_daily_summaries WHERE system_id=?",
    )
    .bind(blob.to_vec())
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?
    else {
        return Ok(());
    };
    let observed = sqlx::query(
        "SELECT MIN(measured_at) AS first,MAX(measured_at) AS last,\
         MAX(generation_power_watts) AS peak FROM telemetry_hot WHERE system_id=?",
    )
    .bind(blob.to_vec())
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    let coverage = totals
        .try_get::<f64, _>("coverage")
        .ok()
        .map_or(0, |value| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let rounded = value.round() as i64;
            i32::try_from(rounded).unwrap_or(10_000).clamp(0, 10_000)
        });
    sqlx::query(
        "INSERT INTO system_lifetime_summaries (system_id,generation,first_observation_at,\
         last_observation_at,generation_energy_wh,consumption_energy_wh,\
         peak_generation_power_watts,coverage_basis_points,calculated_at) \
         VALUES (?,1,?,?,?,?,?,?,?) ON CONFLICT(system_id) DO UPDATE SET \
         first_observation_at=excluded.first_observation_at,\
         last_observation_at=excluded.last_observation_at,\
         generation_energy_wh=excluded.generation_energy_wh,\
         consumption_energy_wh=excluded.consumption_energy_wh,\
         peak_generation_power_watts=excluded.peak_generation_power_watts,\
         coverage_basis_points=excluded.coverage_basis_points,\
         calculated_at=excluded.calculated_at",
    )
    .bind(blob.to_vec())
    .bind(observed.try_get::<i64, _>("first").ok())
    .bind(observed.try_get::<i64, _>("last").ok())
    .bind(totals.try_get::<i64, _>("generation").ok())
    .bind(totals.try_get::<i64, _>("consumption").ok())
    .bind(observed.try_get::<i64, _>("peak").ok())
    .bind(coverage)
    .bind(calculated_at)
    .execute(connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    Ok(())
}

/// Groups samples into local-calendar buckets of one resolution.
fn bucket_samples(
    zone: Tz,
    resolution: Resolution,
    samples: &[Sample],
) -> Result<BTreeMap<String, Bucket>, AggregationError> {
    let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
    for sample in samples {
        let local = local_time(zone, sample.measured_at)?;
        let (label, start, end) = window(zone, resolution, local)?;
        let bucket = buckets.entry(label).or_insert(Bucket {
            start,
            end,
            ..Bucket::default()
        });
        bucket.accept(*sample);
    }
    Ok(buckets)
}

fn local_time(zone: Tz, epoch_millis: i64) -> Result<DateTime<Tz>, AggregationError> {
    DateTime::from_timestamp_millis(epoch_millis)
        .map(|value| value.with_timezone(&zone))
        .ok_or(AggregationError::Timezone)
}

/// Resolves the local bucket a timestamp belongs to as a half-open UTC interval.
fn window(
    zone: Tz,
    resolution: Resolution,
    local: DateTime<Tz>,
) -> Result<(String, i64, i64), AggregationError> {
    let (label, start_local, end_local) = match resolution {
        Resolution::FifteenMinutes => {
            let minute = (local.minute() / 15) * 15;
            let start = local
                .with_minute(minute)
                .and_then(|value| value.with_second(0))
                .and_then(|value| value.with_nanosecond(0))
                .ok_or(AggregationError::Timezone)?;
            (
                start.format("%Y-%m-%dT%H:%M").to_string(),
                start,
                start + chrono::Duration::minutes(15),
            )
        }
        Resolution::Hour => {
            let start = local
                .with_minute(0)
                .and_then(|value| value.with_second(0))
                .and_then(|value| value.with_nanosecond(0))
                .ok_or(AggregationError::Timezone)?;
            (
                start.format("%Y-%m-%dT%H").to_string(),
                start,
                start + chrono::Duration::hours(1),
            )
        }
        Resolution::Day => {
            let date = local.date_naive();
            let start = start_of_day(zone, date)?;
            let end = start_of_day(zone, date.succ_opt().ok_or(AggregationError::Timezone)?)?;
            (date.format("%Y-%m-%d").to_string(), start, end)
        }
        Resolution::Month => {
            let date = NaiveDate::from_ymd_opt(local.year(), local.month(), 1)
                .ok_or(AggregationError::Timezone)?;
            let next = if local.month() == 12 {
                NaiveDate::from_ymd_opt(local.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(local.year(), local.month() + 1, 1)
            }
            .ok_or(AggregationError::Timezone)?;
            (
                date.format("%Y-%m").to_string(),
                start_of_day(zone, date)?,
                start_of_day(zone, next)?,
            )
        }
        Resolution::Year => {
            let date =
                NaiveDate::from_ymd_opt(local.year(), 1, 1).ok_or(AggregationError::Timezone)?;
            let next = NaiveDate::from_ymd_opt(local.year() + 1, 1, 1)
                .ok_or(AggregationError::Timezone)?;
            (
                date.format("%Y").to_string(),
                start_of_day(zone, date)?,
                start_of_day(zone, next)?,
            )
        }
    };
    Ok((
        label,
        start_local.timestamp_millis(),
        end_local.timestamp_millis(),
    ))
}

/// First instant of a local date, choosing the earlier option at a DST overlap.
fn start_of_day(zone: Tz, date: NaiveDate) -> Result<DateTime<Tz>, AggregationError> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or(AggregationError::Timezone)?;
    zone.from_local_datetime(&midnight)
        .earliest()
        // A local midnight skipped by a DST jump starts one hour later.
        .or_else(|| {
            date.and_hms_opt(1, 0, 0)
                .and_then(|value| zone.from_local_datetime(&value).earliest())
        })
        .ok_or(AggregationError::Timezone)
}

fn year_start(zone: Tz, epoch_millis: i64) -> Result<i64, AggregationError> {
    let local = local_time(zone, epoch_millis)?;
    let date = NaiveDate::from_ymd_opt(local.year(), 1, 1).ok_or(AggregationError::Timezone)?;
    Ok(start_of_day(zone, date)?.timestamp_millis())
}

fn next_year_start(zone: Tz, epoch_millis: i64) -> Result<i64, AggregationError> {
    let local = local_time(zone, epoch_millis)?;
    let date = NaiveDate::from_ymd_opt(local.year() + 1, 1, 1).ok_or(AggregationError::Timezone)?;
    Ok(start_of_day(zone, date)?.timestamp_millis())
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AggregationError {
    #[error("aggregation storage is unavailable")]
    Unavailable,
    #[error("system timezone could not be resolved")]
    Timezone,
}
