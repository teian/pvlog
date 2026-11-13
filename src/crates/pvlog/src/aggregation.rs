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
use sqlx::{Connection as _, Row};
use thiserror::Error;

const RESOLUTIONS: [Resolution; 5] = [
    Resolution::FifteenMinutes,
    Resolution::Hour,
    Resolution::Day,
    Resolution::Month,
    Resolution::Year,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Resolution {
    FifteenMinutes,
    Hour,
    Day,
    Month,
    Year,
}

impl Resolution {
    /// Returns the stored name of the resolution.
    pub(crate) const fn name(self) -> &'static str {
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
    grid_import_energy_wh: Option<i64>,
    grid_export_energy_wh: Option<i64>,
    temperature_millidegrees_c: Option<i64>,
    voltage_millivolts: Option<i64>,
    /// Energy the battery held when the sample was taken.
    battery_energy_wh: Option<i64>,
    quality_flags: i64,
}

/// Accumulated state for one local-calendar bucket.
#[derive(Clone, Debug, Default)]
struct Bucket {
    start: i64,
    end: i64,
    generation_energy_wh: i64,
    consumption_energy_wh: i64,
    /// Grid sums stay absent until a sample reports them: a system without a
    /// grid meter imported an unknown amount, not zero.
    grid_import_energy_wh: Option<i64>,
    grid_export_energy_wh: Option<i64>,
    minimum_temperature: Option<i64>,
    maximum_temperature: Option<i64>,
    voltage_sum: i64,
    voltage_points: i64,
    /// Stored battery energy at the first and the last sample that reported it.
    first_battery_energy_wh: Option<i64>,
    last_battery_energy_wh: Option<i64>,
    minimum_watts: Option<i64>,
    maximum_watts: Option<i64>,
    first_watts: Option<i64>,
    last_watts: Option<i64>,
    points: i64,
    quality_flags: i64,
}

impl Bucket {
    /// Folds one sample into the bucket's energy sums, quality flags and power extremes.
    fn accept(&mut self, sample: Sample) {
        self.points += 1;
        self.generation_energy_wh += sample.generation_energy_wh.unwrap_or_default();
        self.consumption_energy_wh += sample.consumption_energy_wh.unwrap_or_default();
        self.quality_flags |= sample.quality_flags;
        add(
            &mut self.grid_import_energy_wh,
            sample.grid_import_energy_wh,
        );
        add(
            &mut self.grid_export_energy_wh,
            sample.grid_export_energy_wh,
        );
        if let Some(temperature) = sample.temperature_millidegrees_c {
            self.minimum_temperature = Some(
                self.minimum_temperature
                    .map_or(temperature, |value| value.min(temperature)),
            );
            self.maximum_temperature = Some(
                self.maximum_temperature
                    .map_or(temperature, |value| value.max(temperature)),
            );
        }
        if let Some(voltage) = sample.voltage_millivolts {
            self.voltage_sum += voltage;
            self.voltage_points += 1;
        }
        if let Some(stored) = sample.battery_energy_wh {
            self.first_battery_energy_wh = self.first_battery_energy_wh.or(Some(stored));
            self.last_battery_energy_wh = Some(stored);
        }
        if let Some(watts) = sample.generation_power_watts {
            self.minimum_watts = Some(self.minimum_watts.map_or(watts, |value| value.min(watts)));
            self.maximum_watts = Some(self.maximum_watts.map_or(watts, |value| value.max(watts)));
            self.first_watts = self.first_watts.or(Some(watts));
            self.last_watts = Some(watts);
        }
    }

    /// Folds a bucket of the same window that follows this one in time into
    /// it, as if its samples had been accepted here.
    fn merge(&mut self, later: &Self) {
        self.points += later.points;
        self.generation_energy_wh += later.generation_energy_wh;
        self.consumption_energy_wh += later.consumption_energy_wh;
        self.quality_flags |= later.quality_flags;
        add(&mut self.grid_import_energy_wh, later.grid_import_energy_wh);
        add(&mut self.grid_export_energy_wh, later.grid_export_energy_wh);
        self.minimum_temperature = extreme(
            self.minimum_temperature,
            later.minimum_temperature,
            i64::min,
        );
        self.maximum_temperature = extreme(
            self.maximum_temperature,
            later.maximum_temperature,
            i64::max,
        );
        self.voltage_sum += later.voltage_sum;
        self.voltage_points += later.voltage_points;
        self.first_battery_energy_wh = self
            .first_battery_energy_wh
            .or(later.first_battery_energy_wh);
        self.last_battery_energy_wh = later.last_battery_energy_wh.or(self.last_battery_energy_wh);
        self.minimum_watts = extreme(self.minimum_watts, later.minimum_watts, i64::min);
        self.maximum_watts = extreme(self.maximum_watts, later.maximum_watts, i64::max);
        self.first_watts = self.first_watts.or(later.first_watts);
        self.last_watts = later.last_watts.or(self.last_watts);
    }

    /// Change of the energy the battery holds across the bucket: positive when
    /// it was charged. Absent when no sample reported the stored energy.
    fn battery_energy_delta(&self) -> Option<i64> {
        Some(self.last_battery_energy_wh? - self.first_battery_energy_wh?)
    }

    /// Mean of the reported voltages, absent when none was reported.
    fn mean_voltage(&self) -> Option<i64> {
        (self.voltage_points > 0).then(|| self.voltage_sum / self.voltage_points)
    }

    /// Expected sample count for the bucket at the system's reporting cadence.
    fn expected(&self, interval_millis: i64) -> i64 {
        ((self.end - self.start) / interval_millis.max(1)).max(self.points)
    }

    /// Returns the share of expected samples actually received in basis points, clamped to 0..=10000.
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

/// The smaller or larger of two values that may each be absent.
fn extreme(left: Option<i64>, right: Option<i64>, pick: fn(i64, i64) -> i64) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(pick(left, right)),
        (value, None) | (None, value) => value,
    }
}

/// Adds a reported value to a sum that is absent until something is reported.
fn add(sum: &mut Option<i64>, value: Option<i64>) {
    if let Some(value) = value {
        *sum = Some(sum.unwrap_or_default() + value);
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
    let mut report = plant_cycle(target, management).await?;
    report.channel_rollups_written =
        crate::channel_aggregation::run_cycle(target, management).await?;
    Ok(report)
}

/// Rebuilds the plant-wide rollups and summaries of every account once.
async fn plant_cycle(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
) -> Result<AggregationReport, AggregationError> {
    let mut report = AggregationReport::default();
    let DatabaseTarget::Sqlite {
        management_path,
        accounts_dir,
    } = target
    else {
        // Plant-wide aggregation from readings is not implemented for
        // PostgreSQL yet; channel aggregates are.
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
            let account = pvlog_storage::SqliteAccountPoolRouter::shared(
                management_path.clone(),
                accounts_dir.clone(),
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
    /// Aggregates of bound measurement channels written.
    pub channel_rollups_written: u64,
}

impl AggregationReport {
    /// Adds another report's counters to this one.
    fn merge(&mut self, other: Self) {
        self.systems_rebuilt += other.systems_rebuilt;
        self.rollups_written += other.rollups_written;
        self.daily_summaries_written += other.daily_summaries_written;
    }
}

/// Lists every provisioned account database in the management registry.
///
/// # Errors
///
/// Returns an error when the management database cannot be read.
#[cfg(feature = "sqlite")]
pub async fn active_accounts(
    management_path: &std::path::Path,
) -> Result<Vec<AccountId>, AggregationError> {
    account_ids(management_path).await
}

/// Reads the identifiers of all accounts whose database is registered as active, ordered by identifier.
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

/// Rebuilds the rollups and daily summaries of one system for one local year of
/// its pending range and records where the next cycle continues; see
/// [`rebuild_run`] for what is pending.
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

    let Some(run) = rebuild_run(&mut connection, &blob, zone).await? else {
        return Ok(AggregationReport::default());
    };
    // One local year at a time, each in its own transaction: a rebuild of years
    // of history neither holds everything in memory nor writes it in one go.
    // Within the year the readings are read one local day at a time; the
    // months and the year grow from the days and are written at the end.
    let chunk_end = next_year_start(zone, run.next_start)?.min(run.range_end);
    let context = |scope_kind: &'static str, scope_id: Vec<u8>| AggregateContext {
        blob: blob.clone(),
        scope_kind,
        scope_id,
        timezone: timezone.clone(),
        zone,
        interval_millis,
        calculated_at: run.calculated_at,
    };
    let mut report = AggregationReport::default();
    let mut transaction = connection
        .begin()
        .await
        .map_err(|_| AggregationError::Unavailable)?;
    let mut coarse: BTreeMap<(&'static str, Vec<u8>), CoarseBuckets> = BTreeMap::new();
    let mut day_from = run.next_start;
    while day_from < chunk_end {
        let (_, _, day_end) = window(zone, Resolution::Day, local_time(zone, day_from)?)?;
        let day_to = day_end.min(chunk_end);
        let touched = day_to > run.touched.0 && day_from < run.touched.1;
        if let Some(samples) = plant_samples(&mut transaction, &blob, day_from, day_to).await? {
            report.systems_rebuilt = 1;
            // The system bucket keeps using the plant-wide columns; per-string
            // buckets come from the channels bound to each string, and an
            // inverter is the sum of the strings beneath it.
            let scopes =
                metered_scopes(&mut transaction, &blob, day_from, day_to, interval_millis).await?;
            for (scope_kind, scope_id, scope_samples) in
                std::iter::once(("system", blob.clone(), samples)).chain(scopes)
            {
                let written = write_aggregates(
                    &mut transaction,
                    &context(scope_kind, scope_id.clone()),
                    &scope_samples,
                    coarse.entry((scope_kind, scope_id)).or_default(),
                    touched,
                )
                .await?;
                report.merge(written);
            }
        }
        day_from = day_to;
    }
    for ((scope_kind, scope_id), buckets) in coarse {
        let rollups: Vec<(Resolution, Bucket)> = buckets.0.into_values().collect();
        write_rollups(&mut transaction, &context(scope_kind, scope_id), &rollups).await?;
        report.rollups_written += u64::try_from(rollups.len()).unwrap_or(u64::MAX);
    }
    if chunk_end < run.range_end {
        sqlx::query(
            "INSERT INTO aggregation_progress (system_id,next_start,range_end,calculated_at) \
             VALUES (?,?,?,?) ON CONFLICT(system_id) DO UPDATE SET next_start=excluded.next_start",
        )
        .bind(blob.clone())
        .bind(chunk_end)
        .bind(run.range_end)
        .bind(run.calculated_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AggregationError::Unavailable)?;
    } else {
        sqlx::query("DELETE FROM aggregation_progress WHERE system_id=?")
            .bind(blob.clone())
            .execute(&mut *transaction)
            .await
            .map_err(|_| AggregationError::Unavailable)?;
        rebuild_lifetime(&mut transaction, &blob, run.calculated_at).await?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| AggregationError::Unavailable)?;
    Ok(report)
}

/// Where a rebuild of a system's aggregates stands.
#[cfg(feature = "sqlite")]
struct RebuildRun {
    /// Start of the next local year to rebuild.
    next_start: i64,
    /// End of the last local year to rebuild.
    range_end: i64,
    /// Time stamped on everything the rebuild writes; readings received later
    /// are pending for the next rebuild.
    calculated_at: i64,
    /// Local days that received readings, as `[start, end)`. Outside them the
    /// quarter-hour, hour and day aggregates are already current; their
    /// readings are still read for the month and year they belong to.
    touched: (i64, i64),
}

/// Continues the running rebuild of a system, or starts one for the local
/// years touched by readings received since the last rebuild, or by every
/// reading after a channel binding changed. `None` when nothing is pending.
#[cfg(feature = "sqlite")]
async fn rebuild_run(
    connection: &mut sqlx::SqliteConnection,
    blob: &[u8],
    zone: Tz,
) -> Result<Option<RebuildRun>, AggregationError> {
    if let Some(row) = sqlx::query(
        "SELECT next_start,range_end,calculated_at FROM aggregation_progress WHERE system_id=?",
    )
    .bind(blob.to_vec())
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?
    {
        let (next_start, range_end) = (row.get("next_start"), row.get("range_end"));
        return Ok(Some(RebuildRun {
            next_start,
            range_end,
            calculated_at: row.get("calculated_at"),
            touched: (next_start, range_end),
        }));
    }
    let watermark: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(calculated_at) FROM telemetry_rollups WHERE system_id=? AND scope_kind='system'",
    )
    .bind(blob.to_vec())
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    // A binding changed after the last rebuild re-attributes history, so every
    // retained observation is pending again, not only the newly received ones.
    let rebound: Option<i64> =
        sqlx::query_scalar("SELECT MAX(updated_at) FROM channel_definitions WHERE system_id=?")
            .bind(blob.to_vec())
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| AggregationError::Unavailable)?;
    let watermark = match (watermark, rebound) {
        (Some(built), Some(changed)) if changed > built => None,
        (built, _) => built,
    };
    let pending: Option<(Option<i64>, Option<i64>)> = sqlx::query(
        "SELECT MIN(measured_at) AS first, MAX(measured_at) AS last FROM telemetry_hot \
         WHERE system_id=? AND received_at>?",
    )
    .bind(blob.to_vec())
    .bind(watermark.unwrap_or(i64::MIN))
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?
    .map(|row| (row.try_get("first").ok(), row.try_get("last").ok()));
    let (Some(Some(first)), Some(Some(last))) = (
        pending.map(|(first, _)| first),
        pending.map(|(_, last)| last),
    ) else {
        return Ok(None);
    };
    // Yearly buckets are the coarsest, so whole local years cover every finer
    // bucket the touched observations belong to.
    let (_, first_day, _) = window(zone, Resolution::Day, local_time(zone, first)?)?;
    let (_, _, last_day_end) = window(zone, Resolution::Day, local_time(zone, last)?)?;
    Ok(Some(RebuildRun {
        next_start: year_start(zone, first)?,
        range_end: next_year_start(zone, last)?,
        calculated_at: Utc::now().timestamp_millis(),
        touched: (first_day, last_day_end),
    }))
}

/// Reads the plant-wide observation columns for one range.
///
/// Returns `None` when the range holds no observation at all, so the caller can
/// stop before touching any aggregate.
#[cfg(feature = "sqlite")]
async fn plant_samples(
    connection: &mut sqlx::SqliteConnection,
    system: &[u8],
    start: i64,
    end: i64,
) -> Result<Option<Vec<Sample>>, AggregationError> {
    let rows = sqlx::query(
        "SELECT measured_at,generation_power_watts,generation_energy_wh,consumption_energy_wh,\
         grid_import_energy_wh,grid_export_energy_wh,temperature_millidegrees_c,voltage_millivolts,battery_energy_wh,quality_flags FROM telemetry_hot WHERE system_id=? AND measured_at>=? AND measured_at<? \
         ORDER BY measured_at",
    )
    .bind(system.to_vec())
    .bind(start)
    .bind(end)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        rows.iter()
            .map(|row| Sample {
                measured_at: row.get("measured_at"),
                generation_power_watts: row.try_get("generation_power_watts").ok(),
                generation_energy_wh: row.try_get("generation_energy_wh").ok(),
                consumption_energy_wh: row.try_get("consumption_energy_wh").ok(),
                grid_import_energy_wh: row.try_get("grid_import_energy_wh").ok().flatten(),
                grid_export_energy_wh: row.try_get("grid_export_energy_wh").ok().flatten(),
                temperature_millidegrees_c: row
                    .try_get("temperature_millidegrees_c")
                    .ok()
                    .flatten(),
                voltage_millivolts: row.try_get("voltage_millivolts").ok().flatten(),
                battery_energy_wh: row.try_get("battery_energy_wh").ok().flatten(),
                quality_flags: row.try_get("quality_flags").unwrap_or_default(),
            })
            .collect(),
    ))
}

/// Collects one sample series per metered string and per inverter above them.
///
/// Only a string's power channels count: channels in watts that are averaged
/// over a period. The energy a bucket needs is that power held for one
/// reporting interval, which is the same integration the plant-wide columns
/// already represent.
#[cfg(feature = "sqlite")]
async fn metered_scopes(
    connection: &mut sqlx::SqliteConnection,
    system: &[u8],
    start: i64,
    end: i64,
    interval_millis: i64,
) -> Result<Vec<(&'static str, Vec<u8>, Vec<Sample>)>, AggregationError> {
    let rows = sqlx::query(
        "SELECT c.target_id AS string_id,p.inverter_id AS inverter_id,h.measured_at AS measured_at,         SUM(v.integer_value) AS power,MAX(h.quality_flags) AS quality_flags          FROM telemetry_hot_extended_values v          JOIN telemetry_hot h ON h.observation_id=v.observation_id          JOIN channel_definitions c ON c.id=v.channel_id          JOIN pv_strings p ON p.id=c.target_id          WHERE h.system_id=? AND h.measured_at>=? AND h.measured_at<?          AND c.lifecycle='active' AND c.target_kind='string'          AND c.unit='W' AND c.aggregation='mean'          GROUP BY c.target_id,h.measured_at ORDER BY c.target_id,h.measured_at",
    )
    .bind(system.to_vec())
    .bind(start)
    .bind(end)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| AggregationError::Unavailable)?;

    let mut strings: BTreeMap<Vec<u8>, Vec<Sample>> = BTreeMap::new();
    let mut inverters: BTreeMap<Vec<u8>, BTreeMap<i64, Sample>> = BTreeMap::new();
    for row in &rows {
        let string_id: Vec<u8> = row.get("string_id");
        let inverter_id: Vec<u8> = row.get("inverter_id");
        let measured_at: i64 = row.get("measured_at");
        let power: Option<i64> = row.try_get("power").ok().flatten();
        let sample = Sample {
            measured_at,
            generation_power_watts: power,
            generation_energy_wh: power.map(|watts| interval_energy(watts, interval_millis)),
            consumption_energy_wh: None,
            grid_import_energy_wh: None,
            grid_export_energy_wh: None,
            temperature_millidegrees_c: None,
            voltage_millivolts: None,
            battery_energy_wh: None,
            quality_flags: row.try_get("quality_flags").unwrap_or_default(),
        };
        strings.entry(string_id).or_default().push(sample);
        let bucket = inverters.entry(inverter_id).or_default();
        let merged = bucket.entry(measured_at).or_insert(Sample {
            measured_at,
            generation_power_watts: Some(0),
            generation_energy_wh: Some(0),
            consumption_energy_wh: None,
            grid_import_energy_wh: None,
            grid_export_energy_wh: None,
            temperature_millidegrees_c: None,
            voltage_millivolts: None,
            battery_energy_wh: None,
            quality_flags: 0,
        });
        merged.generation_power_watts =
            Some(merged.generation_power_watts.unwrap_or_default() + power.unwrap_or_default());
        merged.generation_energy_wh = Some(
            merged.generation_energy_wh.unwrap_or_default()
                + sample.generation_energy_wh.unwrap_or_default(),
        );
        merged.quality_flags |= sample.quality_flags;
    }

    let mut scopes: Vec<(&'static str, Vec<u8>, Vec<Sample>)> = strings
        .into_iter()
        .map(|(id, samples)| ("string", id, samples))
        .collect();
    scopes.extend(
        inverters
            .into_iter()
            .map(|(id, samples)| ("inverter", id, samples.into_values().collect())),
    );
    Ok(scopes)
}

/// Energy produced by holding a power reading for one reporting interval.
const fn interval_energy(watts: i64, interval_millis: i64) -> i64 {
    watts * interval_millis / 3_600_000
}

/// Everything the aggregate writers need beyond the samples themselves.
#[cfg(feature = "sqlite")]
struct AggregateContext {
    blob: Vec<u8>,
    scope_kind: &'static str,
    scope_id: Vec<u8>,
    timezone: String,
    zone: Tz,
    interval_millis: i64,
    calculated_at: i64,
}

/// The month and year buckets of one scope, growing day by day.
#[cfg(feature = "sqlite")]
#[derive(Default)]
struct CoarseBuckets(BTreeMap<(&'static str, i64, String), (Resolution, Bucket)>);

/// Folds a day's samples into the month and year buckets written once the
/// year is done and, for a day that received readings, writes its
/// quarter-hour, hour and day rollups plus the daily summaries they imply.
#[cfg(feature = "sqlite")]
async fn write_aggregates(
    connection: &mut sqlx::SqliteConnection,
    context: &AggregateContext,
    samples: &[Sample],
    coarse: &mut CoarseBuckets,
    touched: bool,
) -> Result<AggregationReport, AggregationError> {
    let mut report = AggregationReport::default();
    let mut daily: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut rollups: Vec<(Resolution, Bucket)> = Vec::new();
    for resolution in RESOLUTIONS {
        for ((start, label), bucket) in bucket_samples(context.zone, resolution, samples)? {
            match resolution {
                Resolution::Month | Resolution::Year => {
                    coarse
                        .0
                        .entry((resolution.name(), start, label))
                        .and_modify(|(_, earlier)| earlier.merge(&bucket))
                        .or_insert((resolution, bucket));
                }
                Resolution::Day => {
                    daily.insert(label, bucket.clone());
                    rollups.push((resolution, bucket));
                }
                Resolution::FifteenMinutes | Resolution::Hour => {
                    rollups.push((resolution, bucket));
                }
            }
        }
    }
    if !touched {
        return Ok(report);
    }
    write_rollups(&mut *connection, context, &rollups).await?;
    report.rollups_written += u64::try_from(rollups.len()).unwrap_or(u64::MAX);
    // Daily and lifetime summaries describe the plant, so only the system scope
    // contributes to them; per-string totals live in the rollups.
    if context.scope_kind == "system" {
        for (local_date, bucket) in &daily {
            write_daily_summary(&mut *connection, context, local_date, bucket).await?;
            report.daily_summaries_written += 1;
        }
    }
    Ok(report)
}

/// Inserts or updates the rollup rows of buckets of one scope, many rows per
/// statement.
#[cfg(feature = "sqlite")]
async fn write_rollups(
    connection: &mut sqlx::SqliteConnection,
    context: &AggregateContext,
    rollups: &[(Resolution, Bucket)],
) -> Result<(), AggregationError> {
    // 24 bound values a row stays well below the parameters SQLite allows.
    const ROWS_PER_STATEMENT: usize = 400;
    for chunk in rollups.chunks(ROWS_PER_STATEMENT) {
        let mut statement = sqlx::QueryBuilder::new(
            "INSERT INTO telemetry_rollups (system_id,scope_kind,scope_id,resolution,\
             bucket_start,bucket_end,timezone,generation,source_generation,point_count,\
             expected_count,generation_energy_sum_wh,consumption_energy_sum_wh,\
             generation_power_min_watts,generation_power_max_watts,\
             generation_power_first_watts,generation_power_last_watts,\
             quality_flags,coverage_basis_points,calculated_at,\
             grid_import_energy_sum_wh,grid_export_energy_sum_wh,\
             temperature_min_millidegrees_c,temperature_max_millidegrees_c,\
             voltage_mean_millivolts,battery_energy_delta_wh) ",
        );
        statement.push_values(chunk, |mut row, (resolution, bucket)| {
            row.push_bind(context.blob.clone())
                .push_bind(context.scope_kind)
                .push_bind(context.scope_id.clone())
                .push_bind(resolution.name())
                .push_bind(bucket.start)
                .push_bind(bucket.end)
                .push_bind(context.timezone.clone())
                .push("1")
                .push("1")
                .push_bind(bucket.points)
                .push_bind(bucket.expected(context.interval_millis))
                .push_bind(bucket.generation_energy_wh)
                .push_bind(bucket.consumption_energy_wh)
                .push_bind(bucket.minimum_watts)
                .push_bind(bucket.maximum_watts)
                .push_bind(bucket.first_watts)
                .push_bind(bucket.last_watts)
                .push_bind(bucket.quality_flags)
                .push_bind(bucket.coverage(context.interval_millis))
                .push_bind(context.calculated_at)
                .push_bind(bucket.grid_import_energy_wh)
                .push_bind(bucket.grid_export_energy_wh)
                .push_bind(bucket.minimum_temperature)
                .push_bind(bucket.maximum_temperature)
                .push_bind(bucket.mean_voltage())
                .push_bind(bucket.battery_energy_delta());
        });
        statement.push(
            " ON CONFLICT(system_id,scope_kind,scope_id,resolution,bucket_start,generation) \
             DO UPDATE SET \
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
             calculated_at=excluded.calculated_at,\
             grid_import_energy_sum_wh=excluded.grid_import_energy_sum_wh,\
             grid_export_energy_sum_wh=excluded.grid_export_energy_sum_wh,\
             temperature_min_millidegrees_c=excluded.temperature_min_millidegrees_c,\
             temperature_max_millidegrees_c=excluded.temperature_max_millidegrees_c,\
             voltage_mean_millivolts=excluded.voltage_mean_millivolts,\
             battery_energy_delta_wh=excluded.battery_energy_delta_wh",
        );
        statement
            .build()
            .execute(&mut *connection)
            .await
            .map_err(|_| AggregationError::Unavailable)?;
    }
    Ok(())
}

/// Inserts or updates the system's daily summary for a local date.
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
         coverage_basis_points,quality_flags,calculated_at,\
         grid_import_energy_wh,grid_export_energy_wh) VALUES (?,?,?,1,?,?,?,?,?,?,?,?) \
         ON CONFLICT(system_id,local_date,generation) DO UPDATE SET \
         grid_import_energy_wh=excluded.grid_import_energy_wh,\
         grid_export_energy_wh=excluded.grid_export_energy_wh,\
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
    .bind(bucket.grid_import_energy_wh)
    .bind(bucket.grid_export_energy_wh)
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
         SUM(grid_import_energy_wh) AS grid_import,SUM(grid_export_energy_wh) AS grid_export,\
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
         peak_generation_power_watts,coverage_basis_points,calculated_at,\
         grid_import_energy_wh,grid_export_energy_wh) \
         VALUES (?,1,?,?,?,?,?,?,?,?,?) ON CONFLICT(system_id) DO UPDATE SET \
         grid_import_energy_wh=excluded.grid_import_energy_wh,\
         grid_export_energy_wh=excluded.grid_export_energy_wh,\
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
    .bind(
        totals
            .try_get::<Option<i64>, _>("grid_import")
            .ok()
            .flatten(),
    )
    .bind(
        totals
            .try_get::<Option<i64>, _>("grid_export")
            .ok()
            .flatten(),
    )
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
) -> Result<BTreeMap<(i64, String), Bucket>, AggregationError> {
    // Keyed by start as well: the hour repeated when the clocks go back has
    // one label but two buckets.
    let mut buckets: BTreeMap<(i64, String), Bucket> = BTreeMap::new();
    // Neighbouring samples mostly share a bucket, whose window is then reused
    // instead of being worked out again.
    let mut current: Option<(String, i64, i64)> = None;
    for sample in samples {
        let (label, start, end) = match &current {
            Some((label, start, end)) if (*start..*end).contains(&sample.measured_at) => {
                (label.clone(), *start, *end)
            }
            _ => {
                let found = window(zone, resolution, local_time(zone, sample.measured_at)?)?;
                current = Some(found.clone());
                found
            }
        };
        let bucket = buckets.entry((start, label)).or_insert(Bucket {
            start,
            end,
            ..Bucket::default()
        });
        bucket.accept(*sample);
    }
    Ok(buckets)
}

/// Converts epoch milliseconds to local time in the zone, failing with `Timezone` when the value is out of range.
pub(crate) fn local_time(zone: Tz, epoch_millis: i64) -> Result<DateTime<Tz>, AggregationError> {
    DateTime::from_timestamp_millis(epoch_millis)
        .map(|value| value.with_timezone(&zone))
        .ok_or(AggregationError::Timezone)
}

/// Resolves the local bucket a timestamp belongs to as a half-open UTC interval.
pub(crate) fn window(
    zone: Tz,
    resolution: Resolution,
    local: DateTime<Tz>,
) -> Result<(String, i64, i64), AggregationError> {
    let (label, start_local, end_local) = match resolution {
        // Quarter-hours and hours step back from the instant itself instead of
        // setting local clock fields: a local time that occurs twice when the
        // clocks go back has no single instant, so the repeated hour is two
        // buckets with the same label and different starts.
        Resolution::FifteenMinutes => {
            let start = local - since_bucket_start(&local, local.minute() % 15);
            (
                start.format("%Y-%m-%dT%H:%M").to_string(),
                start,
                start + chrono::Duration::minutes(15),
            )
        }
        Resolution::Hour => {
            let start = local - since_bucket_start(&local, local.minute());
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

/// Time from the start of a bucket that began `minutes` whole minutes before
/// the minute of `local`.
fn since_bucket_start(local: &DateTime<Tz>, minutes: u32) -> chrono::Duration {
    chrono::Duration::minutes(i64::from(minutes))
        + chrono::Duration::seconds(i64::from(local.second()))
        + chrono::Duration::nanoseconds(i64::from(local.nanosecond()))
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

/// Returns the UTC epoch milliseconds of the start of the local year containing the timestamp.
pub(crate) fn year_start(zone: Tz, epoch_millis: i64) -> Result<i64, AggregationError> {
    let local = local_time(zone, epoch_millis)?;
    let date = NaiveDate::from_ymd_opt(local.year(), 1, 1).ok_or(AggregationError::Timezone)?;
    Ok(start_of_day(zone, date)?.timestamp_millis())
}

/// Returns the UTC epoch milliseconds of the start of the local year after the one containing the timestamp.
pub(crate) fn next_year_start(zone: Tz, epoch_millis: i64) -> Result<i64, AggregationError> {
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
