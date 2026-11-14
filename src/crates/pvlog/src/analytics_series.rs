//! Storage execution of planned telemetry series reads.
//!
//! The planner decides *what* to read; this module performs the physical read
//! for one account database and shapes it into canonical series points.
//!
//! Aggregate resolutions store energy sums rather than power samples, so a
//! power series over a rollup is the mean power implied by the bucket's energy
//! and its width. Fields sourced from the yield-forecasting projections are
//! returned as empty series until that pipeline is wired.
//!
//! Below the system, readings come from the channels bound to the requested
//! equipment. A system-scoped read is the sum of its strings whenever any string
//! is metered, and falls back to the plant-wide observation columns otherwise —
//! matching the rule that an aggregate view is the true sum of its members.

use async_trait::async_trait;
use pvlog_application::{
    GapKind, PlannedSeries, QueryPlan, QueryResolution, QuerySource, SeriesField, SeriesGap,
    SeriesPoint, SeriesQueryRepository, SeriesQueryRepositoryError, SeriesUnit,
};
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::DatabaseTarget;
#[cfg(feature = "postgres")]
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::Row;

const MILLIS_PER_HOUR: i64 = 3_600_000;
const FULL_COVERAGE: u16 = 10_000;

/// Equipment level a series describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesScope {
    System,
    Inverter(uuid::Uuid),
    String(uuid::Uuid),
}

impl SeriesScope {
    /// Parses the wire form, defaulting to the system when nothing is requested.
    #[must_use]
    pub fn parse(kind: Option<&str>, scope_id: Option<uuid::Uuid>) -> Option<Self> {
        match (kind.unwrap_or("system"), scope_id) {
            ("system", _) => Some(Self::System),
            ("inverter", Some(id)) => Some(Self::Inverter(id)),
            ("string", Some(id)) => Some(Self::String(id)),
            _ => None,
        }
    }

    /// Wire name of the scope kind as stored in the rollup `scope_kind` column.
    const fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Inverter(_) => "inverter",
            Self::String(_) => "string",
        }
    }

    /// Identifier stored alongside the scope; the system carries its own.
    fn identifier(self, system_id: SystemId) -> uuid::Uuid {
        match self {
            Self::System => system_id.as_uuid(),
            Self::Inverter(id) | Self::String(id) => id,
        }
    }
}

/// Executes planned reads against the account database that owns a system.
pub struct AccountSeriesRepository {
    target: DatabaseTarget,
    account_id: AccountId,
    scope: SeriesScope,
    start_epoch_millis: i64,
    end_epoch_millis: i64,
}

impl AccountSeriesRepository {
    /// Creates a repository reading one account database for the given scope and half-open time range.
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        account_id: AccountId,
        scope: SeriesScope,
        start_epoch_millis: i64,
        end_epoch_millis: i64,
    ) -> Self {
        Self {
            target,
            account_id,
            scope,
            start_epoch_millis,
            end_epoch_millis,
        }
    }
}

#[async_trait]
impl SeriesQueryRepository for AccountSeriesRepository {
    /// Loads the rows the plan selects (raw or rollup) and builds one series per requested field.
    async fn execute_plan(
        &self,
        system_id: SystemId,
        plan: &QueryPlan,
    ) -> Result<Vec<PlannedSeries>, SeriesQueryRepositoryError> {
        let rows = match &plan.source {
            QuerySource::Raw(_) => self.raw_rows(system_id).await?,
            QuerySource::Rollup(resolution) => self.rollup_rows(system_id, *resolution).await?,
        };
        Ok(plan
            .fields
            .iter()
            .map(|field| series_for(*field, &rows))
            .collect())
    }
}

/// One canonical sample after storage-specific column mapping.
#[derive(Clone, Debug, Default)]
struct SampleRow {
    timestamp_epoch_millis: i64,
    generation_power_watts: Option<i64>,
    generation_energy_wh: Option<i64>,
    consumption_power_watts: Option<i64>,
    consumption_energy_wh: Option<i64>,
    grid_power_watts: Option<i64>,
    battery_power_watts: Option<i64>,
    battery_basis_points: Option<i64>,
    temperature_milli_celsius: Option<i64>,
    quality_flags: u32,
    coverage_basis_points: u16,
    provenance: Option<String>,
}

/// Rollup columns after backend-specific extraction.
struct RollupValues {
    bucket_start: i64,
    bucket_end: i64,
    generation_energy_wh: Option<i64>,
    consumption_energy_wh: Option<i64>,
    grid_import_energy_wh: Option<i64>,
    grid_export_energy_wh: Option<i64>,
    battery_energy_delta_wh: Option<i64>,
    temperature_max_milli_celsius: Option<i64>,
    quality_flags: i64,
    coverage_basis_points: i64,
}

impl AccountSeriesRepository {
    /// Loads raw samples for the system from the configured backend.
    async fn raw_rows(
        &self,
        system_id: SystemId,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                self.sqlite_raw(management_path, accounts_dir, system_id)
                    .await
            }
            DatabaseTarget::Postgres { url } => self.postgres_raw(url, system_id).await,
        }
    }

    /// Loads rollup samples at the given resolution from the configured backend.
    async fn rollup_rows(
        &self,
        system_id: SystemId,
        resolution: QueryResolution,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                self.sqlite_rollups(management_path, accounts_dir, system_id, resolution)
                    .await
            }
            DatabaseTarget::Postgres { url } => {
                self.postgres_rollups(url, system_id, resolution).await
            }
        }
    }

    /// Sums the power channels bound to the requested equipment, one row per instant.
    ///
    /// Only channels in watts that are averaged over a period carry DC power.
    /// A string's voltage or current channel is bound to it as well and must
    /// not enter the sum.
    ///
    /// Returns `None` when the scope is the system and no string is metered, so
    /// the caller can fall back to the plant-wide observation columns.
    ///
    /// A system-scoped read sums only string channels; an inverter-level channel
    /// is deliberately excluded so the same energy is never counted twice.
    #[cfg(feature = "sqlite")]
    async fn sqlite_channel_rows(
        &self,
        connection: &mut sqlx::SqliteConnection,
        system_id: SystemId,
    ) -> Result<Option<Vec<SampleRow>>, SeriesQueryRepositoryError> {
        let system = id_blob(system_id);
        let rows = match self.scope {
            SeriesScope::System => {
                sqlx::query(
                    "SELECT h.measured_at AS measured_at,SUM(v.integer_value) AS total,\
                 MAX(h.quality_flags) AS quality_flags FROM telemetry_hot_extended_values v \
                 JOIN telemetry_hot h ON h.observation_id=v.observation_id \
                 JOIN channel_definitions c ON c.id=v.channel_id \
                 WHERE h.system_id=? AND h.measured_at>=? AND h.measured_at<? \
                 AND c.lifecycle='active' AND c.target_kind='string' AND c.unit='W' AND c.aggregation='mean' \
                 GROUP BY h.measured_at ORDER BY h.measured_at",
                )
                .bind(system)
                .bind(self.start_epoch_millis)
                .bind(self.end_epoch_millis)
                .fetch_all(&mut *connection)
                .await
            }
            SeriesScope::Inverter(inverter) => {
                sqlx::query(
                    "SELECT h.measured_at AS measured_at,SUM(v.integer_value) AS total,\
                 MAX(h.quality_flags) AS quality_flags FROM telemetry_hot_extended_values v \
                 JOIN telemetry_hot h ON h.observation_id=v.observation_id \
                 JOIN channel_definitions c ON c.id=v.channel_id \
                 WHERE h.system_id=? AND h.measured_at>=? AND h.measured_at<? \
                 AND c.lifecycle='active' AND c.target_kind='string' AND c.unit='W' AND c.aggregation='mean' \
                 AND c.target_id IN (SELECT p.id FROM pv_strings p WHERE p.inverter_id=?) \
                 GROUP BY h.measured_at ORDER BY h.measured_at",
                )
                .bind(system)
                .bind(self.start_epoch_millis)
                .bind(self.end_epoch_millis)
                .bind(inverter.as_bytes().to_vec())
                .fetch_all(&mut *connection)
                .await
            }
            SeriesScope::String(string_id) => {
                sqlx::query(
                    "SELECT h.measured_at AS measured_at,SUM(v.integer_value) AS total,\
                 MAX(h.quality_flags) AS quality_flags FROM telemetry_hot_extended_values v \
                 JOIN telemetry_hot h ON h.observation_id=v.observation_id \
                 JOIN channel_definitions c ON c.id=v.channel_id \
                 WHERE h.system_id=? AND h.measured_at>=? AND h.measured_at<? \
                 AND c.lifecycle='active' AND c.target_kind='string' AND c.unit='W' AND c.aggregation='mean' AND c.target_id=? \
                 GROUP BY h.measured_at ORDER BY h.measured_at",
                )
                .bind(system)
                .bind(self.start_epoch_millis)
                .bind(self.end_epoch_millis)
                .bind(string_id.as_bytes().to_vec())
                .fetch_all(&mut *connection)
                .await
            }
        }
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        if rows.is_empty() && self.scope == SeriesScope::System {
            return Ok(None);
        }
        Ok(Some(
            rows.iter()
                .map(|row| SampleRow {
                    timestamp_epoch_millis: row.get("measured_at"),
                    generation_power_watts: optional(row.try_get("total")),
                    quality_flags: narrow_flags(row.try_get("quality_flags").ok()),
                    coverage_basis_points: FULL_COVERAGE,
                    ..SampleRow::default()
                })
                .collect(),
        ))
    }

    /// Reads raw `SQLite` samples, preferring metered channel sums for inverter and string scopes and overlaying them onto plant rows for the system scope.
    #[cfg(feature = "sqlite")]
    async fn sqlite_raw(
        &self,
        management_path: &std::path::Path,
        accounts_dir: &std::path::Path,
        system_id: SystemId,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        let account = self.sqlite_account(management_path, accounts_dir).await?;
        let mut connection = account
            .acquire()
            .await
            .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        let channel_rows = self.sqlite_channel_rows(&mut connection, system_id).await?;
        if let Some(rows) = channel_rows.as_ref()
            && self.scope != SeriesScope::System
        {
            return Ok(rows.clone());
        }
        let rows = sqlx::query(
            "SELECT measured_at,generation_power_watts,generation_energy_wh,\
             consumption_power_watts,consumption_energy_wh,net_grid_power_watts,\
             battery_power_watts,battery_state_basis_points,temperature_millidegrees_c,\
             quality_flags,source_identity FROM telemetry_hot \
             WHERE system_id=? AND measured_at>=? AND measured_at<? \
             ORDER BY measured_at,observation_id",
        )
        .bind(id_blob(system_id))
        .bind(self.start_epoch_millis)
        .bind(self.end_epoch_millis)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        let plant: Vec<SampleRow> = rows
            .iter()
            .map(|row| SampleRow {
                timestamp_epoch_millis: row.get("measured_at"),
                generation_power_watts: optional(row.try_get("generation_power_watts")),
                generation_energy_wh: optional(row.try_get("generation_energy_wh")),
                consumption_power_watts: optional(row.try_get("consumption_power_watts")),
                consumption_energy_wh: optional(row.try_get("consumption_energy_wh")),
                grid_power_watts: optional(row.try_get("net_grid_power_watts")),
                battery_power_watts: optional(row.try_get("battery_power_watts")),
                battery_basis_points: optional(row.try_get("battery_state_basis_points")),
                temperature_milli_celsius: optional(row.try_get("temperature_millidegrees_c")),
                quality_flags: narrow_flags(row.try_get("quality_flags").ok()),
                coverage_basis_points: FULL_COVERAGE,
                provenance: row.try_get("source_identity").ok(),
            })
            .collect();
        Ok(match channel_rows {
            Some(channels) => merge_channel_generation(plant, channels),
            None => plant,
        })
    }

    /// Fallback when `SQLite` support is compiled out; always reports storage as unavailable.
    #[cfg(not(feature = "sqlite"))]
    async fn sqlite_raw(
        &self,
        _management_path: &std::path::Path,
        _accounts_dir: &std::path::Path,
        _system_id: SystemId,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        Err(SeriesQueryRepositoryError::Unavailable)
    }

    /// Reads raw `PostgreSQL` hot observations for the system within the requested range.
    #[cfg(feature = "postgres")]
    async fn postgres_raw(
        &self,
        url: &str,
        system_id: SystemId,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        let mut connection = pvlog_storage::postgres_connection(url)
            .await
            .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        let rows = sqlx::query(
            "SELECT measured_at,generation_power_watts,generation_energy_wh,\
             consumption_power_watts,consumption_energy_wh,quality_flags,source_identity \
             FROM telemetry.hot_observations \
             WHERE account_id=$1 AND system_id=$2 AND measured_at>=$3 AND measured_at<$4 \
             ORDER BY measured_at,observation_id",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(self.start_epoch_millis)
        .bind(self.end_epoch_millis)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        connection
            .close()
            .await
            .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        Ok(rows
            .iter()
            .map(|row| SampleRow {
                timestamp_epoch_millis: row.get("measured_at"),
                generation_power_watts: optional(row.try_get("generation_power_watts")),
                generation_energy_wh: optional(row.try_get("generation_energy_wh")),
                consumption_power_watts: optional(row.try_get("consumption_power_watts")),
                consumption_energy_wh: optional(row.try_get("consumption_energy_wh")),
                quality_flags: narrow_flags(row.try_get("quality_flags").ok()),
                coverage_basis_points: FULL_COVERAGE,
                provenance: row.try_get("source_identity").ok(),
                ..SampleRow::default()
            })
            .collect())
    }

    /// Fallback when `PostgreSQL` support is compiled out; always reports storage as unavailable.
    #[cfg(not(feature = "postgres"))]
    async fn postgres_raw(
        &self,
        _url: &str,
        _system_id: SystemId,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        Err(SeriesQueryRepositoryError::Unavailable)
    }

    /// Reads the latest-generation `SQLite` rollup buckets for the scope and resolution.
    #[cfg(feature = "sqlite")]
    async fn sqlite_rollups(
        &self,
        management_path: &std::path::Path,
        accounts_dir: &std::path::Path,
        system_id: SystemId,
        resolution: QueryResolution,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        let account = self.sqlite_account(management_path, accounts_dir).await?;
        let mut connection = account
            .acquire()
            .await
            .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        let rows = sqlx::query(
            "SELECT bucket_start,bucket_end,generation_energy_sum_wh,consumption_energy_sum_wh,\
             grid_import_energy_sum_wh,grid_export_energy_sum_wh,battery_energy_delta_wh,\
             temperature_max_millidegrees_c,quality_flags,coverage_basis_points \
             FROM telemetry_rollups r WHERE system_id=? AND scope_kind=? AND scope_id=? \
             AND resolution=? AND bucket_start>=? AND bucket_start<? \
             AND generation=(SELECT MAX(latest.generation) FROM telemetry_rollups latest \
             WHERE latest.system_id=r.system_id AND latest.scope_kind=r.scope_kind \
             AND latest.scope_id=r.scope_id AND latest.resolution=r.resolution \
             AND latest.bucket_start=r.bucket_start) ORDER BY bucket_start",
        )
        .bind(id_blob(system_id))
        .bind(self.scope.name())
        .bind(self.scope.identifier(system_id).as_bytes().to_vec())
        .bind(resolution_name(resolution))
        .bind(self.start_epoch_millis)
        .bind(self.end_epoch_millis)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        Ok(rows
            .iter()
            .map(|row| {
                rollup_sample(&RollupValues {
                    bucket_start: row.get("bucket_start"),
                    bucket_end: row.get("bucket_end"),
                    generation_energy_wh: optional(row.try_get("generation_energy_sum_wh")),
                    consumption_energy_wh: optional(row.try_get("consumption_energy_sum_wh")),
                    grid_import_energy_wh: optional(row.try_get("grid_import_energy_sum_wh")),
                    grid_export_energy_wh: optional(row.try_get("grid_export_energy_sum_wh")),
                    battery_energy_delta_wh: optional(row.try_get("battery_energy_delta_wh")),
                    temperature_max_milli_celsius: optional(
                        row.try_get("temperature_max_millidegrees_c"),
                    ),
                    quality_flags: row.try_get("quality_flags").unwrap_or_default(),
                    coverage_basis_points: row.try_get("coverage_basis_points").unwrap_or_default(),
                })
            })
            .collect())
    }

    /// Fallback when `SQLite` support is compiled out; always reports storage as unavailable.
    #[cfg(not(feature = "sqlite"))]
    async fn sqlite_rollups(
        &self,
        _management_path: &std::path::Path,
        _accounts_dir: &std::path::Path,
        _system_id: SystemId,
        _resolution: QueryResolution,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        Err(SeriesQueryRepositoryError::Unavailable)
    }

    /// Reads the latest-generation `PostgreSQL` rollup buckets for the scope and resolution.
    #[cfg(feature = "postgres")]
    async fn postgres_rollups(
        &self,
        url: &str,
        system_id: SystemId,
        resolution: QueryResolution,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        let mut connection = pvlog_storage::postgres_connection(url)
            .await
            .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        let rows = sqlx::query(
            "SELECT bucket_start,bucket_end,generation_energy_sum_wh,consumption_energy_sum_wh,\
             grid_import_energy_sum_wh,grid_export_energy_sum_wh,battery_energy_delta_wh,\
             temperature_max_millidegrees_c,quality_flags::BIGINT AS quality_flags,\
             coverage_basis_points::BIGINT AS coverage_basis_points \
             FROM telemetry.rollups r WHERE account_id=$1 AND system_id=$2 \
             AND scope_kind=$3 AND scope_id=$4 AND resolution=$5 \
             AND bucket_start>=$6 AND bucket_start<$7 \
             AND generation=(SELECT MAX(latest.generation) FROM telemetry.rollups latest \
             WHERE latest.account_id=r.account_id AND latest.system_id=r.system_id \
             AND latest.scope_kind=r.scope_kind AND latest.scope_id=r.scope_id \
             AND latest.resolution=r.resolution AND latest.bucket_start=r.bucket_start) \
             ORDER BY bucket_start",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(self.scope.name())
        .bind(self.scope.identifier(system_id))
        .bind(resolution_name(resolution))
        .bind(self.start_epoch_millis)
        .bind(self.end_epoch_millis)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        connection
            .close()
            .await
            .map_err(|_| SeriesQueryRepositoryError::Unavailable)?;
        Ok(rows
            .iter()
            .map(|row| {
                rollup_sample(&RollupValues {
                    bucket_start: row.get("bucket_start"),
                    bucket_end: row.get("bucket_end"),
                    generation_energy_wh: optional(row.try_get("generation_energy_sum_wh")),
                    consumption_energy_wh: optional(row.try_get("consumption_energy_sum_wh")),
                    grid_import_energy_wh: optional(row.try_get("grid_import_energy_sum_wh")),
                    grid_export_energy_wh: optional(row.try_get("grid_export_energy_sum_wh")),
                    battery_energy_delta_wh: optional(row.try_get("battery_energy_delta_wh")),
                    temperature_max_milli_celsius: optional(
                        row.try_get("temperature_max_millidegrees_c"),
                    ),
                    quality_flags: row.try_get("quality_flags").unwrap_or_default(),
                    coverage_basis_points: row.try_get("coverage_basis_points").unwrap_or_default(),
                })
            })
            .collect())
    }

    /// Fallback when `PostgreSQL` support is compiled out; always reports storage as unavailable.
    #[cfg(not(feature = "postgres"))]
    async fn postgres_rollups(
        &self,
        _url: &str,
        _system_id: SystemId,
        _resolution: QueryResolution,
    ) -> Result<Vec<SampleRow>, SeriesQueryRepositoryError> {
        Err(SeriesQueryRepositoryError::Unavailable)
    }

    /// Routes to the `SQLite` database of the owning account, mapping any routing failure to unavailable.
    #[cfg(feature = "sqlite")]
    async fn sqlite_account(
        &self,
        management_path: &std::path::Path,
        accounts_dir: &std::path::Path,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, SeriesQueryRepositoryError> {
        pvlog_storage::SqliteAccountPoolRouter::shared(
            management_path.to_path_buf(),
            accounts_dir.to_path_buf(),
        )
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)?
        .route(self.account_id)
        .await
        .map_err(|_| SeriesQueryRepositoryError::Unavailable)
    }
}

/// Converts a rollup bucket into a canonical sample, deriving mean power from energy over the bucket width.
fn rollup_sample(values: &RollupValues) -> SampleRow {
    let width = (values.bucket_end - values.bucket_start).max(1);
    let grid = match (values.grid_import_energy_wh, values.grid_export_energy_wh) {
        (None, None) => None,
        (import, export) => Some(mean_watts(
            export.unwrap_or_default() - import.unwrap_or_default(),
            width,
        )),
    };
    SampleRow {
        timestamp_epoch_millis: values.bucket_start,
        generation_power_watts: values
            .generation_energy_wh
            .map(|energy| mean_watts(energy, width)),
        generation_energy_wh: values.generation_energy_wh,
        consumption_power_watts: values
            .consumption_energy_wh
            .map(|energy| mean_watts(energy, width)),
        consumption_energy_wh: values.consumption_energy_wh,
        grid_power_watts: grid,
        battery_power_watts: values
            .battery_energy_delta_wh
            .map(|energy| mean_watts(energy, width)),
        battery_basis_points: None,
        temperature_milli_celsius: values.temperature_max_milli_celsius,
        quality_flags: narrow_flags(Some(values.quality_flags)),
        coverage_basis_points: u16::try_from(values.coverage_basis_points)
            .unwrap_or_default()
            .min(FULL_COVERAGE),
        provenance: None,
    }
}

/// Treats a decode failure and a stored NULL alike: the measurement is absent.
fn optional<E>(decoded: Result<Option<i64>, E>) -> Option<i64> {
    decoded.ok().flatten()
}

/// Narrows stored quality flags to `u32`, treating missing or out-of-range values as no flags.
fn narrow_flags(stored: Option<i64>) -> u32 {
    stored
        .and_then(|flags| u32::try_from(flags).ok())
        .unwrap_or_default()
}

/// Mean power implied by an energy sum spread evenly across a bucket.
fn mean_watts(energy_wh: i64, width_millis: i64) -> i64 {
    energy_wh
        .checked_mul(MILLIS_PER_HOUR)
        .map_or(0, |scaled| scaled / width_millis)
}

/// Storage name of a query resolution; raw falls back to the fifteen-minute rollup.
const fn resolution_name(resolution: QueryResolution) -> &'static str {
    match resolution {
        QueryResolution::Raw | QueryResolution::FifteenMinutes => "fifteen_minute",
        QueryResolution::Hourly => "hour",
        QueryResolution::Daily => "day",
        QueryResolution::Monthly => "month",
        QueryResolution::Yearly => "year",
    }
}

/// Unit in which a series field is reported.
const fn unit_for(field: SeriesField) -> SeriesUnit {
    match field {
        SeriesField::GenerationPower
        | SeriesField::ForecastPower
        | SeriesField::ConsumptionPower
        | SeriesField::GridPower
        | SeriesField::BatteryPower => SeriesUnit::Watts,
        SeriesField::GenerationEnergy
        | SeriesField::ForecastEnergy
        | SeriesField::ExpectedEnergy
        | SeriesField::ConsumptionEnergy => SeriesUnit::WattHours,
        SeriesField::BatteryStateOfCharge
        | SeriesField::GenerationPerformance
        | SeriesField::ForecastRealization => SeriesUnit::BasisPoints,
        SeriesField::Temperature => SeriesUnit::MilliDegreesCelsius,
        SeriesField::Extended | SeriesField::Provenance => SeriesUnit::Integer,
    }
}

/// Picks the sample value backing a field, or `None` when the field has no stored series.
fn value_for(field: SeriesField, row: &SampleRow) -> Option<i64> {
    match field {
        SeriesField::GenerationPower => row.generation_power_watts,
        SeriesField::GenerationEnergy => row.generation_energy_wh,
        SeriesField::ConsumptionPower => row.consumption_power_watts,
        SeriesField::ConsumptionEnergy => row.consumption_energy_wh,
        SeriesField::GridPower => row.grid_power_watts,
        SeriesField::BatteryPower => row.battery_power_watts,
        SeriesField::BatteryStateOfCharge => row.battery_basis_points,
        SeriesField::Temperature => row.temperature_milli_celsius,
        SeriesField::Provenance => row.provenance.as_ref().map(|_| 1),
        // Modeled fields and extended channels have no stored series yet.
        SeriesField::ForecastPower
        | SeriesField::ForecastEnergy
        | SeriesField::ExpectedEnergy
        | SeriesField::GenerationPerformance
        | SeriesField::ForecastRealization
        | SeriesField::Extended => None,
    }
}

/// Builds the planned series for one field from the samples, skipping samples without a value.
fn series_for(field: SeriesField, rows: &[SampleRow]) -> PlannedSeries {
    let points = rows
        .iter()
        .filter_map(|row| {
            value_for(field, row).map(|value| SeriesPoint {
                timestamp_epoch_millis: row.timestamp_epoch_millis,
                value,
                coverage_basis_points: row.coverage_basis_points,
                quality_flags: row.quality_flags,
                provenance: if field == SeriesField::Provenance {
                    row.provenance.clone()
                } else {
                    None
                },
            })
        })
        .collect();
    PlannedSeries {
        field,
        unit: unit_for(field),
        points,
        gaps: gaps_for(rows),
    }
}

/// Reports every bucket that storage recorded as incompletely covered.
fn gaps_for(rows: &[SampleRow]) -> Vec<SeriesGap> {
    rows.windows(2)
        .filter(|pair| pair[0].coverage_basis_points < FULL_COVERAGE)
        .map(|pair| SeriesGap {
            start_epoch_millis: pair[0].timestamp_epoch_millis,
            end_epoch_millis: pair[1].timestamp_epoch_millis,
            kind: if pair[0].coverage_basis_points == 0 {
                GapKind::Missing
            } else {
                GapKind::IncompleteCoverage
            },
        })
        .collect()
}

/// Encodes a system identifier as the 16-byte blob used by `SQLite` keys.
#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}

/// Overlays metered per-string generation onto plant-wide raw samples.
///
/// Once a string is metered its channel sum is the system's generation power,
/// but consumption, grid, battery, and temperature only exist on the plant row,
/// so those fields must survive instead of being replaced by the channel rows.
fn merge_channel_generation(plant: Vec<SampleRow>, channels: Vec<SampleRow>) -> Vec<SampleRow> {
    let mut by_time: std::collections::BTreeMap<i64, SampleRow> = plant
        .into_iter()
        .map(|row| (row.timestamp_epoch_millis, row))
        .collect();
    for channel in channels {
        by_time
            .entry(channel.timestamp_epoch_millis)
            .and_modify(|row| row.generation_power_watts = channel.generation_power_watts)
            .or_insert(channel);
    }
    by_time.into_values().collect()
}
