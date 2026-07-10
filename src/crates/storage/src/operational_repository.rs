//! Rollup, community, integration, and job repositories shared by storage engines.

#[cfg(feature = "postgres")]
use std::fmt;
#[cfg(feature = "sqlite")]
use std::path::PathBuf;

use async_trait::async_trait;
use pvlog_domain::{
    AccountId, AlertRuleId, JobId, ProviderId, SystemId, TeamId, UserId, WebhookSubscriptionId,
};
use serde_json::Value;
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
#[cfg(feature = "sqlite")]
use sqlx::{SqliteConnection, sqlite::SqliteConnectOptions};
use thiserror::Error;
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollupRecord {
    pub system_id: SystemId,
    pub resolution: String,
    pub bucket_start: i64,
    pub bucket_end: i64,
    pub timezone: String,
    pub generation: i64,
    pub point_count: i64,
    pub expected_count: i64,
    pub generation_energy_wh: Option<i64>,
    pub quality_flags: i32,
    pub coverage_basis_points: i32,
    pub calculated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailySummaryRecord {
    pub system_id: SystemId,
    pub local_date: String,
    pub timezone: String,
    pub generation: i64,
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub coverage_basis_points: i32,
    pub quality_flags: i32,
    pub calculated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifetimeSummaryRecord {
    pub system_id: SystemId,
    pub generation: i64,
    pub first_observation_at: Option<i64>,
    pub last_observation_at: Option<i64>,
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub coverage_basis_points: i32,
    pub calculated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamRecord {
    pub id: TeamId,
    pub account_id: AccountId,
    pub name: String,
    pub visibility: String,
    pub owner_user_id: UserId,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamRollupRecord {
    pub team_id: TeamId,
    pub team_account_id: AccountId,
    pub period_start: i64,
    pub period_end: i64,
    pub generation_energy_wh: i64,
    pub normalized_generation_wh_per_kw: Option<i64>,
    pub coverage_basis_points: i32,
    pub source_sequence: i64,
    pub projected_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertRuleRecord {
    pub id: AlertRuleId,
    pub system_id: SystemId,
    pub name: String,
    pub alert_kind: String,
    pub enabled: bool,
    pub condition: Value,
    pub schedule: Value,
    pub debounce_seconds: i64,
    pub cooldown_seconds: i64,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookSubscriptionRecord {
    pub id: WebhookSubscriptionId,
    pub name: String,
    pub endpoint_url: String,
    pub state: String,
    pub event_types: Value,
    pub encryption_key_id: String,
    pub encrypted_signing_secret: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRecord {
    pub id: ProviderId,
    pub provider_kind: String,
    pub name: String,
    pub enabled: bool,
    pub endpoint_url: Option<String>,
    pub credential_secret_ref: Option<String>,
    pub configuration: Value,
    pub license_metadata: Value,
    pub circuit_state: String,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
    pub id: JobId,
    pub job_kind: String,
    pub state: String,
    pub payload: Value,
    pub idempotency_key: Option<String>,
    pub priority: i32,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub available_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[async_trait]
pub trait OperationalRepository: Send + Sync {
    fn account_id(&self) -> AccountId;
    async fn save_rollup(&self, r: &RollupRecord) -> Result<(), OperationalRepositoryError>;
    async fn rollups(
        &self,
        system_id: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<RollupRecord>, OperationalRepositoryError>;
    async fn save_daily_summary(
        &self,
        r: &DailySummaryRecord,
    ) -> Result<(), OperationalRepositoryError>;
    async fn daily_summary(
        &self,
        system_id: SystemId,
        date: &str,
    ) -> Result<Option<DailySummaryRecord>, OperationalRepositoryError>;
    async fn save_lifetime_summary(
        &self,
        r: &LifetimeSummaryRecord,
    ) -> Result<(), OperationalRepositoryError>;
    async fn lifetime_summary(
        &self,
        system_id: SystemId,
    ) -> Result<Option<LifetimeSummaryRecord>, OperationalRepositoryError>;
    async fn save_team(&self, r: &TeamRecord) -> Result<(), OperationalRepositoryError>;
    async fn team(&self, id: TeamId) -> Result<Option<TeamRecord>, OperationalRepositoryError>;
    async fn save_team_rollup(
        &self,
        r: &TeamRollupRecord,
    ) -> Result<(), OperationalRepositoryError>;
    async fn team_rollups(
        &self,
        id: TeamId,
        start: i64,
        end: i64,
    ) -> Result<Vec<TeamRollupRecord>, OperationalRepositoryError>;
    async fn save_alert(&self, r: &AlertRuleRecord) -> Result<(), OperationalRepositoryError>;
    async fn alert(
        &self,
        id: AlertRuleId,
    ) -> Result<Option<AlertRuleRecord>, OperationalRepositoryError>;
    async fn save_webhook(
        &self,
        r: &WebhookSubscriptionRecord,
    ) -> Result<(), OperationalRepositoryError>;
    async fn webhook(
        &self,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSubscriptionRecord>, OperationalRepositoryError>;
    async fn save_provider(&self, r: &ProviderRecord) -> Result<(), OperationalRepositoryError>;
    async fn provider(
        &self,
        id: ProviderId,
    ) -> Result<Option<ProviderRecord>, OperationalRepositoryError>;
    async fn save_job(&self, r: &JobRecord) -> Result<(), OperationalRepositoryError>;
    async fn job(&self, id: JobId) -> Result<Option<JobRecord>, OperationalRepositoryError>;
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteOperationalRepository {
    management_path: PathBuf,
    account: RoutedSqliteAccount,
}
#[cfg(feature = "sqlite")]
impl SqliteOperationalRepository {
    #[must_use]
    pub fn new(management_path: PathBuf, account: RoutedSqliteAccount) -> Self {
        Self {
            management_path,
            account,
        }
    }
    async fn management(&self) -> Result<SqliteConnection, sqlx::Error> {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&self.management_path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresOperationalRepository {
    url: String,
    account_id: AccountId,
}
#[cfg(feature = "postgres")]
impl PostgresOperationalRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }
    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}
#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresOperationalRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresOperationalRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl OperationalRepository for SqliteOperationalRepository {
    fn account_id(&self) -> AccountId {
        self.account.account_id()
    }
    async fn save_rollup(&self, r: &RollupRecord) -> Result<(), OperationalRepositoryError> {
        validate_period(r.bucket_start, r.bucket_end)?;
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO telemetry_rollups (system_id,resolution,bucket_start,bucket_end,timezone,generation,source_generation,point_count,expected_count,generation_energy_sum_wh,quality_flags,coverage_basis_points,calculated_at) VALUES (?,?,?,?,?,?,1,?,?,?,?,?,?) ON CONFLICT(system_id,resolution,bucket_start,generation) DO UPDATE SET bucket_end=excluded.bucket_end,point_count=excluded.point_count,expected_count=excluded.expected_count,generation_energy_sum_wh=excluded.generation_energy_sum_wh,quality_flags=excluded.quality_flags,coverage_basis_points=excluded.coverage_basis_points,calculated_at=excluded.calculated_at").bind(blob(r.system_id.as_uuid())).bind(&r.resolution).bind(r.bucket_start).bind(r.bucket_end).bind(&r.timezone).bind(r.generation).bind(r.point_count).bind(r.expected_count).bind(r.generation_energy_wh).bind(r.quality_flags).bind(r.coverage_basis_points).bind(r.calculated_at).execute(w.connection()).await?;
        Ok(())
    }
    async fn rollups(
        &self,
        s: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<RollupRecord>, OperationalRepositoryError> {
        validate_period(start, end)?;
        let mut c = self.account.acquire().await?;
        let rows=sqlx::query("SELECT system_id,resolution,bucket_start,bucket_end,timezone,generation,point_count,expected_count,generation_energy_sum_wh,quality_flags,coverage_basis_points,calculated_at FROM telemetry_rollups WHERE system_id=? AND bucket_start>=? AND bucket_start<? ORDER BY bucket_start,resolution,generation").bind(blob(s.as_uuid())).bind(start).bind(end).fetch_all(&mut *c).await?;
        rows.iter().map(sqlite_rollup).collect()
    }
    async fn save_daily_summary(
        &self,
        r: &DailySummaryRecord,
    ) -> Result<(), OperationalRepositoryError> {
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO system_daily_summaries (system_id,local_date,timezone,generation,generation_energy_wh,consumption_energy_wh,coverage_basis_points,quality_flags,calculated_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(system_id,local_date,generation) DO UPDATE SET generation_energy_wh=excluded.generation_energy_wh,consumption_energy_wh=excluded.consumption_energy_wh,coverage_basis_points=excluded.coverage_basis_points,quality_flags=excluded.quality_flags,calculated_at=excluded.calculated_at").bind(blob(r.system_id.as_uuid())).bind(&r.local_date).bind(&r.timezone).bind(r.generation).bind(r.generation_energy_wh).bind(r.consumption_energy_wh).bind(r.coverage_basis_points).bind(r.quality_flags).bind(r.calculated_at).execute(w.connection()).await?;
        Ok(())
    }
    async fn daily_summary(
        &self,
        s: SystemId,
        date: &str,
    ) -> Result<Option<DailySummaryRecord>, OperationalRepositoryError> {
        let mut c = self.account.acquire().await?;
        let row=sqlx::query("SELECT system_id,local_date,timezone,generation,generation_energy_wh,consumption_energy_wh,coverage_basis_points,quality_flags,calculated_at FROM system_daily_summaries WHERE system_id=? AND local_date=? ORDER BY generation DESC LIMIT 1").bind(blob(s.as_uuid())).bind(date).fetch_optional(&mut *c).await?;
        row.map(|r| sqlite_daily(&r)).transpose()
    }
    async fn save_lifetime_summary(
        &self,
        r: &LifetimeSummaryRecord,
    ) -> Result<(), OperationalRepositoryError> {
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO system_lifetime_summaries (system_id,generation,first_observation_at,last_observation_at,generation_energy_wh,consumption_energy_wh,coverage_basis_points,calculated_at) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(system_id) DO UPDATE SET generation=excluded.generation,first_observation_at=excluded.first_observation_at,last_observation_at=excluded.last_observation_at,generation_energy_wh=excluded.generation_energy_wh,consumption_energy_wh=excluded.consumption_energy_wh,coverage_basis_points=excluded.coverage_basis_points,calculated_at=excluded.calculated_at")
            .bind(blob(r.system_id.as_uuid()))
            .bind(r.generation)
            .bind(r.first_observation_at)
            .bind(r.last_observation_at)
            .bind(r.generation_energy_wh)
            .bind(r.consumption_energy_wh)
            .bind(r.coverage_basis_points)
            .bind(r.calculated_at)
            .execute(w.connection())
            .await?;
        Ok(())
    }
    async fn lifetime_summary(
        &self,
        s: SystemId,
    ) -> Result<Option<LifetimeSummaryRecord>, OperationalRepositoryError> {
        let mut c = self.account.acquire().await?;
        let row = sqlx::query("SELECT system_id,generation,first_observation_at,last_observation_at,generation_energy_wh,consumption_energy_wh,coverage_basis_points,calculated_at FROM system_lifetime_summaries WHERE system_id=?")
            .bind(blob(s.as_uuid()))
            .fetch_optional(&mut *c)
            .await?;
        row.map(|r| sqlite_lifetime(&r)).transpose()
    }
    async fn save_team(&self, r: &TeamRecord) -> Result<(), OperationalRepositoryError> {
        if r.account_id != self.account_id() {
            return Err(OperationalRepositoryError::AccountMismatch);
        }
        let mut c = self.management().await?;
        sqlx::query("INSERT INTO teams (id,account_id,name,visibility,owner_user_id,created_at,updated_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,visibility=excluded.visibility,owner_user_id=excluded.owner_user_id,updated_at=excluded.updated_at").bind(blob(r.id.as_uuid())).bind(blob(r.account_id.as_uuid())).bind(&r.name).bind(&r.visibility).bind(blob(r.owner_user_id.as_uuid())).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn team(&self, id: TeamId) -> Result<Option<TeamRecord>, OperationalRepositoryError> {
        let mut c = self.management().await?;
        let row=sqlx::query("SELECT id,account_id,name,visibility,owner_user_id,created_at,updated_at FROM teams WHERE id=? AND account_id=?").bind(blob(id.as_uuid())).bind(blob(self.account_id().as_uuid())).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| sqlite_team(&r)).transpose()
    }
    async fn save_team_rollup(
        &self,
        r: &TeamRollupRecord,
    ) -> Result<(), OperationalRepositoryError> {
        if r.team_account_id != self.account_id() {
            return Err(OperationalRepositoryError::AccountMismatch);
        }
        validate_period(r.period_start, r.period_end)?;
        let mut c = self.management().await?;
        sqlx::query("INSERT INTO team_rollup_projections (team_account_id,team_id,period_start,period_end,generation_energy_wh,normalized_generation_wh_per_kw,coverage_basis_points,source_sequence,projected_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(team_account_id,team_id,period_start) DO UPDATE SET period_end=excluded.period_end,generation_energy_wh=excluded.generation_energy_wh,normalized_generation_wh_per_kw=excluded.normalized_generation_wh_per_kw,coverage_basis_points=excluded.coverage_basis_points,source_sequence=excluded.source_sequence,projected_at=excluded.projected_at").bind(blob(r.team_account_id.as_uuid())).bind(blob(r.team_id.as_uuid())).bind(r.period_start).bind(r.period_end).bind(r.generation_energy_wh).bind(r.normalized_generation_wh_per_kw).bind(r.coverage_basis_points).bind(r.source_sequence).bind(r.projected_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn team_rollups(
        &self,
        id: TeamId,
        start: i64,
        end: i64,
    ) -> Result<Vec<TeamRollupRecord>, OperationalRepositoryError> {
        validate_period(start, end)?;
        let mut c = self.management().await?;
        let rows=sqlx::query("SELECT team_account_id,team_id,period_start,period_end,generation_energy_wh,normalized_generation_wh_per_kw,coverage_basis_points,source_sequence,projected_at FROM team_rollup_projections WHERE team_account_id=? AND team_id=? AND period_start>=? AND period_start<? ORDER BY period_start").bind(blob(self.account_id().as_uuid())).bind(blob(id.as_uuid())).bind(start).bind(end).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(sqlite_team_rollup).collect()
    }
    async fn save_alert(&self, r: &AlertRuleRecord) -> Result<(), OperationalRepositoryError> {
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO alert_rules (id,system_id,name,alert_kind,enabled,condition_json,schedule_json,debounce_seconds,cooldown_seconds,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,alert_kind=excluded.alert_kind,enabled=excluded.enabled,condition_json=excluded.condition_json,schedule_json=excluded.schedule_json,debounce_seconds=excluded.debounce_seconds,cooldown_seconds=excluded.cooldown_seconds,updated_at=excluded.updated_at,version=version+1").bind(blob(r.id.as_uuid())).bind(blob(r.system_id.as_uuid())).bind(&r.name).bind(&r.alert_kind).bind(r.enabled).bind(serde_json::to_string(&r.condition)?).bind(serde_json::to_string(&r.schedule)?).bind(r.debounce_seconds).bind(r.cooldown_seconds).bind(r.created_at).bind(r.updated_at).execute(w.connection()).await?;
        Ok(())
    }
    async fn alert(
        &self,
        id: AlertRuleId,
    ) -> Result<Option<AlertRuleRecord>, OperationalRepositoryError> {
        let mut c = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,system_id,name,alert_kind,enabled,condition_json,schedule_json,debounce_seconds,cooldown_seconds,created_at,updated_at FROM alert_rules WHERE id=?").bind(blob(id.as_uuid())).fetch_optional(&mut *c).await?;
        row.map(|r| sqlite_alert(&r)).transpose()
    }
    async fn save_webhook(
        &self,
        r: &WebhookSubscriptionRecord,
    ) -> Result<(), OperationalRepositoryError> {
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO webhook_subscriptions (id,name,endpoint_url,state,event_types_json,encryption_key_id,encrypted_signing_secret,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,endpoint_url=excluded.endpoint_url,state=excluded.state,event_types_json=excluded.event_types_json,encryption_key_id=excluded.encryption_key_id,encrypted_signing_secret=excluded.encrypted_signing_secret,updated_at=excluded.updated_at,version=version+1").bind(blob(r.id.as_uuid())).bind(&r.name).bind(&r.endpoint_url).bind(&r.state).bind(serde_json::to_string(&r.event_types)?).bind(&r.encryption_key_id).bind(&r.encrypted_signing_secret).bind(r.created_at).bind(r.updated_at).execute(w.connection()).await?;
        Ok(())
    }
    async fn webhook(
        &self,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSubscriptionRecord>, OperationalRepositoryError> {
        let mut c = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,name,endpoint_url,state,event_types_json,encryption_key_id,encrypted_signing_secret,created_at,updated_at FROM webhook_subscriptions WHERE id=?").bind(blob(id.as_uuid())).fetch_optional(&mut *c).await?;
        row.map(|r| sqlite_webhook(&r)).transpose()
    }
    async fn save_provider(&self, r: &ProviderRecord) -> Result<(), OperationalRepositoryError> {
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO provider_configurations (id,provider_kind,name,enabled,endpoint_url,credential_secret_ref,configuration_json,license_metadata_json,circuit_state,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,enabled=excluded.enabled,endpoint_url=excluded.endpoint_url,credential_secret_ref=excluded.credential_secret_ref,configuration_json=excluded.configuration_json,license_metadata_json=excluded.license_metadata_json,circuit_state=excluded.circuit_state,updated_at=excluded.updated_at").bind(blob(r.id.as_uuid())).bind(&r.provider_kind).bind(&r.name).bind(r.enabled).bind(&r.endpoint_url).bind(&r.credential_secret_ref).bind(serde_json::to_string(&r.configuration)?).bind(serde_json::to_string(&r.license_metadata)?).bind(&r.circuit_state).bind(r.created_at).bind(r.updated_at).execute(w.connection()).await?;
        Ok(())
    }
    async fn provider(
        &self,
        id: ProviderId,
    ) -> Result<Option<ProviderRecord>, OperationalRepositoryError> {
        let mut c = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,provider_kind,name,enabled,endpoint_url,credential_secret_ref,configuration_json,license_metadata_json,circuit_state,created_at,updated_at FROM provider_configurations WHERE id=?").bind(blob(id.as_uuid())).fetch_optional(&mut *c).await?;
        row.map(|r| sqlite_provider(&r)).transpose()
    }
    async fn save_job(&self, r: &JobRecord) -> Result<(), OperationalRepositoryError> {
        let mut w = self.account.acquire_writer().await?;
        sqlx::query("INSERT INTO account_jobs (id,job_kind,state,payload_json,idempotency_key,priority,attempt_count,max_attempts,available_at,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET state=excluded.state,payload_json=excluded.payload_json,priority=excluded.priority,attempt_count=excluded.attempt_count,max_attempts=excluded.max_attempts,available_at=excluded.available_at,updated_at=excluded.updated_at").bind(blob(r.id.as_uuid())).bind(&r.job_kind).bind(&r.state).bind(serde_json::to_string(&r.payload)?).bind(&r.idempotency_key).bind(r.priority).bind(r.attempt_count).bind(r.max_attempts).bind(r.available_at).bind(r.created_at).bind(r.updated_at).execute(w.connection()).await?;
        Ok(())
    }
    async fn job(&self, id: JobId) -> Result<Option<JobRecord>, OperationalRepositoryError> {
        let mut c = self.account.acquire().await?;
        let row=sqlx::query("SELECT id,job_kind,state,payload_json,idempotency_key,priority,attempt_count,max_attempts,available_at,created_at,updated_at FROM account_jobs WHERE id=?").bind(blob(id.as_uuid())).fetch_optional(&mut *c).await?;
        row.map(|r| sqlite_job(&r)).transpose()
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl OperationalRepository for PostgresOperationalRepository {
    fn account_id(&self) -> AccountId {
        self.account_id
    }
    async fn save_rollup(&self, r: &RollupRecord) -> Result<(), OperationalRepositoryError> {
        validate_period(r.bucket_start, r.bucket_end)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO telemetry.rollups (account_id,system_id,resolution,bucket_start,bucket_end,timezone,generation,source_generation,point_count,expected_count,generation_energy_sum_wh,quality_flags,coverage_basis_points,calculated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,1,$8,$9,$10,$11,$12,$13) ON CONFLICT(account_id,system_id,resolution,bucket_start,generation) DO UPDATE SET bucket_end=excluded.bucket_end,point_count=excluded.point_count,expected_count=excluded.expected_count,generation_energy_sum_wh=excluded.generation_energy_sum_wh,quality_flags=excluded.quality_flags,coverage_basis_points=excluded.coverage_basis_points,calculated_at=excluded.calculated_at").bind(self.account_id.as_uuid()).bind(r.system_id.as_uuid()).bind(&r.resolution).bind(r.bucket_start).bind(r.bucket_end).bind(&r.timezone).bind(r.generation).bind(r.point_count).bind(r.expected_count).bind(r.generation_energy_wh).bind(r.quality_flags).bind(r.coverage_basis_points).bind(r.calculated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn rollups(
        &self,
        s: SystemId,
        start: i64,
        end: i64,
    ) -> Result<Vec<RollupRecord>, OperationalRepositoryError> {
        validate_period(start, end)?;
        let mut c = self.connection().await?;
        let rows=sqlx::query("SELECT system_id,resolution,bucket_start,bucket_end,timezone,generation,point_count,expected_count,generation_energy_sum_wh,quality_flags,coverage_basis_points,calculated_at FROM telemetry.rollups WHERE account_id=$1 AND system_id=$2 AND bucket_start>=$3 AND bucket_start<$4 ORDER BY bucket_start,resolution,generation").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(start).bind(end).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(pg_rollup).collect()
    }
    async fn save_daily_summary(
        &self,
        r: &DailySummaryRecord,
    ) -> Result<(), OperationalRepositoryError> {
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO telemetry.daily_summaries (account_id,system_id,local_date,timezone,generation,generation_energy_wh,consumption_energy_wh,coverage_basis_points,quality_flags,calculated_at) VALUES ($1,$2,$3::date,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(account_id,system_id,local_date,generation) DO UPDATE SET generation_energy_wh=excluded.generation_energy_wh,consumption_energy_wh=excluded.consumption_energy_wh,coverage_basis_points=excluded.coverage_basis_points,quality_flags=excluded.quality_flags,calculated_at=excluded.calculated_at").bind(self.account_id.as_uuid()).bind(r.system_id.as_uuid()).bind(&r.local_date).bind(&r.timezone).bind(r.generation).bind(r.generation_energy_wh).bind(r.consumption_energy_wh).bind(r.coverage_basis_points).bind(r.quality_flags).bind(r.calculated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn daily_summary(
        &self,
        s: SystemId,
        date: &str,
    ) -> Result<Option<DailySummaryRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT system_id,local_date::text AS local_date,timezone,generation,generation_energy_wh,consumption_energy_wh,coverage_basis_points,quality_flags,calculated_at FROM telemetry.daily_summaries WHERE account_id=$1 AND system_id=$2 AND local_date=$3::date ORDER BY generation DESC LIMIT 1").bind(self.account_id.as_uuid()).bind(s.as_uuid()).bind(date).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_daily(&r)).transpose()
    }
    async fn save_lifetime_summary(
        &self,
        r: &LifetimeSummaryRecord,
    ) -> Result<(), OperationalRepositoryError> {
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO telemetry.lifetime_summaries (account_id,system_id,generation,first_observation_at,last_observation_at,generation_energy_wh,consumption_energy_wh,coverage_basis_points,calculated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(account_id,system_id) DO UPDATE SET generation=excluded.generation,first_observation_at=excluded.first_observation_at,last_observation_at=excluded.last_observation_at,generation_energy_wh=excluded.generation_energy_wh,consumption_energy_wh=excluded.consumption_energy_wh,coverage_basis_points=excluded.coverage_basis_points,calculated_at=excluded.calculated_at").bind(self.account_id.as_uuid()).bind(r.system_id.as_uuid()).bind(r.generation).bind(r.first_observation_at).bind(r.last_observation_at).bind(r.generation_energy_wh).bind(r.consumption_energy_wh).bind(r.coverage_basis_points).bind(r.calculated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn lifetime_summary(
        &self,
        s: SystemId,
    ) -> Result<Option<LifetimeSummaryRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT system_id,generation,first_observation_at,last_observation_at,generation_energy_wh,consumption_energy_wh,coverage_basis_points,calculated_at FROM telemetry.lifetime_summaries WHERE account_id=$1 AND system_id=$2").bind(self.account_id.as_uuid()).bind(s.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_lifetime(&r)).transpose()
    }
    async fn save_team(&self, r: &TeamRecord) -> Result<(), OperationalRepositoryError> {
        if r.account_id != self.account_id {
            return Err(OperationalRepositoryError::AccountMismatch);
        }
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO community.teams (account_id,id,name,visibility,owner_user_id,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,visibility=excluded.visibility,owner_user_id=excluded.owner_user_id,updated_at=excluded.updated_at").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(&r.name).bind(&r.visibility).bind(r.owner_user_id.as_uuid()).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn team(&self, id: TeamId) -> Result<Option<TeamRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,account_id,name,visibility,owner_user_id,created_at,updated_at FROM community.teams WHERE account_id=$1 AND id=$2").bind(self.account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_team(&r)).transpose()
    }
    async fn save_team_rollup(
        &self,
        r: &TeamRollupRecord,
    ) -> Result<(), OperationalRepositoryError> {
        if r.team_account_id != self.account_id {
            return Err(OperationalRepositoryError::AccountMismatch);
        }
        validate_period(r.period_start, r.period_end)?;
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO community.team_rollup_projections (account_id,team_id,period_start,period_end,generation_energy_wh,normalized_generation_wh_per_kw,coverage_basis_points,source_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(account_id,team_id,period_start) DO UPDATE SET period_end=excluded.period_end,generation_energy_wh=excluded.generation_energy_wh,normalized_generation_wh_per_kw=excluded.normalized_generation_wh_per_kw,coverage_basis_points=excluded.coverage_basis_points,source_sequence=excluded.source_sequence,projected_at=excluded.projected_at").bind(self.account_id.as_uuid()).bind(r.team_id.as_uuid()).bind(r.period_start).bind(r.period_end).bind(r.generation_energy_wh).bind(r.normalized_generation_wh_per_kw).bind(r.coverage_basis_points).bind(r.source_sequence).bind(r.projected_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn team_rollups(
        &self,
        id: TeamId,
        start: i64,
        end: i64,
    ) -> Result<Vec<TeamRollupRecord>, OperationalRepositoryError> {
        validate_period(start, end)?;
        let mut c = self.connection().await?;
        let rows=sqlx::query("SELECT account_id,team_id,period_start,period_end,generation_energy_wh,normalized_generation_wh_per_kw,coverage_basis_points,source_sequence,projected_at FROM community.team_rollup_projections WHERE account_id=$1 AND team_id=$2 AND period_start>=$3 AND period_start<$4 ORDER BY period_start").bind(self.account_id.as_uuid()).bind(id.as_uuid()).bind(start).bind(end).fetch_all(&mut c).await?;
        c.close().await?;
        rows.iter().map(pg_team_rollup).collect()
    }
    async fn save_alert(&self, r: &AlertRuleRecord) -> Result<(), OperationalRepositoryError> {
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO account_data.alert_rules (account_id,id,system_id,name,alert_kind,enabled,condition,schedule,debounce_seconds,cooldown_seconds,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,alert_kind=excluded.alert_kind,enabled=excluded.enabled,condition=excluded.condition,schedule=excluded.schedule,debounce_seconds=excluded.debounce_seconds,cooldown_seconds=excluded.cooldown_seconds,updated_at=excluded.updated_at,version=account_data.alert_rules.version+1").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(r.system_id.as_uuid()).bind(&r.name).bind(&r.alert_kind).bind(r.enabled).bind(&r.condition).bind(&r.schedule).bind(r.debounce_seconds).bind(r.cooldown_seconds).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn alert(
        &self,
        id: AlertRuleId,
    ) -> Result<Option<AlertRuleRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,system_id,name,alert_kind,enabled,condition,schedule,debounce_seconds,cooldown_seconds,created_at,updated_at FROM account_data.alert_rules WHERE account_id=$1 AND id=$2").bind(self.account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_alert(&r)).transpose()
    }
    async fn save_webhook(
        &self,
        r: &WebhookSubscriptionRecord,
    ) -> Result<(), OperationalRepositoryError> {
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO integrations.webhook_subscriptions (account_id,id,name,endpoint_url,state,event_types,encryption_key_id,encrypted_signing_secret,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,endpoint_url=excluded.endpoint_url,state=excluded.state,event_types=excluded.event_types,encryption_key_id=excluded.encryption_key_id,encrypted_signing_secret=excluded.encrypted_signing_secret,updated_at=excluded.updated_at").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(&r.name).bind(&r.endpoint_url).bind(&r.state).bind(&r.event_types).bind(&r.encryption_key_id).bind(&r.encrypted_signing_secret).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn webhook(
        &self,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSubscriptionRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,name,endpoint_url,state,event_types,encryption_key_id,encrypted_signing_secret,created_at,updated_at FROM integrations.webhook_subscriptions WHERE account_id=$1 AND id=$2").bind(self.account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_webhook(&r)).transpose()
    }
    async fn save_provider(&self, r: &ProviderRecord) -> Result<(), OperationalRepositoryError> {
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO integrations.providers (account_id,id,provider_kind,name,enabled,endpoint_url,credential_secret_ref,configuration,license_metadata,circuit_state,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,enabled=excluded.enabled,endpoint_url=excluded.endpoint_url,credential_secret_ref=excluded.credential_secret_ref,configuration=excluded.configuration,license_metadata=excluded.license_metadata,circuit_state=excluded.circuit_state,updated_at=excluded.updated_at").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(&r.provider_kind).bind(&r.name).bind(r.enabled).bind(&r.endpoint_url).bind(&r.credential_secret_ref).bind(&r.configuration).bind(&r.license_metadata).bind(&r.circuit_state).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn provider(
        &self,
        id: ProviderId,
    ) -> Result<Option<ProviderRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,provider_kind,name,enabled,endpoint_url,credential_secret_ref,configuration,license_metadata,circuit_state,created_at,updated_at FROM integrations.providers WHERE account_id=$1 AND id=$2").bind(self.account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_provider(&r)).transpose()
    }
    async fn save_job(&self, r: &JobRecord) -> Result<(), OperationalRepositoryError> {
        let mut c = self.connection().await?;
        sqlx::query("INSERT INTO jobs.account_jobs (account_id,id,job_kind,state,payload,idempotency_key,priority,attempt_count,max_attempts,available_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(account_id,id) DO UPDATE SET state=excluded.state,payload=excluded.payload,priority=excluded.priority,attempt_count=excluded.attempt_count,max_attempts=excluded.max_attempts,available_at=excluded.available_at,updated_at=excluded.updated_at").bind(self.account_id.as_uuid()).bind(r.id.as_uuid()).bind(&r.job_kind).bind(&r.state).bind(&r.payload).bind(&r.idempotency_key).bind(r.priority).bind(r.attempt_count).bind(r.max_attempts).bind(r.available_at).bind(r.created_at).bind(r.updated_at).execute(&mut c).await?;
        c.close().await?;
        Ok(())
    }
    async fn job(&self, id: JobId) -> Result<Option<JobRecord>, OperationalRepositoryError> {
        let mut c = self.connection().await?;
        let row=sqlx::query("SELECT id,job_kind,state,payload,idempotency_key,priority,attempt_count,max_attempts,available_at,created_at,updated_at FROM jobs.account_jobs WHERE account_id=$1 AND id=$2").bind(self.account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut c).await?;
        c.close().await?;
        row.map(|r| pg_job(&r)).transpose()
    }
}

fn validate_period(start: i64, end: i64) -> Result<(), OperationalRepositoryError> {
    if end <= start {
        Err(OperationalRepositoryError::InvalidPeriod)
    } else {
        Ok(())
    }
}

#[cfg(feature = "postgres")]
fn pid<T>(
    id: Uuid,
    constructor: impl FnOnce(Uuid) -> Result<T, pvlog_domain::IdentifierError>,
) -> Result<T, OperationalRepositoryError> {
    constructor(id).map_err(|_| OperationalRepositoryError::InvalidStoredValue)
}

#[cfg(feature = "postgres")]
fn pg_rollup(r: &sqlx::postgres::PgRow) -> Result<RollupRecord, OperationalRepositoryError> {
    Ok(RollupRecord {
        system_id: pid(r.get("system_id"), SystemId::from_uuid)?,
        resolution: r.get("resolution"),
        bucket_start: r.get("bucket_start"),
        bucket_end: r.get("bucket_end"),
        timezone: r.get("timezone"),
        generation: r.get("generation"),
        point_count: r.get("point_count"),
        expected_count: r.get("expected_count"),
        generation_energy_wh: r.get("generation_energy_sum_wh"),
        quality_flags: r.get("quality_flags"),
        coverage_basis_points: r.get("coverage_basis_points"),
        calculated_at: r.get("calculated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_daily(r: &sqlx::postgres::PgRow) -> Result<DailySummaryRecord, OperationalRepositoryError> {
    Ok(DailySummaryRecord {
        system_id: pid(r.get("system_id"), SystemId::from_uuid)?,
        local_date: r.get("local_date"),
        timezone: r.get("timezone"),
        generation: r.get("generation"),
        generation_energy_wh: r.get("generation_energy_wh"),
        consumption_energy_wh: r.get("consumption_energy_wh"),
        coverage_basis_points: r.get("coverage_basis_points"),
        quality_flags: r.get("quality_flags"),
        calculated_at: r.get("calculated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_lifetime(
    r: &sqlx::postgres::PgRow,
) -> Result<LifetimeSummaryRecord, OperationalRepositoryError> {
    Ok(LifetimeSummaryRecord {
        system_id: pid(r.get("system_id"), SystemId::from_uuid)?,
        generation: r.get("generation"),
        first_observation_at: r.get("first_observation_at"),
        last_observation_at: r.get("last_observation_at"),
        generation_energy_wh: r.get("generation_energy_wh"),
        consumption_energy_wh: r.get("consumption_energy_wh"),
        coverage_basis_points: r.get("coverage_basis_points"),
        calculated_at: r.get("calculated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_team(r: &sqlx::postgres::PgRow) -> Result<TeamRecord, OperationalRepositoryError> {
    Ok(TeamRecord {
        id: pid(r.get("id"), TeamId::from_uuid)?,
        account_id: pid(r.get("account_id"), AccountId::from_uuid)?,
        name: r.get("name"),
        visibility: r.get("visibility"),
        owner_user_id: pid(r.get("owner_user_id"), UserId::from_uuid)?,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_team_rollup(
    r: &sqlx::postgres::PgRow,
) -> Result<TeamRollupRecord, OperationalRepositoryError> {
    Ok(TeamRollupRecord {
        team_id: pid(r.get("team_id"), TeamId::from_uuid)?,
        team_account_id: pid(r.get("account_id"), AccountId::from_uuid)?,
        period_start: r.get("period_start"),
        period_end: r.get("period_end"),
        generation_energy_wh: r.get("generation_energy_wh"),
        normalized_generation_wh_per_kw: r.get("normalized_generation_wh_per_kw"),
        coverage_basis_points: r.get("coverage_basis_points"),
        source_sequence: r.get("source_sequence"),
        projected_at: r.get("projected_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_alert(r: &sqlx::postgres::PgRow) -> Result<AlertRuleRecord, OperationalRepositoryError> {
    Ok(AlertRuleRecord {
        id: pid(r.get("id"), AlertRuleId::from_uuid)?,
        system_id: pid(r.get("system_id"), SystemId::from_uuid)?,
        name: r.get("name"),
        alert_kind: r.get("alert_kind"),
        enabled: r.get("enabled"),
        condition: r.get("condition"),
        schedule: r.get("schedule"),
        debounce_seconds: r.get("debounce_seconds"),
        cooldown_seconds: r.get("cooldown_seconds"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_webhook(
    r: &sqlx::postgres::PgRow,
) -> Result<WebhookSubscriptionRecord, OperationalRepositoryError> {
    Ok(WebhookSubscriptionRecord {
        id: pid(r.get("id"), WebhookSubscriptionId::from_uuid)?,
        name: r.get("name"),
        endpoint_url: r.get("endpoint_url"),
        state: r.get("state"),
        event_types: r.get("event_types"),
        encryption_key_id: r.get("encryption_key_id"),
        encrypted_signing_secret: r.get("encrypted_signing_secret"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_provider(r: &sqlx::postgres::PgRow) -> Result<ProviderRecord, OperationalRepositoryError> {
    Ok(ProviderRecord {
        id: pid(r.get("id"), ProviderId::from_uuid)?,
        provider_kind: r.get("provider_kind"),
        name: r.get("name"),
        enabled: r.get("enabled"),
        endpoint_url: r.get("endpoint_url"),
        credential_secret_ref: r.get("credential_secret_ref"),
        configuration: r.get("configuration"),
        license_metadata: r.get("license_metadata"),
        circuit_state: r.get("circuit_state"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn pg_job(r: &sqlx::postgres::PgRow) -> Result<JobRecord, OperationalRepositoryError> {
    Ok(JobRecord {
        id: pid(r.get("id"), JobId::from_uuid)?,
        job_kind: r.get("job_kind"),
        state: r.get("state"),
        payload: r.get("payload"),
        idempotency_key: r.get("idempotency_key"),
        priority: r.get("priority"),
        attempt_count: r.get("attempt_count"),
        max_attempts: r.get("max_attempts"),
        available_at: r.get("available_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
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
) -> Result<T, OperationalRepositoryError> {
    let id = Uuid::from_slice(&v).map_err(|_| OperationalRepositoryError::InvalidStoredValue)?;
    f(id).map_err(|_| OperationalRepositoryError::InvalidStoredValue)
}
#[cfg(feature = "sqlite")]
fn sqlite_rollup(r: &sqlx::sqlite::SqliteRow) -> Result<RollupRecord, OperationalRepositoryError> {
    Ok(RollupRecord {
        system_id: sid(r.get("system_id"), SystemId::from_uuid)?,
        resolution: r.get("resolution"),
        bucket_start: r.get("bucket_start"),
        bucket_end: r.get("bucket_end"),
        timezone: r.get("timezone"),
        generation: r.get("generation"),
        point_count: r.get("point_count"),
        expected_count: r.get("expected_count"),
        generation_energy_wh: r.get("generation_energy_sum_wh"),
        quality_flags: r.get("quality_flags"),
        coverage_basis_points: r.get("coverage_basis_points"),
        calculated_at: r.get("calculated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_daily(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<DailySummaryRecord, OperationalRepositoryError> {
    Ok(DailySummaryRecord {
        system_id: sid(r.get("system_id"), SystemId::from_uuid)?,
        local_date: r.get("local_date"),
        timezone: r.get("timezone"),
        generation: r.get("generation"),
        generation_energy_wh: r.get("generation_energy_wh"),
        consumption_energy_wh: r.get("consumption_energy_wh"),
        coverage_basis_points: r.get("coverage_basis_points"),
        quality_flags: r.get("quality_flags"),
        calculated_at: r.get("calculated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_lifetime(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<LifetimeSummaryRecord, OperationalRepositoryError> {
    Ok(LifetimeSummaryRecord {
        system_id: sid(r.get("system_id"), SystemId::from_uuid)?,
        generation: r.get("generation"),
        first_observation_at: r.get("first_observation_at"),
        last_observation_at: r.get("last_observation_at"),
        generation_energy_wh: r.get("generation_energy_wh"),
        consumption_energy_wh: r.get("consumption_energy_wh"),
        coverage_basis_points: r.get("coverage_basis_points"),
        calculated_at: r.get("calculated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_team(r: &sqlx::sqlite::SqliteRow) -> Result<TeamRecord, OperationalRepositoryError> {
    Ok(TeamRecord {
        id: sid(r.get("id"), TeamId::from_uuid)?,
        account_id: sid(r.get("account_id"), AccountId::from_uuid)?,
        name: r.get("name"),
        visibility: r.get("visibility"),
        owner_user_id: sid(r.get("owner_user_id"), UserId::from_uuid)?,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_team_rollup(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<TeamRollupRecord, OperationalRepositoryError> {
    Ok(TeamRollupRecord {
        team_id: sid(r.get("team_id"), TeamId::from_uuid)?,
        team_account_id: sid(r.get("team_account_id"), AccountId::from_uuid)?,
        period_start: r.get("period_start"),
        period_end: r.get("period_end"),
        generation_energy_wh: r.get("generation_energy_wh"),
        normalized_generation_wh_per_kw: r.get("normalized_generation_wh_per_kw"),
        coverage_basis_points: r.get("coverage_basis_points"),
        source_sequence: r.get("source_sequence"),
        projected_at: r.get("projected_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_alert(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<AlertRuleRecord, OperationalRepositoryError> {
    Ok(AlertRuleRecord {
        id: sid(r.get("id"), AlertRuleId::from_uuid)?,
        system_id: sid(r.get("system_id"), SystemId::from_uuid)?,
        name: r.get("name"),
        alert_kind: r.get("alert_kind"),
        enabled: r.get("enabled"),
        condition: serde_json::from_str(&r.get::<String, _>("condition_json"))?,
        schedule: serde_json::from_str(&r.get::<String, _>("schedule_json"))?,
        debounce_seconds: r.get("debounce_seconds"),
        cooldown_seconds: r.get("cooldown_seconds"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_webhook(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<WebhookSubscriptionRecord, OperationalRepositoryError> {
    Ok(WebhookSubscriptionRecord {
        id: sid(r.get("id"), WebhookSubscriptionId::from_uuid)?,
        name: r.get("name"),
        endpoint_url: r.get("endpoint_url"),
        state: r.get("state"),
        event_types: serde_json::from_str(&r.get::<String, _>("event_types_json"))?,
        encryption_key_id: r.get("encryption_key_id"),
        encrypted_signing_secret: r.get("encrypted_signing_secret"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_provider(
    r: &sqlx::sqlite::SqliteRow,
) -> Result<ProviderRecord, OperationalRepositoryError> {
    Ok(ProviderRecord {
        id: sid(r.get("id"), ProviderId::from_uuid)?,
        provider_kind: r.get("provider_kind"),
        name: r.get("name"),
        enabled: r.get("enabled"),
        endpoint_url: r.get("endpoint_url"),
        credential_secret_ref: r.get("credential_secret_ref"),
        configuration: serde_json::from_str(&r.get::<String, _>("configuration_json"))?,
        license_metadata: serde_json::from_str(&r.get::<String, _>("license_metadata_json"))?,
        circuit_state: r.get("circuit_state"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_job(r: &sqlx::sqlite::SqliteRow) -> Result<JobRecord, OperationalRepositoryError> {
    Ok(JobRecord {
        id: sid(r.get("id"), JobId::from_uuid)?,
        job_kind: r.get("job_kind"),
        state: r.get("state"),
        payload: serde_json::from_str(&r.get::<String, _>("payload_json"))?,
        idempotency_key: r.get("idempotency_key"),
        priority: r.get("priority"),
        attempt_count: r.get("attempt_count"),
        max_attempts: r.get("max_attempts"),
        available_at: r.get("available_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[derive(Debug, Error)]
pub enum OperationalRepositoryError {
    #[error("operational database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    Routing(#[from] crate::SqliteRoutingError),
    #[error("operational JSON value is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("period must be non-empty")]
    InvalidPeriod,
    #[error("record belongs to another account")]
    AccountMismatch,
    #[error("operational storage contains an invalid value")]
    InvalidStoredValue,
}
