//! Management-plane repository contracts shared by both database engines.

use std::{collections::BTreeSet, path::PathBuf};

use async_trait::async_trait;
use pvlog_domain::{
    AccountId, ApiCredentialId, AuditEventId, MembershipId, Permission, PrincipalId, SessionId,
    SystemId, UserId,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
#[cfg(feature = "sqlite")]
use sqlx::{SqliteConnection, sqlite::SqliteConnectOptions};
use thiserror::Error;
use uuid::Uuid;

/// Persistence representation of a local user without credential material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserRecord {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Persistence representation of an account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountRecord {
    pub id: AccountId,
    pub slug: String,
    pub display_name: String,
    pub status: String,
    pub created_by: Option<UserId>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Global ownership index for a system stored in an account database.
///
/// The management plane uses this mapping to resolve an account and authorize a
/// system request before opening account-owned storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemRegistryRecord {
    pub system_id: SystemId,
    pub account_id: AccountId,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Account-scoped membership record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MembershipRecord {
    pub id: MembershipId,
    pub account_id: AccountId,
    pub user_id: UserId,
    pub status: String,
    pub joined_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Server-side browser session record containing only keyed digests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRecord {
    pub id: SessionId,
    pub user_id: UserId,
    pub session_digest: [u8; 32],
    pub csrf_digest: [u8; 32],
    pub created_at: i64,
    pub last_seen_at: i64,
    pub idle_expires_at: i64,
    pub absolute_expires_at: i64,
    pub revoked_at: Option<i64>,
}

/// Account-scoped API credential record containing only a keyed digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiCredentialRecord {
    pub id: ApiCredentialId,
    pub account_id: AccountId,
    pub owner_user_id: UserId,
    pub system_id: Option<SystemId>,
    pub name: String,
    pub credential_digest: [u8; 32],
    pub scopes: BTreeSet<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

/// One explicit user permission grant at account or system scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationGrant {
    pub account_id: AccountId,
    pub user_id: UserId,
    pub permission: Permission,
    pub system_id: Option<SystemId>,
    pub granted_by: UserId,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// Storage backend selected by a routing record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingBackend {
    Sqlite,
    Postgres,
}

/// Safe management-plane storage routing metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingRecord {
    pub account_id: AccountId,
    pub backend: RoutingBackend,
    pub state: String,
    pub opaque_locator: Option<String>,
    pub schema_version: i64,
}

/// Append-only management audit event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord {
    pub id: AuditEventId,
    pub occurred_at: i64,
    pub request_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub account_id: Option<AccountId>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub outcome: String,
    pub previous_event_hash: Option<[u8; 32]>,
    pub event_hash: [u8; 32],
    pub safe_metadata: serde_json::Value,
}

/// Authorization-relevant management persistence shared by both engines.
#[async_trait]
pub trait ManagementRepository: Send + Sync {
    async fn save_user(&self, record: &UserRecord) -> Result<(), ManagementRepositoryError>;
    async fn user(&self, id: UserId) -> Result<Option<UserRecord>, ManagementRepositoryError>;
    async fn save_account(&self, record: &AccountRecord) -> Result<(), ManagementRepositoryError>;
    async fn account(
        &self,
        id: AccountId,
    ) -> Result<Option<AccountRecord>, ManagementRepositoryError>;
    async fn save_system_registry(
        &self,
        record: &SystemRegistryRecord,
    ) -> Result<(), ManagementRepositoryError>;
    async fn system_registry(
        &self,
        system_id: SystemId,
    ) -> Result<Option<SystemRegistryRecord>, ManagementRepositoryError>;
    async fn save_membership(
        &self,
        record: &MembershipRecord,
    ) -> Result<(), ManagementRepositoryError>;
    async fn active_membership(
        &self,
        account_id: AccountId,
        user_id: UserId,
    ) -> Result<Option<MembershipRecord>, ManagementRepositoryError>;
    async fn active_accounts_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<AccountRecord>, ManagementRepositoryError>;
    async fn systems_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<SystemId>, ManagementRepositoryError>;
    async fn save_session(&self, record: &SessionRecord) -> Result<(), ManagementRepositoryError>;
    async fn active_session_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<SessionRecord>, ManagementRepositoryError>;
    async fn revoke_session(
        &self,
        session_id: SessionId,
        now: i64,
    ) -> Result<(), ManagementRepositoryError>;
    async fn revoke_oldest_sessions_above_limit(
        &self,
        user_id: UserId,
        keep: u32,
        now: i64,
    ) -> Result<(), ManagementRepositoryError>;
    async fn save_api_credential(
        &self,
        record: &ApiCredentialRecord,
    ) -> Result<(), ManagementRepositoryError>;
    async fn api_credential(
        &self,
        account_id: AccountId,
        credential_id: ApiCredentialId,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError>;
    async fn active_api_credential_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError>;
    async fn grant_user_permission(
        &self,
        grant: &AuthorizationGrant,
    ) -> Result<(), ManagementRepositoryError>;
    async fn user_is_authorized(
        &self,
        user_id: UserId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError>;
    /// Evaluates a user or API credential role assignment at account or system scope.
    async fn principal_is_authorized(
        &self,
        principal: PrincipalId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError>;
    async fn user_is_instance_authorized(
        &self,
        user_id: UserId,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError>;
    async fn routing(
        &self,
        account_id: AccountId,
    ) -> Result<Option<RoutingRecord>, ManagementRepositoryError>;
    async fn append_audit(&self, record: &AuditRecord) -> Result<(), ManagementRepositoryError>;
    async fn account_audit(
        &self,
        account_id: AccountId,
        limit: u32,
    ) -> Result<Vec<AuditRecord>, ManagementRepositoryError>;
}

/// `SQLite` management repository.
#[derive(Clone, Debug)]
pub struct SqliteManagementRepository {
    #[cfg_attr(not(feature = "sqlite"), allow(dead_code))]
    path: PathBuf,
}

impl SqliteManagementRepository {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    #[cfg(feature = "sqlite")]
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

/// `PostgreSQL` management repository.
#[derive(Clone, Debug)]
pub struct PostgresManagementRepository {
    #[cfg_attr(not(feature = "postgres"), allow(dead_code))]
    url: String,
}

impl PostgresManagementRepository {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self { url }
    }

    #[cfg(feature = "postgres")]
    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl ManagementRepository for SqliteManagementRepository {
    async fn save_user(&self, record: &UserRecord) -> Result<(), ManagementRepositoryError> {
        validate_user(record)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO users (id, email, display_name, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET email = excluded.email, \
             display_name = excluded.display_name, status = excluded.status, \
             updated_at = excluded.updated_at, version = version + 1",
        )
        .bind(uuid_blob(record.id.as_uuid()))
        .bind(&record.email)
        .bind(&record.display_name)
        .bind(&record.status)
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn user(&self, id: UserId) -> Result<Option<UserRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT id, email, display_name, status, created_at, updated_at FROM users WHERE id = ?",
        )
        .bind(uuid_blob(id.as_uuid()))
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| sqlite_user(&row)).transpose()
    }

    async fn save_account(&self, record: &AccountRecord) -> Result<(), ManagementRepositoryError> {
        validate_account(record)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO accounts \
             (id, slug, display_name, status, created_by, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET slug = excluded.slug, \
             display_name = excluded.display_name, status = excluded.status, \
             updated_at = excluded.updated_at, version = version + 1",
        )
        .bind(uuid_blob(record.id.as_uuid()))
        .bind(&record.slug)
        .bind(&record.display_name)
        .bind(&record.status)
        .bind(record.created_by.map(|id| uuid_blob(id.as_uuid())))
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn account(
        &self,
        id: AccountId,
    ) -> Result<Option<AccountRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT id, slug, display_name, status, created_by, created_at, updated_at \
             FROM accounts WHERE id = ?",
        )
        .bind(uuid_blob(id.as_uuid()))
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| sqlite_account(&row)).transpose()
    }

    async fn save_system_registry(
        &self,
        record: &SystemRegistryRecord,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO system_registry (system_id, account_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?) ON CONFLICT(system_id) DO UPDATE SET \
             account_id = excluded.account_id, updated_at = excluded.updated_at",
        )
        .bind(uuid_blob(record.system_id.as_uuid()))
        .bind(uuid_blob(record.account_id.as_uuid()))
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn system_registry(
        &self,
        system_id: SystemId,
    ) -> Result<Option<SystemRegistryRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT system_id, account_id, created_at, updated_at FROM system_registry \
             WHERE system_id = ?",
        )
        .bind(uuid_blob(system_id.as_uuid()))
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| sqlite_system_registry(&row)).transpose()
    }

    async fn save_membership(
        &self,
        record: &MembershipRecord,
    ) -> Result<(), ManagementRepositoryError> {
        validate_membership(record)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO memberships \
             (id, account_id, user_id, status, joined_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(account_id, user_id) DO UPDATE SET \
             status = excluded.status, joined_at = excluded.joined_at, updated_at = excluded.updated_at",
        )
        .bind(uuid_blob(record.id.as_uuid()))
        .bind(uuid_blob(record.account_id.as_uuid()))
        .bind(uuid_blob(record.user_id.as_uuid()))
        .bind(&record.status)
        .bind(record.joined_at)
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn active_membership(
        &self,
        account_id: AccountId,
        user_id: UserId,
    ) -> Result<Option<MembershipRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT membership.id, membership.account_id, membership.user_id, membership.status, \
                    membership.joined_at, membership.created_at, membership.updated_at \
             FROM memberships membership \
             JOIN users user ON user.id = membership.user_id AND user.status = 'active' \
             JOIN accounts account ON account.id = membership.account_id AND account.status = 'active' \
             WHERE membership.account_id = ? AND membership.user_id = ? \
               AND membership.status = 'active'",
        )
        .bind(uuid_blob(account_id.as_uuid()))
        .bind(uuid_blob(user_id.as_uuid()))
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| sqlite_membership(&row)).transpose()
    }

    async fn active_accounts_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<AccountRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT account.id,account.slug,account.display_name,account.status,account.created_by, \
             account.created_at,account.updated_at FROM accounts account \
             JOIN memberships membership ON membership.account_id=account.id \
             WHERE membership.user_id=? AND membership.status='active' AND account.status='active' \
             ORDER BY account.slug",
        )
        .bind(uuid_blob(user_id.as_uuid()))
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(sqlite_account).collect()
    }

    async fn systems_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<SystemId>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT system_id FROM system_registry WHERE account_id=? ORDER BY system_id",
        )
        .bind(uuid_blob(account_id.as_uuid()))
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.into_iter().map(system_id_from_blob).collect()
    }

    async fn save_session(&self, record: &SessionRecord) -> Result<(), ManagementRepositoryError> {
        validate_session(record)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO sessions \
             (id, user_id, session_digest, csrf_digest, authentication_method, created_at, \
              last_seen_at, idle_expires_at, absolute_expires_at, revoked_at) \
             VALUES (?, ?, ?, ?, 'local', ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET \
             last_seen_at = excluded.last_seen_at, idle_expires_at = excluded.idle_expires_at, \
             absolute_expires_at = excluded.absolute_expires_at, revoked_at = excluded.revoked_at",
        )
        .bind(uuid_blob(record.id.as_uuid()))
        .bind(uuid_blob(record.user_id.as_uuid()))
        .bind(record.session_digest.as_slice())
        .bind(record.csrf_digest.as_slice())
        .bind(record.created_at)
        .bind(record.last_seen_at)
        .bind(record.idle_expires_at)
        .bind(record.absolute_expires_at)
        .bind(record.revoked_at)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn active_session_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<SessionRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT session.id, session.user_id, session.session_digest, session.csrf_digest, \
                    session.created_at, session.last_seen_at, session.idle_expires_at, \
                    session.absolute_expires_at, session.revoked_at FROM sessions session \
             JOIN users user ON user.id = session.user_id AND user.status = 'active' \
             WHERE session.session_digest = ? AND session.revoked_at IS NULL \
               AND session.idle_expires_at > ? AND session.absolute_expires_at > ?",
        )
        .bind(digest.as_slice())
        .bind(now)
        .bind(now)
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| sqlite_session(&row)).transpose()
    }

    async fn revoke_session(
        &self,
        session_id: SessionId,
        now: i64,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query("UPDATE sessions SET revoked_at=COALESCE(revoked_at, ?) WHERE id=?")
            .bind(now)
            .bind(uuid_blob(session_id.as_uuid()))
            .execute(&mut connection)
            .await?;
        connection.close().await?;
        Ok(())
    }

    async fn revoke_oldest_sessions_above_limit(
        &self,
        user_id: UserId,
        keep: u32,
        now: i64,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query(
            "UPDATE sessions SET revoked_at=? WHERE id IN ( \
             SELECT id FROM sessions WHERE user_id=? AND revoked_at IS NULL \
             ORDER BY created_at DESC, id DESC LIMIT -1 OFFSET ?)",
        )
        .bind(now)
        .bind(uuid_blob(user_id.as_uuid()))
        .bind(i64::from(keep))
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn save_api_credential(
        &self,
        record: &ApiCredentialRecord,
    ) -> Result<(), ManagementRepositoryError> {
        validate_credential(record)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        sqlx::query(
            "INSERT INTO api_credentials \
             (id, owner_user_id, account_id, system_id, name, credential_digest, created_at, \
              expires_at, revoked_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, expires_at = excluded.expires_at, \
             revoked_at = excluded.revoked_at",
        )
        .bind(uuid_blob(record.id.as_uuid()))
        .bind(uuid_blob(record.owner_user_id.as_uuid()))
        .bind(uuid_blob(record.account_id.as_uuid()))
        .bind(record.system_id.map(|id| uuid_blob(id.as_uuid())))
        .bind(&record.name)
        .bind(record.credential_digest.as_slice())
        .bind(record.created_at)
        .bind(record.expires_at)
        .bind(record.revoked_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM api_credential_scopes WHERE credential_id = ?")
            .bind(uuid_blob(record.id.as_uuid()))
            .execute(&mut *transaction)
            .await?;
        for scope in &record.scopes {
            sqlx::query(
                "INSERT INTO api_credential_scopes \
                 (id, credential_id, scope, account_id, system_id) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid_blob(Uuid::now_v7()))
            .bind(uuid_blob(record.id.as_uuid()))
            .bind(scope)
            .bind(uuid_blob(record.account_id.as_uuid()))
            .bind(record.system_id.map(|id| uuid_blob(id.as_uuid())))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn api_credential(
        &self,
        account_id: AccountId,
        credential_id: ApiCredentialId,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        sqlite_credential_by(
            self,
            "id = ? AND account_id = ?",
            uuid_blob(credential_id.as_uuid()),
            Some(uuid_blob(account_id.as_uuid())),
            None,
        )
        .await
    }

    async fn active_api_credential_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        sqlite_credential_by(self, "credential_digest = ? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?) AND EXISTS (SELECT 1 FROM users WHERE users.id = api_credentials.owner_user_id AND users.status = 'active') AND EXISTS (SELECT 1 FROM accounts WHERE accounts.id = api_credentials.account_id AND accounts.status = 'active')", digest.to_vec(), None, Some(now)).await
    }

    async fn grant_user_permission(
        &self,
        grant: &AuthorizationGrant,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        let role_id = Uuid::now_v7();
        let assignment_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO rbac_roles \
             (id, account_id, name, role_kind, created_by, created_at, updated_at) \
             VALUES (?, ?, ?, 'custom', ?, ?, ?)",
        )
        .bind(uuid_blob(role_id))
        .bind(uuid_blob(grant.account_id.as_uuid()))
        .bind(format!("grant-{assignment_id}"))
        .bind(uuid_blob(grant.granted_by.as_uuid()))
        .bind(grant.created_at)
        .bind(grant.created_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("INSERT INTO rbac_role_permissions (role_id, permission) VALUES (?, ?)")
            .bind(uuid_blob(role_id))
            .bind(permission_name(grant.permission))
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO rbac_role_assignments \
             (id, role_id, principal_type, principal_id, scope_type, account_id, system_id, \
              delegated_by, created_at, expires_at) VALUES (?, ?, 'user', ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid_blob(assignment_id))
        .bind(uuid_blob(role_id))
        .bind(uuid_blob(grant.user_id.as_uuid()))
        .bind(if grant.system_id.is_some() {
            "system"
        } else {
            "account"
        })
        .bind(uuid_blob(grant.account_id.as_uuid()))
        .bind(grant.system_id.map(|id| uuid_blob(id.as_uuid())))
        .bind(uuid_blob(grant.granted_by.as_uuid()))
        .bind(grant.created_at)
        .bind(grant.expires_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn user_is_authorized(
        &self,
        user_id: UserId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let allowed = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM rbac_role_assignments assignment \
             JOIN rbac_roles role ON role.id = assignment.role_id \
             JOIN rbac_role_permissions permission ON permission.role_id = role.id \
             JOIN memberships membership ON membership.account_id = assignment.account_id \
                  AND membership.user_id = assignment.principal_id \
             JOIN users user ON user.id = assignment.principal_id AND user.status = 'active' \
             JOIN accounts account ON account.id = assignment.account_id AND account.status = 'active' \
             WHERE assignment.principal_type = 'user' AND assignment.principal_id = ? \
               AND assignment.account_id = ? AND role.account_id = ? \
               AND membership.status = 'active' AND permission.permission = ? \
               AND assignment.revoked_at IS NULL \
               AND (assignment.expires_at IS NULL OR assignment.expires_at > ?) \
               AND (assignment.scope_type = 'account' OR \
                    (assignment.scope_type = 'system' AND assignment.system_id = ?)))",
        )
        .bind(uuid_blob(user_id.as_uuid()))
        .bind(uuid_blob(account_id.as_uuid()))
        .bind(uuid_blob(account_id.as_uuid()))
        .bind(permission_name(permission))
        .bind(now)
        .bind(system_id.map(|id| uuid_blob(id.as_uuid())))
        .fetch_one(&mut connection)
        .await?;
        connection.close().await?;
        Ok(allowed)
    }

    async fn principal_is_authorized(
        &self,
        principal: PrincipalId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        let (principal_type, principal_id) = sqlite_principal(principal);
        let mut connection = self.connection().await?;
        let allowed = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM rbac_role_assignments assignment \
             JOIN rbac_roles role ON role.id = assignment.role_id \
             JOIN rbac_role_permissions permission ON permission.role_id = role.id \
             JOIN accounts account ON account.id = assignment.account_id AND account.status = 'active' \
             LEFT JOIN memberships membership ON assignment.principal_type = 'user' \
                  AND membership.account_id = assignment.account_id \
                  AND membership.user_id = assignment.principal_id AND membership.status = 'active' \
             LEFT JOIN users user_record ON user_record.id = COALESCE(membership.user_id, \
                  (SELECT owner_user_id FROM api_credentials WHERE id = assignment.principal_id)) \
                  AND user_record.status = 'active' \
             LEFT JOIN api_credentials credential ON assignment.principal_type = 'api_credential' \
                  AND credential.id = assignment.principal_id AND credential.account_id = assignment.account_id \
                  AND credential.revoked_at IS NULL AND (credential.expires_at IS NULL OR credential.expires_at > ?) \
             WHERE assignment.principal_type = ? AND assignment.principal_id = ? \
               AND assignment.account_id = ? AND role.account_id = ? \
               AND permission.permission = ? AND assignment.revoked_at IS NULL \
               AND (assignment.expires_at IS NULL OR assignment.expires_at > ?) \
               AND (assignment.scope_type = 'account' OR \
                    (assignment.scope_type = 'system' AND assignment.system_id = ?)) \
               AND ((assignment.principal_type = 'user' AND membership.id IS NOT NULL) \
                    OR (assignment.principal_type = 'api_credential' AND credential.id IS NOT NULL)))",
        )
        .bind(now)
        .bind(principal_type)
        .bind(principal_id)
        .bind(uuid_blob(account_id.as_uuid()))
        .bind(uuid_blob(account_id.as_uuid()))
        .bind(permission_name(permission))
        .bind(now)
        .bind(system_id.map(|id| uuid_blob(id.as_uuid())))
        .fetch_one(&mut connection)
        .await?;
        connection.close().await?;
        Ok(allowed)
    }

    async fn user_is_instance_authorized(
        &self,
        user_id: UserId,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let allowed = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM rbac_role_assignments assignment \
             JOIN rbac_roles role ON role.id=assignment.role_id AND role.account_id IS NULL \
             JOIN rbac_role_permissions role_permission ON role_permission.role_id=role.id \
             JOIN users user_record ON user_record.id=assignment.principal_id \
                  AND user_record.status='active' \
             WHERE assignment.principal_type='user' AND assignment.principal_id=? \
               AND assignment.scope_type='instance' AND assignment.account_id IS NULL \
               AND assignment.system_id IS NULL AND assignment.revoked_at IS NULL \
               AND (assignment.expires_at IS NULL OR assignment.expires_at>?) \
               AND role_permission.permission=?)",
        )
        .bind(uuid_blob(user_id.as_uuid()))
        .bind(now)
        .bind(permission_name(permission))
        .fetch_one(&mut connection)
        .await?;
        connection.close().await?;
        Ok(allowed)
    }

    async fn routing(
        &self,
        account_id: AccountId,
    ) -> Result<Option<RoutingRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT account_id, opaque_locator, lifecycle_state, schema_version \
             FROM account_database_registry WHERE account_id = ?",
        )
        .bind(uuid_blob(account_id.as_uuid()))
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| {
            Ok(RoutingRecord {
                account_id: account_id_from_blob(row.get("account_id"))?,
                backend: RoutingBackend::Sqlite,
                state: row.get("lifecycle_state"),
                opaque_locator: Some(row.get("opaque_locator")),
                schema_version: row.get("schema_version"),
            })
        })
        .transpose()
    }

    async fn append_audit(&self, record: &AuditRecord) -> Result<(), ManagementRepositoryError> {
        validate_audit(record)?;
        let mut connection = self.connection().await?;
        sqlx::query(
            "INSERT INTO global_audit_events \
             (id, occurred_at, request_id, actor_type, actor_id, account_id, action, target_type, \
              target_id, outcome, previous_event_hash, event_hash, safe_metadata_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid_blob(record.id.as_uuid()))
        .bind(record.occurred_at)
        .bind(record.request_id.map(uuid_blob))
        .bind(&record.actor_type)
        .bind(record.actor_id.map(uuid_blob))
        .bind(record.account_id.map(|id| uuid_blob(id.as_uuid())))
        .bind(&record.action)
        .bind(&record.target_type)
        .bind(record.target_id.map(uuid_blob))
        .bind(&record.outcome)
        .bind(record.previous_event_hash.map(|hash| hash.to_vec()))
        .bind(record.event_hash.as_slice())
        .bind(serde_json::to_string(&record.safe_metadata)?)
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn account_audit(
        &self,
        account_id: AccountId,
        limit: u32,
    ) -> Result<Vec<AuditRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(
            "SELECT id, occurred_at, request_id, actor_type, actor_id, account_id, action, \
                    target_type, target_id, outcome, previous_event_hash, event_hash, safe_metadata_json \
             FROM global_audit_events WHERE account_id = ? ORDER BY occurred_at DESC, id DESC LIMIT ?",
        )
        .bind(uuid_blob(account_id.as_uuid()))
        .bind(limit)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(sqlite_audit).collect()
    }
}

// PostgreSQL implements the same contract with native UUID/JSON values and account-owned keys.
#[cfg(feature = "postgres")]
#[async_trait]
impl ManagementRepository for PostgresManagementRepository {
    async fn save_user(&self, record: &UserRecord) -> Result<(), ManagementRepositoryError> {
        validate_user(record)?;
        let mut connection = self.connection().await?;
        sqlx::query("INSERT INTO management.users (id,email,display_name,status,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO UPDATE SET email=excluded.email,display_name=excluded.display_name,status=excluded.status,updated_at=excluded.updated_at,version=management.users.version+1")
            .bind(record.id.as_uuid()).bind(&record.email).bind(&record.display_name).bind(&record.status).bind(record.created_at).bind(record.updated_at).execute(&mut connection).await?;
        connection.close().await?;
        Ok(())
    }

    async fn user(&self, id: UserId) -> Result<Option<UserRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query("SELECT id,email,display_name,status,created_at,updated_at FROM management.users WHERE id=$1").bind(id.as_uuid()).fetch_optional(&mut connection).await?;
        connection.close().await?;
        row.map(|row| postgres_user(&row)).transpose()
    }

    async fn save_account(&self, record: &AccountRecord) -> Result<(), ManagementRepositoryError> {
        validate_account(record)?;
        let mut connection = self.connection().await?;
        sqlx::query("INSERT INTO management.accounts (id,slug,display_name,status,created_by,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET slug=excluded.slug,display_name=excluded.display_name,status=excluded.status,updated_at=excluded.updated_at,version=management.accounts.version+1")
            .bind(record.id.as_uuid()).bind(&record.slug).bind(&record.display_name).bind(&record.status).bind(record.created_by.map(UserId::as_uuid)).bind(record.created_at).bind(record.updated_at).execute(&mut connection).await?;
        connection.close().await?;
        Ok(())
    }

    async fn account(
        &self,
        id: AccountId,
    ) -> Result<Option<AccountRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query("SELECT id,slug,display_name,status,created_by,created_at,updated_at FROM management.accounts WHERE id=$1").bind(id.as_uuid()).fetch_optional(&mut connection).await?;
        connection.close().await?;
        row.map(|row| postgres_account(&row)).transpose()
    }

    async fn save_system_registry(
        &self,
        record: &SystemRegistryRecord,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query("INSERT INTO management.system_registry (system_id,account_id,created_at,updated_at) VALUES ($1,$2,$3,$4) ON CONFLICT(system_id) DO UPDATE SET account_id=excluded.account_id,updated_at=excluded.updated_at")
            .bind(record.system_id.as_uuid()).bind(record.account_id.as_uuid()).bind(record.created_at).bind(record.updated_at).execute(&mut connection).await?;
        connection.close().await?;
        Ok(())
    }

    async fn system_registry(
        &self,
        system_id: SystemId,
    ) -> Result<Option<SystemRegistryRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query("SELECT system_id,account_id,created_at,updated_at FROM management.system_registry WHERE system_id=$1")
            .bind(system_id.as_uuid()).fetch_optional(&mut connection).await?;
        connection.close().await?;
        row.map(|row| postgres_system_registry(&row)).transpose()
    }

    async fn save_membership(
        &self,
        record: &MembershipRecord,
    ) -> Result<(), ManagementRepositoryError> {
        validate_membership(record)?;
        let mut connection = self.connection().await?;
        sqlx::query("INSERT INTO management.memberships (account_id,id,user_id,status,joined_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(account_id,user_id) DO UPDATE SET status=excluded.status,joined_at=excluded.joined_at,updated_at=excluded.updated_at")
            .bind(record.account_id.as_uuid()).bind(record.id.as_uuid()).bind(record.user_id.as_uuid()).bind(&record.status).bind(record.joined_at).bind(record.created_at).bind(record.updated_at).execute(&mut connection).await?;
        connection.close().await?;
        Ok(())
    }

    async fn active_membership(
        &self,
        account_id: AccountId,
        user_id: UserId,
    ) -> Result<Option<MembershipRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query("SELECT membership.id,membership.account_id,membership.user_id,membership.status,membership.joined_at,membership.created_at,membership.updated_at FROM management.memberships membership JOIN management.users user_record ON user_record.id=membership.user_id AND user_record.status='active' JOIN management.accounts account ON account.id=membership.account_id AND account.status='active' WHERE membership.account_id=$1 AND membership.user_id=$2 AND membership.status='active'").bind(account_id.as_uuid()).bind(user_id.as_uuid()).fetch_optional(&mut connection).await?;
        connection.close().await?;
        row.map(|row| postgres_membership(&row)).transpose()
    }

    async fn active_accounts_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<AccountRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query("SELECT account.id,account.slug,account.display_name,account.status,account.created_by,account.created_at,account.updated_at FROM management.accounts account JOIN management.memberships membership ON membership.account_id=account.id WHERE membership.user_id=$1 AND membership.status='active' AND account.status='active' ORDER BY account.slug")
            .bind(user_id.as_uuid()).fetch_all(&mut connection).await?;
        connection.close().await?;
        rows.iter().map(postgres_account).collect()
    }

    async fn systems_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<SystemId>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query_scalar::<_, Uuid>(
            "SELECT system_id FROM management.system_registry WHERE account_id=$1 ORDER BY system_id",
        )
        .bind(account_id.as_uuid())
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.into_iter()
            .map(|id| pg_id(id, SystemId::from_uuid))
            .collect()
    }

    async fn save_session(&self, record: &SessionRecord) -> Result<(), ManagementRepositoryError> {
        validate_session(record)?;
        let mut connection = self.connection().await?;
        sqlx::query("INSERT INTO management.sessions (id,user_id,session_digest,csrf_digest,authentication_method,created_at,last_seen_at,idle_expires_at,absolute_expires_at,revoked_at) VALUES ($1,$2,$3,$4,'local',$5,$6,$7,$8,$9) ON CONFLICT(id) DO UPDATE SET last_seen_at=excluded.last_seen_at,idle_expires_at=excluded.idle_expires_at,absolute_expires_at=excluded.absolute_expires_at,revoked_at=excluded.revoked_at")
            .bind(record.id.as_uuid()).bind(record.user_id.as_uuid()).bind(record.session_digest.as_slice()).bind(record.csrf_digest.as_slice()).bind(record.created_at).bind(record.last_seen_at).bind(record.idle_expires_at).bind(record.absolute_expires_at).bind(record.revoked_at).execute(&mut connection).await?;
        connection.close().await?;
        Ok(())
    }

    async fn active_session_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<SessionRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row=sqlx::query("SELECT session.id,session.user_id,session.session_digest,session.csrf_digest,session.created_at,session.last_seen_at,session.idle_expires_at,session.absolute_expires_at,session.revoked_at FROM management.sessions session JOIN management.users user_record ON user_record.id=session.user_id AND user_record.status='active' WHERE session.session_digest=$1 AND session.revoked_at IS NULL AND session.idle_expires_at>$2 AND session.absolute_expires_at>$2").bind(digest.as_slice()).bind(now).fetch_optional(&mut connection).await?;
        connection.close().await?;
        row.map(|row| postgres_session(&row)).transpose()
    }

    async fn revoke_session(
        &self,
        session_id: SessionId,
        now: i64,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query(
            "UPDATE management.sessions SET revoked_at=COALESCE(revoked_at,$1) WHERE id=$2",
        )
        .bind(now)
        .bind(session_id.as_uuid())
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn revoke_oldest_sessions_above_limit(
        &self,
        user_id: UserId,
        keep: u32,
        now: i64,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        sqlx::query(
            "UPDATE management.sessions SET revoked_at=$1 WHERE id IN ( \
             SELECT id FROM management.sessions WHERE user_id=$2 AND revoked_at IS NULL \
             ORDER BY created_at DESC, id DESC OFFSET $3)",
        )
        .bind(now)
        .bind(user_id.as_uuid())
        .bind(i64::from(keep))
        .execute(&mut connection)
        .await?;
        connection.close().await?;
        Ok(())
    }

    async fn save_api_credential(
        &self,
        record: &ApiCredentialRecord,
    ) -> Result<(), ManagementRepositoryError> {
        validate_credential(record)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        sqlx::query("INSERT INTO management.api_credentials (account_id,id,owner_user_id,system_id,name,credential_digest,created_at,expires_at,revoked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,expires_at=excluded.expires_at,revoked_at=excluded.revoked_at")
            .bind(record.account_id.as_uuid()).bind(record.id.as_uuid()).bind(record.owner_user_id.as_uuid()).bind(record.system_id.map(SystemId::as_uuid)).bind(&record.name).bind(record.credential_digest.as_slice()).bind(record.created_at).bind(record.expires_at).bind(record.revoked_at).execute(&mut *transaction).await?;
        sqlx::query(
            "DELETE FROM management.api_credential_scopes WHERE account_id=$1 AND credential_id=$2",
        )
        .bind(record.account_id.as_uuid())
        .bind(record.id.as_uuid())
        .execute(&mut *transaction)
        .await?;
        for scope in &record.scopes {
            sqlx::query("INSERT INTO management.api_credential_scopes (account_id,id,credential_id,scope,system_id) VALUES ($1,$2,$3,$4,$5)").bind(record.account_id.as_uuid()).bind(Uuid::now_v7()).bind(record.id.as_uuid()).bind(scope).bind(record.system_id.map(SystemId::as_uuid)).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn api_credential(
        &self,
        account_id: AccountId,
        credential_id: ApiCredentialId,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        postgres_credential_by(self, Some((account_id, credential_id)), None).await
    }
    async fn active_api_credential_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        postgres_credential_by(self, None, Some((digest, now))).await
    }

    async fn grant_user_permission(
        &self,
        grant: &AuthorizationGrant,
    ) -> Result<(), ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        let role = Uuid::now_v7();
        let assignment = Uuid::now_v7();
        sqlx::query("INSERT INTO management.rbac_roles (account_id,id,name,role_kind,created_by,created_at,updated_at) VALUES ($1,$2,$3,'custom',$4,$5,$5)").bind(grant.account_id.as_uuid()).bind(role).bind(format!("grant-{assignment}")).bind(grant.granted_by.as_uuid()).bind(grant.created_at).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO management.rbac_role_permissions (account_id,role_id,permission) VALUES ($1,$2,$3)").bind(grant.account_id.as_uuid()).bind(role).bind(permission_name(grant.permission)).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO management.rbac_role_assignments (account_id,id,role_id,principal_type,principal_id,scope_type,system_id,delegated_by,created_at,expires_at) VALUES ($1,$2,$3,'user',$4,$5,$6,$7,$8,$9)").bind(grant.account_id.as_uuid()).bind(assignment).bind(role).bind(grant.user_id.as_uuid()).bind(if grant.system_id.is_some(){"system"}else{"account"}).bind(grant.system_id.map(SystemId::as_uuid)).bind(grant.granted_by.as_uuid()).bind(grant.created_at).bind(grant.expires_at).execute(&mut *transaction).await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn user_is_authorized(
        &self,
        user_id: UserId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let allowed=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM management.rbac_role_assignments assignment JOIN management.rbac_roles role ON role.account_id=assignment.account_id AND role.id=assignment.role_id JOIN management.rbac_role_permissions permission ON permission.account_id=role.account_id AND permission.role_id=role.id JOIN management.memberships membership ON membership.account_id=assignment.account_id AND membership.user_id=assignment.principal_id JOIN management.users user_record ON user_record.id=assignment.principal_id AND user_record.status='active' JOIN management.accounts account ON account.id=assignment.account_id AND account.status='active' WHERE assignment.principal_type='user' AND assignment.principal_id=$1 AND assignment.account_id=$2 AND membership.status='active' AND permission.permission=$3 AND assignment.revoked_at IS NULL AND (assignment.expires_at IS NULL OR assignment.expires_at>$4) AND (assignment.scope_type='account' OR (assignment.scope_type='system' AND assignment.system_id=$5)))").bind(user_id.as_uuid()).bind(account_id.as_uuid()).bind(permission_name(permission)).bind(now).bind(system_id.map(SystemId::as_uuid)).fetch_one(&mut connection).await?;
        connection.close().await?;
        Ok(allowed)
    }

    async fn principal_is_authorized(
        &self,
        principal: PrincipalId,
        account_id: AccountId,
        system_id: Option<SystemId>,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        let (principal_type, principal_id) = postgres_principal(principal);
        let mut connection = self.connection().await?;
        let allowed = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM management.rbac_role_assignments assignment JOIN management.rbac_roles role ON role.account_id=assignment.account_id AND role.id=assignment.role_id JOIN management.rbac_role_permissions permission ON permission.account_id=role.account_id AND permission.role_id=role.id JOIN management.accounts account ON account.id=assignment.account_id AND account.status='active' LEFT JOIN management.memberships membership ON assignment.principal_type='user' AND membership.account_id=assignment.account_id AND membership.user_id=assignment.principal_id AND membership.status='active' LEFT JOIN management.api_credentials credential ON assignment.principal_type='api_credential' AND credential.account_id=assignment.account_id AND credential.id=assignment.principal_id AND credential.revoked_at IS NULL AND (credential.expires_at IS NULL OR credential.expires_at>$1) LEFT JOIN management.users user_record ON user_record.id=COALESCE(membership.user_id,credential.owner_user_id) AND user_record.status='active' WHERE assignment.principal_type=$2 AND assignment.principal_id=$3 AND assignment.account_id=$4 AND permission.permission=$5 AND assignment.revoked_at IS NULL AND (assignment.expires_at IS NULL OR assignment.expires_at>$1) AND (assignment.scope_type='account' OR (assignment.scope_type='system' AND assignment.system_id=$6)) AND ((assignment.principal_type='user' AND membership.id IS NOT NULL) OR (assignment.principal_type='api_credential' AND credential.id IS NOT NULL)))")
            .bind(now).bind(principal_type).bind(principal_id).bind(account_id.as_uuid()).bind(permission_name(permission)).bind(system_id.map(SystemId::as_uuid)).fetch_one(&mut connection).await?;
        connection.close().await?;
        Ok(allowed)
    }

    async fn user_is_instance_authorized(
        &self,
        user_id: UserId,
        permission: Permission,
        now: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let allowed = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM management.rbac_role_assignments assignment JOIN management.rbac_roles role ON role.account_id IS NULL AND role.id=assignment.role_id JOIN management.rbac_role_permissions role_permission ON role_permission.account_id IS NULL AND role_permission.role_id=role.id JOIN management.users user_record ON user_record.id=assignment.principal_id AND user_record.status='active' WHERE assignment.principal_type='user' AND assignment.principal_id=$1 AND assignment.scope_type='instance' AND assignment.account_id IS NULL AND assignment.system_id IS NULL AND assignment.revoked_at IS NULL AND (assignment.expires_at IS NULL OR assignment.expires_at>$2) AND role_permission.permission=$3)")
            .bind(user_id.as_uuid()).bind(now).bind(permission_name(permission)).fetch_one(&mut connection).await?;
        connection.close().await?;
        Ok(allowed)
    }

    async fn routing(
        &self,
        account_id: AccountId,
    ) -> Result<Option<RoutingRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let row=sqlx::query("SELECT account_id,migration_state,schema_version FROM management.account_storage_registry WHERE account_id=$1").bind(account_id.as_uuid()).fetch_optional(&mut connection).await?;
        connection.close().await?;
        row.map(|row| {
            Ok(RoutingRecord {
                account_id: pg_id(row.get("account_id"), AccountId::from_uuid)?,
                backend: RoutingBackend::Postgres,
                state: row.get("migration_state"),
                opaque_locator: None,
                schema_version: row.get("schema_version"),
            })
        })
        .transpose()
    }

    async fn append_audit(&self, record: &AuditRecord) -> Result<(), ManagementRepositoryError> {
        validate_audit(record)?;
        let mut connection = self.connection().await?;
        sqlx::query("INSERT INTO management.global_audit_events (id,occurred_at,request_id,actor_type,actor_id,account_id,action,target_type,target_id,outcome,previous_event_hash,event_hash,safe_metadata) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(record.id.as_uuid()).bind(record.occurred_at).bind(record.request_id).bind(&record.actor_type).bind(record.actor_id).bind(record.account_id.map(AccountId::as_uuid)).bind(&record.action).bind(&record.target_type).bind(record.target_id).bind(&record.outcome).bind(record.previous_event_hash.map(|h|h.to_vec())).bind(record.event_hash.as_slice()).bind(&record.safe_metadata).execute(&mut connection).await?;
        connection.close().await?;
        Ok(())
    }
    async fn account_audit(
        &self,
        account_id: AccountId,
        limit: u32,
    ) -> Result<Vec<AuditRecord>, ManagementRepositoryError> {
        let mut connection = self.connection().await?;
        let rows=sqlx::query("SELECT id,occurred_at,request_id,actor_type,actor_id,account_id,action,target_type,target_id,outcome,previous_event_hash,event_hash,safe_metadata FROM management.global_audit_events WHERE account_id=$1 ORDER BY occurred_at DESC,id DESC LIMIT $2").bind(account_id.as_uuid()).bind(i64::from(limit)).fetch_all(&mut connection).await?;
        connection.close().await?;
        rows.iter().map(postgres_audit).collect()
    }
}

#[cfg(not(feature = "sqlite"))]
#[async_trait]
impl ManagementRepository for SqliteManagementRepository {
    async fn save_user(&self, _: &UserRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn user(&self, _: UserId) -> Result<Option<UserRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn save_account(&self, _: &AccountRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn account(
        &self,
        _: AccountId,
    ) -> Result<Option<AccountRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn save_system_registry(
        &self,
        _: &SystemRegistryRecord,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn system_registry(
        &self,
        _: SystemId,
    ) -> Result<Option<SystemRegistryRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn save_membership(&self, _: &MembershipRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn active_membership(
        &self,
        _: AccountId,
        _: UserId,
    ) -> Result<Option<MembershipRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn active_accounts_for_user(
        &self,
        _: UserId,
    ) -> Result<Vec<AccountRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn systems_for_account(
        &self,
        _: AccountId,
    ) -> Result<Vec<SystemId>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn save_session(&self, _: &SessionRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn active_session_by_digest(
        &self,
        _: &[u8; 32],
        _: i64,
    ) -> Result<Option<SessionRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn revoke_session(&self, _: SessionId, _: i64) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn revoke_oldest_sessions_above_limit(
        &self,
        _: UserId,
        _: u32,
        _: i64,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn save_api_credential(
        &self,
        _: &ApiCredentialRecord,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn api_credential(
        &self,
        _: AccountId,
        _: ApiCredentialId,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn active_api_credential_by_digest(
        &self,
        _: &[u8; 32],
        _: i64,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn grant_user_permission(
        &self,
        _: &AuthorizationGrant,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn user_is_authorized(
        &self,
        _: UserId,
        _: AccountId,
        _: Option<SystemId>,
        _: Permission,
        _: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn principal_is_authorized(
        &self,
        _: PrincipalId,
        _: AccountId,
        _: Option<SystemId>,
        _: Permission,
        _: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn user_is_instance_authorized(
        &self,
        _: UserId,
        _: Permission,
        _: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn routing(
        &self,
        _: AccountId,
    ) -> Result<Option<RoutingRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn append_audit(&self, _: &AuditRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
    async fn account_audit(
        &self,
        _: AccountId,
        _: u32,
    ) -> Result<Vec<AuditRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("sqlite"))
    }
}

#[cfg(not(feature = "postgres"))]
#[async_trait]
impl ManagementRepository for PostgresManagementRepository {
    async fn save_user(&self, _: &UserRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn user(&self, _: UserId) -> Result<Option<UserRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn save_account(&self, _: &AccountRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn account(
        &self,
        _: AccountId,
    ) -> Result<Option<AccountRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn save_system_registry(
        &self,
        _: &SystemRegistryRecord,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn system_registry(
        &self,
        _: SystemId,
    ) -> Result<Option<SystemRegistryRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn save_membership(&self, _: &MembershipRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn active_membership(
        &self,
        _: AccountId,
        _: UserId,
    ) -> Result<Option<MembershipRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn active_accounts_for_user(
        &self,
        _: UserId,
    ) -> Result<Vec<AccountRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn systems_for_account(
        &self,
        _: AccountId,
    ) -> Result<Vec<SystemId>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn save_session(&self, _: &SessionRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn active_session_by_digest(
        &self,
        _: &[u8; 32],
        _: i64,
    ) -> Result<Option<SessionRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn revoke_session(&self, _: SessionId, _: i64) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn revoke_oldest_sessions_above_limit(
        &self,
        _: UserId,
        _: u32,
        _: i64,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn save_api_credential(
        &self,
        _: &ApiCredentialRecord,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn api_credential(
        &self,
        _: AccountId,
        _: ApiCredentialId,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn active_api_credential_by_digest(
        &self,
        _: &[u8; 32],
        _: i64,
    ) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn grant_user_permission(
        &self,
        _: &AuthorizationGrant,
    ) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn user_is_authorized(
        &self,
        _: UserId,
        _: AccountId,
        _: Option<SystemId>,
        _: Permission,
        _: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn principal_is_authorized(
        &self,
        _: PrincipalId,
        _: AccountId,
        _: Option<SystemId>,
        _: Permission,
        _: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn user_is_instance_authorized(
        &self,
        _: UserId,
        _: Permission,
        _: i64,
    ) -> Result<bool, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn routing(
        &self,
        _: AccountId,
    ) -> Result<Option<RoutingRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn append_audit(&self, _: &AuditRecord) -> Result<(), ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
    async fn account_audit(
        &self,
        _: AccountId,
        _: u32,
    ) -> Result<Vec<AuditRecord>, ManagementRepositoryError> {
        Err(ManagementRepositoryError::AdapterDisabled("postgres"))
    }
}

// Mapping and validation helpers are kept below so both adapters reject the same unsafe states.
fn validate_user(record: &UserRecord) -> Result<(), ManagementRepositoryError> {
    if record.email.trim().is_empty()
        || record.display_name.trim().is_empty()
        || !matches!(
            record.status.as_str(),
            "invited" | "active" | "disabled" | "deleted"
        )
    {
        return Err(ManagementRepositoryError::InvalidRecord("user"));
    }
    Ok(())
}
fn validate_account(record: &AccountRecord) -> Result<(), ManagementRepositoryError> {
    if record.slug.trim().is_empty()
        || record.display_name.trim().is_empty()
        || !matches!(
            record.status.as_str(),
            "provisioning" | "active" | "suspended" | "quarantined" | "deleting" | "deleted"
        )
    {
        return Err(ManagementRepositoryError::InvalidRecord("account"));
    }
    Ok(())
}
fn validate_membership(record: &MembershipRecord) -> Result<(), ManagementRepositoryError> {
    if !matches!(
        record.status.as_str(),
        "invited" | "active" | "suspended" | "revoked"
    ) {
        return Err(ManagementRepositoryError::InvalidRecord("membership"));
    }
    Ok(())
}
fn validate_session(record: &SessionRecord) -> Result<(), ManagementRepositoryError> {
    if record.idle_expires_at > record.absolute_expires_at {
        return Err(ManagementRepositoryError::InvalidRecord("session"));
    }
    Ok(())
}
fn validate_credential(record: &ApiCredentialRecord) -> Result<(), ManagementRepositoryError> {
    if record.name.trim().is_empty() || record.scopes.is_empty() {
        return Err(ManagementRepositoryError::InvalidRecord("api_credential"));
    }
    Ok(())
}
fn validate_audit(record: &AuditRecord) -> Result<(), ManagementRepositoryError> {
    if record.action.trim().is_empty()
        || record.target_type.trim().is_empty()
        || !matches!(record.outcome.as_str(), "succeeded" | "denied" | "failed")
    {
        return Err(ManagementRepositoryError::InvalidRecord("audit"));
    }
    Ok(())
}

fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::InstanceRead => "instance_read",
        Permission::InstanceManage => "instance_manage",
        Permission::AccountRead => "account_read",
        Permission::AccountManage => "account_manage",
        Permission::MembershipManage => "membership_manage",
        Permission::RoleManage => "role_manage",
        Permission::SystemRead => "system_read",
        Permission::SystemManage => "system_manage",
        Permission::TelemetryRead => "telemetry_read",
        Permission::TelemetryWrite => "telemetry_write",
        Permission::CredentialManage => "credential_manage",
        Permission::IntegrationManage => "integration_manage",
        Permission::AuditRead => "audit_read",
    }
}

#[cfg(feature = "sqlite")]
fn sqlite_principal(principal: PrincipalId) -> (&'static str, Vec<u8>) {
    match principal {
        PrincipalId::User(id) => ("user", uuid_blob(id.as_uuid())),
        PrincipalId::ApiCredential(id) => ("api_credential", uuid_blob(id.as_uuid())),
    }
}

#[cfg(feature = "postgres")]
fn postgres_principal(principal: PrincipalId) -> (&'static str, Uuid) {
    match principal {
        PrincipalId::User(id) => ("user", id.as_uuid()),
        PrincipalId::ApiCredential(id) => ("api_credential", id.as_uuid()),
    }
}

#[cfg(feature = "sqlite")]
fn uuid_blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn blob_uuid(bytes: Vec<u8>) -> Result<Uuid, ManagementRepositoryError> {
    Uuid::from_slice(&bytes).map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn user_id_from_blob(bytes: Vec<u8>) -> Result<UserId, ManagementRepositoryError> {
    UserId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn account_id_from_blob(bytes: Vec<u8>) -> Result<AccountId, ManagementRepositoryError> {
    AccountId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn system_id_from_blob(bytes: Vec<u8>) -> Result<SystemId, ManagementRepositoryError> {
    SystemId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn membership_id_from_blob(bytes: Vec<u8>) -> Result<MembershipId, ManagementRepositoryError> {
    MembershipId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn session_id_from_blob(bytes: Vec<u8>) -> Result<SessionId, ManagementRepositoryError> {
    SessionId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn credential_id_from_blob(bytes: Vec<u8>) -> Result<ApiCredentialId, ManagementRepositoryError> {
    ApiCredentialId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "sqlite")]
fn audit_id_from_blob(bytes: Vec<u8>) -> Result<AuditEventId, ManagementRepositoryError> {
    AuditEventId::from_uuid(blob_uuid(bytes)?)
        .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
fn fixed_digest(bytes: Vec<u8>) -> Result<[u8; 32], ManagementRepositoryError> {
    bytes
        .try_into()
        .map_err(|_| ManagementRepositoryError::InvalidStoredDigest)
}

#[cfg(feature = "sqlite")]
fn sqlite_user(row: &sqlx::sqlite::SqliteRow) -> Result<UserRecord, ManagementRepositoryError> {
    Ok(UserRecord {
        id: user_id_from_blob(row.get("id"))?,
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_account(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<AccountRecord, ManagementRepositoryError> {
    Ok(AccountRecord {
        id: account_id_from_blob(row.get("id"))?,
        slug: row.get("slug"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        created_by: row
            .get::<Option<Vec<u8>>, _>("created_by")
            .map(user_id_from_blob)
            .transpose()?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_system_registry(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<SystemRegistryRecord, ManagementRepositoryError> {
    Ok(SystemRegistryRecord {
        system_id: system_id_from_blob(row.get("system_id"))?,
        account_id: account_id_from_blob(row.get("account_id"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_membership(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<MembershipRecord, ManagementRepositoryError> {
    Ok(MembershipRecord {
        id: membership_id_from_blob(row.get("id"))?,
        account_id: account_id_from_blob(row.get("account_id"))?,
        user_id: user_id_from_blob(row.get("user_id"))?,
        status: row.get("status"),
        joined_at: row.get("joined_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "sqlite")]
fn sqlite_session(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<SessionRecord, ManagementRepositoryError> {
    Ok(SessionRecord {
        id: session_id_from_blob(row.get("id"))?,
        user_id: user_id_from_blob(row.get("user_id"))?,
        session_digest: fixed_digest(row.get("session_digest"))?,
        csrf_digest: fixed_digest(row.get("csrf_digest"))?,
        created_at: row.get("created_at"),
        last_seen_at: row.get("last_seen_at"),
        idle_expires_at: row.get("idle_expires_at"),
        absolute_expires_at: row.get("absolute_expires_at"),
        revoked_at: row.get("revoked_at"),
    })
}

#[cfg(feature = "sqlite")]
async fn sqlite_credential_by(
    repo: &SqliteManagementRepository,
    clause: &str,
    first: Vec<u8>,
    second: Option<Vec<u8>>,
    now: Option<i64>,
) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
    let mut connection = repo.connection().await?;
    let sql = format!(
        "SELECT id,account_id,owner_user_id,system_id,name,credential_digest,created_at,expires_at,revoked_at FROM api_credentials WHERE {clause}"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(first);
    if let Some(value) = second {
        query = query.bind(value);
    }
    if let Some(value) = now {
        query = query.bind(value);
    }
    let row = query.fetch_optional(&mut connection).await?;
    let Some(row) = row else {
        connection.close().await?;
        return Ok(None);
    };
    let id = credential_id_from_blob(row.get("id"))?;
    let scopes = sqlx::query_scalar::<_, String>(
        "SELECT scope FROM api_credential_scopes WHERE credential_id=? ORDER BY scope",
    )
    .bind(uuid_blob(id.as_uuid()))
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .collect();
    let record = ApiCredentialRecord {
        id,
        account_id: account_id_from_blob(row.get("account_id"))?,
        owner_user_id: user_id_from_blob(row.get("owner_user_id"))?,
        system_id: row
            .get::<Option<Vec<u8>>, _>("system_id")
            .map(|v| {
                SystemId::from_uuid(blob_uuid(v)?)
                    .map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
            })
            .transpose()?,
        name: row.get("name"),
        credential_digest: fixed_digest(row.get("credential_digest"))?,
        scopes,
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
    };
    connection.close().await?;
    Ok(Some(record))
}

#[cfg(feature = "sqlite")]
fn sqlite_audit(row: &sqlx::sqlite::SqliteRow) -> Result<AuditRecord, ManagementRepositoryError> {
    Ok(AuditRecord {
        id: audit_id_from_blob(row.get("id"))?,
        occurred_at: row.get("occurred_at"),
        request_id: row
            .get::<Option<Vec<u8>>, _>("request_id")
            .map(blob_uuid)
            .transpose()?,
        actor_type: row.get("actor_type"),
        actor_id: row
            .get::<Option<Vec<u8>>, _>("actor_id")
            .map(blob_uuid)
            .transpose()?,
        account_id: row
            .get::<Option<Vec<u8>>, _>("account_id")
            .map(account_id_from_blob)
            .transpose()?,
        action: row.get("action"),
        target_type: row.get("target_type"),
        target_id: row
            .get::<Option<Vec<u8>>, _>("target_id")
            .map(blob_uuid)
            .transpose()?,
        outcome: row.get("outcome"),
        previous_event_hash: row
            .get::<Option<Vec<u8>>, _>("previous_event_hash")
            .map(fixed_digest)
            .transpose()?,
        event_hash: fixed_digest(row.get("event_hash"))?,
        safe_metadata: serde_json::from_str(&row.get::<String, _>("safe_metadata_json"))?,
    })
}

#[cfg(feature = "postgres")]
fn pg_id<T>(
    uuid: Uuid,
    wrap: impl FnOnce(Uuid) -> Result<T, pvlog_domain::IdentifierError>,
) -> Result<T, ManagementRepositoryError> {
    wrap(uuid).map_err(|_| ManagementRepositoryError::InvalidStoredIdentifier)
}
#[cfg(feature = "postgres")]
fn postgres_user(row: &sqlx::postgres::PgRow) -> Result<UserRecord, ManagementRepositoryError> {
    Ok(UserRecord {
        id: pg_id(row.get("id"), UserId::from_uuid)?,
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn postgres_account(
    row: &sqlx::postgres::PgRow,
) -> Result<AccountRecord, ManagementRepositoryError> {
    Ok(AccountRecord {
        id: pg_id(row.get("id"), AccountId::from_uuid)?,
        slug: row.get("slug"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        created_by: row
            .get::<Option<Uuid>, _>("created_by")
            .map(|id| pg_id(id, UserId::from_uuid))
            .transpose()?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn postgres_system_registry(
    row: &sqlx::postgres::PgRow,
) -> Result<SystemRegistryRecord, ManagementRepositoryError> {
    Ok(SystemRegistryRecord {
        system_id: pg_id(row.get("system_id"), SystemId::from_uuid)?,
        account_id: pg_id(row.get("account_id"), AccountId::from_uuid)?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn postgres_membership(
    row: &sqlx::postgres::PgRow,
) -> Result<MembershipRecord, ManagementRepositoryError> {
    Ok(MembershipRecord {
        id: pg_id(row.get("id"), MembershipId::from_uuid)?,
        account_id: pg_id(row.get("account_id"), AccountId::from_uuid)?,
        user_id: pg_id(row.get("user_id"), UserId::from_uuid)?,
        status: row.get("status"),
        joined_at: row.get("joined_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
#[cfg(feature = "postgres")]
fn postgres_session(
    row: &sqlx::postgres::PgRow,
) -> Result<SessionRecord, ManagementRepositoryError> {
    Ok(SessionRecord {
        id: pg_id(row.get("id"), SessionId::from_uuid)?,
        user_id: pg_id(row.get("user_id"), UserId::from_uuid)?,
        session_digest: fixed_digest(row.get("session_digest"))?,
        csrf_digest: fixed_digest(row.get("csrf_digest"))?,
        created_at: row.get("created_at"),
        last_seen_at: row.get("last_seen_at"),
        idle_expires_at: row.get("idle_expires_at"),
        absolute_expires_at: row.get("absolute_expires_at"),
        revoked_at: row.get("revoked_at"),
    })
}
#[cfg(feature = "postgres")]
async fn postgres_credential_by(
    repo: &PostgresManagementRepository,
    key: Option<(AccountId, ApiCredentialId)>,
    digest: Option<(&[u8; 32], i64)>,
) -> Result<Option<ApiCredentialRecord>, ManagementRepositoryError> {
    let mut connection = repo.connection().await?;
    let row = if let Some((account, id)) = key {
        sqlx::query("SELECT id,account_id,owner_user_id,system_id,name,credential_digest,created_at,expires_at,revoked_at FROM management.api_credentials WHERE account_id=$1 AND id=$2").bind(account.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut connection).await?
    } else if let Some((digest, now)) = digest {
        sqlx::query("SELECT credential.id,credential.account_id,credential.owner_user_id,credential.system_id,credential.name,credential.credential_digest,credential.created_at,credential.expires_at,credential.revoked_at FROM management.api_credentials credential JOIN management.users user_record ON user_record.id=credential.owner_user_id AND user_record.status='active' JOIN management.accounts account ON account.id=credential.account_id AND account.status='active' WHERE credential.credential_digest=$1 AND credential.revoked_at IS NULL AND (credential.expires_at IS NULL OR credential.expires_at>$2)").bind(digest.as_slice()).bind(now).fetch_optional(&mut connection).await?
    } else {
        return Err(ManagementRepositoryError::InvalidRecord(
            "credential_lookup",
        ));
    };
    let Some(row) = row else {
        connection.close().await?;
        return Ok(None);
    };
    let id = pg_id(row.get("id"), ApiCredentialId::from_uuid)?;
    let account_id = pg_id(row.get("account_id"), AccountId::from_uuid)?;
    let scopes=sqlx::query_scalar::<_,String>("SELECT scope FROM management.api_credential_scopes WHERE account_id=$1 AND credential_id=$2 ORDER BY scope").bind(account_id.as_uuid()).bind(id.as_uuid()).fetch_all(&mut connection).await?.into_iter().collect();
    let record = ApiCredentialRecord {
        id,
        account_id,
        owner_user_id: pg_id(row.get("owner_user_id"), UserId::from_uuid)?,
        system_id: row
            .get::<Option<Uuid>, _>("system_id")
            .map(|id| pg_id(id, SystemId::from_uuid))
            .transpose()?,
        name: row.get("name"),
        credential_digest: fixed_digest(row.get("credential_digest"))?,
        scopes,
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
    };
    connection.close().await?;
    Ok(Some(record))
}
#[cfg(feature = "postgres")]
fn postgres_audit(row: &sqlx::postgres::PgRow) -> Result<AuditRecord, ManagementRepositoryError> {
    Ok(AuditRecord {
        id: pg_id(row.get("id"), AuditEventId::from_uuid)?,
        occurred_at: row.get("occurred_at"),
        request_id: row.get("request_id"),
        actor_type: row.get("actor_type"),
        actor_id: row.get("actor_id"),
        account_id: row
            .get::<Option<Uuid>, _>("account_id")
            .map(|id| pg_id(id, AccountId::from_uuid))
            .transpose()?,
        action: row.get("action"),
        target_type: row.get("target_type"),
        target_id: row.get("target_id"),
        outcome: row.get("outcome"),
        previous_event_hash: row
            .get::<Option<Vec<u8>>, _>("previous_event_hash")
            .map(fixed_digest)
            .transpose()?,
        event_hash: fixed_digest(row.get("event_hash"))?,
        safe_metadata: row.get("safe_metadata"),
    })
}

/// Management repository failure.
#[derive(Debug, Error)]
pub enum ManagementRepositoryError {
    #[error("management database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("management JSON value is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid {0} management record")]
    InvalidRecord(&'static str),
    #[error("management storage contains a non-UUIDv7 identifier")]
    InvalidStoredIdentifier,
    #[error("management storage contains an invalid credential digest")]
    InvalidStoredDigest,
    #[error("the {0} management adapter is disabled")]
    AdapterDisabled(&'static str),
}
