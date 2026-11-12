//! Production reporting adapter backed by account telemetry and forecasting storage.

use async_trait::async_trait;
use pvlog_api::{
    DataState, MonthlyProductionResponse, ReportingApiError, ReportingApiUseCases,
    SeasonProductionResponse, SeasonalResponse, StatisticsResponse, SystemOverviewResponse,
    WeatherForecastPointResponse, WeatherForecastResponse, YearlyProductionResponse,
};

use crate::daily_summaries::{ALL_DATES, daily_repositories, daily_values};
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::{DatabaseTarget, SqliteAccountPoolConfig, SqliteAccountPoolRouter};
use sqlx::Row;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};

#[derive(Clone, Debug)]
pub struct StorageReportingApi {
    target: DatabaseTarget,
}

impl StorageReportingApi {
    #[must_use]
    pub const fn new(target: DatabaseTarget) -> Self {
        Self { target }
    }
}

#[async_trait]
impl ReportingApiUseCases for StorageReportingApi {
    async fn system_overview(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SystemOverviewResponse, ReportingApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = sqlite_account(management_path, accounts_dir, account_id).await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let row = sqlx::query(
                        "SELECT s.name,s.timezone,s.lifecycle,s.description,s.commissioning_date,s.visibility,\
                         s.status_interval_seconds,\
                         COUNT(DISTINCT i.id) AS inverter_count,\
                         COUNT(DISTINCT p.id) AS string_count,\
                         COALESCE(SUM(p.rated_power_watts),MAX(s.declared_capacity_watts)) AS capacity_watts,\
                         (SELECT coverage_basis_points FROM system_lifetime_summaries l \
                          WHERE l.system_id=s.id) AS availability,\
                         (SELECT MAX(measured_at) FROM telemetry_hot h WHERE h.system_id=s.id) AS last_seen \
                         FROM systems s LEFT JOIN inverters i ON i.system_id=s.id \
                         LEFT JOIN pv_strings p ON p.inverter_id=i.id WHERE s.id=? \
                         GROUP BY s.id,s.name,s.timezone,s.lifecycle,s.status_interval_seconds",
                    ).bind(id_blob(system_id)).fetch_optional(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    row.map(|row| sqlite_overview(system_id, &row))
                        .ok_or(ReportingApiError::NotFound)
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Err(ReportingApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let mut connection = PgConnection::connect(url)
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let row = sqlx::query(
                        "SELECT s.name,s.timezone,s.lifecycle,s.description,s.commissioning_date::TEXT AS commissioning_date,\
                         s.visibility,s.status_interval_seconds::BIGINT AS status_interval_seconds,\
                         COUNT(DISTINCT i.id)::BIGINT AS inverter_count,\
                         COUNT(DISTINCT p.id)::BIGINT AS string_count,\
                         COALESCE(SUM(p.rated_power_watts),MAX(s.declared_capacity_watts))::BIGINT AS capacity_watts,\
                         (SELECT coverage_basis_points::BIGINT FROM telemetry.lifetime_summaries l \
                          WHERE l.account_id=s.account_id AND l.system_id=s.id) AS availability,\
                         (SELECT MAX(measured_at) FROM telemetry.hot_observations h \
                          WHERE h.account_id=s.account_id AND h.system_id=s.id) AS last_seen \
                         FROM account_data.systems s LEFT JOIN account_data.inverters i ON i.account_id=s.account_id AND i.system_id=s.id \
                         LEFT JOIN account_data.pv_strings p ON p.account_id=i.account_id AND p.inverter_id=i.id \
                         WHERE s.account_id=$1 AND s.id=$2 GROUP BY s.id,s.name,s.timezone,s.lifecycle,s.status_interval_seconds",
                    ).bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_optional(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    row.map(|row| postgres_overview(system_id, &row))
                        .ok_or(ReportingApiError::NotFound)
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id, system_id);
                    Err(ReportingApiError::Unavailable)
                }
            }
        }
    }

    async fn statistics(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<StatisticsResponse, ReportingApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = sqlite_account(management_path, accounts_dir, account_id).await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let lifetime = sqlx::query("SELECT generation_energy_wh,consumption_energy_wh,peak_generation_power_watts,first_observation_at,last_observation_at,coverage_basis_points FROM system_lifetime_summaries WHERE system_id=?")
                        .bind(id_blob(system_id)).fetch_optional(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    let rows = sqlx::query("SELECT bucket_start,generation_energy_sum_wh,consumption_energy_sum_wh,coverage_basis_points FROM telemetry_rollups r WHERE system_id=? AND scope_kind='system' AND resolution='month' AND generation=(SELECT MAX(latest.generation) FROM telemetry_rollups latest WHERE latest.system_id=r.system_id AND latest.scope_kind=r.scope_kind AND latest.scope_id=r.scope_id AND latest.resolution=r.resolution AND latest.bucket_start=r.bucket_start) ORDER BY bucket_start")
                        .bind(id_blob(system_id)).fetch_all(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    let mut response = statistics_response(
                        system_id,
                        lifetime.as_ref().map(LifetimeValues::from_row),
                        rows.iter().map(MonthlyValues::from_row).collect(),
                    );
                    self.include_supplied_days(account_id, system_id, &mut response)
                        .await?;
                    Ok(response)
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Err(ReportingApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let mut connection = PgConnection::connect(url)
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let lifetime = sqlx::query("SELECT generation_energy_wh,consumption_energy_wh,peak_generation_power_watts,first_observation_at,last_observation_at,coverage_basis_points::BIGINT AS coverage_basis_points FROM telemetry.lifetime_summaries WHERE account_id=$1 AND system_id=$2")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_optional(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    let rows = sqlx::query("SELECT bucket_start,generation_energy_sum_wh,consumption_energy_sum_wh,coverage_basis_points::BIGINT AS coverage_basis_points FROM telemetry.rollups r WHERE account_id=$1 AND system_id=$2 AND scope_kind='system' AND resolution='month' AND generation=(SELECT MAX(latest.generation) FROM telemetry.rollups latest WHERE latest.account_id=r.account_id AND latest.system_id=r.system_id AND latest.scope_kind=r.scope_kind AND latest.scope_id=r.scope_id AND latest.resolution=r.resolution AND latest.bucket_start=r.bucket_start) ORDER BY bucket_start")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_all(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let mut response = statistics_response(
                        system_id,
                        lifetime.as_ref().map(LifetimeValues::from_row),
                        rows.iter().map(MonthlyValues::from_row).collect(),
                    );
                    self.include_supplied_days(account_id, system_id, &mut response)
                        .await?;
                    Ok(response)
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id, system_id);
                    Err(ReportingApiError::Unavailable)
                }
            }
        }
    }

    async fn seasonal(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SeasonalResponse, ReportingApiError> {
        // Every day that reports a value counts, whether it was computed from
        // readings or supplied by the source: history imported as daily
        // summaries belongs to its season like any other day.
        let repositories = daily_repositories(&self.target, account_id)
            .await
            .map_err(|_| ReportingApiError::Unavailable)?;
        let (from, to) = ALL_DATES;
        let days = daily_values(repositories.summaries.as_ref(), system_id, from, to)
            .await
            .map_err(|_| ReportingApiError::Unavailable)?
            .into_iter()
            .map(|day| (day.date, day.energy.generation_energy_wh))
            .collect::<Vec<_>>();
        Ok(seasonal_response(system_id, &days))
    }

    async fn weather_forecast(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<WeatherForecastResponse, ReportingApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = sqlite_account(management_path, accounts_dir, account_id).await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let run = sqlx::query("SELECT id,issued_at,attribution FROM weather_data_runs WHERE system_id=? AND data_kind='forecast' ORDER BY issued_at DESC,fetched_at DESC LIMIT 1")
                        .bind(id_blob(system_id)).fetch_optional(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    let Some(run) = run else {
                        return Ok(empty_weather(system_id));
                    };
                    let run_id: Vec<u8> = run.get("id");
                    let points = sqlx::query("SELECT w.interval_start,w.interval_end,COALESCE(w.plane_of_array_wm2,w.global_horizontal_wm2) AS irradiance,w.ambient_temperature_millicelsius,w.wind_speed_millimetres_per_second,w.cloud_cover_basis_points,(SELECT r.energy_central_wh FROM yield_result_projections y JOIN yield_calculation_results r ON r.id=y.result_id WHERE y.system_id=? AND y.basis='forecast' AND y.scope_kind='system' AND y.scope_id=? AND y.interval_start=w.interval_start LIMIT 1) AS predicted_energy_wh FROM weather_data_points w WHERE w.run_id=? ORDER BY w.interval_start")
                        .bind(id_blob(system_id)).bind(id_blob(system_id)).bind(run_id).fetch_all(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    Ok(weather_response(
                        system_id,
                        run.try_get("issued_at").ok(),
                        run.try_get("attribution").ok(),
                        points.iter().map(WeatherValues::from_row).collect(),
                    ))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Err(ReportingApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let mut connection = PgConnection::connect(url)
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let run = sqlx::query("SELECT id,issued_at,attribution FROM account_data.weather_data_runs WHERE account_id=$1 AND system_id=$2 AND data_kind='forecast' ORDER BY issued_at DESC,fetched_at DESC LIMIT 1")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_optional(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    let Some(run) = run else {
                        connection
                            .close()
                            .await
                            .map_err(|_| ReportingApiError::Unavailable)?;
                        return Ok(empty_weather(system_id));
                    };
                    let run_id: uuid::Uuid = run.get("id");
                    let points = sqlx::query("SELECT w.interval_start,w.interval_end,COALESCE(w.plane_of_array_wm2,w.global_horizontal_wm2)::BIGINT AS irradiance,w.ambient_temperature_millicelsius::BIGINT AS ambient_temperature_millicelsius,w.wind_speed_millimetres_per_second::BIGINT AS wind_speed_millimetres_per_second,w.cloud_cover_basis_points::BIGINT AS cloud_cover_basis_points,(SELECT r.energy_central_wh FROM account_data.yield_result_projections y JOIN account_data.yield_calculation_results r ON r.account_id=y.account_id AND r.id=y.result_id WHERE y.account_id=$1 AND y.system_id=$2 AND y.basis='forecast' AND y.scope_kind='system' AND y.scope_id=$2 AND y.interval_start=w.interval_start LIMIT 1) AS predicted_energy_wh FROM account_data.weather_data_points w WHERE w.account_id=$1 AND w.run_id=$3 ORDER BY w.interval_start")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).bind(run_id).fetch_all(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    Ok(weather_response(
                        system_id,
                        run.try_get("issued_at").ok(),
                        run.try_get("attribution").ok(),
                        points.iter().map(WeatherValues::from_row).collect(),
                    ))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id, system_id);
                    Err(ReportingApiError::Unavailable)
                }
            }
        }
    }
}

/// Shapes one overview row; the equipment counts come from the join, the
/// availability and last-seen values from the aggregates the worker maintains.
#[cfg(feature = "sqlite")]
fn sqlite_overview(system_id: SystemId, row: &sqlx::sqlite::SqliteRow) -> SystemOverviewResponse {
    let last_seen: Option<i64> = row.try_get("last_seen").ok().flatten();
    let status_interval_seconds = status_interval(row.try_get("status_interval_seconds").ok());
    SystemOverviewResponse {
        id: system_id,
        name: row.get("name"),
        timezone: row.get("timezone"),
        lifecycle: row.get("lifecycle"),
        inverter_count: u64::try_from(row.get::<i64, _>("inverter_count")).unwrap_or_default(),
        string_count: u64::try_from(row.get::<i64, _>("string_count")).unwrap_or_default(),
        capacity_watts: row.try_get("capacity_watts").ok(),
        description: row.try_get("description").unwrap_or_default(),
        commissioning_date: row.try_get("commissioning_date").ok().flatten(),
        availability_basis_points: overview_availability(
            row.try_get("availability").ok().flatten(),
        ),
        last_seen_epoch_millis: last_seen,
        status_interval_seconds,
        data_state: DataState::at(last_seen, status_interval_seconds, now_millis()),
        visibility: row.try_get("visibility").unwrap_or_default(),
    }
}

/// Shapes one `PostgreSQL` overview row into a system overview response.
#[cfg(feature = "postgres")]
fn postgres_overview(system_id: SystemId, row: &sqlx::postgres::PgRow) -> SystemOverviewResponse {
    let last_seen: Option<i64> = row.try_get("last_seen").ok().flatten();
    let status_interval_seconds = status_interval(row.try_get("status_interval_seconds").ok());
    SystemOverviewResponse {
        id: system_id,
        name: row.get("name"),
        timezone: row.get("timezone"),
        lifecycle: row.get("lifecycle"),
        inverter_count: u64::try_from(row.get::<i64, _>("inverter_count")).unwrap_or_default(),
        string_count: u64::try_from(row.get::<i64, _>("string_count")).unwrap_or_default(),
        capacity_watts: row.try_get("capacity_watts").ok(),
        description: row.try_get("description").unwrap_or_default(),
        commissioning_date: row.try_get("commissioning_date").ok().flatten(),
        availability_basis_points: overview_availability(
            row.try_get("availability").ok().flatten(),
        ),
        last_seen_epoch_millis: last_seen,
        status_interval_seconds,
        data_state: DataState::at(last_seen, status_interval_seconds, now_millis()),
        visibility: row.try_get("visibility").unwrap_or_default(),
    }
}

/// Narrows stored availability to basis points, dropping out-of-range values.
/// Narrows a stored status interval, falling back to the default when it is unreadable.
fn status_interval(stored: Option<i64>) -> u32 {
    stored
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(pvlog_application::DEFAULT_STATUS_INTERVAL_SECONDS)
}

/// Current time in epoch milliseconds.
fn now_millis() -> i64 {
    let now = time::OffsetDateTime::now_utc();
    now.unix_timestamp() * 1_000 + i64::from(now.nanosecond() / 1_000_000)
}

fn overview_availability(stored: Option<i64>) -> Option<u16> {
    stored.and_then(|value| u16::try_from(value).ok())
}

#[cfg(feature = "sqlite")]
async fn sqlite_account(
    management_path: &std::path::Path,
    accounts_dir: &std::path::Path,
    account_id: AccountId,
) -> Result<pvlog_storage::RoutedSqliteAccount, ReportingApiError> {
    SqliteAccountPoolRouter::new(
        management_path.to_path_buf(),
        accounts_dir.to_path_buf(),
        SqliteAccountPoolConfig::default(),
    )
    .map_err(|_| ReportingApiError::Unavailable)?
    .route(account_id)
    .await
    .map_err(|_| ReportingApiError::Unavailable)
}

#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}

struct LifetimeValues {
    generation_energy_wh: Option<i64>,
    consumption_energy_wh: Option<i64>,
    peak_generation_power_watts: Option<i64>,
    first_observation_at: Option<i64>,
    last_observation_at: Option<i64>,
    coverage_basis_points: u16,
}

impl LifetimeValues {
    fn from_row<R: Row>(row: &R) -> Self
    where
        for<'c> &'c str: sqlx::ColumnIndex<R>,
        for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    {
        Self {
            generation_energy_wh: row.try_get("generation_energy_wh").ok(),
            consumption_energy_wh: row.try_get("consumption_energy_wh").ok(),
            peak_generation_power_watts: row.try_get("peak_generation_power_watts").ok(),
            first_observation_at: row.try_get("first_observation_at").ok(),
            last_observation_at: row.try_get("last_observation_at").ok(),
            coverage_basis_points: basis_points(row, "coverage_basis_points"),
        }
    }
}

struct MonthlyValues {
    bucket_start: i64,
    generation_energy_wh: Option<i64>,
    consumption_energy_wh: Option<i64>,
    coverage_basis_points: u16,
}

impl MonthlyValues {
    fn from_row<R: Row>(row: &R) -> Self
    where
        for<'c> &'c str: sqlx::ColumnIndex<R>,
        for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    {
        Self {
            bucket_start: row.get("bucket_start"),
            generation_energy_wh: row.try_get("generation_energy_sum_wh").ok(),
            consumption_energy_wh: row.try_get("consumption_energy_sum_wh").ok(),
            coverage_basis_points: basis_points(row, "coverage_basis_points"),
        }
    }
}

impl StorageReportingApi {
    /// Brings days whose value is a supplied summary into the statistics.
    ///
    /// The month rollups and the lifetime summary only know interval readings.
    /// For each day that reports the source's summary instead, the difference
    /// to what was computed is added to its month and to the lifetime totals.
    async fn include_supplied_days(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        response: &mut StatisticsResponse,
    ) -> Result<(), ReportingApiError> {
        let unavailable = |_| ReportingApiError::Unavailable;
        let repositories = daily_repositories(&self.target, account_id)
            .await
            .map_err(unavailable)?;
        let (from, to) = ALL_DATES;
        let days = daily_values(repositories.summaries.as_ref(), system_id, from, to)
            .await
            .map_err(unavailable)?;
        let Some(zone) = repositories
            .configuration
            .system(system_id)
            .await
            .map_err(|_| ReportingApiError::Unavailable)?
            .and_then(|system| system.timezone.parse::<chrono_tz::Tz>().ok())
        else {
            return Ok(());
        };
        for day in days.iter().filter(|day| day.basis.is_supplied()) {
            let Some(month_start) = local_month_start(zone, &day.date) else {
                continue;
            };
            let computed = day.computed.as_ref();
            let generation = day.energy.generation_energy_wh.map(|value| {
                value
                    - computed
                        .and_then(|c| c.generation_energy_wh)
                        .unwrap_or_default()
            });
            let consumption = day.energy.consumption_energy_wh.map(|value| {
                value
                    - computed
                        .and_then(|c| c.consumption_energy_wh)
                        .unwrap_or_default()
            });
            let index = match response
                .monthly
                .binary_search_by_key(&month_start, |month| month.bucket_start_epoch_millis)
            {
                Ok(index) => index,
                Err(index) => {
                    response.monthly.insert(
                        index,
                        MonthlyProductionResponse {
                            bucket_start_epoch_millis: month_start,
                            generation_energy_wh: None,
                            consumption_energy_wh: None,
                            coverage_basis_points: 0,
                            supplied_days: 0,
                        },
                    );
                    index
                }
            };
            let month = &mut response.monthly[index];
            month.supplied_days += 1;
            add_delta(&mut month.generation_energy_wh, generation);
            add_delta(&mut month.consumption_energy_wh, consumption);
            add_delta(&mut response.generation_energy_wh, generation);
            add_delta(&mut response.consumption_energy_wh, consumption);
        }
        response.yearly = yearly(zone, &response.monthly);
        Ok(())
    }
}

/// Adds a difference to a total that is absent until something contributes.
fn add_delta(total: &mut Option<i64>, delta: Option<i64>) {
    if let Some(delta) = delta {
        *total = Some(total.unwrap_or_default() + delta);
    }
}

/// First instant of the local month a date belongs to, in epoch milliseconds.
fn local_month_start(zone: chrono_tz::Tz, date: &str) -> Option<i64> {
    use chrono::{Datelike as _, TimeZone as _};
    let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let first = date.with_day(1)?;
    zone.from_local_datetime(&first.and_hms_opt(0, 0, 0)?)
        .earliest()
        // A local midnight skipped by a DST jump starts one hour later.
        .or_else(|| {
            zone.from_local_datetime(&first.and_hms_opt(1, 0, 0)?)
                .earliest()
        })
        .map(|start| start.timestamp_millis())
}

/// Sums the months of each local calendar year.
fn yearly(
    zone: chrono_tz::Tz,
    monthly: &[MonthlyProductionResponse],
) -> Vec<YearlyProductionResponse> {
    use chrono::Datelike as _;
    let mut years: std::collections::BTreeMap<i32, YearlyProductionResponse> =
        std::collections::BTreeMap::new();
    for month in monthly {
        let Some(year) = chrono::DateTime::from_timestamp_millis(month.bucket_start_epoch_millis)
            .map(|start| start.with_timezone(&zone).year())
        else {
            continue;
        };
        let entry = years.entry(year).or_insert(YearlyProductionResponse {
            year,
            generation_energy_wh: None,
            consumption_energy_wh: None,
            supplied_days: 0,
        });
        add_delta(&mut entry.generation_energy_wh, month.generation_energy_wh);
        add_delta(
            &mut entry.consumption_energy_wh,
            month.consumption_energy_wh,
        );
        entry.supplied_days += month.supplied_days;
    }
    years.into_values().collect()
}

fn statistics_response(
    system_id: SystemId,
    lifetime: Option<LifetimeValues>,
    rows: Vec<MonthlyValues>,
) -> StatisticsResponse {
    let (
        generation_energy_wh,
        consumption_energy_wh,
        peak_generation_power_watts,
        first_observation_at_epoch_millis,
        last_observation_at_epoch_millis,
        coverage_basis_points,
    ) = lifetime.map_or((None, None, None, None, None, 0), |values| {
        (
            values.generation_energy_wh,
            values.consumption_energy_wh,
            values.peak_generation_power_watts,
            values.first_observation_at,
            values.last_observation_at,
            values.coverage_basis_points,
        )
    });
    StatisticsResponse {
        system_id,
        generation_energy_wh,
        consumption_energy_wh,
        peak_generation_power_watts,
        first_observation_at_epoch_millis,
        last_observation_at_epoch_millis,
        coverage_basis_points,
        monthly: rows
            .into_iter()
            .map(|row| MonthlyProductionResponse {
                bucket_start_epoch_millis: row.bucket_start,
                generation_energy_wh: row.generation_energy_wh,
                consumption_energy_wh: row.consumption_energy_wh,
                coverage_basis_points: row.coverage_basis_points,
                supplied_days: 0,
            })
            .collect(),
        yearly: Vec::new(),
    }
}

/// Groups daily generation by meteorological season; days without a value are not counted.
#[must_use]
pub fn seasonal_response(system_id: SystemId, days: &[(String, Option<i64>)]) -> SeasonalResponse {
    let mut totals = [
        ("winter", 0_i64, 0_u64),
        ("spring", 0, 0),
        ("summer", 0, 0),
        ("autumn", 0, 0),
    ];
    for (date, energy) in days {
        let Some(energy) = energy else {
            continue;
        };
        let month = date
            .get(5..7)
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or_default();
        let index = match month {
            12 | 1 | 2 => 0,
            3..=5 => 1,
            6..=8 => 2,
            9..=11 => 3,
            _ => continue,
        };
        totals[index].1 += energy;
        totals[index].2 += 1;
    }
    SeasonalResponse {
        system_id,
        seasons: totals
            .into_iter()
            .map(
                |(season, generation_energy_wh, measured_days)| SeasonProductionResponse {
                    season: season.to_owned(),
                    generation_energy_wh,
                    measured_days,
                    average_daily_energy_wh: if measured_days == 0 {
                        0
                    } else {
                        generation_energy_wh / i64::try_from(measured_days).unwrap_or(1)
                    },
                },
            )
            .collect(),
    }
}

fn empty_weather(system_id: SystemId) -> WeatherForecastResponse {
    WeatherForecastResponse {
        system_id,
        issued_at_epoch_millis: None,
        attribution: None,
        points: Vec::new(),
    }
}

struct WeatherValues {
    interval_start: i64,
    interval_end: i64,
    irradiance: Option<i64>,
    ambient_temperature: Option<i64>,
    wind_speed: Option<i64>,
    cloud_cover: Option<u16>,
    predicted_energy: Option<i64>,
}

impl WeatherValues {
    fn from_row<R: Row>(row: &R) -> Self
    where
        for<'c> &'c str: sqlx::ColumnIndex<R>,
        for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    {
        Self {
            interval_start: row.get("interval_start"),
            interval_end: row.get("interval_end"),
            irradiance: row.try_get("irradiance").ok(),
            ambient_temperature: row.try_get("ambient_temperature_millicelsius").ok(),
            wind_speed: row.try_get("wind_speed_millimetres_per_second").ok(),
            cloud_cover: optional_basis_points(row, "cloud_cover_basis_points"),
            predicted_energy: row.try_get("predicted_energy_wh").ok(),
        }
    }
}

fn basis_points<R: Row>(row: &R, column: &str) -> u16
where
    for<'c> &'c str: sqlx::ColumnIndex<R>,
    for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    optional_basis_points(row, column).unwrap_or_default()
}

fn optional_basis_points<R: Row>(row: &R, column: &str) -> Option<u16>
where
    for<'c> &'c str: sqlx::ColumnIndex<R>,
    for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    row.try_get::<i64, _>(column)
        .ok()
        .and_then(|value| u16::try_from(value).ok())
}

fn weather_response(
    system_id: SystemId,
    issued_at: Option<i64>,
    attribution: Option<String>,
    rows: Vec<WeatherValues>,
) -> WeatherForecastResponse {
    WeatherForecastResponse {
        system_id,
        issued_at_epoch_millis: issued_at,
        attribution,
        points: rows
            .into_iter()
            .map(|row| WeatherForecastPointResponse {
                interval_start_epoch_millis: row.interval_start,
                interval_end_epoch_millis: row.interval_end,
                irradiance_watts_per_square_metre: row.irradiance,
                ambient_temperature_millicelsius: row.ambient_temperature,
                wind_speed_millimetres_per_second: row.wind_speed,
                cloud_cover_basis_points: row.cloud_cover,
                predicted_energy_wh: row.predicted_energy,
            })
            .collect(),
    }
}
