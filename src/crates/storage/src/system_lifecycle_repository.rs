//! Concrete account-storage adapter for the audited system lifecycle service.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_application::{PortError, SystemLifecycleRecord, SystemLifecycleRepository};
use pvlog_domain::{AccountId, AuditEventId, SystemId, SystemLifecycle, UserId, Visibility};
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use uuid::Uuid;

#[cfg(feature = "sqlite")]
use crate::SqliteAccountPoolRouter;
use crate::{AuditRecord, ManagementRepository, SystemRegistryRecord};

#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteSystemLifecycleRepository {
    router: SqliteAccountPoolRouter,
    management: Arc<dyn ManagementRepository>,
}

#[cfg(feature = "sqlite")]
impl SqliteSystemLifecycleRepository {
    #[must_use]
    pub fn new(router: SqliteAccountPoolRouter, management: Arc<dyn ManagementRepository>) -> Self {
        Self { router, management }
    }

    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, PortError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| PortError::Unavailable)?
            .map(|record| record.account_id)
            .ok_or(PortError::NotFound)
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl SystemLifecycleRepository for SqliteSystemLifecycleRepository {
    async fn system(&self, id: SystemId) -> Result<Option<SystemLifecycleRecord>, PortError> {
        let account_id = match self.account_for(id).await {
            Ok(account_id) => account_id,
            Err(PortError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        let account = self.router.route(account_id).await.map_err(route_error)?;
        let mut connection = account.acquire().await.map_err(route_error)?;
        let row = sqlx::query(
            "SELECT id,name,timezone,visibility,lifecycle,version,created_at,updated_at \
             FROM systems WHERE id=?",
        )
        .bind(blob(id.as_uuid()))
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| PortError::Unavailable)?;
        row.map(|row| sqlite_record(account_id, &row)).transpose()
    }

    async fn create(&self, record: SystemLifecycleRecord) -> Result<(), PortError> {
        let account = self
            .router
            .route(record.account_id)
            .await
            .map_err(route_error)?;
        let mut writer = account.acquire_writer().await.map_err(route_error)?;
        sqlx::query(
            "INSERT INTO systems \
             (id,name,description,timezone,visibility,lifecycle,status_interval_seconds, \
              power_calculation_mode,net_calculation_mode,created_at,updated_at) \
             VALUES (?,?, '',?,?,?,300,'reported','separate_flows',?,?)",
        )
        .bind(blob(record.id.as_uuid()))
        .bind(&record.name)
        .bind(&record.timezone)
        .bind(visibility_name(record.visibility))
        .bind(lifecycle_name(record.lifecycle))
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(writer.connection())
        .await
        .map_err(|_| PortError::Unavailable)?;
        self.management
            .save_system_registry(&SystemRegistryRecord {
                system_id: record.id,
                account_id: record.account_id,
                created_at: record.created_at,
                updated_at: record.updated_at,
            })
            .await
            .map_err(|_| PortError::Unavailable)
    }

    async fn save(
        &self,
        record: SystemLifecycleRecord,
        expected_version: u64,
    ) -> Result<bool, PortError> {
        let account_id = self.account_for(record.id).await?;
        if account_id != record.account_id {
            return Err(PortError::Conflict);
        }
        let account = self.router.route(account_id).await.map_err(route_error)?;
        let mut writer = account.acquire_writer().await.map_err(route_error)?;
        let changed = sqlx::query(
            "UPDATE systems SET name=?,timezone=?,visibility=?,lifecycle=?,updated_at=?, \
             version=version+1 WHERE id=? AND version=?",
        )
        .bind(&record.name)
        .bind(&record.timezone)
        .bind(visibility_name(record.visibility))
        .bind(lifecycle_name(record.lifecycle))
        .bind(record.updated_at)
        .bind(blob(record.id.as_uuid()))
        .bind(i64::try_from(expected_version).map_err(|_| PortError::Conflict)?)
        .execute(writer.connection())
        .await
        .map_err(|_| PortError::Unavailable)?
        .rows_affected()
            == 1;
        if changed {
            self.management
                .save_system_registry(&SystemRegistryRecord {
                    system_id: record.id,
                    account_id,
                    created_at: record.created_at,
                    updated_at: record.updated_at,
                })
                .await
                .map_err(|_| PortError::Unavailable)?;
        }
        Ok(changed)
    }

    async fn delete(&self, id: SystemId, expected_version: u64) -> Result<bool, PortError> {
        let account_id = self.account_for(id).await?;
        let account = self.router.route(account_id).await.map_err(route_error)?;
        let mut writer = account.acquire_writer().await.map_err(route_error)?;
        Ok(sqlx::query("DELETE FROM systems WHERE id=? AND version=?")
            .bind(blob(id.as_uuid()))
            .bind(i64::try_from(expected_version).map_err(|_| PortError::Conflict)?)
            .execute(writer.connection())
            .await
            .map_err(|_| PortError::Unavailable)?
            .rows_affected()
            == 1)
    }

    async fn audit(
        &self,
        actor: UserId,
        id: SystemId,
        action: &'static str,
        outcome: &'static str,
        now: i64,
    ) -> Result<(), PortError> {
        append_management_audit(&*self.management, actor, id, action, outcome, now).await
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresSystemLifecycleRepository {
    url: String,
    management: Arc<dyn ManagementRepository>,
}

#[cfg(feature = "postgres")]
impl PostgresSystemLifecycleRepository {
    #[must_use]
    pub fn new(url: String, management: Arc<dyn ManagementRepository>) -> Self {
        Self { url, management }
    }

    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, PortError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| PortError::Unavailable)?
            .map(|record| record.account_id)
            .ok_or(PortError::NotFound)
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl SystemLifecycleRepository for PostgresSystemLifecycleRepository {
    async fn system(&self, id: SystemId) -> Result<Option<SystemLifecycleRecord>, PortError> {
        let account_id = match self.account_for(id).await {
            Ok(account_id) => account_id,
            Err(PortError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut connection = PgConnection::connect(&self.url)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let row = sqlx::query("SELECT id,name,timezone,visibility,lifecycle,version,created_at,updated_at FROM account_data.systems WHERE account_id=$1 AND id=$2")
            .bind(account_id.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut connection).await
            .map_err(|_| PortError::Unavailable)?;
        connection
            .close()
            .await
            .map_err(|_| PortError::Unavailable)?;
        row.map(|row| postgres_record(account_id, &row)).transpose()
    }

    async fn create(&self, record: SystemLifecycleRecord) -> Result<(), PortError> {
        let mut connection = PgConnection::connect(&self.url)
            .await
            .map_err(|_| PortError::Unavailable)?;
        sqlx::query("INSERT INTO account_data.systems (account_id,id,name,description,timezone,visibility,lifecycle,status_interval_seconds,power_calculation_mode,net_calculation_mode,created_at,updated_at) VALUES ($1,$2,$3,'',$4,$5,$6,300,'reported','separate_flows',$7,$8)")
            .bind(record.account_id.as_uuid()).bind(record.id.as_uuid()).bind(&record.name).bind(&record.timezone).bind(visibility_name(record.visibility)).bind(lifecycle_name(record.lifecycle)).bind(record.created_at).bind(record.updated_at).execute(&mut connection).await.map_err(|_| PortError::Unavailable)?;
        connection
            .close()
            .await
            .map_err(|_| PortError::Unavailable)?;
        self.management
            .save_system_registry(&SystemRegistryRecord {
                system_id: record.id,
                account_id: record.account_id,
                created_at: record.created_at,
                updated_at: record.updated_at,
            })
            .await
            .map_err(|_| PortError::Unavailable)
    }

    async fn save(
        &self,
        record: SystemLifecycleRecord,
        expected_version: u64,
    ) -> Result<bool, PortError> {
        let account_id = self.account_for(record.id).await?;
        if account_id != record.account_id {
            return Err(PortError::Conflict);
        }
        let mut connection = PgConnection::connect(&self.url)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let changed = sqlx::query("UPDATE account_data.systems SET name=$1,timezone=$2,visibility=$3,lifecycle=$4,updated_at=$5,version=version+1 WHERE account_id=$6 AND id=$7 AND version=$8")
            .bind(&record.name).bind(&record.timezone).bind(visibility_name(record.visibility)).bind(lifecycle_name(record.lifecycle)).bind(record.updated_at).bind(account_id.as_uuid()).bind(record.id.as_uuid()).bind(i64::try_from(expected_version).map_err(|_| PortError::Conflict)?).execute(&mut connection).await.map_err(|_| PortError::Unavailable)?.rows_affected() == 1;
        connection
            .close()
            .await
            .map_err(|_| PortError::Unavailable)?;
        if changed {
            self.management
                .save_system_registry(&SystemRegistryRecord {
                    system_id: record.id,
                    account_id,
                    created_at: record.created_at,
                    updated_at: record.updated_at,
                })
                .await
                .map_err(|_| PortError::Unavailable)?;
        }
        Ok(changed)
    }

    async fn delete(&self, id: SystemId, expected_version: u64) -> Result<bool, PortError> {
        let account_id = self.account_for(id).await?;
        let mut connection = PgConnection::connect(&self.url)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let deleted = sqlx::query(
            "DELETE FROM account_data.systems WHERE account_id=$1 AND id=$2 AND version=$3",
        )
        .bind(account_id.as_uuid())
        .bind(id.as_uuid())
        .bind(i64::try_from(expected_version).map_err(|_| PortError::Conflict)?)
        .execute(&mut connection)
        .await
        .map_err(|_| PortError::Unavailable)?
        .rows_affected()
            == 1;
        connection
            .close()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(deleted)
    }

    async fn audit(
        &self,
        actor: UserId,
        id: SystemId,
        action: &'static str,
        outcome: &'static str,
        now: i64,
    ) -> Result<(), PortError> {
        append_management_audit(&*self.management, actor, id, action, outcome, now).await
    }
}

async fn append_management_audit(
    management: &dyn ManagementRepository,
    actor: UserId,
    system_id: SystemId,
    action: &'static str,
    outcome: &'static str,
    now: i64,
) -> Result<(), PortError> {
    let account_id = management
        .system_registry(system_id)
        .await
        .map_err(|_| PortError::Unavailable)?
        .map(|record| record.account_id)
        .ok_or(PortError::NotFound)?;
    let id = AuditEventId::new();
    let mut event_hash = [0; 32];
    event_hash[..16].copy_from_slice(id.as_uuid().as_bytes());
    event_hash[16..].copy_from_slice(id.as_uuid().as_bytes());
    management
        .append_audit(&AuditRecord {
            id,
            occurred_at: now,
            request_id: None,
            actor_type: "user".to_owned(),
            actor_id: Some(actor.as_uuid()),
            account_id: Some(account_id),
            action: action.to_owned(),
            target_type: "system".to_owned(),
            target_id: Some(system_id.as_uuid()),
            outcome: outcome.to_owned(),
            previous_event_hash: None,
            event_hash,
            safe_metadata: serde_json::json!({}),
        })
        .await
        .map_err(|_| PortError::Unavailable)
}

fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Account => "account",
        Visibility::Unlisted => "unlisted",
        Visibility::Public => "public",
    }
}

fn lifecycle_name(lifecycle: SystemLifecycle) -> &'static str {
    match lifecycle {
        SystemLifecycle::Active => "active",
        SystemLifecycle::Archived => "archived",
        SystemLifecycle::PendingDeletion => "deleting",
    }
}

fn parse_visibility(value: &str) -> Result<Visibility, PortError> {
    match value {
        "private" => Ok(Visibility::Private),
        "account" => Ok(Visibility::Account),
        "unlisted" => Ok(Visibility::Unlisted),
        "public" => Ok(Visibility::Public),
        _ => Err(PortError::Unavailable),
    }
}

fn parse_lifecycle(value: &str) -> Result<SystemLifecycle, PortError> {
    match value {
        "active" => Ok(SystemLifecycle::Active),
        "archived" => Ok(SystemLifecycle::Archived),
        "deleting" => Ok(SystemLifecycle::PendingDeletion),
        _ => Err(PortError::Unavailable),
    }
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
fn sqlite_record(
    account_id: AccountId,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<SystemLifecycleRecord, PortError> {
    let id = Uuid::from_slice(&row.get::<Vec<u8>, _>("id")).map_err(|_| PortError::Unavailable)?;
    Ok(SystemLifecycleRecord {
        id: SystemId::from_uuid(id).map_err(|_| PortError::Unavailable)?,
        account_id,
        name: row.get("name"),
        timezone: row.get("timezone"),
        visibility: parse_visibility(&row.get::<String, _>("visibility"))?,
        lifecycle: parse_lifecycle(&row.get::<String, _>("lifecycle"))?,
        version: u64::try_from(row.get::<i64, _>("version")).map_err(|_| PortError::Unavailable)?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn postgres_record(
    account_id: AccountId,
    row: &sqlx::postgres::PgRow,
) -> Result<SystemLifecycleRecord, PortError> {
    Ok(SystemLifecycleRecord {
        id: SystemId::from_uuid(row.get("id")).map_err(|_| PortError::Unavailable)?,
        account_id,
        name: row.get("name"),
        timezone: row.get("timezone"),
        visibility: parse_visibility(&row.get::<String, _>("visibility"))?,
        lifecycle: parse_lifecycle(&row.get::<String, _>("lifecycle"))?,
        version: u64::try_from(row.get::<i64, _>("version")).map_err(|_| PortError::Unavailable)?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(feature = "sqlite")]
fn route_error(_: crate::SqliteRoutingError) -> PortError {
    PortError::Unavailable
}
