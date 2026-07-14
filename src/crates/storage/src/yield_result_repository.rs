//! Cross-engine persistence for immutable modeled-yield runs, results, and active projections.

#[cfg(feature = "postgres")]
use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{
    AccountId, CalculationBasis, EstimateRange, ForecastCompleteness, ForecastCompletenessReason,
    InverterId, ModelVersion, StringId, SystemId, TimeRange, UtcTimestamp, WattHours, Watts,
    WeatherDataRunId, YieldCalculationResult, YieldCalculationRunId, YieldResultId, YieldScope,
};
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
use thiserror::Error;
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;
use crate::{ForecastRetentionClass, WeatherRunInsertOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YieldCalculationState {
    Pending,
    Running,
    Completed,
    Failed,
    Superseded,
}

impl YieldCalculationState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Superseded => "superseded",
        }
    }

    fn parse(value: &str) -> Result<Self, YieldResultRepositoryError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "superseded" => Ok(Self::Superseded),
            _ => Err(YieldResultRepositoryError::Corrupt(
                "unknown calculation state",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldCalculationRunRecord {
    pub id: YieldCalculationRunId,
    pub system_id: SystemId,
    pub weather_run_id: WeatherDataRunId,
    pub basis: CalculationBasis,
    pub model_version: ModelVersion,
    pub configuration_digest: [u8; 32],
    pub state: YieldCalculationState,
    pub requested_at: i64,
    pub completed_at: Option<i64>,
    pub safe_error_code: Option<String>,
    pub retention_class: ForecastRetentionClass,
    pub retain_until: Option<i64>,
    pub referenced_at: Option<i64>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredYieldResult {
    pub result: YieldCalculationResult,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YieldInvalidationReason {
    Equipment,
    Settings,
    ProviderRevision,
    LateTelemetry,
    Correction,
    ModelVersion,
}

impl YieldInvalidationReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Equipment => "equipment",
            Self::Settings => "settings",
            Self::ProviderRevision => "provider_revision",
            Self::LateTelemetry => "late_telemetry",
            Self::Correction => "correction",
            Self::ModelVersion => "model_version",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YieldInvalidationState {
    Pending,
    Leased,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldInvalidationRecord {
    pub id: Uuid,
    pub system_id: SystemId,
    pub range: TimeRange,
    pub reason: YieldInvalidationReason,
    pub state: YieldInvalidationState,
    pub idempotency_key: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[async_trait]
pub trait YieldResultRepository: Send + Sync {
    fn account_id(&self) -> AccountId;

    async fn insert_run(
        &self,
        run: &YieldCalculationRunRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldResultRepositoryError>;

    async fn calculation_run(
        &self,
        id: YieldCalculationRunId,
    ) -> Result<Option<YieldCalculationRunRecord>, YieldResultRepositoryError>;

    async fn update_run_state(
        &self,
        id: YieldCalculationRunId,
        state: YieldCalculationState,
        completed_at: Option<i64>,
        safe_error_code: Option<&str>,
    ) -> Result<bool, YieldResultRepositoryError>;

    async fn insert_results_and_project(
        &self,
        run: &YieldCalculationRunRecord,
        results: &[StoredYieldResult],
        projected_at: i64,
    ) -> Result<(), YieldResultRepositoryError>;

    async fn active_results(
        &self,
        system_id: SystemId,
        basis: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError>;

    async fn result_history(
        &self,
        system_id: SystemId,
        basis: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError>;

    async fn insert_invalidation(
        &self,
        invalidation: &YieldInvalidationRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldResultRepositoryError>;

    async fn pending_invalidations(
        &self,
        system_id: SystemId,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<YieldInvalidationRecord>, YieldResultRepositoryError>;

    async fn complete_invalidation(
        &self,
        id: Uuid,
        completed_at: i64,
    ) -> Result<bool, YieldResultRepositoryError>;

    async fn retain_calculation_run(
        &self,
        id: YieldCalculationRunId,
        retention_class: ForecastRetentionClass,
        retain_until: Option<i64>,
        referenced_at: Option<i64>,
    ) -> Result<bool, YieldResultRepositoryError>;

    async fn purge_expired_calculation_runs(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<u64, YieldResultRepositoryError>;
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteYieldResultRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteYieldResultRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresYieldResultRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresYieldResultRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "sqlite")]
async fn insert_sqlite_result(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    run: &YieldCalculationRunRecord,
    stored: &StoredYieldResult,
) -> Result<(), YieldResultRepositoryError> {
    let result = &stored.result;
    let (scope_kind, scope_id) = scope_columns(result.scope);
    let (completeness, reasons) = completeness_columns(&result.completeness)?;
    let power = power_columns(result.power);
    let energy = energy_columns(result.energy);
    sqlx::query(
        "INSERT INTO yield_calculation_results \
         (id,calculation_run_id,system_id,scope_kind,scope_id,interval_start,interval_end,configuration_digest, \
          power_central_watts,power_lower_watts,power_upper_watts,energy_central_wh,energy_lower_wh,energy_upper_wh, \
          included_capacity_watts,total_effective_capacity_watts,completeness,incomplete_reasons_json,uncertainty_known,created_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(blob(result.id.as_uuid()))
    .bind(blob(run.id.as_uuid()))
    .bind(blob(run.system_id.as_uuid()))
    .bind(scope_kind)
    .bind(blob(scope_id))
    .bind(timestamp_i64(result.interval.start)?)
    .bind(timestamp_i64(result.interval.end)?)
    .bind(result.configuration_digest.to_vec())
    .bind(power[0])
    .bind(power[1])
    .bind(power[2])
    .bind(energy[0])
    .bind(energy[1])
    .bind(energy[2])
    .bind(result.included_capacity.value())
    .bind(result.total_effective_capacity.value())
    .bind(completeness)
    .bind(reasons)
    .bind(i64::from(uncertainty_known(result)))
    .bind(stored.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn insert_postgres_result(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: AccountId,
    run: &YieldCalculationRunRecord,
    stored: &StoredYieldResult,
) -> Result<(), YieldResultRepositoryError> {
    let result = &stored.result;
    let (scope_kind, scope_id) = scope_columns(result.scope);
    let (completeness, reasons) = completeness_columns(&result.completeness)?;
    let power = power_columns(result.power);
    let energy = energy_columns(result.energy);
    sqlx::query::<sqlx::Postgres>(
        "INSERT INTO account_data.yield_calculation_results \
         (account_id,id,calculation_run_id,system_id,scope_kind,scope_id,interval_start,interval_end,configuration_digest, \
          power_central_watts,power_lower_watts,power_upper_watts,energy_central_wh,energy_lower_wh,energy_upper_wh, \
          included_capacity_watts,total_effective_capacity_watts,completeness,incomplete_reasons,uncertainty_known,created_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)",
    )
    .bind(account_id.as_uuid())
    .bind(result.id.as_uuid())
    .bind(run.id.as_uuid())
    .bind(run.system_id.as_uuid())
    .bind(scope_kind)
    .bind(scope_id)
    .bind(timestamp_i64(result.interval.start)?)
    .bind(timestamp_i64(result.interval.end)?)
    .bind(result.configuration_digest.to_vec())
    .bind(power[0])
    .bind(power[1])
    .bind(power[2])
    .bind(energy[0])
    .bind(energy[1])
    .bind(energy[2])
    .bind(result.included_capacity.value())
    .bind(result.total_effective_capacity.value())
    .bind(completeness)
    .bind(serde_json::from_str::<serde_json::Value>(&reasons)?)
    .bind(uncertainty_known(result))
    .bind(stored.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn sqlite_run(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<YieldCalculationRunRecord, YieldResultRepositoryError> {
    run_from_columns(
        YieldCalculationRunId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("id")?)?)?,
        SystemId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("system_id")?)?)?,
        WeatherDataRunId::from_uuid(uuid_from_blob(
            &row.try_get::<Vec<u8>, _>("weather_run_id")?,
        )?)?,
        row.try_get("basis")?,
        row.try_get("model_identifier")?,
        row.try_get("model_revision")?,
        row.try_get("configuration_digest")?,
        row.try_get("state")?,
        row.try_get("requested_at")?,
        row.try_get("completed_at")?,
        row.try_get("safe_error_code")?,
        row.try_get("retention_class")?,
        row.try_get("retain_until")?,
        row.try_get("referenced_at")?,
        row.try_get("idempotency_key")?,
    )
}

#[cfg(feature = "postgres")]
fn postgres_run(
    row: &sqlx::postgres::PgRow,
) -> Result<YieldCalculationRunRecord, YieldResultRepositoryError> {
    run_from_columns(
        YieldCalculationRunId::from_uuid(row.try_get("id")?)?,
        SystemId::from_uuid(row.try_get("system_id")?)?,
        WeatherDataRunId::from_uuid(row.try_get("weather_run_id")?)?,
        row.try_get("basis")?,
        row.try_get("model_identifier")?,
        i64::from(row.try_get::<i32, _>("model_revision")?),
        row.try_get("configuration_digest")?,
        row.try_get("state")?,
        row.try_get("requested_at")?,
        row.try_get("completed_at")?,
        row.try_get("safe_error_code")?,
        row.try_get("retention_class")?,
        row.try_get("retain_until")?,
        row.try_get("referenced_at")?,
        row.try_get("idempotency_key")?,
    )
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn run_from_columns(
    id: YieldCalculationRunId,
    system_id: SystemId,
    weather_run_id: WeatherDataRunId,
    basis_value: String,
    model_identifier: String,
    model_revision: i64,
    configuration_digest: Vec<u8>,
    state: String,
    requested_at: i64,
    completed_at: Option<i64>,
    safe_error_code: Option<String>,
    retention_class: String,
    retain_until: Option<i64>,
    referenced_at: Option<i64>,
    idempotency_key: String,
) -> Result<YieldCalculationRunRecord, YieldResultRepositoryError> {
    Ok(YieldCalculationRunRecord {
        id,
        system_id,
        weather_run_id,
        basis: parse_basis(&basis_value)?,
        model_version: ModelVersion {
            identifier: model_identifier,
            revision: u16::try_from(model_revision)
                .map_err(|_| YieldResultRepositoryError::Corrupt("model revision out of range"))?,
        },
        configuration_digest: digest(&configuration_digest)?,
        state: YieldCalculationState::parse(&state)?,
        requested_at,
        completed_at,
        safe_error_code,
        retention_class: parse_retention(&retention_class)?,
        retain_until,
        referenced_at,
        idempotency_key,
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_result(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<StoredYieldResult, YieldResultRepositoryError> {
    result_from_columns(
        YieldResultId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("id")?)?)?,
        YieldCalculationRunId::from_uuid(uuid_from_blob(
            &row.try_get::<Vec<u8>, _>("calculation_run_id")?,
        )?)?,
        WeatherDataRunId::from_uuid(uuid_from_blob(
            &row.try_get::<Vec<u8>, _>("weather_run_id")?,
        )?)?,
        row.try_get("basis")?,
        row.try_get("scope_kind")?,
        uuid_from_blob(&row.try_get::<Vec<u8>, _>("scope_id")?)?,
        row.try_get("interval_start")?,
        row.try_get("interval_end")?,
        row.try_get("model_identifier")?,
        row.try_get("model_revision")?,
        row.try_get("configuration_digest")?,
        estimate_from_i64(row, "power")?,
        estimate_from_i64(row, "energy")?,
        row.try_get("included_capacity_watts")?,
        row.try_get("total_effective_capacity_watts")?,
        row.try_get("completeness")?,
        row.try_get("incomplete_reasons_json")?,
        row.try_get("created_at")?,
    )
}

#[cfg(feature = "postgres")]
fn postgres_result(
    row: &sqlx::postgres::PgRow,
) -> Result<StoredYieldResult, YieldResultRepositoryError> {
    result_from_columns(
        YieldResultId::from_uuid(row.try_get("id")?)?,
        YieldCalculationRunId::from_uuid(row.try_get("calculation_run_id")?)?,
        WeatherDataRunId::from_uuid(row.try_get("weather_run_id")?)?,
        row.try_get("basis")?,
        row.try_get("scope_kind")?,
        row.try_get("scope_id")?,
        row.try_get("interval_start")?,
        row.try_get("interval_end")?,
        row.try_get("model_identifier")?,
        i64::from(row.try_get::<i32, _>("model_revision")?),
        row.try_get("configuration_digest")?,
        estimate_from_pg(row, "power")?,
        estimate_from_pg(row, "energy")?,
        row.try_get("included_capacity_watts")?,
        row.try_get("total_effective_capacity_watts")?,
        row.try_get("completeness")?,
        serde_json::to_string(&row.try_get::<serde_json::Value, _>("incomplete_reasons")?)?,
        row.try_get("created_at")?,
    )
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn result_from_columns(
    id: YieldResultId,
    calculation_run_id: YieldCalculationRunId,
    weather_run_id: WeatherDataRunId,
    basis_value: String,
    scope_kind: String,
    scope_id: Uuid,
    interval_start: i64,
    interval_end: i64,
    model_identifier: String,
    model_revision: i64,
    configuration_digest: Vec<u8>,
    power: Option<EstimateRange<i64>>,
    energy: Option<EstimateRange<i64>>,
    included_capacity: i64,
    total_effective_capacity: i64,
    completeness: String,
    reasons: String,
    created_at: i64,
) -> Result<StoredYieldResult, YieldResultRepositoryError> {
    let scope = parse_scope(&scope_kind, scope_id)?;
    Ok(StoredYieldResult {
        result: YieldCalculationResult {
            id,
            calculation_run_id,
            weather_run_id,
            basis: parse_basis(&basis_value)?,
            scope,
            interval: TimeRange::new(timestamp(interval_start)?, timestamp(interval_end)?)
                .map_err(|_| YieldResultRepositoryError::Corrupt("invalid result interval"))?,
            model_version: ModelVersion {
                identifier: model_identifier,
                revision: u16::try_from(model_revision).map_err(|_| {
                    YieldResultRepositoryError::Corrupt("model revision out of range")
                })?,
            },
            configuration_digest: digest(&configuration_digest)?,
            power: power.map(|value| value.map(Watts::new)),
            energy: energy.map(|value| value.map(WattHours::new)),
            included_capacity: Watts::new(nonnegative_i64(included_capacity, "included capacity")?),
            total_effective_capacity: Watts::new(nonnegative_i64(
                total_effective_capacity,
                "total effective capacity",
            )?),
            completeness: parse_completeness(&completeness, &reasons)?,
        },
        created_at,
    })
}

trait MapEstimate<T> {
    fn map<U>(self, mapper: impl Fn(T) -> U) -> EstimateRange<U>;
}

impl<T> MapEstimate<T> for EstimateRange<T> {
    fn map<U>(self, mapper: impl Fn(T) -> U) -> EstimateRange<U> {
        EstimateRange {
            central: mapper(self.central),
            lower: self.lower.map(&mapper),
            upper: self.upper.map(mapper),
        }
    }
}

#[cfg(feature = "sqlite")]
fn estimate_from_i64(
    row: &sqlx::sqlite::SqliteRow,
    prefix: &str,
) -> Result<Option<EstimateRange<i64>>, YieldResultRepositoryError> {
    estimate_from_values(
        row.try_get(format!("{prefix}_central_watts").as_str())
            .or_else(|_| row.try_get(format!("{prefix}_central_wh").as_str()))?,
        row.try_get(format!("{prefix}_lower_watts").as_str())
            .or_else(|_| row.try_get(format!("{prefix}_lower_wh").as_str()))?,
        row.try_get(format!("{prefix}_upper_watts").as_str())
            .or_else(|_| row.try_get(format!("{prefix}_upper_wh").as_str()))?,
    )
}

#[cfg(feature = "postgres")]
fn estimate_from_pg(
    row: &sqlx::postgres::PgRow,
    prefix: &str,
) -> Result<Option<EstimateRange<i64>>, YieldResultRepositoryError> {
    estimate_from_values(
        row.try_get(format!("{prefix}_central_watts").as_str())
            .or_else(|_| row.try_get(format!("{prefix}_central_wh").as_str()))?,
        row.try_get(format!("{prefix}_lower_watts").as_str())
            .or_else(|_| row.try_get(format!("{prefix}_lower_wh").as_str()))?,
        row.try_get(format!("{prefix}_upper_watts").as_str())
            .or_else(|_| row.try_get(format!("{prefix}_upper_wh").as_str()))?,
    )
}

fn estimate_from_values(
    central: Option<i64>,
    lower: Option<i64>,
    upper: Option<i64>,
) -> Result<Option<EstimateRange<i64>>, YieldResultRepositoryError> {
    central
        .map(|central| {
            Ok(EstimateRange {
                central,
                lower,
                upper,
            })
        })
        .transpose()
}

fn validate_run(run: &YieldCalculationRunRecord) -> Result<(), YieldResultRepositoryError> {
    if run.idempotency_key.trim().is_empty()
        || run.model_version.identifier.trim().is_empty()
        || run.model_version.revision == 0
    {
        return Err(YieldResultRepositoryError::Validation(
            "calculation idempotency key and model version are required",
        ));
    }
    validate_state(run.state, run.completed_at, run.safe_error_code.as_deref())
}

fn validate_invalidation(
    invalidation: &YieldInvalidationRecord,
) -> Result<(), YieldResultRepositoryError> {
    if invalidation.idempotency_key.trim().is_empty()
        || invalidation.state != YieldInvalidationState::Pending
        || invalidation.completed_at.is_some()
    {
        return Err(YieldResultRepositoryError::Validation(
            "new invalidation must be pending with an idempotency key",
        ));
    }
    Ok(())
}

fn validate_retention(
    retention_class: ForecastRetentionClass,
    retain_until: Option<i64>,
    referenced_at: Option<i64>,
) -> Result<(), YieldResultRepositoryError> {
    if retention_class == ForecastRetentionClass::Referenced && referenced_at.is_none() {
        return Err(YieldResultRepositoryError::Validation(
            "referenced result retention requires a reference time",
        ));
    }
    if retention_class != ForecastRetentionClass::Referenced && referenced_at.is_some() {
        return Err(YieldResultRepositoryError::Validation(
            "only referenced calculation runs may carry a reference time",
        ));
    }
    if retention_class == ForecastRetentionClass::Working && retain_until.is_none() {
        return Err(YieldResultRepositoryError::Validation(
            "working calculation retention requires an expiry",
        ));
    }
    Ok(())
}

fn validate_retention_limit(limit: u32) -> Result<(), YieldResultRepositoryError> {
    if limit == 0 || limit > 10_000 {
        Err(YieldResultRepositoryError::Validation(
            "retention limit must be between 1 and 10000",
        ))
    } else {
        Ok(())
    }
}

fn validate_state(
    state: YieldCalculationState,
    completed_at: Option<i64>,
    safe_error_code: Option<&str>,
) -> Result<(), YieldResultRepositoryError> {
    if matches!(
        state,
        YieldCalculationState::Completed | YieldCalculationState::Failed
    ) != completed_at.is_some()
    {
        return Err(YieldResultRepositoryError::Validation(
            "terminal calculation state and completion time must agree",
        ));
    }
    if safe_error_code.is_some_and(|code| code.trim().is_empty() || code.len() > 128) {
        return Err(YieldResultRepositoryError::Validation(
            "safe calculation error code is invalid",
        ));
    }
    Ok(())
}

fn validate_result_batch(
    run: &YieldCalculationRunRecord,
    results: &[StoredYieldResult],
) -> Result<(), YieldResultRepositoryError> {
    if results.is_empty() {
        return Err(YieldResultRepositoryError::Validation(
            "calculation result batch is empty",
        ));
    }
    for stored in results {
        let result = &stored.result;
        if result.calculation_run_id != run.id
            || result.weather_run_id != run.weather_run_id
            || result.basis != run.basis
            || result.model_version != run.model_version
            || result.configuration_digest != run.configuration_digest
            || result.total_effective_capacity.value() < result.included_capacity.value()
        {
            return Err(YieldResultRepositoryError::Validation(
                "result provenance or effective capacity does not match its run",
            ));
        }
        if let YieldScope::System(system_id) = result.scope
            && system_id != run.system_id
        {
            return Err(YieldResultRepositoryError::Validation(
                "system result scope does not match its run",
            ));
        }
    }
    Ok(())
}

fn validate_query(range: TimeRange, limit: u32) -> Result<(), YieldResultRepositoryError> {
    if limit == 0 || limit > 10_000 {
        return Err(YieldResultRepositoryError::Validation(
            "result query limit must be between 1 and 10000",
        ));
    }
    timestamp_i64(range.start)?;
    timestamp_i64(range.end)?;
    Ok(())
}

fn completeness_columns(
    completeness: &ForecastCompleteness,
) -> Result<(&'static str, String), YieldResultRepositoryError> {
    let (kind, reasons) = match completeness {
        ForecastCompleteness::Complete => ("complete", &[][..]),
        ForecastCompleteness::Partial { reasons } => ("partial", reasons.as_slice()),
        ForecastCompleteness::Unavailable { reasons } => ("unavailable", reasons.as_slice()),
    };
    Ok((kind, serde_json::to_string(reasons)?))
}

fn parse_completeness(
    kind: &str,
    json: &str,
) -> Result<ForecastCompleteness, YieldResultRepositoryError> {
    let values: Vec<String> = serde_json::from_str(json)?;
    let reasons = values
        .iter()
        .map(|value| parse_reason(value))
        .collect::<Result<Vec<_>, _>>()?;
    match kind {
        "complete" if reasons.is_empty() => Ok(ForecastCompleteness::Complete),
        "partial" if !reasons.is_empty() => Ok(ForecastCompleteness::Partial { reasons }),
        "unavailable" if !reasons.is_empty() => Ok(ForecastCompleteness::Unavailable { reasons }),
        _ => Err(YieldResultRepositoryError::Corrupt(
            "invalid result completeness",
        )),
    }
}

fn parse_reason(value: &str) -> Result<ForecastCompletenessReason, YieldResultRepositoryError> {
    match value {
        "missing_system_location" => Ok(ForecastCompletenessReason::MissingSystemLocation),
        "missing_module_identity" => Ok(ForecastCompletenessReason::MissingModuleIdentity),
        "missing_orientation" => Ok(ForecastCompletenessReason::MissingOrientation),
        "missing_tilt" => Ok(ForecastCompletenessReason::MissingTilt),
        "missing_module_capacity" => Ok(ForecastCompletenessReason::MissingModuleCapacity),
        "missing_module_specification" => {
            Ok(ForecastCompletenessReason::MissingModuleSpecification)
        }
        "missing_forecast_settings" => Ok(ForecastCompletenessReason::MissingForecastSettings),
        "missing_weather_input" => Ok(ForecastCompletenessReason::MissingWeatherInput),
        "unsupported_weather_input" => Ok(ForecastCompletenessReason::UnsupportedWeatherInput),
        "incompatible_input_run" => Ok(ForecastCompletenessReason::IncompatibleInputRun),
        "partial_effective_capacity" => Ok(ForecastCompletenessReason::PartialEffectiveCapacity),
        "insufficient_weather_coverage" => {
            Ok(ForecastCompletenessReason::InsufficientWeatherCoverage)
        }
        "insufficient_actual_coverage" => {
            Ok(ForecastCompletenessReason::InsufficientActualCoverage)
        }
        "missing_actual_telemetry" => Ok(ForecastCompletenessReason::MissingActualTelemetry),
        "non_positive_expected_energy" => Ok(ForecastCompletenessReason::NonPositiveExpectedEnergy),
        "no_effective_equipment" => Ok(ForecastCompletenessReason::NoEffectiveEquipment),
        _ => Err(YieldResultRepositoryError::Corrupt(
            "unknown completeness reason",
        )),
    }
}

fn power_columns(estimate: Option<EstimateRange<Watts>>) -> [Option<i64>; 3] {
    estimate.map_or([None; 3], |value| {
        [
            Some(value.central.value()),
            value.lower.map(Watts::value),
            value.upper.map(Watts::value),
        ]
    })
}

fn energy_columns(estimate: Option<EstimateRange<WattHours>>) -> [Option<i64>; 3] {
    estimate.map_or([None; 3], |value| {
        [
            Some(value.central.value()),
            value.lower.map(WattHours::value),
            value.upper.map(WattHours::value),
        ]
    })
}

fn uncertainty_known(result: &YieldCalculationResult) -> bool {
    result
        .power
        .is_some_and(|value| value.lower.is_some() && value.upper.is_some())
        || result
            .energy
            .is_some_and(|value| value.lower.is_some() && value.upper.is_some())
}

fn scope_columns(scope: YieldScope) -> (&'static str, Uuid) {
    match scope {
        YieldScope::String(id) => ("string", id.as_uuid()),
        YieldScope::Inverter(id) => ("inverter", id.as_uuid()),
        YieldScope::System(id) => ("system", id.as_uuid()),
    }
}

fn parse_scope(kind: &str, id: Uuid) -> Result<YieldScope, YieldResultRepositoryError> {
    match kind {
        "string" => Ok(YieldScope::String(StringId::from_uuid(id)?)),
        "inverter" => Ok(YieldScope::Inverter(InverterId::from_uuid(id)?)),
        "system" => Ok(YieldScope::System(SystemId::from_uuid(id)?)),
        _ => Err(YieldResultRepositoryError::Corrupt("unknown yield scope")),
    }
}

const fn basis(value: CalculationBasis) -> &'static str {
    match value {
        CalculationBasis::Forecast => "forecast",
        CalculationBasis::Expected => "expected",
    }
}

fn parse_basis(value: &str) -> Result<CalculationBasis, YieldResultRepositoryError> {
    match value {
        "forecast" => Ok(CalculationBasis::Forecast),
        "expected" => Ok(CalculationBasis::Expected),
        _ => Err(YieldResultRepositoryError::Corrupt(
            "unknown calculation basis",
        )),
    }
}

fn parse_retention(value: &str) -> Result<ForecastRetentionClass, YieldResultRepositoryError> {
    match value {
        "working" => Ok(ForecastRetentionClass::Working),
        "issued" => Ok(ForecastRetentionClass::Issued),
        "referenced" => Ok(ForecastRetentionClass::Referenced),
        _ => Err(YieldResultRepositoryError::Corrupt(
            "unknown result retention class",
        )),
    }
}

#[cfg(feature = "sqlite")]
fn sqlite_invalidation(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<YieldInvalidationRecord, YieldResultRepositoryError> {
    invalidation_from_columns(
        uuid_from_blob(&row.try_get::<Vec<u8>, _>("id")?)?,
        SystemId::from_uuid(uuid_from_blob(&row.try_get::<Vec<u8>, _>("system_id")?)?)?,
        row.try_get("range_start")?,
        row.try_get("range_end")?,
        row.try_get("reason")?,
        row.try_get("state")?,
        row.try_get("idempotency_key")?,
        row.try_get("created_at")?,
        row.try_get("completed_at")?,
    )
}

#[cfg(feature = "postgres")]
fn postgres_invalidation(
    row: &sqlx::postgres::PgRow,
) -> Result<YieldInvalidationRecord, YieldResultRepositoryError> {
    invalidation_from_columns(
        row.try_get("id")?,
        SystemId::from_uuid(row.try_get("system_id")?)?,
        row.try_get("range_start")?,
        row.try_get("range_end")?,
        row.try_get("reason")?,
        row.try_get("state")?,
        row.try_get("idempotency_key")?,
        row.try_get("created_at")?,
        row.try_get("completed_at")?,
    )
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn invalidation_from_columns(
    id: Uuid,
    system_id: SystemId,
    range_start: i64,
    range_end: i64,
    reason: String,
    state: String,
    idempotency_key: String,
    created_at: i64,
    completed_at: Option<i64>,
) -> Result<YieldInvalidationRecord, YieldResultRepositoryError> {
    Ok(YieldInvalidationRecord {
        id,
        system_id,
        range: TimeRange::new(timestamp(range_start)?, timestamp(range_end)?)
            .map_err(|_| YieldResultRepositoryError::Corrupt("invalid invalidation range"))?,
        reason: match reason.as_str() {
            "equipment" => YieldInvalidationReason::Equipment,
            "settings" => YieldInvalidationReason::Settings,
            "provider_revision" => YieldInvalidationReason::ProviderRevision,
            "late_telemetry" => YieldInvalidationReason::LateTelemetry,
            "correction" => YieldInvalidationReason::Correction,
            "model_version" => YieldInvalidationReason::ModelVersion,
            _ => {
                return Err(YieldResultRepositoryError::Corrupt(
                    "unknown invalidation reason",
                ));
            }
        },
        state: match state.as_str() {
            "pending" => YieldInvalidationState::Pending,
            "leased" => YieldInvalidationState::Leased,
            "completed" => YieldInvalidationState::Completed,
            "failed" => YieldInvalidationState::Failed,
            _ => {
                return Err(YieldResultRepositoryError::Corrupt(
                    "unknown invalidation state",
                ));
            }
        },
        idempotency_key,
        created_at,
        completed_at,
    })
}

fn timestamp_i64(value: UtcTimestamp) -> Result<i64, YieldResultRepositoryError> {
    i64::try_from(value.epoch_millis())
        .map_err(|_| YieldResultRepositoryError::Validation("timestamp is out of range"))
}

fn timestamp(value: i64) -> Result<UtcTimestamp, YieldResultRepositoryError> {
    UtcTimestamp::from_epoch_millis(value)
        .map_err(|_| YieldResultRepositoryError::Corrupt("timestamp is out of range"))
}

fn nonnegative_i64(value: i64, field: &'static str) -> Result<i64, YieldResultRepositoryError> {
    if value >= 0 {
        Ok(value)
    } else {
        Err(YieldResultRepositoryError::Corrupt(field))
    }
}

fn digest(value: &[u8]) -> Result<[u8; 32], YieldResultRepositoryError> {
    value
        .try_into()
        .map_err(|_| YieldResultRepositoryError::Corrupt("invalid configuration digest"))
}

fn blob(value: Uuid) -> Vec<u8> {
    value.as_bytes().to_vec()
}

fn uuid_from_blob(value: &[u8]) -> Result<Uuid, YieldResultRepositoryError> {
    Uuid::from_slice(value)
        .map_err(|_| YieldResultRepositoryError::Corrupt("invalid UUID storage value"))
}

#[derive(Debug, Error)]
pub enum YieldResultRepositoryError {
    #[error("yield result repository query failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[cfg(feature = "sqlite")]
    #[error("yield result SQLite routing failed: {0}")]
    SqliteRouting(#[from] crate::SqliteRoutingError),
    #[error("yield result repository identifier is invalid: {0}")]
    Identifier(#[from] pvlog_domain::IdentifierError),
    #[error("yield result repository JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yield result repository input is invalid: {0}")]
    Validation(&'static str),
    #[error("yield result repository conflict: {0}")]
    Conflict(&'static str),
    #[error("yield result repository row is corrupt: {0}")]
    Corrupt(&'static str),
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresYieldResultRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresYieldResultRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl YieldResultRepository for SqliteYieldResultRepository {
    fn account_id(&self) -> AccountId {
        self.account.account_id()
    }

    async fn insert_run(
        &self,
        run: &YieldCalculationRunRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldResultRepositoryError> {
        validate_run(run)?;
        let mut writer = self.account.acquire_writer().await?;
        let existing = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT id FROM yield_calculation_runs WHERE system_id=? AND idempotency_key=?",
        )
        .bind(blob(run.system_id.as_uuid()))
        .bind(&run.idempotency_key)
        .fetch_optional(writer.connection())
        .await?;
        if let Some(existing) = existing {
            return if uuid_from_blob(&existing)? == run.id.as_uuid() {
                Ok(WeatherRunInsertOutcome::AlreadyPresent)
            } else {
                Err(YieldResultRepositoryError::Conflict(
                    "calculation idempotency key belongs to another run",
                ))
            };
        }
        sqlx::query(
            "INSERT INTO yield_calculation_runs \
             (id,system_id,weather_run_id,basis,model_identifier,model_revision,configuration_digest, \
              state,requested_at,completed_at,safe_error_code,retention_class,retain_until,referenced_at,idempotency_key) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(blob(run.id.as_uuid()))
        .bind(blob(run.system_id.as_uuid()))
        .bind(blob(run.weather_run_id.as_uuid()))
        .bind(basis(run.basis))
        .bind(&run.model_version.identifier)
        .bind(i64::from(run.model_version.revision))
        .bind(run.configuration_digest.to_vec())
        .bind(run.state.as_str())
        .bind(run.requested_at)
        .bind(run.completed_at)
        .bind(&run.safe_error_code)
        .bind(run.retention_class.as_str())
        .bind(run.retain_until)
        .bind(run.referenced_at)
        .bind(&run.idempotency_key)
        .execute(writer.connection())
        .await?;
        Ok(WeatherRunInsertOutcome::Inserted)
    }

    async fn calculation_run(
        &self,
        id: YieldCalculationRunId,
    ) -> Result<Option<YieldCalculationRunRecord>, YieldResultRepositoryError> {
        let mut connection = self.account.acquire().await?;
        let row = sqlx::query(
            "SELECT id,system_id,weather_run_id,basis,model_identifier,model_revision,configuration_digest, \
             state,requested_at,completed_at,safe_error_code,retention_class,retain_until,referenced_at,idempotency_key \
             FROM yield_calculation_runs WHERE id=?",
        )
        .bind(blob(id.as_uuid()))
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| sqlite_run(&row)).transpose()
    }

    async fn update_run_state(
        &self,
        id: YieldCalculationRunId,
        state: YieldCalculationState,
        completed_at: Option<i64>,
        safe_error_code: Option<&str>,
    ) -> Result<bool, YieldResultRepositoryError> {
        validate_state(state, completed_at, safe_error_code)?;
        let mut writer = self.account.acquire_writer().await?;
        let changed = sqlx::query(
            "UPDATE yield_calculation_runs SET state=?,completed_at=?,safe_error_code=? WHERE id=?",
        )
        .bind(state.as_str())
        .bind(completed_at)
        .bind(safe_error_code)
        .bind(blob(id.as_uuid()))
        .execute(writer.connection())
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    async fn insert_results_and_project(
        &self,
        run: &YieldCalculationRunRecord,
        results: &[StoredYieldResult],
        projected_at: i64,
    ) -> Result<(), YieldResultRepositoryError> {
        validate_result_batch(run, results)?;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        for stored in results {
            insert_sqlite_result(&mut transaction, run, stored).await?;
            let (scope_kind, scope_id) = scope_columns(stored.result.scope);
            sqlx::query(
                "INSERT INTO yield_result_projections \
                 (system_id,basis,scope_kind,scope_id,interval_start,result_id,projected_at) \
                 VALUES (?,?,?,?,?,?,?) ON CONFLICT(system_id,basis,scope_kind,scope_id,interval_start) \
                 DO UPDATE SET result_id=excluded.result_id,projected_at=excluded.projected_at",
            )
            .bind(blob(run.system_id.as_uuid()))
            .bind(basis(run.basis))
            .bind(scope_kind)
            .bind(blob(scope_id))
            .bind(timestamp_i64(stored.result.interval.start)?)
            .bind(blob(stored.result.id.as_uuid()))
            .bind(projected_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn active_results(
        &self,
        system_id: SystemId,
        basis_value: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError> {
        validate_query(range, limit)?;
        let (scope_kind, scope_id) = scope_columns(scope);
        let mut connection = self.account.acquire().await?;
        let rows = sqlx::query(
            "SELECT r.*,c.weather_run_id,c.basis,c.model_identifier,c.model_revision \
             FROM yield_result_projections p JOIN yield_calculation_results r ON r.id=p.result_id \
             JOIN yield_calculation_runs c ON c.id=r.calculation_run_id \
             WHERE p.system_id=? AND p.basis=? AND p.scope_kind=? AND p.scope_id=? \
             AND r.interval_start<? AND r.interval_end>? ORDER BY r.interval_start LIMIT ?",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(basis(basis_value))
        .bind(scope_kind)
        .bind(blob(scope_id))
        .bind(timestamp_i64(range.end)?)
        .bind(timestamp_i64(range.start)?)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await?;
        rows.iter().map(sqlite_result).collect()
    }

    async fn result_history(
        &self,
        system_id: SystemId,
        basis_value: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError> {
        validate_query(range, limit)?;
        let (scope_kind, scope_id) = scope_columns(scope);
        let mut connection = self.account.acquire().await?;
        let rows = sqlx::query(
            "SELECT r.*,c.weather_run_id,c.basis,c.model_identifier,c.model_revision \
             FROM yield_calculation_results r JOIN yield_calculation_runs c ON c.id=r.calculation_run_id \
             WHERE r.system_id=? AND c.basis=? AND r.scope_kind=? AND r.scope_id=? \
             AND r.interval_start<? AND r.interval_end>? \
             ORDER BY r.interval_start,c.requested_at DESC,r.id DESC LIMIT ?",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(basis(basis_value))
        .bind(scope_kind)
        .bind(blob(scope_id))
        .bind(timestamp_i64(range.end)?)
        .bind(timestamp_i64(range.start)?)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await?;
        rows.iter().map(sqlite_result).collect()
    }

    async fn insert_invalidation(
        &self,
        invalidation: &YieldInvalidationRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldResultRepositoryError> {
        validate_invalidation(invalidation)?;
        let mut writer = self.account.acquire_writer().await?;
        let existing = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT id FROM yield_invalidations WHERE system_id=? AND idempotency_key=?",
        )
        .bind(blob(invalidation.system_id.as_uuid()))
        .bind(&invalidation.idempotency_key)
        .fetch_optional(writer.connection())
        .await?;
        if let Some(existing) = existing {
            return if uuid_from_blob(&existing)? == invalidation.id {
                Ok(WeatherRunInsertOutcome::AlreadyPresent)
            } else {
                Err(YieldResultRepositoryError::Conflict(
                    "invalidation idempotency key belongs to another request",
                ))
            };
        }
        sqlx::query(
            "INSERT INTO yield_invalidations \
             (id,system_id,range_start,range_end,reason,state,idempotency_key,created_at,completed_at) \
             VALUES (?,?,?,?,?,'pending',?,?,NULL)",
        )
        .bind(blob(invalidation.id))
        .bind(blob(invalidation.system_id.as_uuid()))
        .bind(timestamp_i64(invalidation.range.start)?)
        .bind(timestamp_i64(invalidation.range.end)?)
        .bind(invalidation.reason.as_str())
        .bind(&invalidation.idempotency_key)
        .bind(invalidation.created_at)
        .execute(writer.connection())
        .await?;
        Ok(WeatherRunInsertOutcome::Inserted)
    }

    async fn pending_invalidations(
        &self,
        system_id: SystemId,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<YieldInvalidationRecord>, YieldResultRepositoryError> {
        validate_query(range, limit)?;
        let mut connection = self.account.acquire().await?;
        let rows = sqlx::query(
            "SELECT * FROM yield_invalidations WHERE system_id=? AND state='pending' \
             AND range_start<? AND range_end>? ORDER BY range_start,created_at,id LIMIT ?",
        )
        .bind(blob(system_id.as_uuid()))
        .bind(timestamp_i64(range.end)?)
        .bind(timestamp_i64(range.start)?)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await?;
        rows.iter().map(sqlite_invalidation).collect()
    }

    async fn complete_invalidation(
        &self,
        id: Uuid,
        completed_at: i64,
    ) -> Result<bool, YieldResultRepositoryError> {
        let mut writer = self.account.acquire_writer().await?;
        let changed = sqlx::query(
            "UPDATE yield_invalidations SET state='completed',completed_at=? \
             WHERE id=? AND state IN ('pending','leased')",
        )
        .bind(completed_at)
        .bind(blob(id))
        .execute(writer.connection())
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    async fn retain_calculation_run(
        &self,
        id: YieldCalculationRunId,
        retention_class: ForecastRetentionClass,
        retain_until: Option<i64>,
        referenced_at: Option<i64>,
    ) -> Result<bool, YieldResultRepositoryError> {
        validate_retention(retention_class, retain_until, referenced_at)?;
        let mut writer = self.account.acquire_writer().await?;
        let changed = sqlx::query(
            "UPDATE yield_calculation_runs SET retention_class=?,retain_until=?,referenced_at=? WHERE id=?",
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

    async fn purge_expired_calculation_runs(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<u64, YieldResultRepositoryError> {
        validate_retention_limit(limit)?;
        let mut writer = self.account.acquire_writer().await?;
        let changed = sqlx::query(
            "DELETE FROM yield_calculation_runs WHERE id IN (SELECT c.id FROM yield_calculation_runs c \
             WHERE c.retention_class='working' AND c.referenced_at IS NULL AND c.retain_until<=? \
             AND NOT EXISTS (SELECT 1 FROM yield_result_projections p JOIN yield_calculation_results r \
                 ON r.id=p.result_id WHERE r.calculation_run_id=c.id) \
             ORDER BY c.retain_until,c.id LIMIT ?)",
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
impl YieldResultRepository for PostgresYieldResultRepository {
    fn account_id(&self) -> AccountId {
        self.account_id
    }

    async fn insert_run(
        &self,
        run: &YieldCalculationRunRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldResultRepositoryError> {
        validate_run(run)?;
        let mut connection = self.connection().await?;
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM account_data.yield_calculation_runs \
             WHERE account_id=$1 AND system_id=$2 AND idempotency_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(run.system_id.as_uuid())
        .bind(&run.idempotency_key)
        .fetch_optional(&mut connection)
        .await?;
        if let Some(existing) = existing {
            return if existing == run.id.as_uuid() {
                Ok(WeatherRunInsertOutcome::AlreadyPresent)
            } else {
                Err(YieldResultRepositoryError::Conflict(
                    "calculation idempotency key belongs to another run",
                ))
            };
        }
        sqlx::query::<sqlx::Postgres>(
            "INSERT INTO account_data.yield_calculation_runs \
             (account_id,id,system_id,weather_run_id,basis,model_identifier,model_revision,configuration_digest, \
              state,requested_at,completed_at,safe_error_code,retention_class,retain_until,referenced_at,idempotency_key) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
        )
        .bind(self.account_id.as_uuid())
        .bind(run.id.as_uuid())
        .bind(run.system_id.as_uuid())
        .bind(run.weather_run_id.as_uuid())
        .bind(basis(run.basis))
        .bind(&run.model_version.identifier)
        .bind(i32::from(run.model_version.revision))
        .bind(run.configuration_digest.to_vec())
        .bind(run.state.as_str())
        .bind(run.requested_at)
        .bind(run.completed_at)
        .bind(&run.safe_error_code)
        .bind(run.retention_class.as_str())
        .bind(run.retain_until)
        .bind(run.referenced_at)
        .bind(&run.idempotency_key)
        .execute(&mut connection)
        .await?;
        Ok(WeatherRunInsertOutcome::Inserted)
    }

    async fn calculation_run(
        &self,
        id: YieldCalculationRunId,
    ) -> Result<Option<YieldCalculationRunRecord>, YieldResultRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT id,system_id,weather_run_id,basis,model_identifier,model_revision,configuration_digest, \
             state,requested_at,completed_at,safe_error_code,retention_class,retain_until,referenced_at,idempotency_key \
             FROM account_data.yield_calculation_runs WHERE account_id=$1 AND id=$2",
        )
        .bind(self.account_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut connection)
        .await?;
        row.map(|row| postgres_run(&row)).transpose()
    }

    async fn update_run_state(
        &self,
        id: YieldCalculationRunId,
        state: YieldCalculationState,
        completed_at: Option<i64>,
        safe_error_code: Option<&str>,
    ) -> Result<bool, YieldResultRepositoryError> {
        validate_state(state, completed_at, safe_error_code)?;
        let mut connection = self.connection().await?;
        let changed = sqlx::query(
            "UPDATE account_data.yield_calculation_runs SET state=$1,completed_at=$2,safe_error_code=$3 \
             WHERE account_id=$4 AND id=$5",
        )
        .bind(state.as_str())
        .bind(completed_at)
        .bind(safe_error_code)
        .bind(self.account_id.as_uuid())
        .bind(id.as_uuid())
        .execute(&mut connection)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    async fn insert_results_and_project(
        &self,
        run: &YieldCalculationRunRecord,
        results: &[StoredYieldResult],
        projected_at: i64,
    ) -> Result<(), YieldResultRepositoryError> {
        validate_result_batch(run, results)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        for stored in results {
            insert_postgres_result(&mut transaction, self.account_id, run, stored).await?;
            let (scope_kind, scope_id) = scope_columns(stored.result.scope);
            sqlx::query::<sqlx::Postgres>(
                "INSERT INTO account_data.yield_result_projections \
                 (account_id,system_id,basis,scope_kind,scope_id,interval_start,result_id,projected_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(account_id,system_id,basis,scope_kind,scope_id,interval_start) \
                 DO UPDATE SET result_id=excluded.result_id,projected_at=excluded.projected_at",
            )
            .bind(self.account_id.as_uuid())
            .bind(run.system_id.as_uuid())
            .bind(basis(run.basis))
            .bind(scope_kind)
            .bind(scope_id)
            .bind(timestamp_i64(stored.result.interval.start)?)
            .bind(stored.result.id.as_uuid())
            .bind(projected_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn active_results(
        &self,
        system_id: SystemId,
        basis_value: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError> {
        self.query_results(system_id, basis_value, scope, range, limit, true)
            .await
    }

    async fn result_history(
        &self,
        system_id: SystemId,
        basis_value: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError> {
        self.query_results(system_id, basis_value, scope, range, limit, false)
            .await
    }

    async fn insert_invalidation(
        &self,
        invalidation: &YieldInvalidationRecord,
    ) -> Result<WeatherRunInsertOutcome, YieldResultRepositoryError> {
        validate_invalidation(invalidation)?;
        let mut connection = self.connection().await?;
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM account_data.yield_invalidations \
             WHERE account_id=$1 AND system_id=$2 AND idempotency_key=$3",
        )
        .bind(self.account_id.as_uuid())
        .bind(invalidation.system_id.as_uuid())
        .bind(&invalidation.idempotency_key)
        .fetch_optional(&mut connection)
        .await?;
        if let Some(existing) = existing {
            return if existing == invalidation.id {
                Ok(WeatherRunInsertOutcome::AlreadyPresent)
            } else {
                Err(YieldResultRepositoryError::Conflict(
                    "invalidation idempotency key belongs to another request",
                ))
            };
        }
        sqlx::query::<sqlx::Postgres>(
            "INSERT INTO account_data.yield_invalidations \
             (account_id,id,system_id,range_start,range_end,reason,state,idempotency_key,created_at,completed_at) \
             VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,NULL)",
        )
        .bind(self.account_id.as_uuid())
        .bind(invalidation.id)
        .bind(invalidation.system_id.as_uuid())
        .bind(timestamp_i64(invalidation.range.start)?)
        .bind(timestamp_i64(invalidation.range.end)?)
        .bind(invalidation.reason.as_str())
        .bind(&invalidation.idempotency_key)
        .bind(invalidation.created_at)
        .execute(&mut connection)
        .await?;
        Ok(WeatherRunInsertOutcome::Inserted)
    }

    async fn pending_invalidations(
        &self,
        system_id: SystemId,
        range: TimeRange,
        limit: u32,
    ) -> Result<Vec<YieldInvalidationRecord>, YieldResultRepositoryError> {
        validate_query(range, limit)?;
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT * FROM account_data.yield_invalidations WHERE account_id=$1 AND system_id=$2 \
             AND state='pending' AND range_start<$3 AND range_end>$4 \
             ORDER BY range_start,created_at,id LIMIT $5",
        )
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(timestamp_i64(range.end)?)
        .bind(timestamp_i64(range.start)?)
        .bind(i64::from(limit))
        .fetch_all(&mut connection)
        .await?;
        rows.iter().map(postgres_invalidation).collect()
    }

    async fn complete_invalidation(
        &self,
        id: Uuid,
        completed_at: i64,
    ) -> Result<bool, YieldResultRepositoryError> {
        let mut connection = self.connection().await?;
        let changed = sqlx::query(
            "UPDATE account_data.yield_invalidations SET state='completed',completed_at=$1 \
             WHERE account_id=$2 AND id=$3 AND state IN ('pending','leased')",
        )
        .bind(completed_at)
        .bind(self.account_id.as_uuid())
        .bind(id)
        .execute(&mut connection)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    async fn retain_calculation_run(
        &self,
        id: YieldCalculationRunId,
        retention_class: ForecastRetentionClass,
        retain_until: Option<i64>,
        referenced_at: Option<i64>,
    ) -> Result<bool, YieldResultRepositoryError> {
        validate_retention(retention_class, retain_until, referenced_at)?;
        let mut connection = self.connection().await?;
        let changed = sqlx::query(
            "UPDATE account_data.yield_calculation_runs \
             SET retention_class=$1,retain_until=$2,referenced_at=$3 WHERE account_id=$4 AND id=$5",
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

    async fn purge_expired_calculation_runs(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<u64, YieldResultRepositoryError> {
        validate_retention_limit(limit)?;
        let mut connection = self.connection().await?;
        let changed = sqlx::query(
            "DELETE FROM account_data.yield_calculation_runs c WHERE c.account_id=$1 AND c.id IN ( \
             SELECT candidate.id FROM account_data.yield_calculation_runs candidate \
             WHERE candidate.account_id=$1 AND candidate.retention_class='working' \
             AND candidate.referenced_at IS NULL AND candidate.retain_until<=$2 \
             AND NOT EXISTS (SELECT 1 FROM account_data.yield_result_projections p \
                 JOIN account_data.yield_calculation_results r ON r.account_id=p.account_id AND r.id=p.result_id \
                 WHERE p.account_id=$1 AND r.calculation_run_id=candidate.id) \
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

#[cfg(feature = "postgres")]
impl PostgresYieldResultRepository {
    async fn query_results(
        &self,
        system_id: SystemId,
        basis_value: CalculationBasis,
        scope: YieldScope,
        range: TimeRange,
        limit: u32,
        active: bool,
    ) -> Result<Vec<StoredYieldResult>, YieldResultRepositoryError> {
        validate_query(range, limit)?;
        let (scope_kind, scope_id) = scope_columns(scope);
        let mut connection = self.connection().await?;
        let sql = if active {
            "SELECT r.*,c.weather_run_id,c.basis,c.model_identifier,c.model_revision \
             FROM account_data.yield_result_projections p \
             JOIN account_data.yield_calculation_results r ON r.account_id=p.account_id AND r.id=p.result_id \
             JOIN account_data.yield_calculation_runs c ON c.account_id=r.account_id AND c.id=r.calculation_run_id \
             WHERE p.account_id=$1 AND p.system_id=$2 AND p.basis=$3 AND p.scope_kind=$4 AND p.scope_id=$5 \
             AND r.interval_start<$6 AND r.interval_end>$7 ORDER BY r.interval_start LIMIT $8"
        } else {
            "SELECT r.*,c.weather_run_id,c.basis,c.model_identifier,c.model_revision \
             FROM account_data.yield_calculation_results r \
             JOIN account_data.yield_calculation_runs c ON c.account_id=r.account_id AND c.id=r.calculation_run_id \
             WHERE r.account_id=$1 AND r.system_id=$2 AND c.basis=$3 AND r.scope_kind=$4 AND r.scope_id=$5 \
             AND r.interval_start<$6 AND r.interval_end>$7 \
             ORDER BY r.interval_start,c.requested_at DESC,r.id DESC LIMIT $8"
        };
        let rows = sqlx::query(sql)
            .bind(self.account_id.as_uuid())
            .bind(system_id.as_uuid())
            .bind(basis(basis_value))
            .bind(scope_kind)
            .bind(scope_id)
            .bind(timestamp_i64(range.end)?)
            .bind(timestamp_i64(range.start)?)
            .bind(i64::from(limit))
            .fetch_all(&mut connection)
            .await?;
        rows.iter().map(postgres_result).collect()
    }
}
