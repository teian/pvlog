//! Production reporting adapter backed by account telemetry and forecasting storage.

use async_trait::async_trait;
use pvlog_api::{
    MonthlyProductionResponse, ReportingApiError, ReportingApiUseCases, SeasonProductionResponse,
    SeasonalResponse, StatisticsResponse, SystemOverviewResponse, WeatherForecastPointResponse,
    WeatherForecastResponse,
};
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
                        "SELECT s.name,s.timezone,s.lifecycle,COUNT(DISTINCT i.id) AS inverter_count,\
                         COUNT(DISTINCT p.id) AS string_count,SUM(p.rated_power_watts) AS capacity_watts \
                         FROM systems s LEFT JOIN inverters i ON i.system_id=s.id \
                         LEFT JOIN pv_strings p ON p.inverter_id=i.id WHERE s.id=? GROUP BY s.id,s.name,s.timezone,s.lifecycle",
                    ).bind(id_blob(system_id)).fetch_optional(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    row.map(|row| SystemOverviewResponse {
                        id: system_id,
                        name: row.get("name"),
                        timezone: row.get("timezone"),
                        lifecycle: row.get("lifecycle"),
                        inverter_count: u64::try_from(row.get::<i64, _>("inverter_count"))
                            .unwrap_or_default(),
                        string_count: u64::try_from(row.get::<i64, _>("string_count"))
                            .unwrap_or_default(),
                        capacity_watts: row.try_get("capacity_watts").ok(),
                    })
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
                        "SELECT s.name,s.timezone,s.lifecycle,COUNT(DISTINCT i.id)::BIGINT AS inverter_count,\
                         COUNT(DISTINCT p.id)::BIGINT AS string_count,SUM(p.rated_power_watts)::BIGINT AS capacity_watts \
                         FROM account_data.systems s LEFT JOIN account_data.inverters i ON i.account_id=s.account_id AND i.system_id=s.id \
                         LEFT JOIN account_data.pv_strings p ON p.account_id=i.account_id AND p.inverter_id=i.id \
                         WHERE s.account_id=$1 AND s.id=$2 GROUP BY s.id,s.name,s.timezone,s.lifecycle",
                    ).bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_optional(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    row.map(|row| SystemOverviewResponse {
                        id: system_id,
                        name: row.get("name"),
                        timezone: row.get("timezone"),
                        lifecycle: row.get("lifecycle"),
                        inverter_count: u64::try_from(row.get::<i64, _>("inverter_count"))
                            .unwrap_or_default(),
                        string_count: u64::try_from(row.get::<i64, _>("string_count"))
                            .unwrap_or_default(),
                        capacity_watts: row.try_get("capacity_watts").ok(),
                    })
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
                    let rows = sqlx::query("SELECT bucket_start,generation_energy_sum_wh,consumption_energy_sum_wh,coverage_basis_points FROM telemetry_rollups r WHERE system_id=? AND resolution='month' AND generation=(SELECT MAX(latest.generation) FROM telemetry_rollups latest WHERE latest.system_id=r.system_id AND latest.resolution=r.resolution AND latest.bucket_start=r.bucket_start) ORDER BY bucket_start")
                        .bind(id_blob(system_id)).fetch_all(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    Ok(statistics_response(
                        system_id,
                        lifetime.as_ref().map(|row| LifetimeValues::from_row(row)),
                        rows.iter().map(MonthlyValues::from_row).collect(),
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
                    let lifetime = sqlx::query("SELECT generation_energy_wh,consumption_energy_wh,peak_generation_power_watts,first_observation_at,last_observation_at,coverage_basis_points::BIGINT AS coverage_basis_points FROM telemetry.lifetime_summaries WHERE account_id=$1 AND system_id=$2")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_optional(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    let rows = sqlx::query("SELECT bucket_start,generation_energy_sum_wh,consumption_energy_sum_wh,coverage_basis_points::BIGINT AS coverage_basis_points FROM telemetry.rollups r WHERE account_id=$1 AND system_id=$2 AND resolution='month' AND generation=(SELECT MAX(latest.generation) FROM telemetry.rollups latest WHERE latest.account_id=r.account_id AND latest.system_id=r.system_id AND latest.resolution=r.resolution AND latest.bucket_start=r.bucket_start) ORDER BY bucket_start")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_all(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    Ok(statistics_response(
                        system_id,
                        lifetime.as_ref().map(|row| LifetimeValues::from_row(row)),
                        rows.iter().map(MonthlyValues::from_row).collect(),
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

    async fn seasonal(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SeasonalResponse, ReportingApiError> {
        let days = match &self.target {
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
                    sqlx::query("SELECT local_date,generation_energy_wh FROM system_daily_summaries d WHERE system_id=? AND generation=(SELECT MAX(latest.generation) FROM system_daily_summaries latest WHERE latest.system_id=d.system_id AND latest.local_date=d.local_date) ORDER BY local_date")
                        .bind(id_blob(system_id)).fetch_all(&mut *connection).await.map_err(|_| ReportingApiError::Unavailable)?
                        .into_iter().map(|row| (row.get::<String, _>("local_date"), row.try_get::<i64, _>("generation_energy_wh").ok())).collect::<Vec<_>>()
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    return Err(ReportingApiError::Unavailable);
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    let mut connection = PgConnection::connect(url)
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    let rows = sqlx::query("SELECT local_date::TEXT AS local_date,generation_energy_wh FROM telemetry.daily_summaries d WHERE account_id=$1 AND system_id=$2 AND generation=(SELECT MAX(latest.generation) FROM telemetry.daily_summaries latest WHERE latest.account_id=d.account_id AND latest.system_id=d.system_id AND latest.local_date=d.local_date) ORDER BY local_date")
                        .bind(account_id.as_uuid()).bind(system_id.as_uuid()).fetch_all(&mut connection).await.map_err(|_| ReportingApiError::Unavailable)?;
                    connection
                        .close()
                        .await
                        .map_err(|_| ReportingApiError::Unavailable)?;
                    rows.into_iter()
                        .map(|row| {
                            (
                                row.get::<String, _>("local_date"),
                                row.try_get::<i64, _>("generation_energy_wh").ok(),
                            )
                        })
                        .collect::<Vec<_>>()
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id, system_id);
                    return Err(ReportingApiError::Unavailable);
                }
            }
        };
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

fn statistics_response(
    system_id: SystemId,
    lifetime: Option<LifetimeValues>,
    rows: Vec<MonthlyValues>,
) -> StatisticsResponse {
    StatisticsResponse {
        system_id,
        generation_energy_wh: lifetime
            .as_ref()
            .and_then(|values| values.generation_energy_wh),
        consumption_energy_wh: lifetime
            .as_ref()
            .and_then(|values| values.consumption_energy_wh),
        peak_generation_power_watts: lifetime
            .as_ref()
            .and_then(|values| values.peak_generation_power_watts),
        first_observation_at_epoch_millis: lifetime
            .as_ref()
            .and_then(|values| values.first_observation_at),
        last_observation_at_epoch_millis: lifetime
            .as_ref()
            .and_then(|values| values.last_observation_at),
        coverage_basis_points: lifetime
            .as_ref()
            .map_or(0, |values| values.coverage_basis_points),
        monthly: rows
            .into_iter()
            .map(|row| MonthlyProductionResponse {
                bucket_start_epoch_millis: row.bucket_start,
                generation_energy_wh: row.generation_energy_wh,
                consumption_energy_wh: row.consumption_energy_wh,
                coverage_basis_points: row.coverage_basis_points,
            })
            .collect(),
    }
}

fn seasonal_response(system_id: SystemId, days: &[(String, Option<i64>)]) -> SeasonalResponse {
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

#[cfg(test)]
mod tests {
    use super::seasonal_response;
    use pvlog_domain::SystemId;

    #[test]
    fn seasonal_report_groups_daily_energy_and_ignores_missing_values() {
        let response = seasonal_response(
            SystemId::new(),
            &[
                ("2026-01-10".to_owned(), Some(1_000)),
                ("2026-02-10".to_owned(), Some(3_000)),
                ("2026-04-10".to_owned(), Some(5_000)),
                ("2026-07-10".to_owned(), None),
            ],
        );

        assert_eq!(response.seasons[0].generation_energy_wh, 4_000);
        assert_eq!(response.seasons[0].measured_days, 2);
        assert_eq!(response.seasons[0].average_daily_energy_wh, 2_000);
        assert_eq!(response.seasons[1].generation_energy_wh, 5_000);
        assert_eq!(response.seasons[2].measured_days, 0);
    }
}
