//! Storage-backed forecast settings, runs, and modeled yield series.
//!
//! Settings are stored per PV string, because that is the level at which
//! orientation, tilt, and the loss chain actually differ. A scope above a
//! string therefore reads and writes every string beneath it, and reports
//! partial readiness when only some of them are configured.
//!
//! Modeled values are never computed here: this module reads what the worker's
//! calculation cycle wrote, so an API read and a background rebuild cannot
//! disagree about the same interval.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    ForecastApiError, ForecastApiUseCases, ForecastFreshness, ForecastInputCompletenessResponse,
    ForecastLossInput, ForecastProvenanceResponse, ForecastResourceScope, ForecastRunQuery,
    ForecastRunResponse, ForecastSettingsInput, ForecastSettingsResponse, PerformancePointResponse,
    PerformanceQuery, PerformanceSeriesResponse, YieldSeriesPointResponse, YieldSeriesQuery,
    YieldSeriesResponse,
};
use pvlog_application::Clock;
use pvlog_domain::{
    AccountId, CalculationBasis, ForecastCompleteness, ForecastCompletenessReason, SystemId,
    UserId, WeatherDataKind, WeatherDataRunId, YieldCalculationRunId,
};
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(feature = "sqlite")]
use sqlx::Row;
use uuid::Uuid;

/// Serves the forecast endpoints from account forecasting storage.
pub struct StorageForecastApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
    /// Age beyond which a weather run is reported as stale rather than fresh.
    stale_after_millis: i64,
}

impl StorageForecastApi {
    /// Creates the forecast API over the given storage target, management registry and clock; results older than `stale_after_millis` are reported as stale.
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
        stale_after_millis: i64,
    ) -> Self {
        Self {
            target,
            management,
            clock,
            stale_after_millis,
        }
    }

    /// Returns the current clock time as epoch milliseconds, or zero if it does not fit in an `i64`.
    fn now(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }

    /// Resolves the system a scope refers to; the account scope has none.
    const fn system_of(scope: &ForecastResourceScope) -> Result<SystemId, ForecastApiError> {
        match scope {
            ForecastResourceScope::Account { .. } => Err(ForecastApiError::InvalidPath),
            ForecastResourceScope::System { system_id, .. }
            | ForecastResourceScope::Inverter { system_id, .. }
            | ForecastResourceScope::String { system_id, .. } => Ok(*system_id),
        }
    }

    /// Looks up the account that owns a system, failing with `NotFound` for unknown systems and `Unavailable` when the registry cannot be read.
    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, ForecastApiError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| ForecastApiError::Unavailable)?
            .map(|record| record.account_id)
            .ok_or(ForecastApiError::NotFound)
    }

    /// Opens the routed `SQLite` account database for a system, failing with `Unavailable` when the target is not `SQLite` or routing fails.
    #[cfg(feature = "sqlite")]
    async fn account(
        &self,
        system_id: SystemId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, ForecastApiError> {
        let DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } = &self.target
        else {
            return Err(ForecastApiError::Unavailable);
        };
        let account_id = self.account_for(system_id).await?;
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.clone(),
            accounts_dir.clone(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| ForecastApiError::Unavailable)?
        .route(account_id)
        .await
        .map_err(|_| ForecastApiError::Unavailable)
    }

    /// Classifies a fetch timestamp as fresh or stale against the configured staleness window.
    fn freshness(&self, fetched_at: i64) -> ForecastFreshness {
        if self.now().saturating_sub(fetched_at) <= self.stale_after_millis {
            ForecastFreshness::Fresh
        } else {
            ForecastFreshness::Stale
        }
    }
}

#[async_trait]
impl ForecastApiUseCases for StorageForecastApi {
    /// Returns the newest forecast settings for the scope's strings, or `NotFound` when none are stored.
    async fn settings(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastSettingsResponse, ForecastApiError> {
        #[cfg(feature = "sqlite")]
        {
            let system_id = Self::system_of(&scope)?;
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            let strings = scope_strings(&mut connection, &scope).await?;
            let row = newest_settings(&mut connection, &strings).await?;
            let Some(row) = row else {
                return Err(ForecastApiError::NotFound);
            };
            return Ok(ForecastSettingsResponse {
                scope,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                model_identifier: row.model_identifier,
                model_revision: row.model_revision,
                losses: row.losses,
                calibration_basis_points: row.calibration_basis_points,
                version: 1,
            });
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = scope;
            Err(ForecastApiError::Unavailable)
        }
    }

    /// Validates the input and writes it as a new settings period for every string in the scope, closing the open period and rejecting backdated periods, then returns the stored settings.
    async fn update_settings(
        &self,
        actor: UserId,
        scope: ForecastResourceScope,
        _expected_version: u64,
        input: ForecastSettingsInput,
    ) -> Result<ForecastSettingsResponse, ForecastApiError> {
        validate_settings(&input)?;
        #[cfg(feature = "sqlite")]
        {
            let system_id = Self::system_of(&scope)?;
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            let strings = scope_strings(&mut connection, &scope).await?;
            if strings.is_empty() {
                return Err(ForecastApiError::NotFound);
            }
            let now = self.now();
            let digest = configuration_digest(&input);
            let mut transaction = sqlx::Connection::begin(&mut *connection)
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            for string_id in &strings {
                // A backdated period would leave two open rows for the string.
                let later: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM pv_string_forecast_settings \
                     WHERE string_id=? AND effective_from>? LIMIT 1",
                )
                .bind(string_id.as_bytes().to_vec())
                .bind(input.effective_from)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
                if later.is_some() {
                    return Err(ForecastApiError::Validation(
                        "effective_from",
                        "must not be earlier than an existing settings period",
                    ));
                }
                // Settings are versioned by effective range, so a change closes
                // the open period rather than rewriting recorded history.
                sqlx::query(
                    "UPDATE pv_string_forecast_settings SET effective_to=? \
                     WHERE string_id=? AND effective_to IS NULL AND effective_from<?",
                )
                .bind(input.effective_from)
                .bind(string_id.as_bytes().to_vec())
                .bind(input.effective_from)
                .execute(&mut *transaction)
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
                sqlx::query(
                    "INSERT INTO pv_string_forecast_settings \
                     (id,system_id,string_id,effective_from,effective_to,model_identifier,\
                      model_revision,soiling_loss_basis_points,shading_loss_basis_points,\
                      mismatch_loss_basis_points,wiring_loss_basis_points,\
                      unavailability_loss_basis_points,calibration_basis_points,\
                      configuration_digest,created_at,created_by) \
                     VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
                     ON CONFLICT(string_id,effective_from) DO UPDATE SET \
                     effective_to=excluded.effective_to,\
                     model_identifier=excluded.model_identifier,\
                     model_revision=excluded.model_revision,\
                     soiling_loss_basis_points=excluded.soiling_loss_basis_points,\
                     shading_loss_basis_points=excluded.shading_loss_basis_points,\
                     mismatch_loss_basis_points=excluded.mismatch_loss_basis_points,\
                     wiring_loss_basis_points=excluded.wiring_loss_basis_points,\
                     unavailability_loss_basis_points=excluded.unavailability_loss_basis_points,\
                     calibration_basis_points=excluded.calibration_basis_points,\
                     configuration_digest=excluded.configuration_digest",
                )
                .bind(Uuid::now_v7().as_bytes().to_vec())
                .bind(system_id.as_uuid().as_bytes().to_vec())
                .bind(string_id.as_bytes().to_vec())
                .bind(input.effective_from)
                .bind(input.effective_to)
                .bind(&input.model_identifier)
                .bind(i64::from(input.model_revision))
                .bind(i64::from(input.losses.soiling_basis_points))
                .bind(i64::from(input.losses.shading_basis_points))
                .bind(i64::from(input.losses.mismatch_basis_points))
                .bind(i64::from(input.losses.wiring_basis_points))
                .bind(i64::from(input.losses.unavailability_basis_points))
                .bind(i64::from(input.calibration_basis_points))
                .bind(digest.to_vec())
                .bind(now)
                .bind(actor.as_uuid().as_bytes().to_vec())
                .execute(&mut *transaction)
                .await
                .map_err(|_| ForecastApiError::Conflict)?;
            }
            transaction
                .commit()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            drop(connection);
            return self.settings(scope).await;
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (actor, scope, input);
            Err(ForecastApiError::Unavailable)
        }
    }

    /// Aggregates per-string readiness into the scope's included versus total effective capacity, with the de-duplicated reasons that block completeness.
    async fn input_completeness(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastInputCompletenessResponse, ForecastApiError> {
        #[cfg(feature = "sqlite")]
        {
            let system_id = Self::system_of(&scope)?;
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            let readiness = string_readiness(&mut connection, &scope).await?;
            let total: i64 = readiness.iter().map(|entry| entry.capacity_watts).sum();
            let included: i64 = readiness
                .iter()
                .filter(|entry| entry.reasons.is_empty())
                .map(|entry| entry.capacity_watts)
                .sum();
            let mut reasons: Vec<ForecastCompletenessReason> = readiness
                .iter()
                .flat_map(|entry| entry.reasons.clone())
                .collect();
            reasons.sort_unstable();
            reasons.dedup();
            return Ok(ForecastInputCompletenessResponse {
                scope,
                effective_at: self.now(),
                included_capacity_watts: included,
                total_effective_capacity_watts: total,
                complete: total > 0 && included == total,
                reasons,
                version: 1,
            });
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = scope;
            Err(ForecastApiError::Unavailable)
        }
    }

    /// Lists the weather data runs overlapping the query window for the scope's system.
    async fn forecast_runs(
        &self,
        scope: ForecastResourceScope,
        query: ForecastRunQuery,
    ) -> Result<Vec<ForecastRunResponse>, ForecastApiError> {
        #[cfg(feature = "sqlite")]
        {
            let system_id = Self::system_of(&scope)?;
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            let rows = sqlx::query(
                "SELECT id,data_kind,issued_at,fetched_at,valid_from,valid_to,resolution_seconds,\
                 adapter,source_url,license_identifier,attribution,provider_configuration_id \
                 FROM weather_data_runs WHERE system_id=? AND valid_from<? AND valid_to>? \
                 AND data_kind=? ORDER BY fetched_at DESC LIMIT ?",
            )
            .bind(system_id.as_uuid().as_bytes().to_vec())
            .bind(query.end_epoch_millis)
            .bind(query.start_epoch_millis)
            .bind(weather_kind_name(query.kind))
            .bind(i64::from(query.limit.clamp(1, 200)))
            .fetch_all(&mut *connection)
            .await
            .map_err(|_| ForecastApiError::Unavailable)?;
            return rows
                .iter()
                .map(|row| {
                    let fetched_at: i64 = row.get("fetched_at");
                    Ok(ForecastRunResponse {
                        id: run_id(row, "id")?,
                        system_id,
                        kind: query.kind,
                        issued_at: row.try_get("issued_at").ok().flatten(),
                        valid_from: row.get("valid_from"),
                        valid_to: row.get("valid_to"),
                        resolution_seconds: u32::try_from(row.get::<i64, _>("resolution_seconds"))
                            .unwrap_or_default(),
                        freshness: self.freshness(fetched_at),
                        provenance: provenance(row, fetched_at),
                    })
                })
                .collect();
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (scope, query);
            Err(ForecastApiError::Unavailable)
        }
    }

    /// Returns the modeled yield series for the scope and query window, with run provenance and freshness.
    async fn yield_series(
        &self,
        scope: ForecastResourceScope,
        query: YieldSeriesQuery,
    ) -> Result<YieldSeriesResponse, ForecastApiError> {
        #[cfg(feature = "sqlite")]
        {
            let system_id = Self::system_of(&scope)?;
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            let (scope_kind, scope_id) = projection_scope(&scope, system_id);
            let rows = sqlx::query(
                "SELECT r.interval_start,r.interval_end,r.power_central_watts,r.power_lower_watts,\
                 r.power_upper_watts,r.energy_central_wh,r.energy_lower_wh,r.energy_upper_wh,\
                 r.completeness,r.included_capacity_watts,r.total_effective_capacity_watts,\
                 r.calculation_run_id,c.weather_run_id,c.model_identifier,c.model_revision,\
                 c.configuration_digest,w.fetched_at,w.adapter,w.source_url,w.license_identifier,\
                 w.attribution,w.provider_configuration_id \
                 FROM yield_result_projections p \
                 JOIN yield_calculation_results r ON r.id=p.result_id \
                 JOIN yield_calculation_runs c ON c.id=r.calculation_run_id \
                 JOIN weather_data_runs w ON w.id=c.weather_run_id \
                 WHERE p.system_id=? AND p.basis=? AND p.scope_kind=? AND p.scope_id=? \
                 AND p.interval_start>=? AND p.interval_start<? \
                 ORDER BY p.interval_start LIMIT ?",
            )
            .bind(system_id.as_uuid().as_bytes().to_vec())
            .bind(basis_name(query.basis))
            .bind(scope_kind)
            .bind(scope_id.as_bytes().to_vec())
            .bind(query.start_epoch_millis)
            .bind(query.end_epoch_millis)
            .bind(i64::from(query.maximum_points.clamp(1, 10_000)))
            .fetch_all(&mut *connection)
            .await
            .map_err(|_| ForecastApiError::Unavailable)?;
            let Some(first) = rows.first() else {
                return Err(ForecastApiError::NotFound);
            };
            let fetched_at: i64 = first.get("fetched_at");
            let points: Vec<YieldSeriesPointResponse> = rows
                .iter()
                .filter(|row| {
                    query.include_partial
                        || row.get::<String, _>("completeness").as_str() == "complete"
                })
                .map(|row| YieldSeriesPointResponse {
                    interval_start: row.get("interval_start"),
                    interval_end: row.get("interval_end"),
                    central_power_watts: row.try_get("power_central_watts").ok().flatten(),
                    lower_power_watts: row.try_get("power_lower_watts").ok().flatten(),
                    upper_power_watts: row.try_get("power_upper_watts").ok().flatten(),
                    central_energy_watt_hours: row.try_get("energy_central_wh").ok().flatten(),
                    lower_energy_watt_hours: row.try_get("energy_lower_wh").ok().flatten(),
                    upper_energy_watt_hours: row.try_get("energy_upper_wh").ok().flatten(),
                    coverage_basis_points: coverage_of(row),
                    completeness: completeness_of(&row.get::<String, _>("completeness")),
                })
                .collect();
            return Ok(YieldSeriesResponse {
                scope,
                basis: query.basis,
                resolution: query.resolution,
                issue_time: None,
                weather_run_id: run_id(first, "weather_run_id")?,
                calculation_run_id: calculation_run_id(first, "calculation_run_id")?,
                model_identifier: first.get("model_identifier"),
                model_revision: u16::try_from(first.get::<i64, _>("model_revision"))
                    .unwrap_or_default(),
                configuration_digest: hex(&first.get::<Vec<u8>, _>("configuration_digest")),
                freshness: self.freshness(fetched_at),
                provenance: provenance(first, fetched_at),
                included_capacity_watts: first.get("included_capacity_watts"),
                total_effective_capacity_watts: first.get("total_effective_capacity_watts"),
                completeness: completeness_of(&first.get::<String, _>("completeness")),
                unavailable_reasons: Vec::new(),
                points,
            });
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (scope, query);
            Err(ForecastApiError::Unavailable)
        }
    }

    /// Compares measured rollup energy against the modeled series per interval, leaving the measured side empty where no bucket exists.
    async fn performance_series(
        &self,
        scope: ForecastResourceScope,
        query: PerformanceQuery,
    ) -> Result<PerformanceSeriesResponse, ForecastApiError> {
        let modeled = self
            .yield_series(
                scope,
                YieldSeriesQuery {
                    start_epoch_millis: query.start_epoch_millis,
                    end_epoch_millis: query.end_epoch_millis,
                    basis: query.metric.basis(),
                    resolution: query.resolution,
                    weather_run_id: query.weather_run_id,
                    include_partial: true,
                    maximum_points: query.maximum_points,
                },
            )
            .await?;
        #[cfg(feature = "sqlite")]
        {
            let system_id = Self::system_of(&scope)?;
            let account = self.account(system_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
            let (scope_kind, scope_id) = projection_scope(&scope, system_id);
            let mut points = Vec::with_capacity(modeled.points.len());
            for point in &modeled.points {
                // Measured energy comes from the same scoped rollups the charts
                // draw, so a comparison never mixes two aggregation paths.
                let measured: Option<i64> = sqlx::query_scalar(
                    "SELECT SUM(generation_energy_sum_wh) FROM telemetry_rollups \
                     WHERE system_id=? AND scope_kind=? AND scope_id=? \
                     AND resolution='fifteen_minute' \
                     AND bucket_start>=? AND bucket_start<?",
                )
                .bind(system_id.as_uuid().as_bytes().to_vec())
                .bind(scope_kind)
                .bind(scope_id.as_bytes().to_vec())
                .bind(point.interval_start)
                .bind(point.interval_end)
                .fetch_one(&mut *connection)
                .await
                .map_err(|_| ForecastApiError::Unavailable)?;
                points.push(PerformancePointResponse {
                    interval_start: point.interval_start,
                    interval_end: point.interval_end,
                    actual_energy_watt_hours: measured,
                    modeled_energy_watt_hours: point.central_energy_watt_hours,
                    ratio_basis_points: realization(measured, point.central_energy_watt_hours),
                    // Measured coverage is full whenever a bucket exists; the
                    // modeled side carries the capacity share it could cover.
                    actual_coverage_basis_points: if measured.is_some() { 10_000 } else { 0 },
                    modeled_coverage_basis_points: point.coverage_basis_points,
                    unavailable_reason: None,
                });
            }
            return Ok(PerformanceSeriesResponse {
                scope,
                metric: query.metric,
                basis: query.metric.basis(),
                resolution: query.resolution,
                issue_time: modeled.issue_time,
                weather_run_id: modeled.weather_run_id,
                calculation_run_id: modeled.calculation_run_id,
                model_identifier: modeled.model_identifier,
                model_revision: modeled.model_revision,
                configuration_digest: modeled.configuration_digest,
                freshness: modeled.freshness,
                provenance: modeled.provenance,
                points,
            });
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (scope, query, modeled);
            Err(ForecastApiError::Unavailable)
        }
    }
}

/// Ratio of measured to modeled energy, in basis points.
fn realization(measured: Option<i64>, modeled: Option<i64>) -> Option<u32> {
    match (measured, modeled) {
        (Some(measured), Some(modeled)) if modeled > 0 => {
            u32::try_from((measured.saturating_mul(10_000) / modeled).clamp(0, i64::from(u32::MAX)))
                .ok()
        }
        _ => None,
    }
}

/// Rejects settings with a blank model identifier, a zero revision or an effective range that ends before it starts.
fn validate_settings(input: &ForecastSettingsInput) -> Result<(), ForecastApiError> {
    if input.model_identifier.trim().is_empty() || input.model_revision == 0 {
        return Err(ForecastApiError::InvalidPath);
    }
    if input
        .effective_to
        .is_some_and(|end| end <= input.effective_from)
    {
        return Err(ForecastApiError::InvalidPath);
    }
    Ok(())
}

/// Stable digest of the settings that produced a result, so a recalculation can
/// be told apart from a configuration change.
fn configuration_digest(input: &ForecastSettingsInput) -> [u8; 32] {
    let material = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        input.model_identifier,
        input.model_revision,
        input.losses.soiling_basis_points,
        input.losses.shading_basis_points,
        input.losses.mismatch_basis_points,
        input.losses.wiring_basis_points,
        input.losses.unavailability_basis_points,
        input.calibration_basis_points,
    );
    *blake3::hash(material.as_bytes()).as_bytes()
}

/// Returns the stored name of a calculation basis.
const fn basis_name(basis: CalculationBasis) -> &'static str {
    match basis {
        CalculationBasis::Forecast => "forecast",
        CalculationBasis::Expected => "expected",
    }
}

/// Returns the stored name of a weather data kind.
const fn weather_kind_name(kind: WeatherDataKind) -> &'static str {
    match kind {
        WeatherDataKind::Forecast => "forecast",
        WeatherDataKind::Observed => "observed",
        WeatherDataKind::Reanalysis => "reanalysis",
    }
}

/// Maps a scope to the projection scope kind and identifier; account and system scopes both project onto the system.
fn projection_scope(scope: &ForecastResourceScope, system_id: SystemId) -> (&'static str, Uuid) {
    match scope {
        ForecastResourceScope::String { string_id, .. } => ("string", string_id.as_uuid()),
        ForecastResourceScope::Inverter { inverter_id, .. } => ("inverter", inverter_id.as_uuid()),
        ForecastResourceScope::Account { .. } | ForecastResourceScope::System { .. } => {
            ("system", system_id.as_uuid())
        }
    }
}

/// Decodes a stored completeness label, treating unknown values as unavailable; partial and unavailable results carry no reasons yet.
fn completeness_of(stored: &str) -> ForecastCompleteness {
    match stored {
        "complete" => ForecastCompleteness::Complete,
        "partial" => ForecastCompleteness::Partial {
            reasons: Vec::new(),
        },
        _ => ForecastCompleteness::Unavailable {
            reasons: Vec::new(),
        },
    }
}

/// Encodes bytes as a lowercase hexadecimal string.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Computes the included-capacity share of the total in basis points, returning zero when the total is not positive.
#[cfg(feature = "sqlite")]
fn coverage_of(row: &sqlx::sqlite::SqliteRow) -> u16 {
    let included: i64 = row.try_get("included_capacity_watts").unwrap_or_default();
    let total: i64 = row
        .try_get("total_effective_capacity_watts")
        .unwrap_or_default();
    if total <= 0 {
        return 0;
    }
    u16::try_from(included.saturating_mul(10_000) / total).unwrap_or(10_000)
}

/// Builds the provenance block from a weather run row, using empty values for missing columns.
#[cfg(feature = "sqlite")]
fn provenance(row: &sqlx::sqlite::SqliteRow, fetched_at: i64) -> ForecastProvenanceResponse {
    ForecastProvenanceResponse {
        provider_id: hex(&row
            .try_get::<Vec<u8>, _>("provider_configuration_id")
            .unwrap_or_default()),
        adapter: row.try_get("adapter").unwrap_or_default(),
        source_url: row.try_get("source_url").unwrap_or_default(),
        license_identifier: row.try_get("license_identifier").unwrap_or_default(),
        attribution: row.try_get("attribution").unwrap_or_default(),
        fetched_at,
    }
}

/// Reads a weather run identifier from a blob column, failing with `Unavailable` if it is not a valid identifier.
#[cfg(feature = "sqlite")]
fn run_id(
    row: &sqlx::sqlite::SqliteRow,
    column: &str,
) -> Result<WeatherDataRunId, ForecastApiError> {
    Uuid::from_slice(&row.get::<Vec<u8>, _>(column))
        .ok()
        .and_then(|value| WeatherDataRunId::from_uuid(value).ok())
        .ok_or(ForecastApiError::Unavailable)
}

/// Reads a yield calculation run identifier from a blob column, failing with `Unavailable` if it is not a valid identifier.
#[cfg(feature = "sqlite")]
fn calculation_run_id(
    row: &sqlx::sqlite::SqliteRow,
    column: &str,
) -> Result<YieldCalculationRunId, ForecastApiError> {
    Uuid::from_slice(&row.get::<Vec<u8>, _>(column))
        .ok()
        .and_then(|value| YieldCalculationRunId::from_uuid(value).ok())
        .ok_or(ForecastApiError::Unavailable)
}

/// Stored settings for one string.
#[cfg(feature = "sqlite")]
struct StoredSettings {
    effective_from: i64,
    effective_to: Option<i64>,
    model_identifier: String,
    model_revision: u16,
    losses: ForecastLossInput,
    calibration_basis_points: i32,
}

/// Every PV string a scope covers.
#[cfg(feature = "sqlite")]
async fn scope_strings(
    connection: &mut sqlx::SqliteConnection,
    scope: &ForecastResourceScope,
) -> Result<Vec<Uuid>, ForecastApiError> {
    let rows = match scope {
        ForecastResourceScope::String {
            system_id,
            inverter_id,
            string_id,
            ..
        } => {
            sqlx::query(
                "SELECT p.id FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
                 WHERE p.id=? AND i.id=? AND i.system_id=?",
            )
            .bind(string_id.as_uuid().as_bytes().to_vec())
            .bind(inverter_id.as_uuid().as_bytes().to_vec())
            .bind(system_id.as_uuid().as_bytes().to_vec())
            .fetch_all(&mut *connection)
            .await
        }
        ForecastResourceScope::Inverter {
            system_id,
            inverter_id,
            ..
        } => {
            sqlx::query(
                "SELECT p.id FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
                 WHERE i.id=? AND i.system_id=?",
            )
            .bind(inverter_id.as_uuid().as_bytes().to_vec())
            .bind(system_id.as_uuid().as_bytes().to_vec())
            .fetch_all(&mut *connection)
            .await
        }
        ForecastResourceScope::System { system_id, .. } => {
            sqlx::query(
                "SELECT p.id FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
             WHERE i.system_id=?",
            )
            .bind(system_id.as_uuid().as_bytes().to_vec())
            .fetch_all(&mut *connection)
            .await
        }
        ForecastResourceScope::Account { .. } => return Err(ForecastApiError::InvalidPath),
    }
    .map_err(|_| ForecastApiError::Unavailable)?;
    Ok(rows
        .iter()
        .filter_map(|row| Uuid::from_slice(&row.get::<Vec<u8>, _>("id")).ok())
        .collect())
}

/// Newest settings row shared by the strings in a scope.
#[cfg(feature = "sqlite")]
async fn newest_settings(
    connection: &mut sqlx::SqliteConnection,
    strings: &[Uuid],
) -> Result<Option<StoredSettings>, ForecastApiError> {
    for string_id in strings {
        let row = sqlx::query(
            "SELECT effective_from,effective_to,model_identifier,model_revision,\
             soiling_loss_basis_points,shading_loss_basis_points,mismatch_loss_basis_points,\
             wiring_loss_basis_points,unavailability_loss_basis_points,calibration_basis_points \
             FROM pv_string_forecast_settings WHERE string_id=? \
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(string_id.as_bytes().to_vec())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| ForecastApiError::Unavailable)?;
        if let Some(row) = row {
            return Ok(Some(StoredSettings {
                effective_from: row.get("effective_from"),
                effective_to: row.try_get("effective_to").ok().flatten(),
                model_identifier: row.get("model_identifier"),
                model_revision: u16::try_from(row.get::<i64, _>("model_revision"))
                    .unwrap_or_default(),
                losses: ForecastLossInput {
                    soiling_basis_points: basis_points(&row, "soiling_loss_basis_points"),
                    shading_basis_points: basis_points(&row, "shading_loss_basis_points"),
                    mismatch_basis_points: basis_points(&row, "mismatch_loss_basis_points"),
                    wiring_basis_points: basis_points(&row, "wiring_loss_basis_points"),
                    unavailability_basis_points: basis_points(
                        &row,
                        "unavailability_loss_basis_points",
                    ),
                },
                calibration_basis_points: i32::try_from(
                    row.get::<i64, _>("calibration_basis_points"),
                )
                .unwrap_or_default(),
            }));
        }
    }
    Ok(None)
}

/// Reads a basis-point column as `u16`, defaulting to zero when missing or out of range.
#[cfg(feature = "sqlite")]
fn basis_points(row: &sqlx::sqlite::SqliteRow, column: &str) -> u16 {
    u16::try_from(row.try_get::<i64, _>(column).unwrap_or_default()).unwrap_or_default()
}

/// What a string still lacks before it can be modeled.
#[cfg(feature = "sqlite")]
struct StringReadiness {
    capacity_watts: i64,
    reasons: Vec<ForecastCompletenessReason>,
}

/// Determines per string in the scope its capacity and the missing inputs that keep it from being modeled.
#[cfg(feature = "sqlite")]
async fn string_readiness(
    connection: &mut sqlx::SqliteConnection,
    scope: &ForecastResourceScope,
) -> Result<Vec<StringReadiness>, ForecastApiError> {
    let strings = scope_strings(connection, scope).await?;
    let mut out = Vec::with_capacity(strings.len());
    for string_id in &strings {
        let Some(row) = sqlx::query(
            "SELECT p.rated_power_watts,p.orientation_degrees,p.tilt_degrees,\
             p.module_peak_power_watts,s.latitude_e6,\
             (SELECT COUNT(*) FROM pv_string_forecast_settings f WHERE f.string_id=p.id) \
              AS settings_count \
             FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
             JOIN systems s ON s.id=i.system_id WHERE p.id=?",
        )
        .bind(string_id.as_bytes().to_vec())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| ForecastApiError::Unavailable)?
        else {
            continue;
        };
        let mut reasons = Vec::new();
        if row
            .try_get::<Option<i64>, _>("latitude_e6")
            .ok()
            .flatten()
            .is_none()
        {
            reasons.push(ForecastCompletenessReason::MissingSystemLocation);
        }
        if row
            .try_get::<Option<i64>, _>("orientation_degrees")
            .ok()
            .flatten()
            .is_none()
        {
            reasons.push(ForecastCompletenessReason::MissingOrientation);
        }
        if row
            .try_get::<Option<i64>, _>("tilt_degrees")
            .ok()
            .flatten()
            .is_none()
        {
            reasons.push(ForecastCompletenessReason::MissingTilt);
        }
        if row
            .try_get::<Option<i64>, _>("module_peak_power_watts")
            .ok()
            .flatten()
            .is_none()
        {
            reasons.push(ForecastCompletenessReason::MissingModuleCapacity);
        }
        if row.try_get::<i64, _>("settings_count").unwrap_or_default() == 0 {
            reasons.push(ForecastCompletenessReason::MissingForecastSettings);
        }
        out.push(StringReadiness {
            capacity_watts: row.try_get("rated_power_watts").unwrap_or_default(),
            reasons,
        });
    }
    Ok(out)
}
