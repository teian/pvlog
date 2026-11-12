//! Storage-backed range summaries, string breakdowns, and tariff periods.
//!
//! Every figure comes from the scoped rollups the aggregation cycle writes, so
//! a summary and the chart beneath it can never disagree.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike as _, Duration, NaiveDate, TimeZone as _};
use chrono_tz::Tz;
use pvlog_api::{
    BestBucket, InsightApiError, InsightApiUseCases, InsightRange, PreviousPeriod, RangeSummary,
    RevenueSummary, StringContribution, WeatherClassShare, WeatherMonth,
};
use pvlog_application::Clock;
use pvlog_domain::{AccountId, SystemId, UserId};
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(feature = "sqlite")]
use sqlx::Row;
use uuid::Uuid;

use crate::tariffs::{TariffInput, overlaps, periods, revenue_for};

/// Serves the dashboard's summary, breakdown, and tariff endpoints.
pub struct StorageInsightApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageInsightApi {
    /// Creates the insight API over the given database target, management repository, and clock.
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

    /// Looks up the account owning a system; an unregistered system is `NotFound`.
    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, InsightApiError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| InsightApiError::Unavailable)?
            .map(|record| record.account_id)
            .ok_or(InsightApiError::NotFound)
    }

    /// Routes to the `SQLite` account database that holds the system, failing as `Unavailable` for other targets.
    #[cfg(feature = "sqlite")]
    async fn account(
        &self,
        system_id: SystemId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, InsightApiError> {
        let DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } = &self.target
        else {
            return Err(InsightApiError::Unavailable);
        };
        let account_id = self.account_for(system_id).await?;
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.clone(),
            accounts_dir.clone(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| InsightApiError::Unavailable)?
        .route(account_id)
        .await
        .map_err(|_| InsightApiError::Unavailable)
    }

    /// Returns the clock's current time in epoch milliseconds.
    fn now(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }
}

/// Half-open local range plus the rollup resolution that fills it.
struct RangeWindow {
    start: i64,
    end: i64,
    previous_start: i64,
    resolution: &'static str,
}

/// Resolves a requested range against the system's local calendar.
///
/// The previous window is the same shape immediately before, so a comparison
/// always contrasts like with like — a month against a month, not thirty days.
fn window(zone: Tz, now: i64, range: InsightRange) -> Option<RangeWindow> {
    let local = DateTime::from_timestamp_millis(now)?.with_timezone(&zone);
    let midnight = start_of_day(zone, local.date_naive())?;
    Some(match range {
        InsightRange::Live | InsightRange::Day => RangeWindow {
            start: midnight,
            end: now,
            previous_start: start_of_day(zone, local.date_naive().pred_opt()?)?,
            resolution: "fifteen_minute",
        },
        InsightRange::Week => {
            let weekday = i64::from(local.weekday().num_days_from_monday());
            let start = start_of_day(zone, (local - Duration::days(weekday)).date_naive())?;
            RangeWindow {
                start,
                end: now,
                previous_start: start_of_day(
                    zone,
                    (local - Duration::days(weekday + 7)).date_naive(),
                )?,
                resolution: "day",
            }
        }
        InsightRange::Month => {
            let first = NaiveDate::from_ymd_opt(local.year(), local.month(), 1)?;
            let previous = if local.month() == 1 {
                NaiveDate::from_ymd_opt(local.year() - 1, 12, 1)?
            } else {
                NaiveDate::from_ymd_opt(local.year(), local.month() - 1, 1)?
            };
            RangeWindow {
                start: start_of_day(zone, first)?,
                end: now,
                previous_start: start_of_day(zone, previous)?,
                resolution: "day",
            }
        }
        InsightRange::Year => RangeWindow {
            start: start_of_day(zone, NaiveDate::from_ymd_opt(local.year(), 1, 1)?)?,
            end: now,
            previous_start: start_of_day(zone, NaiveDate::from_ymd_opt(local.year() - 1, 1, 1)?)?,
            resolution: "month",
        },
    })
}

/// Returns the first instant of a local date in epoch milliseconds, falling back to 01:00 when midnight does not exist.
fn start_of_day(zone: Tz, date: NaiveDate) -> Option<i64> {
    let midnight = date.and_hms_opt(0, 0, 0)?;
    zone.from_local_datetime(&midnight)
        .earliest()
        .or_else(|| {
            date.and_hms_opt(1, 0, 0)
                .and_then(|value| zone.from_local_datetime(&value).earliest())
        })
        .map(|value| value.timestamp_millis())
}

#[async_trait]
impl InsightApiUseCases for StorageInsightApi {
    /// Sums generation over the range window and the previous one, and reports best bucket, peak, and last values for the scope.
    async fn range_summary(
        &self,
        _actor: UserId,
        system_id: SystemId,
        scope: Option<String>,
        scope_id: Option<Uuid>,
        range: InsightRange,
    ) -> Result<RangeSummary, InsightApiError> {
        #[cfg(feature = "sqlite")]
        {
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let (zone, _) = system_settings(&mut connection, system_id).await?;
            let window = window(zone, self.now(), range).ok_or(InsightApiError::Unavailable)?;
            let (scope_kind, scope_identifier) =
                resolve_scope(scope.as_deref(), scope_id, system_id)
                    .ok_or(InsightApiError::InvalidInput("scope"))?;

            let buckets = read_buckets(
                &mut connection,
                system_id,
                scope_kind,
                scope_identifier,
                window.resolution,
                window.start,
                window.end,
            )
            .await?;
            let previous = read_buckets(
                &mut connection,
                system_id,
                scope_kind,
                scope_identifier,
                window.resolution,
                window.previous_start,
                window.start,
            )
            .await?;

            let generation: i64 = buckets.iter().map(|bucket| bucket.energy_wh).sum();
            let previous_total: i64 = previous.iter().map(|bucket| bucket.energy_wh).sum();
            let best = buckets
                .iter()
                .max_by_key(|bucket| bucket.energy_wh)
                .map(|bucket| BestBucket {
                    start_epoch_millis: bucket.start,
                    energy_wh: bucket.energy_wh,
                });
            let tariff_periods = periods(&mut connection, system_id)
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let priced: Vec<(i64, i64)> = buckets
                .iter()
                .map(|bucket| (bucket.start, bucket.energy_wh))
                .collect();
            let revenue = revenue_for(&priced, &tariff_periods);

            return Ok(RangeSummary {
                range,
                start_epoch_millis: window.start,
                end_epoch_millis: window.end,
                generation_energy_wh: generation,
                consumption_energy_wh: Some(
                    buckets
                        .iter()
                        .filter_map(|bucket| bucket.consumption_wh)
                        .sum(),
                ),
                peak_power_watts: buckets.iter().filter_map(|bucket| bucket.peak_watts).max(),
                current_power_watts: buckets.last().and_then(|bucket| bucket.last_watts),
                average_bucket_energy_wh: (!buckets.is_empty())
                    .then(|| generation / i64::try_from(buckets.len()).unwrap_or(1)),
                best_bucket: best,
                previous_period: (previous_total > 0).then(|| PreviousPeriod {
                    generation_energy_wh: previous_total,
                    change_basis_points: (generation - previous_total).saturating_mul(10_000)
                        / previous_total,
                }),
                revenue: RevenueSummary {
                    minor_units: revenue.minor_units,
                    currency_code: revenue.currency_code,
                    periods_applied: revenue.periods_applied,
                    unpriced_energy_wh: revenue.unpriced_energy_wh,
                },
                capacity_watts: capacity_watts(&mut connection, system_id).await?,
            });
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (system_id, scope, scope_id, range);
            Err(InsightApiError::Unavailable)
        }
    }

    /// Reports each PV string's rated power, generated energy, and operating status for the range window.
    async fn string_breakdown(
        &self,
        _actor: UserId,
        system_id: SystemId,
        range: InsightRange,
    ) -> Result<Vec<StringContribution>, InsightApiError> {
        #[cfg(feature = "sqlite")]
        {
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let (zone, interval_millis) = system_settings(&mut connection, system_id).await?;
            let window = window(zone, self.now(), range).ok_or(InsightApiError::Unavailable)?;
            let now = self.now();

            let rows = sqlx::query(
                "SELECT p.id AS string_id,p.name AS name,p.rated_power_watts AS rated,\
                 i.id AS inverter_id,i.name AS inverter_name,\
                 (SELECT COUNT(*) FROM channel_definitions c WHERE c.target_kind='string' \
                  AND c.target_id=p.id AND c.lifecycle='active' \
                  AND c.unit='W' AND c.aggregation='mean') AS channel_count,\
                 (SELECT COALESCE(SUM(r.generation_energy_sum_wh),0) FROM telemetry_rollups r \
                  WHERE r.system_id=? AND r.scope_kind='string' AND r.scope_id=p.id \
                  AND r.resolution=? AND r.bucket_start>=? AND r.bucket_start<?) AS energy_wh,\
                 (SELECT MAX(r.bucket_end) FROM telemetry_rollups r WHERE r.system_id=? \
                  AND r.scope_kind='string' AND r.scope_id=p.id) AS last_bucket_end,\
                 (SELECT r.generation_power_max_watts FROM telemetry_rollups r \
                  WHERE r.system_id=? AND r.scope_kind='string' AND r.scope_id=p.id \
                  ORDER BY r.bucket_start DESC LIMIT 1) AS last_peak \
                 FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
                 WHERE i.system_id=? ORDER BY i.name,p.name",
            )
            .bind(id_blob(system_id))
            .bind(window.resolution)
            .bind(window.start)
            .bind(window.end)
            .bind(id_blob(system_id))
            .bind(id_blob(system_id))
            .bind(id_blob(system_id))
            .fetch_all(&mut *connection)
            .await
            .map_err(|_| InsightApiError::Unavailable)?;

            return rows
                .iter()
                .map(|row| {
                    let metered = row.try_get::<i64, _>("channel_count").unwrap_or_default() > 0;
                    let last_end: Option<i64> = row.try_get("last_bucket_end").ok().flatten();
                    let last_peak: Option<i64> = row.try_get("last_peak").ok().flatten();
                    Ok(StringContribution {
                        string_id: row_uuid(row, "string_id")?,
                        name: row.get("name"),
                        inverter_id: row_uuid(row, "inverter_id")?,
                        inverter_name: row.get("inverter_name"),
                        rated_power_watts: row.try_get("rated").unwrap_or_default(),
                        generation_energy_wh: row.try_get("energy_wh").unwrap_or_default(),
                        status: string_status(metered, last_end, last_peak, now, interval_millis),
                        metered,
                    })
                })
                .collect();
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (system_id, range);
            Err(InsightApiError::Unavailable)
        }
    }

    /// Computes monthly generation and weather-class shares for the most recent months, skipping months with no valid window.
    async fn weather_composition(
        &self,
        _actor: UserId,
        system_id: SystemId,
        months: u32,
    ) -> Result<Vec<WeatherMonth>, InsightApiError> {
        #[cfg(feature = "sqlite")]
        {
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let (zone, _) = system_settings(&mut connection, system_id).await?;
            let mut out = Vec::new();
            for offset in (0..months).rev() {
                let Some((start, end)) = month_window(zone, self.now(), offset) else {
                    continue;
                };
                let energy: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(generation_energy_sum_wh),0) FROM telemetry_rollups \
                     WHERE system_id=? AND scope_kind='system' AND resolution='day' \
                     AND bucket_start>=? AND bucket_start<?",
                )
                .bind(id_blob(system_id))
                .bind(start)
                .bind(end)
                .fetch_one(&mut *connection)
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
                let shares = weather_shares(&mut connection, system_id, start, end).await?;
                out.push(WeatherMonth {
                    month_start_epoch_millis: start,
                    generation_energy_wh: energy,
                    classes: shares
                        .into_iter()
                        .map(|(class, share)| WeatherClassShare {
                            class: class.to_owned(),
                            share_basis_points: share,
                            generation_energy_wh: energy * i64::from(share) / 10_000,
                        })
                        .collect(),
                });
            }
            return Ok(out);
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (system_id, months);
            Err(InsightApiError::Unavailable)
        }
    }

    /// Lists the stored tariff periods as JSON values.
    async fn tariffs(
        &self,
        _actor: UserId,
        system_id: SystemId,
    ) -> Result<Vec<serde_json::Value>, InsightApiError> {
        #[cfg(feature = "sqlite")]
        {
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let stored = periods(&mut connection, system_id)
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            return stored
                .iter()
                .map(|period| {
                    serde_json::to_value(period).map_err(|_| InsightApiError::Unavailable)
                })
                .collect();
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = system_id;
            Err(InsightApiError::Unavailable)
        }
    }

    /// Validates tariff input (direction and currency code) and inserts or updates the tariff period, answering `Conflict` when it overlaps an existing one.
    async fn upsert_tariff(
        &self,
        _actor: UserId,
        system_id: SystemId,
        tariff_id: Option<Uuid>,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, InsightApiError> {
        let input: TariffInput =
            serde_json::from_value(input).map_err(|_| InsightApiError::InvalidInput("tariff"))?;
        if !matches!(input.direction.as_str(), "import" | "export") {
            return Err(InsightApiError::InvalidInput("direction"));
        }
        if input.currency_code.len() != 3 {
            return Err(InsightApiError::InvalidInput("currency_code"));
        }
        #[cfg(feature = "sqlite")]
        {
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let existing = periods(&mut connection, system_id)
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            if overlaps(&input, &existing, tariff_id) {
                return Err(InsightApiError::Conflict("overlapping_tariff_period"));
            }
            let id = tariff_id.unwrap_or_else(Uuid::now_v7);
            let now = self.now();
            sqlx::query(
                "INSERT INTO tariffs (id,system_id,name,direction,currency_code,\
                 minor_units_per_kwh,effective_from,effective_to,created_at,updated_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET \
                 name=excluded.name,direction=excluded.direction,\
                 currency_code=excluded.currency_code,\
                 minor_units_per_kwh=excluded.minor_units_per_kwh,\
                 effective_from=excluded.effective_from,effective_to=excluded.effective_to,\
                 updated_at=excluded.updated_at,version=version+1 \
                 WHERE tariffs.system_id=excluded.system_id",
            )
            .bind(id.as_bytes().to_vec())
            .bind(id_blob(system_id))
            .bind(&input.name)
            .bind(&input.direction)
            .bind(&input.currency_code)
            .bind(input.minor_units_per_kwh)
            .bind(input.effective_from_epoch_millis)
            .bind(input.effective_to_epoch_millis)
            .bind(now)
            .bind(now)
            .execute(&mut *connection)
            .await
            .map_err(|_| InsightApiError::Unavailable)?;
            let stored = periods(&mut connection, system_id)
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            return stored
                .iter()
                .find(|period| period.id == id)
                .map(|period| serde_json::to_value(period).unwrap_or_default())
                .ok_or(InsightApiError::Unavailable);
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (system_id, tariff_id, input);
            Err(InsightApiError::Unavailable)
        }
    }

    /// Deletes a tariff period by identifier; a missing row is `NotFound`.
    async fn delete_tariff(
        &self,
        _actor: UserId,
        system_id: SystemId,
        tariff_id: Uuid,
    ) -> Result<(), InsightApiError> {
        #[cfg(feature = "sqlite")]
        {
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            let deleted = sqlx::query("DELETE FROM tariffs WHERE id=? AND system_id=?")
                .bind(tariff_id.as_bytes().to_vec())
                .bind(id_blob(system_id))
                .execute(&mut *connection)
                .await
                .map_err(|_| InsightApiError::Unavailable)?;
            return if deleted.rows_affected() == 0 {
                Err(InsightApiError::NotFound)
            } else {
                Ok(())
            };
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (system_id, tariff_id);
            Err(InsightApiError::Unavailable)
        }
    }
}

/// Local month boundary `offset` months back from now.
fn month_window(zone: Tz, now: i64, offset: u32) -> Option<(i64, i64)> {
    let local = DateTime::from_timestamp_millis(now)?.with_timezone(&zone);
    let months_back = i32::try_from(offset).ok()?;
    let total = local.year() * 12 + i32::try_from(local.month()).ok()? - 1 - months_back;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let start = NaiveDate::from_ymd_opt(year, u32::try_from(month).ok()?, 1)?;
    let next_total = total + 1;
    let next = NaiveDate::from_ymd_opt(
        next_total.div_euclid(12),
        u32::try_from(next_total.rem_euclid(12) + 1).ok()?,
        1,
    )?;
    Some((start_of_day(zone, start)?, start_of_day(zone, next)?))
}

/// Splits a month's weather intervals into sky-condition classes.
///
/// Classes come from recorded cloud cover. Precipitation classes the dashboard
/// also draws — showers and snow — have no column in `weather_data_points`, so
/// they are not invented here.
#[cfg(feature = "sqlite")]
async fn weather_shares(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    start: i64,
    end: i64,
) -> Result<Vec<(&'static str, u16)>, InsightApiError> {
    let rows = sqlx::query(
        "SELECT p.cloud_cover_basis_points AS cover FROM weather_data_points p \
         JOIN weather_data_runs r ON r.id=p.run_id \
         WHERE r.system_id=? AND p.interval_start>=? AND p.interval_start<? \
         AND p.cloud_cover_basis_points IS NOT NULL",
    )
    .bind(id_blob(system_id))
    .bind(start)
    .bind(end)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| InsightApiError::Unavailable)?;
    let total = rows.len();
    if total == 0 {
        return Ok(Vec::new());
    }
    let mut counts = [0_usize; 4];
    for row in &rows {
        let cover: i64 = row.try_get("cover").unwrap_or_default();
        let index = match cover {
            ..=2_000 => 0,
            2_001..=5_000 => 1,
            5_001..=8_000 => 2,
            _ => 3,
        };
        counts[index] += 1;
    }
    let names = ["clear", "partly_cloudy", "mostly_cloudy", "overcast"];
    Ok(names
        .iter()
        .zip(counts.iter())
        .map(|(name, count)| {
            let share = u16::try_from(count * 10_000 / total).unwrap_or(10_000);
            (*name, share)
        })
        .collect())
}

/// One aggregate bucket reduced to what a summary needs.
struct SummaryBucket {
    start: i64,
    energy_wh: i64,
    consumption_wh: Option<i64>,
    peak_watts: Option<i64>,
    last_watts: Option<i64>,
}

/// Maps a requested scope name and identifier to a rollup scope kind and UUID, or `None` when the combination is invalid.
fn resolve_scope(
    kind: Option<&str>,
    scope_id: Option<Uuid>,
    system_id: SystemId,
) -> Option<(&'static str, Uuid)> {
    match (kind.unwrap_or("system"), scope_id) {
        ("system", _) => Some(("system", system_id.as_uuid())),
        ("inverter", Some(id)) => Some(("inverter", id)),
        ("string", Some(id)) => Some(("string", id)),
        _ => None,
    }
}

/// Derives a string's operating state from how recently it reported.
///
/// The states mirror the dashboard: nothing recent is `offline`, a recent zero
/// during expected production is `idle`, and anything else is `online`. A
/// `fault` is left to the alert rules, which know the thresholds.
fn string_status(
    metered: bool,
    last_bucket_end: Option<i64>,
    last_peak_watts: Option<i64>,
    now: i64,
    interval_millis: i64,
) -> String {
    if !metered {
        return "offline".to_owned();
    }
    let stale_after = interval_millis.saturating_mul(4).max(900_000);
    match last_bucket_end {
        Some(end) if now.saturating_sub(end) <= stale_after => {
            if last_peak_watts.unwrap_or_default() > 0 {
                "online".to_owned()
            } else {
                "idle".to_owned()
            }
        }
        _ => "offline".to_owned(),
    }
}

/// Reads the system's time zone and status interval (in milliseconds); an unknown system is `NotFound`.
#[cfg(feature = "sqlite")]
async fn system_settings(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
) -> Result<(Tz, i64), InsightApiError> {
    let row = sqlx::query("SELECT timezone,status_interval_seconds FROM systems WHERE id=?")
        .bind(id_blob(system_id))
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| InsightApiError::Unavailable)?
        .ok_or(InsightApiError::NotFound)?;
    let timezone: String = row.get("timezone");
    let zone: Tz = timezone.parse().map_err(|_| InsightApiError::Unavailable)?;
    Ok((
        zone,
        row.get::<i64, _>("status_interval_seconds")
            .saturating_mul(1_000),
    ))
}

/// Reads rollup buckets for a scope and resolution within a half-open time range, ordered by start.
#[cfg(feature = "sqlite")]
async fn read_buckets(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    scope_kind: &str,
    scope_id: Uuid,
    resolution: &str,
    start: i64,
    end: i64,
) -> Result<Vec<SummaryBucket>, InsightApiError> {
    let rows = sqlx::query(
        "SELECT bucket_start,generation_energy_sum_wh,consumption_energy_sum_wh,\
         generation_power_max_watts,generation_power_last_watts FROM telemetry_rollups \
         WHERE system_id=? AND scope_kind=? AND scope_id=? AND resolution=? \
         AND bucket_start>=? AND bucket_start<? ORDER BY bucket_start",
    )
    .bind(id_blob(system_id))
    .bind(scope_kind)
    .bind(scope_id.as_bytes().to_vec())
    .bind(resolution)
    .bind(start)
    .bind(end)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| InsightApiError::Unavailable)?;
    Ok(rows
        .iter()
        .map(|row| SummaryBucket {
            start: row.get("bucket_start"),
            energy_wh: row
                .try_get::<Option<i64>, _>("generation_energy_sum_wh")
                .ok()
                .flatten()
                .unwrap_or_default(),
            consumption_wh: row
                .try_get::<Option<i64>, _>("consumption_energy_sum_wh")
                .ok()
                .flatten(),
            peak_watts: row
                .try_get::<Option<i64>, _>("generation_power_max_watts")
                .ok()
                .flatten(),
            last_watts: row
                .try_get::<Option<i64>, _>("generation_power_last_watts")
                .ok()
                .flatten(),
        })
        .collect())
}

/// Sums the rated power of all PV strings in watts, falling back to the declared capacity
/// while the system has no strings; `None` when neither is configured.
#[cfg(feature = "sqlite")]
async fn capacity_watts(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
) -> Result<Option<i64>, InsightApiError> {
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE((SELECT SUM(p.rated_power_watts) FROM pv_strings p \
         JOIN inverters i ON i.id=p.inverter_id WHERE i.system_id=?1),\
         (SELECT declared_capacity_watts FROM systems WHERE id=?1))",
    )
    .bind(id_blob(system_id))
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| InsightApiError::Unavailable)?;
    Ok(total.filter(|value| *value > 0))
}

/// Reads a UUID blob column, failing as `Unavailable` when it is malformed.
#[cfg(feature = "sqlite")]
fn row_uuid(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Uuid, InsightApiError> {
    Uuid::from_slice(&row.get::<Vec<u8>, _>(column)).map_err(|_| InsightApiError::Unavailable)
}

/// Encodes a system identifier as the blob bytes used in SQL bindings.
#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}
