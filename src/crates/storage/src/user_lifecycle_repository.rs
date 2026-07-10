//! Persistent local-user lifecycle adapters for both management database engines.

#[cfg(feature = "postgres")]
use std::fmt;
#[cfg(feature = "sqlite")]
use std::path::PathBuf;

use async_trait::async_trait;
use pvlog_application::{
    InvitationRecord, LifecycleCreateOutcome, LifecycleUserRecord, PortError,
    UserLifecycleRepository,
};
use pvlog_domain::{CredentialDigest, UserId, UserStatus};
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
#[cfg(feature = "sqlite")]
use sqlx::{SqliteConnection, sqlite::SqliteConnectOptions};
use uuid::Uuid;

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteUserLifecycleRepository {
    path: PathBuf,
}

#[cfg(feature = "sqlite")]
impl SqliteUserLifecycleRepository {
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
pub struct PostgresUserLifecycleRepository {
    url: String,
}

#[cfg(feature = "postgres")]
impl PostgresUserLifecycleRepository {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresUserLifecycleRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresUserLifecycleRepository")
            .field("url", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl UserLifecycleRepository for SqliteUserLifecycleRepository {
    async fn user(&self, id: UserId) -> Result<Option<LifecycleUserRecord>, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let row = sqlx::query(
            "SELECT user.id,user.email,user.display_name,user.status,user.email_verified_at,\
             user.disabled_at,credential.locked_until,user.created_at,user.updated_at \
             FROM users user LEFT JOIN local_credentials credential ON credential.user_id=user.id \
             WHERE user.id=?",
        )
        .bind(blob(id.as_uuid()))
        .fetch_optional(&mut connection)
        .await
        .map_err(port)?;
        connection.close().await.map_err(port)?;
        row.map(|row| sqlite_user(&row)).transpose()
    }

    async fn create_user(
        &self,
        record: &LifecycleUserRecord,
    ) -> Result<LifecycleCreateOutcome, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let result = sqlx::query(
            "INSERT INTO users (id,email,display_name,status,email_verified_at,disabled_at,created_at,updated_at) \
             VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(email) DO NOTHING",
        )
        .bind(blob(record.id.as_uuid()))
        .bind(&record.email)
        .bind(&record.display_name)
        .bind(status_name(record.status))
        .bind(record.email_verified_at)
        .bind(record.disabled_at)
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&mut connection)
        .await
        .map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(if result.rows_affected() == 1 {
            LifecycleCreateOutcome::Created
        } else {
            LifecycleCreateOutcome::Existing
        })
    }

    async fn create_invitation(&self, invitation: &InvitationRecord) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query(
            "INSERT INTO user_invitations (id,email,token_digest,invited_by,expires_at,created_at) \
             VALUES (?,?,?,?,?,?)",
        )
        .bind(blob(invitation.id.as_uuid()))
        .bind(&invitation.email)
        .bind(invitation.token_digest.as_bytes().as_slice())
        .bind(blob(invitation.invited_by.as_uuid()))
        .bind(invitation.expires_at)
        .bind(invitation.created_at)
        .execute(&mut connection)
        .await
        .map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(())
    }

    async fn accept_invitation(
        &self,
        digest: &CredentialDigest,
        display_name: &str,
        activated: bool,
        now: i64,
    ) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let invitation = sqlx::query(
            "SELECT id,email FROM user_invitations WHERE token_digest=? AND accepted_at IS NULL \
             AND revoked_at IS NULL AND expires_at>?",
        )
        .bind(digest.as_bytes().as_slice())
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(port)?;
        let Some(invitation) = invitation else {
            transaction.rollback().await.map_err(port)?;
            return Ok(false);
        };
        let invitation_id: Vec<u8> = invitation.get("id");
        let email: String = invitation.get("email");
        let status = if activated { "active" } else { "invited" };
        let verified_at = activated.then_some(now);
        let user_id = UserId::new();
        sqlx::query(
            "INSERT INTO users (id,email,display_name,status,email_verified_at,created_at,updated_at) \
             VALUES (?,?,?,?,?,?,?) ON CONFLICT(email) DO NOTHING",
        )
        .bind(blob(user_id.as_uuid()))
        .bind(&email)
        .bind(display_name)
        .bind(status)
        .bind(verified_at)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(port)?;
        sqlx::query(
            "UPDATE users SET display_name=?,status=?,email_verified_at=COALESCE(email_verified_at,?),\
             disabled_at=NULL,updated_at=?,version=version+1 WHERE email=? AND status='invited'",
        )
        .bind(display_name)
        .bind(status)
        .bind(verified_at)
        .bind(now)
        .bind(&email)
        .execute(&mut *transaction)
        .await
        .map_err(port)?;
        let result = sqlx::query(
            "UPDATE user_invitations SET accepted_at=? WHERE id=? AND accepted_at IS NULL",
        )
        .bind(now)
        .bind(invitation_id)
        .execute(&mut *transaction)
        .await
        .map_err(port)?;
        transaction.commit().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }

    async fn activate_user(
        &self,
        id: UserId,
        email_verified_at: Option<i64>,
        now: i64,
    ) -> Result<bool, PortError> {
        sqlite_status_update(self, id, "active", email_verified_at, None, now).await
    }

    async fn disable_user(&self, id: UserId, now: i64) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let result = sqlx::query(
            "UPDATE users SET status='disabled',disabled_at=?,updated_at=?,version=version+1 \
             WHERE id=? AND status NOT IN ('disabled','deleted')",
        )
        .bind(now)
        .bind(now)
        .bind(blob(id.as_uuid()))
        .execute(&mut *transaction)
        .await
        .map_err(port)?;
        revoke_sqlite_sessions(&mut transaction, id, now).await?;
        transaction.commit().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }

    async fn unlock_user(&self, id: UserId, now: i64) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=? AND status<>'deleted')",
        )
        .bind(blob(id.as_uuid()))
        .fetch_one(&mut *transaction)
        .await
        .map_err(port)?;
        if exists {
            sqlx::query(
                "UPDATE local_credentials SET failed_attempts=0,locked_until=NULL WHERE user_id=?",
            )
            .bind(blob(id.as_uuid()))
            .execute(&mut *transaction)
            .await
            .map_err(port)?;
            sqlx::query("UPDATE users SET updated_at=?,version=version+1 WHERE id=?")
                .bind(now)
                .bind(blob(id.as_uuid()))
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
        }
        transaction.commit().await.map_err(port)?;
        Ok(exists)
    }

    async fn delete_user(&self, id: UserId, now: i64) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let result = sqlx::query(
            "UPDATE users SET email='deleted-'||lower(hex(id))||'@invalid.local',\
             display_name='Deleted user',status='deleted',email_verified_at=NULL,disabled_at=?,\
             updated_at=?,version=version+1 WHERE id=? AND status<>'deleted'",
        )
        .bind(now)
        .bind(now)
        .bind(blob(id.as_uuid()))
        .execute(&mut *transaction)
        .await
        .map_err(port)?;
        if result.rows_affected() == 1 {
            sqlx::query("DELETE FROM external_identities WHERE user_id=?")
                .bind(blob(id.as_uuid()))
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
            sqlx::query("DELETE FROM local_credentials WHERE user_id=?")
                .bind(blob(id.as_uuid()))
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
            sqlx::query("DELETE FROM password_recovery_tokens WHERE user_id=?")
                .bind(blob(id.as_uuid()))
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
            revoke_sqlite_sessions(&mut transaction, id, now).await?;
        }
        transaction.commit().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl UserLifecycleRepository for PostgresUserLifecycleRepository {
    async fn user(&self, id: UserId) -> Result<Option<LifecycleUserRecord>, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let row=sqlx::query("SELECT user_record.id,user_record.email,user_record.display_name,user_record.status,user_record.email_verified_at,user_record.disabled_at,credential.locked_until,user_record.created_at,user_record.updated_at FROM management.users user_record LEFT JOIN management.local_credentials credential ON credential.user_id=user_record.id WHERE user_record.id=$1").bind(id.as_uuid()).fetch_optional(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        row.map(|row| postgres_user(&row)).transpose()
    }

    async fn create_user(
        &self,
        record: &LifecycleUserRecord,
    ) -> Result<LifecycleCreateOutcome, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let result=sqlx::query("INSERT INTO management.users (id,email,display_name,status,email_verified_at,disabled_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(record.id.as_uuid()).bind(&record.email).bind(&record.display_name).bind(status_name(record.status)).bind(record.email_verified_at).bind(record.disabled_at).bind(record.created_at).bind(record.updated_at).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(if result.rows_affected() == 1 {
            LifecycleCreateOutcome::Created
        } else {
            LifecycleCreateOutcome::Existing
        })
    }

    async fn create_invitation(&self, invitation: &InvitationRecord) -> Result<(), PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        sqlx::query("INSERT INTO management.user_invitations (id,email,token_digest,invited_by,expires_at,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(invitation.id.as_uuid()).bind(&invitation.email).bind(invitation.token_digest.as_bytes().as_slice()).bind(invitation.invited_by.as_uuid()).bind(invitation.expires_at).bind(invitation.created_at).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(())
    }

    async fn accept_invitation(
        &self,
        digest: &CredentialDigest,
        display_name: &str,
        activated: bool,
        now: i64,
    ) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let invitation=sqlx::query("SELECT id,email FROM management.user_invitations WHERE token_digest=$1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at>$2 FOR UPDATE").bind(digest.as_bytes().as_slice()).bind(now).fetch_optional(&mut *transaction).await.map_err(port)?;
        let Some(invitation) = invitation else {
            transaction.rollback().await.map_err(port)?;
            return Ok(false);
        };
        let invitation_id: Uuid = invitation.get("id");
        let email: String = invitation.get("email");
        let status = if activated { "active" } else { "invited" };
        let verified_at = activated.then_some(now);
        let user_id = UserId::new();
        sqlx::query("INSERT INTO management.users (id,email,display_name,status,email_verified_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(user_id.as_uuid()).bind(&email).bind(display_name).bind(status).bind(verified_at).bind(now).bind(now).execute(&mut *transaction).await.map_err(port)?;
        sqlx::query("UPDATE management.users SET display_name=$1,status=$2,email_verified_at=COALESCE(email_verified_at,$3),disabled_at=NULL,updated_at=$4,version=version+1 WHERE lower(email)=lower($5) AND status='invited'").bind(display_name).bind(status).bind(verified_at).bind(now).bind(&email).execute(&mut *transaction).await.map_err(port)?;
        let result=sqlx::query("UPDATE management.user_invitations SET accepted_at=$1 WHERE id=$2 AND accepted_at IS NULL").bind(now).bind(invitation_id).execute(&mut *transaction).await.map_err(port)?;
        transaction.commit().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }

    async fn activate_user(
        &self,
        id: UserId,
        email_verified_at: Option<i64>,
        now: i64,
    ) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let result=sqlx::query("UPDATE management.users SET status='active',email_verified_at=COALESCE(email_verified_at,$1),disabled_at=NULL,updated_at=$2,version=version+1 WHERE id=$3 AND status<>'deleted'").bind(email_verified_at).bind(now).bind(id.as_uuid()).execute(&mut connection).await.map_err(port)?;
        connection.close().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }

    async fn disable_user(&self, id: UserId, now: i64) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let result=sqlx::query("UPDATE management.users SET status='disabled',disabled_at=$1,updated_at=$1,version=version+1 WHERE id=$2 AND status NOT IN ('disabled','deleted')").bind(now).bind(id.as_uuid()).execute(&mut *transaction).await.map_err(port)?;
        sqlx::query(
            "UPDATE management.sessions SET revoked_at=COALESCE(revoked_at,$1) WHERE user_id=$2",
        )
        .bind(now)
        .bind(id.as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(port)?;
        transaction.commit().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }

    async fn unlock_user(&self, id: UserId, now: i64) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM management.users WHERE id=$1 AND status<>'deleted')",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(port)?;
        if exists {
            sqlx::query("UPDATE management.local_credentials SET failed_attempts=0,locked_until=NULL WHERE user_id=$1").bind(id.as_uuid()).execute(&mut *transaction).await.map_err(port)?;
            sqlx::query("UPDATE management.users SET updated_at=$1,version=version+1 WHERE id=$2")
                .bind(now)
                .bind(id.as_uuid())
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
        }
        transaction.commit().await.map_err(port)?;
        Ok(exists)
    }

    async fn delete_user(&self, id: UserId, now: i64) -> Result<bool, PortError> {
        let mut connection = self.connection().await.map_err(port)?;
        let mut transaction = connection.begin().await.map_err(port)?;
        let result=sqlx::query("UPDATE management.users SET email='deleted-'||replace(id::text,'-','')||'@invalid.local',display_name='Deleted user',status='deleted',email_verified_at=NULL,disabled_at=$1,updated_at=$1,version=version+1 WHERE id=$2 AND status<>'deleted'").bind(now).bind(id.as_uuid()).execute(&mut *transaction).await.map_err(port)?;
        if result.rows_affected() == 1 {
            sqlx::query("DELETE FROM management.external_identities WHERE user_id=$1")
                .bind(id.as_uuid())
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
            sqlx::query("DELETE FROM management.local_credentials WHERE user_id=$1")
                .bind(id.as_uuid())
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
            sqlx::query("DELETE FROM management.password_recovery_tokens WHERE user_id=$1")
                .bind(id.as_uuid())
                .execute(&mut *transaction)
                .await
                .map_err(port)?;
            sqlx::query("UPDATE management.sessions SET revoked_at=COALESCE(revoked_at,$1) WHERE user_id=$2").bind(now).bind(id.as_uuid()).execute(&mut *transaction).await.map_err(port)?;
        }
        transaction.commit().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }
}

#[cfg(feature = "sqlite")]
async fn sqlite_status_update(
    repository: &SqliteUserLifecycleRepository,
    id: UserId,
    status: &str,
    email_verified_at: Option<i64>,
    disabled_at: Option<i64>,
    now: i64,
) -> Result<bool, PortError> {
    let mut connection = repository.connection().await.map_err(port)?;
    let result = sqlx::query(
        "UPDATE users SET status=?,email_verified_at=COALESCE(email_verified_at,?),disabled_at=?,\
         updated_at=?,version=version+1 WHERE id=? AND status<>'deleted'",
    )
    .bind(status)
    .bind(email_verified_at)
    .bind(disabled_at)
    .bind(now)
    .bind(blob(id.as_uuid()))
    .execute(&mut connection)
    .await
    .map_err(port)?;
    connection.close().await.map_err(port)?;
    Ok(result.rows_affected() == 1)
}

#[cfg(feature = "sqlite")]
async fn revoke_sqlite_sessions(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: UserId,
    now: i64,
) -> Result<(), PortError> {
    sqlx::query("UPDATE sessions SET revoked_at=COALESCE(revoked_at,?) WHERE user_id=?")
        .bind(now)
        .bind(blob(id.as_uuid()))
        .execute(&mut **transaction)
        .await
        .map_err(port)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn sqlite_user(row: &sqlx::sqlite::SqliteRow) -> Result<LifecycleUserRecord, PortError> {
    Ok(LifecycleUserRecord {
        id: sqlite_id(row.get("id"))?,
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: parse_status(row.get::<String, _>("status").as_str())?,
        email_verified_at: row.get("email_verified_at"),
        disabled_at: row.get("disabled_at"),
        locked_until: row.get("locked_until"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
fn postgres_user(row: &sqlx::postgres::PgRow) -> Result<LifecycleUserRecord, PortError> {
    Ok(LifecycleUserRecord {
        id: UserId::from_uuid(row.get("id"))
            .map_err(|_| PortError::Rejected("invalid_user_id".to_owned()))?,
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: parse_status(row.get::<String, _>("status").as_str())?,
        email_verified_at: row.get("email_verified_at"),
        disabled_at: row.get("disabled_at"),
        locked_until: row.get("locked_until"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn status_name(status: UserStatus) -> &'static str {
    match status {
        UserStatus::Invited => "invited",
        UserStatus::Active => "active",
        UserStatus::Disabled => "disabled",
        UserStatus::Deleted => "deleted",
    }
}

fn parse_status(value: &str) -> Result<UserStatus, PortError> {
    match value {
        "invited" => Ok(UserStatus::Invited),
        "active" => Ok(UserStatus::Active),
        "disabled" => Ok(UserStatus::Disabled),
        "deleted" => Ok(UserStatus::Deleted),
        _ => Err(PortError::Rejected("invalid_user_status".to_owned())),
    }
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn sqlite_id(value: Vec<u8>) -> Result<UserId, PortError> {
    let uuid =
        Uuid::from_slice(&value).map_err(|_| PortError::Rejected("invalid_user_id".to_owned()))?;
    UserId::from_uuid(uuid).map_err(|_| PortError::Rejected("invalid_user_id".to_owned()))
}

#[allow(clippy::needless_pass_by_value)]
fn port(error: sqlx::Error) -> PortError {
    if error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
    {
        PortError::Conflict
    } else {
        PortError::Unavailable
    }
}
