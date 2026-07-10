//! Account-scoped hot telemetry, idempotency, and correction repositories.

#[cfg(feature = "postgres")]
use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, CorrectionId, ObservationId, SystemId};
use serde_json::Value;
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use thiserror::Error;
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredObservation {
    pub id: ObservationId,
    pub system_id: SystemId,
    pub measured_at: i64,
    pub received_at: i64,
    pub source_kind: String,
    pub source_identity: String,
    pub idempotency_identity: Option<String>,
    pub quality_flags: i32,
    pub generation_power_watts: Option<i64>,
    pub generation_energy_wh: Option<i64>,
    pub consumption_power_watts: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub provenance: Value,
    pub canonical_hash: [u8; 32],
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyRecord {
    pub id: Uuid,
    pub principal_type: String,
    pub principal_id: Uuid,
    pub operation: String,
    pub key: String,
    pub request_hash: [u8; 32],
    pub response_status: i32,
    pub response: Value,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrectionRecord {
    pub id: CorrectionId,
    pub system_id: SystemId,
    pub observation_id: ObservationId,
    pub measured_at: i64,
    pub operation: String,
    pub expected_version: i64,
    pub replacement: Option<Value>,
    pub reason: String,
    pub actor_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationInsertOutcome {
    Inserted,
    Duplicate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyOutcome {
    Stored,
    Replay(IdempotencyRecord),
}

#[async_trait]
pub trait TelemetryRepository: Send + Sync {
    fn account_id(&self) -> AccountId;
    async fn insert_observation(
        &self,
        observation: &StoredObservation,
    ) -> Result<ObservationInsertOutcome, TelemetryRepositoryError>;
    async fn observations(
        &self,
        system_id: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<StoredObservation>, TelemetryRepositoryError>;
    async fn store_idempotency(
        &self,
        record: &IdempotencyRecord,
    ) -> Result<IdempotencyOutcome, TelemetryRepositoryError>;
    async fn append_correction(
        &self,
        correction: &CorrectionRecord,
    ) -> Result<(), TelemetryRepositoryError>;
    async fn corrections(
        &self,
        system_id: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<CorrectionRecord>, TelemetryRepositoryError>;
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteTelemetryRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteTelemetryRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresTelemetryRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresTelemetryRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }
    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresTelemetryRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresTelemetryRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl TelemetryRepository for SqliteTelemetryRepository {
    fn account_id(&self) -> AccountId {
        self.account.account_id()
    }
    async fn insert_observation(
        &self,
        o: &StoredObservation,
    ) -> Result<ObservationInsertOutcome, TelemetryRepositoryError> {
        validate_observation(o)?;
        let mut writer = self.account.acquire_writer().await?;
        let result=sqlx::query("INSERT OR IGNORE INTO telemetry_hot (observation_id,system_id,measured_at,received_at,source_kind,source_identity,idempotency_identity,quality_flags,generation_power_watts,generation_energy_wh,consumption_power_watts,consumption_energy_wh,provenance_json,canonical_hash,version) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(blob(o.id.as_uuid())).bind(blob(o.system_id.as_uuid())).bind(o.measured_at).bind(o.received_at).bind(&o.source_kind).bind(&o.source_identity).bind(&o.idempotency_identity).bind(o.quality_flags).bind(o.generation_power_watts).bind(o.generation_energy_wh).bind(o.consumption_power_watts).bind(o.consumption_energy_wh).bind(serde_json::to_string(&o.provenance)?).bind(o.canonical_hash.as_slice()).bind(o.version).execute(writer.connection()).await?;
        if result.rows_affected() == 1 {
            return Ok(ObservationInsertOutcome::Inserted);
        }
        let existing = sqlite_existing_observation(writer.connection(), o).await?;
        match existing {
            Some(hash) if hash == o.canonical_hash => Ok(ObservationInsertOutcome::Duplicate),
            Some(_) => Err(TelemetryRepositoryError::UniquenessConflict),
            None => Err(TelemetryRepositoryError::ConstraintConflict),
        }
    }
    async fn observations(
        &self,
        s: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<StoredObservation>, TelemetryRepositoryError> {
        validate_range(start, end)?;
        let mut c = self.account.acquire().await?;
        let rows=sqlx::query("SELECT observation_id,system_id,measured_at,received_at,source_kind,source_identity,idempotency_identity,quality_flags,generation_power_watts,generation_energy_wh,consumption_power_watts,consumption_energy_wh,provenance_json,canonical_hash,version FROM telemetry_hot WHERE system_id=? AND measured_at>=? AND measured_at<? ORDER BY measured_at,observation_id").bind(blob(s.as_uuid())).bind(start).bind(end).fetch_all(&mut *c).await?;
        rows.iter().map(sqlite_observation).collect()
    }
    async fn store_idempotency(
        &self,
        r: &IdempotencyRecord,
    ) -> Result<IdempotencyOutcome, TelemetryRepositoryError> {
        validate_idempotency(r)?;
        let mut writer = self.account.acquire_writer().await?;
        let existing = sqlite_idempotency(writer.connection(), r).await?;
        if let Some(existing) = existing {
            return idempotency_outcome(existing, r);
        }
        sqlx::query("INSERT INTO idempotency_records (id,principal_type,principal_id,operation,idempotency_key,request_hash,response_status,response_json,created_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?,?)").bind(blob(r.id)).bind(&r.principal_type).bind(blob(r.principal_id)).bind(&r.operation).bind(&r.key).bind(r.request_hash.as_slice()).bind(r.response_status).bind(serde_json::to_string(&r.response)?).bind(r.created_at).bind(r.expires_at).execute(writer.connection()).await?;
        Ok(IdempotencyOutcome::Stored)
    }
    async fn append_correction(
        &self,
        r: &CorrectionRecord,
    ) -> Result<(), TelemetryRepositoryError> {
        validate_correction(r)?;
        let mut writer = self.account.acquire_writer().await?;
        let result=sqlx::query("INSERT OR IGNORE INTO correction_overlays (id,system_id,observation_id,measured_at,operation,expected_version,replacement_json,reason,actor_id,request_id,created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)").bind(blob(r.id.as_uuid())).bind(blob(r.system_id.as_uuid())).bind(blob(r.observation_id.as_uuid())).bind(r.measured_at).bind(&r.operation).bind(r.expected_version).bind(r.replacement.as_ref().map(serde_json::to_string).transpose()?).bind(&r.reason).bind(r.actor_id.map(blob)).bind(r.request_id.map(blob)).bind(r.created_at).execute(writer.connection()).await?;
        if result.rows_affected() == 0 {
            return Err(TelemetryRepositoryError::OptimisticConflict);
        }
        Ok(())
    }
    async fn corrections(
        &self,
        s: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<CorrectionRecord>, TelemetryRepositoryError> {
        validate_range(start, end)?;
        let mut c = self.account.acquire().await?;
        let rows=sqlx::query("SELECT id,system_id,observation_id,measured_at,operation,expected_version,replacement_json,reason,actor_id,request_id,created_at FROM correction_overlays WHERE system_id=? AND measured_at>=? AND measured_at<? ORDER BY measured_at,id").bind(blob(s.as_uuid())).bind(start).bind(end).fetch_all(&mut *c).await?;
        rows.iter().map(sqlite_correction).collect()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl TelemetryRepository for PostgresTelemetryRepository {
    fn account_id(&self) -> AccountId {
        self.account_id
    }
    async fn insert_observation(
        &self,
        o: &StoredObservation,
    ) -> Result<ObservationInsertOutcome, TelemetryRepositoryError> {
        validate_observation(o)?;
        let mut c = self.connection().await?;
        let result=sqlx::query("INSERT INTO telemetry.hot_observations (account_id,observation_id,system_id,measured_at,received_at,source_kind,source_identity,idempotency_identity,quality_flags,generation_power_watts,generation_energy_wh,consumption_power_watts,consumption_energy_wh,provenance,canonical_hash,version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT DO NOTHING").bind(self.account_id.as_uuid()).bind(o.id.as_uuid()).bind(o.system_id.as_uuid()).bind(o.measured_at).bind(o.received_at).bind(&o.source_kind).bind(&o.source_identity).bind(&o.idempotency_identity).bind(o.quality_flags).bind(o.generation_power_watts).bind(o.generation_energy_wh).bind(o.consumption_power_watts).bind(o.consumption_energy_wh).bind(&o.provenance).bind(o.canonical_hash.as_slice()).bind(o.version).execute(&mut c).await?;
        if result.rows_affected() == 1 {
            c.close().await?;
            return Ok(ObservationInsertOutcome::Inserted);
        }
        let hash = pg_existing_observation(&mut c, self.account_id, o).await?;
        c.close().await?;
        match hash {
            Some(hash) if hash == o.canonical_hash => Ok(ObservationInsertOutcome::Duplicate),
            Some(_) => Err(TelemetryRepositoryError::UniquenessConflict),
            None => Err(TelemetryRepositoryError::ConstraintConflict),
        }
    }
    async fn observations(
        &self,
        s: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<StoredObservation>, TelemetryRepositoryError> {
        validate_range(start, end)?;
        let mut c = self.connection().await?;
        let rows=sqlx::query("SELECT observation_id,system_id,measured_at,received_at,source_kind,source_identity,idempotency_identity,quality_flags,generation_power_watts,generation_energy_wh,consumption_power_watts,consumption_energy_wh,provenance,canonical_hash,version FROM telemetry.hot_observations WHERE account_id=$1 AND system_id=$2 AND measured_at>=$3 AND measured_at<$4 ORDER BY measured_at,observation_id").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(start).bind(end).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(pg_observation).collect()
    }
    async fn store_idempotency(
        &self,
        r: &IdempotencyRecord,
    ) -> Result<IdempotencyOutcome, TelemetryRepositoryError> {
        validate_idempotency(r)?;
        let mut c = self.connection().await?;
        if let Some(existing) = pg_idempotency(&mut c, self.account_id, r).await? {
            c.close().await?;
            return idempotency_outcome(existing, r);
        }
        let result=sqlx::query("INSERT INTO telemetry.idempotency_records (account_id,id,principal_type,principal_id,operation,idempotency_key,request_hash,response_status,response,created_at,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING").bind(self.account_id.as_uuid()).bind(r.id).bind(&r.principal_type).bind(r.principal_id).bind(&r.operation).bind(&r.key).bind(r.request_hash.as_slice()).bind(r.response_status).bind(&r.response).bind(r.created_at).bind(r.expires_at).execute(&mut c).await?;
        if result.rows_affected() == 1 {
            c.close().await?;
            return Ok(IdempotencyOutcome::Stored);
        }
        let existing = pg_idempotency(&mut c, self.account_id, r)
            .await?
            .ok_or(TelemetryRepositoryError::ConstraintConflict)?;
        c.close().await?;
        idempotency_outcome(existing, r)
    }
    async fn append_correction(
        &self,
        r: &CorrectionRecord,
    ) -> Result<(), TelemetryRepositoryError> {
        validate_correction(r)?;
        let mut c = self.connection().await?;
        let result=sqlx::query("INSERT INTO telemetry.correction_overlays (account_id,id,system_id,observation_id,measured_at,operation,expected_version,replacement,reason,actor_id,request_id,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(r.system_id.as_uuid()).bind(r.observation_id.as_uuid()).bind(r.measured_at).bind(&r.operation).bind(r.expected_version).bind(&r.replacement).bind(&r.reason).bind(r.actor_id).bind(r.request_id).bind(r.created_at).execute(&mut c).await?;
        c.close().await?;
        if result.rows_affected() == 0 {
            return Err(TelemetryRepositoryError::OptimisticConflict);
        }
        Ok(())
    }
    async fn corrections(
        &self,
        s: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<CorrectionRecord>, TelemetryRepositoryError> {
        validate_range(start, end)?;
        let mut c = self.connection().await?;
        let rows=sqlx::query("SELECT id,system_id,observation_id,measured_at,operation,expected_version,replacement,reason,actor_id,request_id,created_at FROM telemetry.correction_overlays WHERE account_id=$1 AND system_id=$2 AND measured_at>=$3 AND measured_at<$4 ORDER BY measured_at,id").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(start).bind(end).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(pg_correction).collect()
    }
}

fn validate_range(start: i64, end: i64) -> Result<(), TelemetryRepositoryError> {
    if end <= start {
        Err(TelemetryRepositoryError::InvalidRange)
    } else {
        Ok(())
    }
}
fn validate_observation(o: &StoredObservation) -> Result<(), TelemetryRepositoryError> {
    if o.source_identity.is_empty() || o.version <= 0 || !(0..=65535).contains(&o.quality_flags) {
        return Err(TelemetryRepositoryError::InvalidRecord("observation"));
    }
    Ok(())
}
fn validate_idempotency(r: &IdempotencyRecord) -> Result<(), TelemetryRepositoryError> {
    if r.key.is_empty() || r.operation.is_empty() || r.expires_at <= r.created_at {
        return Err(TelemetryRepositoryError::InvalidRecord("idempotency"));
    }
    Ok(())
}
fn validate_correction(r: &CorrectionRecord) -> Result<(), TelemetryRepositoryError> {
    let valid = match r.operation.as_str() {
        "replace" => r.replacement.is_some(),
        "delete" => r.replacement.is_none(),
        _ => false,
    };
    if !valid || r.expected_version <= 0 || r.reason.is_empty() {
        return Err(TelemetryRepositoryError::InvalidRecord("correction"));
    }
    Ok(())
}
fn idempotency_outcome(
    existing: IdempotencyRecord,
    requested: &IdempotencyRecord,
) -> Result<IdempotencyOutcome, TelemetryRepositoryError> {
    if existing.request_hash == requested.request_hash {
        Ok(IdempotencyOutcome::Replay(existing))
    } else {
        Err(TelemetryRepositoryError::IdempotencyConflict)
    }
}
fn fixed(v: Vec<u8>) -> Result<[u8; 32], TelemetryRepositoryError> {
    v.try_into()
        .map_err(|_| TelemetryRepositoryError::InvalidStoredValue)
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn sid<T>(
    v: Vec<u8>,
    f: impl FnOnce(Uuid) -> Result<T, pvlog_domain::IdentifierError>,
) -> Result<T, TelemetryRepositoryError> {
    let id = Uuid::from_slice(&v).map_err(|_| TelemetryRepositoryError::InvalidStoredValue)?;
    f(id).map_err(|_| TelemetryRepositoryError::InvalidStoredValue)
}
#[cfg(feature = "sqlite")]
async fn sqlite_existing_observation(
    c: &mut sqlx::SqliteConnection,
    o: &StoredObservation,
) -> Result<Option<[u8; 32]>, TelemetryRepositoryError> {
    let value=sqlx::query_scalar::<_,Vec<u8>>("SELECT canonical_hash FROM telemetry_hot WHERE (system_id=? AND source_kind=? AND source_identity=? AND measured_at=?) OR (system_id=? AND idempotency_identity IS NOT NULL AND idempotency_identity=?) LIMIT 1").bind(blob(o.system_id.as_uuid())).bind(&o.source_kind).bind(&o.source_identity).bind(o.measured_at).bind(blob(o.system_id.as_uuid())).bind(&o.idempotency_identity).fetch_optional(c).await?;
    value.map(fixed).transpose()
}
#[cfg(feature = "sqlite")]
async fn sqlite_idempotency(
    c: &mut sqlx::SqliteConnection,
    r: &IdempotencyRecord,
) -> Result<Option<IdempotencyRecord>, TelemetryRepositoryError> {
    let row=sqlx::query("SELECT id,principal_type,principal_id,operation,idempotency_key,request_hash,response_status,response_json,created_at,expires_at FROM idempotency_records WHERE principal_type=? AND principal_id=? AND operation=? AND idempotency_key=?").bind(&r.principal_type).bind(blob(r.principal_id)).bind(&r.operation).bind(&r.key).fetch_optional(c).await?;
    row.map(|row| sqlite_idempotency_row(&row)).transpose()
}
#[cfg(feature = "sqlite")]
fn sqlite_observation(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<StoredObservation, TelemetryRepositoryError> {
    Ok(StoredObservation {
        id: sid(r.get("observation_id"), ObservationId::from_uuid)?,
        system_id: sid(r.get("system_id"), SystemId::from_uuid)?,
        measured_at: r.get("measured_at"),
        received_at: r.get("received_at"),
        source_kind: r.get("source_kind"),
        source_identity: r.get("source_identity"),
        idempotency_identity: r.get("idempotency_identity"),
        quality_flags: r.get("quality_flags"),
        generation_power_watts: r.get("generation_power_watts"),
        generation_energy_wh: r.get("generation_energy_wh"),
        consumption_power_watts: r.get("consumption_power_watts"),
        consumption_energy_wh: r.get("consumption_energy_wh"),
        provenance: serde_json::from_str(&r.get::<String, _>("provenance_json"))?,
        canonical_hash: fixed(r.get("canonical_hash"))?,
        version: r.get("version"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_idempotency_row(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<IdempotencyRecord, TelemetryRepositoryError> {
    Ok(IdempotencyRecord {
        id: Uuid::from_slice(&r.get::<Vec<u8>, _>("id"))
            .map_err(|_| TelemetryRepositoryError::InvalidStoredValue)?,
        principal_type: r.get("principal_type"),
        principal_id: Uuid::from_slice(&r.get::<Vec<u8>, _>("principal_id"))
            .map_err(|_| TelemetryRepositoryError::InvalidStoredValue)?,
        operation: r.get("operation"),
        key: r.get("idempotency_key"),
        request_hash: fixed(r.get("request_hash"))?,
        response_status: r.get("response_status"),
        response: serde_json::from_str(&r.get::<String, _>("response_json"))?,
        created_at: r.get("created_at"),
        expires_at: r.get("expires_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_correction(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<CorrectionRecord, TelemetryRepositoryError> {
    Ok(CorrectionRecord {
        id: sid(r.get("id"), CorrectionId::from_uuid)?,
        system_id: sid(r.get("system_id"), SystemId::from_uuid)?,
        observation_id: sid(r.get("observation_id"), ObservationId::from_uuid)?,
        measured_at: r.get("measured_at"),
        operation: r.get("operation"),
        expected_version: r.get("expected_version"),
        replacement: r
            .get::<Option<String>, _>("replacement_json")
            .map(|v| serde_json::from_str(&v))
            .transpose()?,
        reason: r.get("reason"),
        actor_id: r
            .get::<Option<Vec<u8>>, _>("actor_id")
            .map(|v| Uuid::from_slice(&v).map_err(|_| TelemetryRepositoryError::InvalidStoredValue))
            .transpose()?,
        request_id: r
            .get::<Option<Vec<u8>>, _>("request_id")
            .map(|v| Uuid::from_slice(&v).map_err(|_| TelemetryRepositoryError::InvalidStoredValue))
            .transpose()?,
        created_at: r.get("created_at"),
    })
}

#[cfg(feature = "postgres")]
fn pid<T>(
    v: Uuid,
    f: impl FnOnce(Uuid) -> Result<T, pvlog_domain::IdentifierError>,
) -> Result<T, TelemetryRepositoryError> {
    f(v).map_err(|_| TelemetryRepositoryError::InvalidStoredValue)
}
#[cfg(feature = "postgres")]
async fn pg_existing_observation(
    c: &mut PgConnection,
    a: AccountId,
    o: &StoredObservation,
) -> Result<Option<[u8; 32]>, TelemetryRepositoryError> {
    let value=sqlx::query_scalar::<_,Vec<u8>>("SELECT canonical_hash FROM telemetry.hot_observations WHERE account_id=$1 AND ((system_id=$2 AND source_kind=$3 AND source_identity=$4 AND measured_at=$5) OR (system_id=$2 AND idempotency_identity IS NOT NULL AND idempotency_identity=$6)) LIMIT 1").bind(a.as_uuid()).bind(o.system_id.as_uuid()).bind(&o.source_kind).bind(&o.source_identity).bind(o.measured_at).bind(&o.idempotency_identity).fetch_optional(c).await?;
    value.map(fixed).transpose()
}
#[cfg(feature = "postgres")]
async fn pg_idempotency(
    c: &mut PgConnection,
    a: AccountId,
    r: &IdempotencyRecord,
) -> Result<Option<IdempotencyRecord>, TelemetryRepositoryError> {
    let row=sqlx::query("SELECT id,principal_type,principal_id,operation,idempotency_key,request_hash,response_status,response,created_at,expires_at FROM telemetry.idempotency_records WHERE account_id=$1 AND principal_type=$2 AND principal_id=$3 AND operation=$4 AND idempotency_key=$5").bind(a.as_uuid()).bind(&r.principal_type).bind(r.principal_id).bind(&r.operation).bind(&r.key).fetch_optional(c).await?;
    row.map(|row| pg_idempotency_row(&row)).transpose()
}
#[cfg(feature = "postgres")]
fn pg_observation(
    r: &sqlx::postgres::PgRow,
) -> Result<StoredObservation, TelemetryRepositoryError> {
    Ok(StoredObservation {
        id: pid(r.get("observation_id"), ObservationId::from_uuid)?,
        system_id: pid(r.get("system_id"), SystemId::from_uuid)?,
        measured_at: r.get("measured_at"),
        received_at: r.get("received_at"),
        source_kind: r.get("source_kind"),
        source_identity: r.get("source_identity"),
        idempotency_identity: r.get("idempotency_identity"),
        quality_flags: r.get("quality_flags"),
        generation_power_watts: r.get("generation_power_watts"),
        generation_energy_wh: r.get("generation_energy_wh"),
        consumption_power_watts: r.get("consumption_power_watts"),
        consumption_energy_wh: r.get("consumption_energy_wh"),
        provenance: r.get("provenance"),
        canonical_hash: fixed(r.get("canonical_hash"))?,
        version: r.get("version"),
    })
}
#[cfg(feature = "postgres")]
fn pg_idempotency_row(
    r: &sqlx::postgres::PgRow,
) -> Result<IdempotencyRecord, TelemetryRepositoryError> {
    Ok(IdempotencyRecord {
        id: r.get("id"),
        principal_type: r.get("principal_type"),
        principal_id: r.get("principal_id"),
        operation: r.get("operation"),
        key: r.get("idempotency_key"),
        request_hash: fixed(r.get("request_hash"))?,
        response_status: r.get("response_status"),
        response: r.get("response"),
        created_at: r.get("created_at"),
        expires_at: r.get("expires_at"),
    })
}
#[cfg(feature = "postgres")]
fn pg_correction(r: &sqlx::postgres::PgRow) -> Result<CorrectionRecord, TelemetryRepositoryError> {
    Ok(CorrectionRecord {
        id: pid(r.get("id"), CorrectionId::from_uuid)?,
        system_id: pid(r.get("system_id"), SystemId::from_uuid)?,
        observation_id: pid(r.get("observation_id"), ObservationId::from_uuid)?,
        measured_at: r.get("measured_at"),
        operation: r.get("operation"),
        expected_version: r.get("expected_version"),
        replacement: r.get("replacement"),
        reason: r.get("reason"),
        actor_id: r.get("actor_id"),
        request_id: r.get("request_id"),
        created_at: r.get("created_at"),
    })
}

#[derive(Debug, Error)]
pub enum TelemetryRepositoryError {
    #[error("telemetry database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    Routing(#[from] crate::SqliteRoutingError),
    #[error("telemetry JSON value is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid {0} record")]
    InvalidRecord(&'static str),
    #[error("telemetry range must be non-empty")]
    InvalidRange,
    #[error("observation uniqueness key conflicts with different content")]
    UniquenessConflict,
    #[error("idempotency key conflicts with a different request")]
    IdempotencyConflict,
    #[error("correction expected version already exists")]
    OptimisticConflict,
    #[error("database constraint rejected the record")]
    ConstraintConflict,
    #[error("telemetry storage contains an invalid value")]
    InvalidStoredValue,
}
