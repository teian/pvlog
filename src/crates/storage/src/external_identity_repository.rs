//! Management persistence for provider-neutral external identity linking.

#[cfg(feature = "postgres")]
use std::fmt;
#[cfg(feature = "sqlite")]
use std::path::PathBuf;

use async_trait::async_trait;
use pvlog_application::{
    ExternalIdentityLinkingRepository, IdentityClaims, LinkedIdentityRecord, PortError,
};
use pvlog_domain::{ConnectorId, ExternalIdentityId, UserId};
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
#[cfg(feature = "sqlite")]
use sqlx::{SqliteConnection, sqlite::SqliteConnectOptions};
use uuid::Uuid;

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteExternalIdentityRepository {
    path: PathBuf,
}

#[cfg(feature = "sqlite")]
impl SqliteExternalIdentityRepository {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    async fn connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&self.path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresExternalIdentityRepository {
    url: String,
}

#[cfg(feature = "postgres")]
impl PostgresExternalIdentityRepository {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self { url }
    }
    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresExternalIdentityRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresExternalIdentityRepository")
            .field("url", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl ExternalIdentityLinkingRepository for SqliteExternalIdentityRepository {
    async fn find_by_connector_subject(
        &self,
        connector_id: ConnectorId,
        subject: &str,
    ) -> Result<Option<LinkedIdentityRecord>, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let row = sqlx::query("SELECT id,connector_id,user_id,provider_subject,linked_at,last_authenticated_at FROM external_identities WHERE connector_id=? AND provider_subject=?")
            .bind(blob(connector_id.as_uuid())).bind(subject).fetch_optional(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        row.as_ref().map(sqlite_identity).transpose()
    }

    async fn create_user_from_external_claims(
        &self,
        claims: &IdentityClaims,
        now: i64,
    ) -> Result<UserId, PortError> {
        let user_id = UserId::new();
        let email = claims
            .email
            .clone()
            .unwrap_or_else(|| format!("external-{user_id}@invalid.local"));
        let display_name = claims
            .display_name
            .clone()
            .unwrap_or_else(|| "External user".to_owned());
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("INSERT INTO users (id,email,display_name,status,email_verified_at,created_at,updated_at) VALUES (?,?,?,'active',?,?,?)")
            .bind(blob(user_id.as_uuid())).bind(email).bind(display_name).bind(claims.email_verified.filter(|verified| *verified).map(|_| now)).bind(now).bind(now).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(user_id)
    }

    async fn link(&self, identity: LinkedIdentityRecord) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("INSERT INTO external_identities (id,connector_id,user_id,provider_subject,linked_at,last_authenticated_at) VALUES (?,?,?,?,?,?)")
            .bind(blob(identity.id.as_uuid())).bind(blob(identity.connector_id.as_uuid())).bind(blob(identity.user_id.as_uuid())).bind(identity.subject).bind(identity.linked_at_epoch_millis).bind(identity.last_login_at_epoch_millis).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)
    }

    async fn touch_login(
        &self,
        identity_id: ExternalIdentityId,
        now: i64,
    ) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("UPDATE external_identities SET last_authenticated_at=? WHERE id=?")
            .bind(now)
            .bind(blob(identity_id.as_uuid()))
            .execute(&mut connection)
            .await
            .map_err(port)?;
        connection.close().await.map_err(port)
    }

    async fn find_for_user(
        &self,
        identity_id: ExternalIdentityId,
        user_id: UserId,
    ) -> Result<Option<LinkedIdentityRecord>, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let row = sqlx::query("SELECT id,connector_id,user_id,provider_subject,linked_at,last_authenticated_at FROM external_identities WHERE id=? AND user_id=?")
            .bind(blob(identity_id.as_uuid())).bind(blob(user_id.as_uuid())).fetch_optional(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        row.as_ref().map(sqlite_identity).transpose()
    }

    async fn has_local_login(&self, user_id: UserId) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let exists =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_credentials WHERE user_id=?)")
                .bind(blob(user_id.as_uuid()))
                .fetch_one(&mut connection)
                .await
                .map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(exists)
    }

    async fn external_identity_count(&self, user_id: UserId) -> Result<u32, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM external_identities WHERE user_id=?")
                .bind(blob(user_id.as_uuid()))
                .fetch_one(&mut connection)
                .await
                .map_err(port)?;
        connection.close().await.map_err(port)?;
        u32::try_from(count).map_err(|_| PortError::Unavailable)
    }

    async fn unlink(&self, identity_id: ExternalIdentityId) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("DELETE FROM external_identities WHERE id=?")
            .bind(blob(identity_id.as_uuid()))
            .execute(&mut connection)
            .await
            .map_err(port)?;
        connection.close().await.map_err(port)
    }

    async fn audit(
        &self,
        user_id: UserId,
        action: &'static str,
        now: i64,
    ) -> Result<(), PortError> {
        append_sqlite_audit(&self.path, user_id, action, now).await
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl ExternalIdentityLinkingRepository for PostgresExternalIdentityRepository {
    async fn find_by_connector_subject(
        &self,
        connector_id: ConnectorId,
        subject: &str,
    ) -> Result<Option<LinkedIdentityRecord>, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let row = sqlx::query("SELECT id,connector_id,user_id,provider_subject,linked_at,last_authenticated_at FROM management.external_identities WHERE connector_id=$1 AND provider_subject=$2")
            .bind(connector_id.as_uuid()).bind(subject).fetch_optional(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        row.as_ref().map(postgres_identity).transpose()
    }

    async fn create_user_from_external_claims(
        &self,
        claims: &IdentityClaims,
        now: i64,
    ) -> Result<UserId, PortError> {
        let user_id = UserId::new();
        let email = claims
            .email
            .clone()
            .unwrap_or_else(|| format!("external-{user_id}@invalid.local"));
        let display_name = claims
            .display_name
            .clone()
            .unwrap_or_else(|| "External user".to_owned());
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("INSERT INTO management.users (id,email,display_name,status,email_verified_at,created_at,updated_at) VALUES ($1,$2,$3,'active',$4,$5,$5)")
            .bind(user_id.as_uuid()).bind(email).bind(display_name).bind(claims.email_verified.filter(|verified| *verified).map(|_| now)).bind(now).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(user_id)
    }

    async fn link(&self, identity: LinkedIdentityRecord) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("INSERT INTO management.external_identities (id,connector_id,user_id,provider_subject,linked_at,last_authenticated_at) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(identity.id.as_uuid()).bind(identity.connector_id.as_uuid()).bind(identity.user_id.as_uuid()).bind(identity.subject).bind(identity.linked_at_epoch_millis).bind(identity.last_login_at_epoch_millis).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)
    }

    async fn touch_login(
        &self,
        identity_id: ExternalIdentityId,
        now: i64,
    ) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query(
            "UPDATE management.external_identities SET last_authenticated_at=$1 WHERE id=$2",
        )
        .bind(now)
        .bind(identity_id.as_uuid())
        .execute(&mut connection)
        .await
        .map_err(port)?;
        connection.close().await.map_err(port)
    }

    async fn find_for_user(
        &self,
        identity_id: ExternalIdentityId,
        user_id: UserId,
    ) -> Result<Option<LinkedIdentityRecord>, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let row = sqlx::query("SELECT id,connector_id,user_id,provider_subject,linked_at,last_authenticated_at FROM management.external_identities WHERE id=$1 AND user_id=$2")
            .bind(identity_id.as_uuid()).bind(user_id.as_uuid()).fetch_optional(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        row.as_ref().map(postgres_identity).transpose()
    }

    async fn has_local_login(&self, user_id: UserId) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let exists = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM management.local_credentials WHERE user_id=$1)",
        )
        .bind(user_id.as_uuid())
        .fetch_one(&mut connection)
        .await
        .map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(exists)
    }

    async fn external_identity_count(&self, user_id: UserId) -> Result<u32, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM management.external_identities WHERE user_id=$1",
        )
        .bind(user_id.as_uuid())
        .fetch_one(&mut connection)
        .await
        .map_err(port)?;
        connection.close().await.map_err(port)?;
        u32::try_from(count).map_err(|_| PortError::Unavailable)
    }

    async fn unlink(&self, identity_id: ExternalIdentityId) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("DELETE FROM management.external_identities WHERE id=$1")
            .bind(identity_id.as_uuid())
            .execute(&mut connection)
            .await
            .map_err(port)?;
        connection.close().await.map_err(port)
    }

    async fn audit(
        &self,
        user_id: UserId,
        action: &'static str,
        now: i64,
    ) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let id = Uuid::now_v7();
        let hash = audit_hash(id);
        sqlx::query("INSERT INTO management.global_audit_events (id,occurred_at,actor_type,actor_id,action,target_type,target_id,outcome,event_hash,safe_metadata) VALUES ($1,$2,'user',$3,$4,'external_identity',$3,'succeeded',$5,'{}'::jsonb)")
            .bind(id).bind(now).bind(user_id.as_uuid()).bind(action).bind(hash.as_slice()).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)
    }
}

#[cfg(feature = "sqlite")]
async fn append_sqlite_audit(
    path: &PathBuf,
    user_id: UserId,
    action: &'static str,
    now: i64,
) -> Result<(), PortError> {
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await
    .map_err(port)?;
    let id = Uuid::now_v7();
    let hash = audit_hash(id);
    sqlx::query("INSERT INTO global_audit_events (id,occurred_at,actor_type,actor_id,action,target_type,target_id,outcome,event_hash,safe_metadata_json) VALUES (? ,?,'user',?,?, 'external_identity',?,'succeeded',?,'{}')")
        .bind(blob(id)).bind(now).bind(blob(user_id.as_uuid())).bind(action).bind(blob(user_id.as_uuid())).bind(hash.as_slice()).execute(&mut connection).await.map_err(port)?;
    connection.close().await.map_err(port)
}

#[cfg(feature = "sqlite")]
fn sqlite_identity(row: &sqlx::sqlite::SqliteRow) -> Result<LinkedIdentityRecord, PortError> {
    Ok(LinkedIdentityRecord {
        id: external_id(blob_uuid(&row.get::<Vec<u8>, _>("id"))?)?,
        connector_id: connector_id(blob_uuid(&row.get::<Vec<u8>, _>("connector_id"))?)?,
        user_id: user_id(blob_uuid(&row.get::<Vec<u8>, _>("user_id"))?)?,
        subject: row.get("provider_subject"),
        linked_at_epoch_millis: row.get("linked_at"),
        last_login_at_epoch_millis: row.get("last_authenticated_at"),
    })
}

#[cfg(feature = "postgres")]
fn postgres_identity(row: &sqlx::postgres::PgRow) -> Result<LinkedIdentityRecord, PortError> {
    Ok(LinkedIdentityRecord {
        id: external_id(row.get("id"))?,
        connector_id: connector_id(row.get("connector_id"))?,
        user_id: user_id(row.get("user_id"))?,
        subject: row.get("provider_subject"),
        linked_at_epoch_millis: row.get("linked_at"),
        last_login_at_epoch_millis: row.get("last_authenticated_at"),
    })
}

fn audit_hash(id: Uuid) -> [u8; 32] {
    let mut hash = [0; 32];
    hash[..16].copy_from_slice(id.as_bytes());
    hash[16..].copy_from_slice(id.as_bytes());
    hash
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}
#[cfg(feature = "sqlite")]
fn blob_uuid(value: &[u8]) -> Result<Uuid, PortError> {
    Uuid::from_slice(value).map_err(|_| PortError::Unavailable)
}
fn user_id(id: Uuid) -> Result<UserId, PortError> {
    UserId::from_uuid(id).map_err(|_| PortError::Unavailable)
}
fn connector_id(id: Uuid) -> Result<ConnectorId, PortError> {
    ConnectorId::from_uuid(id).map_err(|_| PortError::Unavailable)
}
fn external_id(id: Uuid) -> Result<ExternalIdentityId, PortError> {
    ExternalIdentityId::from_uuid(id).map_err(|_| PortError::Unavailable)
}
fn port(_: sqlx::Error) -> PortError {
    PortError::Unavailable
}
