//! Turns stored weather runs into modeled yield for every configured string.
//!
//! The cycle is deterministic and idempotent: the same weather run and the same
//! settings always produce the same numbers, and a run already calculated under
//! the current configuration digest is skipped rather than recomputed.
//!
//! Results are written once and never updated — the schema enforces that — so a
//! configuration change produces a new run rather than rewriting history.

use std::collections::BTreeMap;

use chrono::Utc;
use pvlog_domain::SystemId;
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(feature = "sqlite")]
use sqlx::Row;
use thiserror::Error;
use uuid::Uuid;

use crate::yield_model::{IntervalWeather, LossChain, model_interval};

/// Counts produced by one calculation cycle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct YieldCycleReport {
    pub runs_calculated: u64,
    pub intervals_written: u64,
}

impl YieldCycleReport {
    /// Adds another report's counters to this one.
    fn merge(&mut self, other: Self) {
        self.runs_calculated += other.runs_calculated;
        self.intervals_written += other.intervals_written;
    }
}

/// Models every weather run that has no result under the current settings.
///
/// # Errors
///
/// Returns an error when account storage is unavailable.
pub async fn run_cycle(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
) -> Result<YieldCycleReport, YieldCycleError> {
    let report = YieldCycleReport::default();
    let DatabaseTarget::Sqlite {
        management_path,
        accounts_dir,
    } = target
    else {
        // The PostgreSQL calculation path is not implemented yet.
        return Ok(report);
    };
    #[cfg(not(feature = "sqlite"))]
    {
        let _ = (management_path, accounts_dir, management);
        return Ok(report);
    }
    #[cfg(feature = "sqlite")]
    {
        let mut report = report;
        for account_id in crate::aggregation::active_accounts(management_path).await? {
            let account = pvlog_storage::SqliteAccountPoolRouter::shared(
                management_path.clone(),
                accounts_dir.clone(),
            )
            .map_err(|_| YieldCycleError::Unavailable)?
            .route(account_id)
            .await
            .map_err(|_| YieldCycleError::Unavailable)?;
            for system_id in management
                .systems_for_account(account_id)
                .await
                .map_err(|_| YieldCycleError::Unavailable)?
            {
                report.merge(calculate_system(&account, system_id).await?);
            }
        }
        Ok(report)
    }
}

/// One string's rating and loss chain at calculation time.
#[cfg(feature = "sqlite")]
struct ConfiguredString {
    string_id: Vec<u8>,
    inverter_id: Vec<u8>,
    rated_power_watts: i64,
    losses: LossChain,
    model_identifier: String,
    model_revision: i64,
    configuration_digest: Vec<u8>,
}

/// Models the system's most recent forecast runs that lack a completed calculation under the current configuration, oldest first, one transaction per run.
#[cfg(feature = "sqlite")]
async fn calculate_system(
    account: &pvlog_storage::RoutedSqliteAccount,
    system_id: SystemId,
) -> Result<YieldCycleReport, YieldCycleError> {
    let mut connection = account
        .acquire()
        .await
        .map_err(|_| YieldCycleError::Unavailable)?;
    let system = system_id.as_uuid().as_bytes().to_vec();

    let strings = configured_strings(&mut connection, &system).await?;
    if strings.is_empty() {
        return Ok(YieldCycleReport::default());
    }
    let digest = system_digest(&strings);

    // Only forecast runs without a completed calculation under this digest.
    let runs = sqlx::query(
        "SELECT id,valid_from,valid_to,resolution_seconds FROM weather_data_runs w \
         WHERE w.system_id=? AND w.data_kind='forecast' \
         AND NOT EXISTS (SELECT 1 FROM yield_calculation_runs c \
          WHERE c.weather_run_id=w.id AND c.configuration_digest=? AND c.state='completed') \
         ORDER BY w.fetched_at DESC LIMIT 8",
    )
    .bind(system.clone())
    .bind(digest.clone())
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| YieldCycleError::Unavailable)?;

    let mut report = YieldCycleReport::default();
    // Newest runs were selected first; apply them oldest first so the newest
    // forecast wins the last-write-wins projection.
    for run in runs.iter().rev() {
        let weather_run_id: Vec<u8> = run.get("id");
        // One transaction per run: a failed write must not leave a run recorded
        // as completed with partial results.
        let mut transaction = sqlx::Connection::begin(&mut *connection)
            .await
            .map_err(|_| YieldCycleError::Unavailable)?;
        let written = calculate_run(
            &mut transaction,
            &system,
            system_id,
            &strings,
            &weather_run_id,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|_| YieldCycleError::Unavailable)?;
        report.runs_calculated += 1;
        report.intervals_written += written;
    }
    Ok(report)
}

/// Calculates and stores yield for one weather run across all configured strings, returning the number of intervals written, or zero when the run has no points.
#[cfg(feature = "sqlite")]
async fn calculate_run(
    connection: &mut sqlx::SqliteConnection,
    system: &[u8],
    system_id: SystemId,
    strings: &[ConfiguredString],
    weather_run_id: &[u8],
) -> Result<u64, YieldCycleError> {
    let points = sqlx::query(
        "SELECT interval_start,interval_end,plane_of_array_wm2,global_horizontal_wm2,\
         ambient_temperature_millicelsius FROM weather_data_points \
         WHERE run_id=? ORDER BY interval_start",
    )
    .bind(weather_run_id.to_vec())
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| YieldCycleError::Unavailable)?;
    if points.is_empty() {
        return Ok(0);
    }

    let Some(first) = strings.first() else {
        return Ok(0);
    };
    let digest = system_digest(strings);
    let now = Utc::now().timestamp_millis();
    let calculation_run_id = Uuid::now_v7().as_bytes().to_vec();
    sqlx::query(
        // The idempotency key ties a run to its weather input and configuration,
        // so repeating the cycle cannot produce a second run for the same work.
        "INSERT INTO yield_calculation_runs (id,system_id,weather_run_id,basis,model_identifier,\
         model_revision,configuration_digest,state,requested_at,completed_at,idempotency_key) \
         VALUES (?,?,?,'forecast',?,?,?,'completed',?,?,?)",
    )
    .bind(calculation_run_id.clone())
    .bind(system.to_vec())
    .bind(weather_run_id.to_vec())
    .bind(&first.model_identifier)
    .bind(first.model_revision)
    .bind(digest.clone())
    .bind(now)
    .bind(now)
    .bind(idempotency_key(weather_run_id, &digest))
    .execute(&mut *connection)
    .await
    .map_err(|_| YieldCycleError::Unavailable)?;

    let total_capacity: i64 = strings.iter().map(|entry| entry.rated_power_watts).sum();
    let mut written = 0_u64;
    for point in &points {
        written += model_point(
            connection,
            &calculation_run_id,
            system,
            system_id,
            strings,
            point,
            total_capacity,
            &digest,
            now,
        )
        .await?;
    }
    Ok(written)
}

/// Digest over every configured string, so a change to any string's settings
/// or rating invalidates earlier calculations.
#[cfg(feature = "sqlite")]
fn system_digest(strings: &[ConfiguredString]) -> Vec<u8> {
    let mut entries: Vec<_> = strings
        .iter()
        .map(|entry| {
            (
                &entry.string_id,
                &entry.configuration_digest,
                entry.rated_power_watts,
            )
        })
        .collect();
    entries.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for (string_id, digest, rated) in entries {
        hasher.update(string_id);
        hasher.update(digest);
        hasher.update(&rated.to_be_bytes());
    }
    hasher.finalize().as_bytes().to_vec()
}

/// Stable key for one weather run modeled under one configuration.
#[cfg(feature = "sqlite")]
fn idempotency_key(weather_run_id: &[u8], digest: &[u8]) -> String {
    use std::fmt::Write as _;
    weather_run_id
        .iter()
        .chain(digest.iter())
        .fold(String::with_capacity(96), |mut key, byte| {
            let _ = write!(key, "{byte:02x}");
            key
        })
}

/// Models one weather interval across every scope of the system.
#[cfg(feature = "sqlite")]
#[allow(clippy::too_many_arguments)]
async fn model_point(
    connection: &mut sqlx::SqliteConnection,
    calculation_run_id: &[u8],
    system: &[u8],
    system_id: SystemId,
    strings: &[ConfiguredString],
    point: &sqlx::sqlite::SqliteRow,
    total_capacity: i64,
    digest: &[u8],
    now: i64,
) -> Result<u64, YieldCycleError> {
    let mut written = 0_u64;

    let interval_start: i64 = point.get("interval_start");
    let interval_end: i64 = point.get("interval_end");
    let irradiance = point
        .try_get::<Option<i64>, _>("plane_of_array_wm2")
        .ok()
        .flatten()
        .or_else(|| {
            point
                .try_get::<Option<i64>, _>("global_horizontal_wm2")
                .ok()
                .flatten()
        })
        .unwrap_or_default();
    let weather = IntervalWeather {
        plane_of_array_wm2: irradiance,
        ambient_temperature_milli_c: point
            .try_get::<Option<i64>, _>("ambient_temperature_millicelsius")
            .ok()
            .flatten(),
    };
    let interval_millis = interval_end - interval_start;

    // Strings roll up into their inverter and then into the system, so the
    // three scopes always agree by construction.
    let mut inverters: BTreeMap<Vec<u8>, (i64, i64)> = BTreeMap::new();
    let mut system_power = 0_i64;
    let mut system_energy = 0_i64;
    for entry in strings {
        let modeled = model_interval(
            entry.rated_power_watts,
            entry.losses,
            weather,
            interval_millis,
        );
        write_result(
            connection,
            calculation_run_id,
            system,
            "string",
            &entry.string_id,
            interval_start,
            interval_end,
            modeled.power_watts,
            modeled.energy_wh,
            entry.rated_power_watts,
            entry.rated_power_watts,
            digest,
            now,
            system_id,
        )
        .await?;
        written += 1;
        let bucket = inverters.entry(entry.inverter_id.clone()).or_insert((0, 0));
        bucket.0 += modeled.power_watts;
        bucket.1 += modeled.energy_wh;
        system_power += modeled.power_watts;
        system_energy += modeled.energy_wh;
    }
    for (inverter_id, (power, energy)) in &inverters {
        write_result(
            connection,
            calculation_run_id,
            system,
            "inverter",
            inverter_id,
            interval_start,
            interval_end,
            *power,
            *energy,
            total_capacity,
            total_capacity,
            digest,
            now,
            system_id,
        )
        .await?;
        written += 1;
    }
    write_result(
        connection,
        calculation_run_id,
        system,
        "system",
        system,
        interval_start,
        interval_end,
        system_power,
        system_energy,
        total_capacity,
        total_capacity,
        digest,
        now,
        system_id,
    )
    .await?;
    written += 1;
    Ok(written)
}

/// Writes one immutable result and points the projection at it.
#[cfg(feature = "sqlite")]
#[allow(clippy::too_many_arguments)]
async fn write_result(
    connection: &mut sqlx::SqliteConnection,
    calculation_run_id: &[u8],
    system: &[u8],
    scope_kind: &str,
    scope_id: &[u8],
    interval_start: i64,
    interval_end: i64,
    power_watts: i64,
    energy_wh: i64,
    included_capacity: i64,
    total_capacity: i64,
    digest: &[u8],
    now: i64,
    _system_id: SystemId,
) -> Result<(), YieldCycleError> {
    let result_id = Uuid::now_v7().as_bytes().to_vec();
    sqlx::query(
        "INSERT INTO yield_calculation_results (id,calculation_run_id,system_id,scope_kind,\
         scope_id,interval_start,interval_end,configuration_digest,power_central_watts,\
         energy_central_wh,included_capacity_watts,total_effective_capacity_watts,completeness,\
         uncertainty_known,created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,'complete',0,?)",
    )
    .bind(result_id.clone())
    .bind(calculation_run_id.to_vec())
    .bind(system.to_vec())
    .bind(scope_kind)
    .bind(scope_id.to_vec())
    .bind(interval_start)
    .bind(interval_end)
    .bind(digest.to_vec())
    .bind(power_watts)
    .bind(energy_wh)
    .bind(included_capacity)
    .bind(total_capacity)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(|_| YieldCycleError::Unavailable)?;

    // The projection is the newest result for an interval, so a later run
    // supersedes an earlier one without deleting what it produced.
    sqlx::query(
        "INSERT INTO yield_result_projections (system_id,basis,scope_kind,scope_id,\
         interval_start,result_id,projected_at) VALUES (?,'forecast',?,?,?,?,?) \
         ON CONFLICT(system_id,basis,scope_kind,scope_id,interval_start) DO UPDATE SET \
         result_id=excluded.result_id,projected_at=excluded.projected_at",
    )
    .bind(system.to_vec())
    .bind(scope_kind)
    .bind(scope_id.to_vec())
    .bind(interval_start)
    .bind(result_id)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(|_| YieldCycleError::Unavailable)?;
    Ok(())
}

/// Strings that carry both a rating and effective forecast settings.
#[cfg(feature = "sqlite")]
async fn configured_strings(
    connection: &mut sqlx::SqliteConnection,
    system: &[u8],
) -> Result<Vec<ConfiguredString>, YieldCycleError> {
    let rows = sqlx::query(
        "SELECT p.id AS string_id,p.inverter_id,p.rated_power_watts,f.model_identifier,\
         f.model_revision,f.soiling_loss_basis_points,f.shading_loss_basis_points,\
         f.mismatch_loss_basis_points,f.wiring_loss_basis_points,\
         f.unavailability_loss_basis_points,f.calibration_basis_points,f.configuration_digest \
         FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
         JOIN pv_string_forecast_settings f ON f.string_id=p.id \
         WHERE i.system_id=? AND f.effective_to IS NULL ORDER BY p.id",
    )
    .bind(system.to_vec())
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| YieldCycleError::Unavailable)?;
    Ok(rows
        .iter()
        .map(|row| ConfiguredString {
            string_id: row.get("string_id"),
            inverter_id: row.get("inverter_id"),
            rated_power_watts: row.try_get("rated_power_watts").unwrap_or_default(),
            losses: LossChain {
                soiling: loss(row, "soiling_loss_basis_points"),
                shading: loss(row, "shading_loss_basis_points"),
                mismatch: loss(row, "mismatch_loss_basis_points"),
                wiring: loss(row, "wiring_loss_basis_points"),
                unavailability: loss(row, "unavailability_loss_basis_points"),
                calibration: i32::try_from(
                    row.try_get::<i64, _>("calibration_basis_points")
                        .unwrap_or_default(),
                )
                .unwrap_or_default(),
            },
            model_identifier: row.get("model_identifier"),
            model_revision: row.try_get("model_revision").unwrap_or(1),
            configuration_digest: row.get("configuration_digest"),
        })
        .collect())
}

/// Reads a loss column as basis points, defaulting to zero when missing or out of range.
#[cfg(feature = "sqlite")]
fn loss(row: &sqlx::sqlite::SqliteRow, column: &str) -> u16 {
    u16::try_from(row.try_get::<i64, _>(column).unwrap_or_default()).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum YieldCycleError {
    #[error("yield calculation storage is unavailable")]
    Unavailable,
}

impl From<crate::aggregation::AggregationError> for YieldCycleError {
    /// Collapses any aggregation error into the unavailable error.
    fn from(_: crate::aggregation::AggregationError) -> Self {
        Self::Unavailable
    }
}
