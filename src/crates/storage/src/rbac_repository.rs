//! Hierarchical RBAC persistence for management databases.

#[cfg(feature = "postgres")]
use std::fmt;
#[cfg(feature = "sqlite")]
use std::path::PathBuf;

use async_trait::async_trait;
use pvlog_application::{PortError, RbacRepository, RbacRoleRecord};
use pvlog_domain::{
    AccountId, ApiCredentialId, BuiltInRole, Permission, PrincipalId, Role, RoleAssignment,
    RoleAssignmentId, RoleId, RoleKind, RoleScope, SystemId, UserId, UtcTimestamp,
};
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
#[cfg(feature = "sqlite")]
use sqlx::{SqliteConnection, sqlite::SqliteConnectOptions};
use uuid::Uuid;

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteRbacRepository {
    path: PathBuf,
}
#[cfg(feature = "sqlite")]
impl SqliteRbacRepository {
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
pub struct PostgresRbacRepository {
    url: String,
}
#[cfg(feature = "postgres")]
impl PostgresRbacRepository {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self { url }
    }
    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}
#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresRbacRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresRbacRepository")
            .field("url", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl RbacRepository for SqliteRbacRepository {
    async fn roles(&self, account_id: Option<AccountId>) -> Result<Vec<RbacRoleRecord>, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let rows = if let Some(account) = account_id {
            sqlx::query("SELECT id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version FROM rbac_roles WHERE account_id=? ORDER BY name,id").bind(blob(account.as_uuid())).fetch_all(&mut c).await.map_err(port)?
        } else {
            sqlx::query("SELECT id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version FROM rbac_roles WHERE account_id IS NULL ORDER BY name,id").fetch_all(&mut c).await.map_err(port)?
        };
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(sqlite_role(&mut c, &row).await?);
        }
        c.close().await.map_err(port)?;
        Ok(result)
    }
    async fn role(&self, id: RoleId) -> Result<Option<RbacRoleRecord>, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let row=sqlx::query("SELECT id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version FROM rbac_roles WHERE id=?").bind(blob(id.as_uuid())).fetch_optional(&mut c).await.map_err(port)?;
        let result = if let Some(row) = row {
            Some(sqlite_role(&mut c, &row).await?)
        } else {
            None
        };
        c.close().await.map_err(port)?;
        Ok(result)
    }
    async fn save_role(&self, record: &RbacRoleRecord) -> Result<(), PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let mut tx = c.begin().await.map_err(port)?;
        let (kind, key) = role_kind(&record.role.kind);
        sqlx::query("INSERT INTO rbac_roles (id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,updated_at=excluded.updated_at,version=excluded.version").bind(blob(record.role.id.as_uuid())).bind(record.role.account_id.map(|id|blob(id.as_uuid()))).bind(&record.role.name).bind(kind).bind(key).bind(record.created_by.map(|id|blob(id.as_uuid()))).bind(record.created_at).bind(record.updated_at).bind(record.version).execute(&mut *tx).await.map_err(port)?;
        sqlx::query("DELETE FROM rbac_role_permissions WHERE role_id=?")
            .bind(blob(record.role.id.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(port)?;
        for permission in &record.role.permissions {
            sqlx::query("INSERT INTO rbac_role_permissions (role_id,permission) VALUES (?,?)")
                .bind(blob(record.role.id.as_uuid()))
                .bind(permission_name(*permission))
                .execute(&mut *tx)
                .await
                .map_err(port)?;
        }
        sqlx::query("DELETE FROM rbac_role_inheritance WHERE role_id=?")
            .bind(blob(record.role.id.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(port)?;
        for parent in &record.role.parent_role_ids {
            sqlx::query("INSERT INTO rbac_role_inheritance (role_id,parent_role_id) VALUES (?,?)")
                .bind(blob(record.role.id.as_uuid()))
                .bind(blob(parent.as_uuid()))
                .execute(&mut *tx)
                .await
                .map_err(port)?;
        }
        tx.commit().await.map_err(port)?;
        Ok(())
    }
    async fn delete_custom_role(&self, id: RoleId) -> Result<bool, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let result = sqlx::query("DELETE FROM rbac_roles WHERE id=? AND role_kind='custom'")
            .bind(blob(id.as_uuid()))
            .execute(&mut c)
            .await
            .map_err(port)?;
        c.close().await.map_err(port)?;
        Ok(result.rows_affected() == 1)
    }
    async fn active_assignments(
        &self,
        principal: PrincipalId,
        now: i64,
    ) -> Result<Vec<RoleAssignment>, PortError> {
        let (pt, pid) = principal_parts(principal);
        let mut c = self.connection().await.map_err(port)?;
        let rows=sqlx::query("SELECT id,role_id,principal_type,principal_id,scope_type,account_id,system_id,delegated_by,created_at,expires_at FROM rbac_role_assignments assignment WHERE principal_type=? AND principal_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?) AND (principal_type<>'user' OR EXISTS(SELECT 1 FROM users WHERE users.id=assignment.principal_id AND users.status='active')) AND (scope_type='instance' OR principal_type<>'user' OR EXISTS(SELECT 1 FROM memberships WHERE memberships.account_id=assignment.account_id AND memberships.user_id=assignment.principal_id AND memberships.status='active')) ORDER BY created_at,id").bind(pt).bind(blob(pid)).bind(now).fetch_all(&mut c).await.map_err(port)?;
        c.close().await.map_err(port)?;
        rows.iter().map(sqlite_assignment).collect()
    }
    async fn save_assignment(&self, a: &RoleAssignment) -> Result<(), PortError> {
        let (pt, pid) = principal_parts(a.principal);
        let (scope, account, system) = scope_parts(a.scope);
        let mut c = self.connection().await.map_err(port)?;
        let mut tx = c.begin().await.map_err(port)?;
        let result=sqlx::query("UPDATE rbac_role_assignments SET id=?,delegated_by=?,created_at=?,expires_at=?,revoked_at=NULL WHERE role_id=? AND principal_type=? AND principal_id=? AND scope_type=? AND account_id IS ? AND system_id IS ?").bind(blob(a.id.as_uuid())).bind(blob(a.granted_by.as_uuid())).bind(epoch(a.granted_at)?).bind(a.expires_at.map(epoch).transpose()?).bind(blob(a.role_id.as_uuid())).bind(pt).bind(blob(pid)).bind(scope).bind(account.map(|id|blob(id.as_uuid()))).bind(system.map(|id|blob(id.as_uuid()))).execute(&mut *tx).await.map_err(port)?;
        if result.rows_affected() == 0 {
            sqlx::query("INSERT INTO rbac_role_assignments (id,role_id,principal_type,principal_id,scope_type,account_id,system_id,delegated_by,created_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?,?)").bind(blob(a.id.as_uuid())).bind(blob(a.role_id.as_uuid())).bind(pt).bind(blob(pid)).bind(scope).bind(account.map(|id|blob(id.as_uuid()))).bind(system.map(|id|blob(id.as_uuid()))).bind(blob(a.granted_by.as_uuid())).bind(epoch(a.granted_at)?).bind(a.expires_at.map(epoch).transpose()?).execute(&mut *tx).await.map_err(port)?;
        }
        tx.commit().await.map_err(port)?;
        Ok(())
    }
    async fn revoke_assignment(&self, id: RoleAssignmentId, now: i64) -> Result<bool, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let r = sqlx::query(
            "UPDATE rbac_role_assignments SET revoked_at=? WHERE id=? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(blob(id.as_uuid()))
        .execute(&mut c)
        .await
        .map_err(port)?;
        c.close().await.map_err(port)?;
        Ok(r.rows_affected() == 1)
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl RbacRepository for PostgresRbacRepository {
    async fn roles(&self, account_id: Option<AccountId>) -> Result<Vec<RbacRoleRecord>, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let rows = if let Some(account) = account_id {
            sqlx::query("SELECT id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version FROM management.rbac_roles WHERE account_id=$1 ORDER BY name,id").bind(account.as_uuid()).fetch_all(&mut c).await.map_err(port)?
        } else {
            sqlx::query("SELECT id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version FROM management.rbac_roles WHERE account_id IS NULL ORDER BY name,id").fetch_all(&mut c).await.map_err(port)?
        };
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(pg_role(&mut c, &row).await?);
        }
        c.close().await.map_err(port)?;
        Ok(result)
    }
    async fn role(&self, id: RoleId) -> Result<Option<RbacRoleRecord>, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let row=sqlx::query("SELECT id,account_id,name,role_kind,built_in_key,created_by,created_at,updated_at,version FROM management.rbac_roles WHERE id=$1 LIMIT 1").bind(id.as_uuid()).fetch_optional(&mut c).await.map_err(port)?;
        let result = if let Some(row) = row {
            Some(pg_role(&mut c, &row).await?)
        } else {
            None
        };
        c.close().await.map_err(port)?;
        Ok(result)
    }
    async fn save_role(&self, record: &RbacRoleRecord) -> Result<(), PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let mut tx = c.begin().await.map_err(port)?;
        let (kind, key) = role_kind(&record.role.kind);
        sqlx::query("INSERT INTO management.rbac_roles (account_id,id,name,role_kind,built_in_key,created_by,created_at,updated_at,version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(account_id,id) DO UPDATE SET name=excluded.name,updated_at=excluded.updated_at,version=excluded.version").bind(record.role.account_id.map(AccountId::as_uuid)).bind(record.role.id.as_uuid()).bind(&record.role.name).bind(kind).bind(key).bind(record.created_by.map(UserId::as_uuid)).bind(record.created_at).bind(record.updated_at).bind(record.version).execute(&mut *tx).await.map_err(port)?;
        sqlx::query("DELETE FROM management.rbac_role_permissions WHERE account_id IS NOT DISTINCT FROM $1 AND role_id=$2").bind(record.role.account_id.map(AccountId::as_uuid)).bind(record.role.id.as_uuid()).execute(&mut *tx).await.map_err(port)?;
        for permission in &record.role.permissions {
            sqlx::query("INSERT INTO management.rbac_role_permissions (account_id,role_id,permission) VALUES ($1,$2,$3)").bind(record.role.account_id.map(AccountId::as_uuid)).bind(record.role.id.as_uuid()).bind(permission_name(*permission)).execute(&mut *tx).await.map_err(port)?;
        }
        sqlx::query("DELETE FROM management.rbac_role_inheritance WHERE account_id IS NOT DISTINCT FROM $1 AND role_id=$2").bind(record.role.account_id.map(AccountId::as_uuid)).bind(record.role.id.as_uuid()).execute(&mut *tx).await.map_err(port)?;
        for parent in &record.role.parent_role_ids {
            sqlx::query("INSERT INTO management.rbac_role_inheritance (account_id,role_id,parent_role_id) VALUES ($1,$2,$3)").bind(record.role.account_id.map(AccountId::as_uuid)).bind(record.role.id.as_uuid()).bind(parent.as_uuid()).execute(&mut *tx).await.map_err(port)?;
        }
        tx.commit().await.map_err(port)?;
        Ok(())
    }
    async fn delete_custom_role(&self, id: RoleId) -> Result<bool, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let r = sqlx::query("DELETE FROM management.rbac_roles WHERE id=$1 AND role_kind='custom'")
            .bind(id.as_uuid())
            .execute(&mut c)
            .await
            .map_err(port)?;
        c.close().await.map_err(port)?;
        Ok(r.rows_affected() == 1)
    }
    async fn active_assignments(
        &self,
        principal: PrincipalId,
        now: i64,
    ) -> Result<Vec<RoleAssignment>, PortError> {
        let (pt, pid) = principal_parts(principal);
        let mut c = self.connection().await.map_err(port)?;
        let rows=sqlx::query("SELECT id,role_id,principal_type,principal_id,scope_type,account_id,system_id,delegated_by,created_at,expires_at FROM management.rbac_role_assignments assignment WHERE principal_type=$1 AND principal_id=$2 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>$3) AND (principal_type<>'user' OR EXISTS(SELECT 1 FROM management.users user_record WHERE user_record.id=assignment.principal_id AND user_record.status='active')) AND (scope_type='instance' OR principal_type<>'user' OR EXISTS(SELECT 1 FROM management.memberships membership WHERE membership.account_id=assignment.account_id AND membership.user_id=assignment.principal_id AND membership.status='active')) ORDER BY created_at,id").bind(pt).bind(pid).bind(now).fetch_all(&mut c).await.map_err(port)?;
        c.close().await.map_err(port)?;
        rows.iter().map(pg_assignment).collect()
    }
    async fn save_assignment(&self, a: &RoleAssignment) -> Result<(), PortError> {
        let (pt, pid) = principal_parts(a.principal);
        let (scope, account, system) = scope_parts(a.scope);
        let mut c = self.connection().await.map_err(port)?;
        sqlx::query("INSERT INTO management.rbac_role_assignments (account_id,id,role_id,principal_type,principal_id,scope_type,system_id,delegated_by,created_at,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(account_id,role_id,principal_type,principal_id,scope_type,system_id) DO UPDATE SET delegated_by=excluded.delegated_by,created_at=excluded.created_at,expires_at=excluded.expires_at,revoked_at=NULL").bind(account.map(AccountId::as_uuid)).bind(a.id.as_uuid()).bind(a.role_id.as_uuid()).bind(pt).bind(pid).bind(scope).bind(system.map(SystemId::as_uuid)).bind(a.granted_by.as_uuid()).bind(epoch(a.granted_at)?).bind(a.expires_at.map(epoch).transpose()?).execute(&mut c).await.map_err(port)?;
        c.close().await.map_err(port)?;
        Ok(())
    }
    async fn revoke_assignment(&self, id: RoleAssignmentId, now: i64) -> Result<bool, PortError> {
        let mut c = self.connection().await.map_err(port)?;
        let r=sqlx::query("UPDATE management.rbac_role_assignments SET revoked_at=$1 WHERE id=$2 AND revoked_at IS NULL").bind(now).bind(id.as_uuid()).execute(&mut c).await.map_err(port)?;
        c.close().await.map_err(port)?;
        Ok(r.rows_affected() == 1)
    }
}

#[cfg(feature = "sqlite")]
async fn sqlite_role(
    c: &mut SqliteConnection,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<RbacRoleRecord, PortError> {
    let id = rid(row.get("id"))?;
    let permissions = sqlx::query_scalar::<_, String>(
        "SELECT permission FROM rbac_role_permissions WHERE role_id=? ORDER BY permission",
    )
    .bind(blob(id.as_uuid()))
    .fetch_all(&mut *c)
    .await
    .map_err(port)?
    .into_iter()
    .map(|v| parse_permission(&v))
    .collect::<Result<_, _>>()?;
    let parents = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT parent_role_id FROM rbac_role_inheritance WHERE role_id=? ORDER BY parent_role_id",
    )
    .bind(blob(id.as_uuid()))
    .fetch_all(&mut *c)
    .await
    .map_err(port)?
    .into_iter()
    .map(rid)
    .collect::<Result<_, _>>()?;
    Ok(RbacRoleRecord {
        role: Role {
            id,
            account_id: row
                .get::<Option<Vec<u8>>, _>("account_id")
                .map(aid)
                .transpose()?,
            name: row.get("name"),
            kind: parse_kind(&row.get::<String, _>("role_kind"), row.get("built_in_key"))?,
            parent_role_ids: parents,
            permissions,
        },
        created_by: row
            .get::<Option<Vec<u8>>, _>("created_by")
            .map(uid)
            .transpose()?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        version: row.get("version"),
    })
}
#[cfg(feature = "postgres")]
async fn pg_role(
    c: &mut PgConnection,
    row: &sqlx::postgres::PgRow,
) -> Result<RbacRoleRecord, PortError> {
    let id = RoleId::from_uuid(row.get("id")).map_err(invalid)?;
    let account: Option<Uuid> = row.get("account_id");
    let permissions=sqlx::query_scalar::<_,String>("SELECT permission FROM management.rbac_role_permissions WHERE account_id IS NOT DISTINCT FROM $1 AND role_id=$2 ORDER BY permission").bind(account).bind(id.as_uuid()).fetch_all(&mut *c).await.map_err(port)?.into_iter().map(|v|parse_permission(&v)).collect::<Result<_,_>>()?;
    let parents=sqlx::query_scalar::<_,Uuid>("SELECT parent_role_id FROM management.rbac_role_inheritance WHERE account_id IS NOT DISTINCT FROM $1 AND role_id=$2 ORDER BY parent_role_id").bind(account).bind(id.as_uuid()).fetch_all(&mut *c).await.map_err(port)?.into_iter().map(|v|RoleId::from_uuid(v).map_err(invalid)).collect::<Result<_,_>>()?;
    Ok(RbacRoleRecord {
        role: Role {
            id,
            account_id: account
                .map(AccountId::from_uuid)
                .transpose()
                .map_err(invalid)?,
            name: row.get("name"),
            kind: parse_kind(&row.get::<String, _>("role_kind"), row.get("built_in_key"))?,
            parent_role_ids: parents,
            permissions,
        },
        created_by: row
            .get::<Option<Uuid>, _>("created_by")
            .map(UserId::from_uuid)
            .transpose()
            .map_err(invalid)?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        version: row.get("version"),
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_assignment(r: &sqlx::sqlite::SqliteRow) -> Result<RoleAssignment, PortError> {
    assignment(
        raid(r.get("id"))?,
        rid(r.get("role_id"))?,
        parse_principal(
            &r.get::<String, _>("principal_type"),
            Uuid::from_slice(&r.get::<Vec<u8>, _>("principal_id")).map_err(invalid)?,
        )?,
        parse_scope(
            &r.get::<String, _>("scope_type"),
            r.get::<Option<Vec<u8>>, _>("account_id")
                .map(aid)
                .transpose()?,
            r.get::<Option<Vec<u8>>, _>("system_id")
                .map(sid)
                .transpose()?,
        )?,
        uid(r.get("delegated_by"))?,
        r.get("created_at"),
        r.get("expires_at"),
    )
}
#[cfg(feature = "postgres")]
fn pg_assignment(r: &sqlx::postgres::PgRow) -> Result<RoleAssignment, PortError> {
    assignment(
        RoleAssignmentId::from_uuid(r.get("id")).map_err(invalid)?,
        RoleId::from_uuid(r.get("role_id")).map_err(invalid)?,
        parse_principal(&r.get::<String, _>("principal_type"), r.get("principal_id"))?,
        parse_scope(
            &r.get::<String, _>("scope_type"),
            r.get::<Option<Uuid>, _>("account_id")
                .map(AccountId::from_uuid)
                .transpose()
                .map_err(invalid)?,
            r.get::<Option<Uuid>, _>("system_id")
                .map(SystemId::from_uuid)
                .transpose()
                .map_err(invalid)?,
        )?,
        UserId::from_uuid(r.get("delegated_by")).map_err(invalid)?,
        r.get("created_at"),
        r.get("expires_at"),
    )
}
fn assignment(
    id: RoleAssignmentId,
    role_id: RoleId,
    principal: PrincipalId,
    scope: RoleScope,
    granted_by: UserId,
    created: i64,
    expires: Option<i64>,
) -> Result<RoleAssignment, PortError> {
    Ok(RoleAssignment {
        id,
        principal,
        role_id,
        scope,
        granted_by,
        granted_at: UtcTimestamp::from_epoch_millis(created).map_err(invalid)?,
        expires_at: expires
            .map(UtcTimestamp::from_epoch_millis)
            .transpose()
            .map_err(invalid)?,
    })
}
fn role_kind(k: &RoleKind) -> (&'static str, Option<&'static str>) {
    match k {
        RoleKind::Custom => ("custom", None),
        RoleKind::BuiltIn(v) => ("built_in", Some(builtin_name(*v))),
    }
}
#[allow(clippy::needless_pass_by_value)]
fn parse_kind(kind: &str, key: Option<String>) -> Result<RoleKind, PortError> {
    if kind == "custom" {
        Ok(RoleKind::Custom)
    } else if kind == "built_in" {
        Ok(RoleKind::BuiltIn(parse_builtin(
            key.as_deref()
                .ok_or_else(|| PortError::Rejected("missing_builtin_key".to_owned()))?,
        )?))
    } else {
        Err(PortError::Rejected("invalid_role_kind".to_owned()))
    }
}
fn builtin_name(v: BuiltInRole) -> &'static str {
    match v {
        BuiltInRole::InstanceAdministrator => "instance_administrator",
        BuiltInRole::AccountOwner => "owner",
        BuiltInRole::AccountAdministrator => "administrator",
        BuiltInRole::Manager => "manager",
        BuiltInRole::Contributor => "contributor",
        BuiltInRole::Operator => "operator",
        BuiltInRole::Analyst => "analyst",
        BuiltInRole::Viewer => "viewer",
        BuiltInRole::Auditor => "auditor",
        BuiltInRole::Uploader => "uploader",
    }
}
fn parse_builtin(v: &str) -> Result<BuiltInRole, PortError> {
    match v {
        "instance_administrator" => Ok(BuiltInRole::InstanceAdministrator),
        "owner" => Ok(BuiltInRole::AccountOwner),
        "administrator" => Ok(BuiltInRole::AccountAdministrator),
        "manager" => Ok(BuiltInRole::Manager),
        "contributor" => Ok(BuiltInRole::Contributor),
        "operator" => Ok(BuiltInRole::Operator),
        "analyst" => Ok(BuiltInRole::Analyst),
        "viewer" => Ok(BuiltInRole::Viewer),
        "auditor" => Ok(BuiltInRole::Auditor),
        "uploader" => Ok(BuiltInRole::Uploader),
        _ => Err(PortError::Rejected("invalid_builtin_key".to_owned())),
    }
}
fn permission_name(p: Permission) -> &'static str {
    match p {
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
fn parse_permission(v: &str) -> Result<Permission, PortError> {
    match v {
        "instance_read" => Ok(Permission::InstanceRead),
        "instance_manage" => Ok(Permission::InstanceManage),
        "account_read" => Ok(Permission::AccountRead),
        "account_manage" => Ok(Permission::AccountManage),
        "membership_manage" => Ok(Permission::MembershipManage),
        "role_manage" => Ok(Permission::RoleManage),
        "system_read" => Ok(Permission::SystemRead),
        "system_manage" => Ok(Permission::SystemManage),
        "telemetry_read" => Ok(Permission::TelemetryRead),
        "telemetry_write" => Ok(Permission::TelemetryWrite),
        "credential_manage" => Ok(Permission::CredentialManage),
        "integration_manage" => Ok(Permission::IntegrationManage),
        "audit_read" => Ok(Permission::AuditRead),
        _ => Err(PortError::Rejected("invalid_permission".to_owned())),
    }
}
fn principal_parts(p: PrincipalId) -> (&'static str, Uuid) {
    match p {
        PrincipalId::User(id) => ("user", id.as_uuid()),
        PrincipalId::ApiCredential(id) => ("api_credential", id.as_uuid()),
    }
}
fn parse_principal(kind: &str, id: Uuid) -> Result<PrincipalId, PortError> {
    match kind {
        "user" => Ok(PrincipalId::User(UserId::from_uuid(id).map_err(invalid)?)),
        "api_credential" => Ok(PrincipalId::ApiCredential(
            ApiCredentialId::from_uuid(id).map_err(invalid)?,
        )),
        _ => Err(PortError::Rejected("invalid_principal".to_owned())),
    }
}
fn scope_parts(s: RoleScope) -> (&'static str, Option<AccountId>, Option<SystemId>) {
    match s {
        RoleScope::Instance => ("instance", None, None),
        RoleScope::Account(a) => ("account", Some(a), None),
        RoleScope::System {
            account_id,
            system_id,
        } => ("system", Some(account_id), Some(system_id)),
    }
}
fn parse_scope(
    kind: &str,
    a: Option<AccountId>,
    s: Option<SystemId>,
) -> Result<RoleScope, PortError> {
    match (kind, a, s) {
        ("instance", None, None) => Ok(RoleScope::Instance),
        ("account", Some(a), None) => Ok(RoleScope::Account(a)),
        ("system", Some(a), Some(s)) => Ok(RoleScope::System {
            account_id: a,
            system_id: s,
        }),
        _ => Err(PortError::Rejected("invalid_role_scope".to_owned())),
    }
}
fn epoch(v: UtcTimestamp) -> Result<i64, PortError> {
    i64::try_from(v.epoch_millis())
        .map_err(|_| PortError::Rejected("timestamp_out_of_range".to_owned()))
}
#[allow(clippy::needless_pass_by_value)]
fn port(e: sqlx::Error) -> PortError {
    if e.as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
    {
        PortError::Conflict
    } else {
        PortError::Unavailable
    }
}
fn invalid<E>(_: E) -> PortError {
    PortError::Rejected("invalid_rbac_value".to_owned())
}
#[cfg(feature = "sqlite")]
fn blob(v: Uuid) -> Vec<u8> {
    v.as_bytes().to_vec()
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn rid(v: Vec<u8>) -> Result<RoleId, PortError> {
    RoleId::from_uuid(Uuid::from_slice(&v).map_err(invalid)?).map_err(invalid)
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn raid(v: Vec<u8>) -> Result<RoleAssignmentId, PortError> {
    RoleAssignmentId::from_uuid(Uuid::from_slice(&v).map_err(invalid)?).map_err(invalid)
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn aid(v: Vec<u8>) -> Result<AccountId, PortError> {
    AccountId::from_uuid(Uuid::from_slice(&v).map_err(invalid)?).map_err(invalid)
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn uid(v: Vec<u8>) -> Result<UserId, PortError> {
    UserId::from_uuid(Uuid::from_slice(&v).map_err(invalid)?).map_err(invalid)
}
#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn sid(v: Vec<u8>) -> Result<SystemId, PortError> {
    SystemId::from_uuid(Uuid::from_slice(&v).map_err(invalid)?).map_err(invalid)
}
