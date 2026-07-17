//! Hierarchical RBAC role management and privilege-escalation prevention.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_trait::async_trait;
use pvlog_domain::{
    AccessRequest, AccountId, BuiltInRole, Permission, PrincipalId, RbacEvaluator, Role,
    RoleAssignment, RoleAssignmentId, RoleId, RoleKind, RoleScope, UserId, built_in_permissions,
};
use thiserror::Error;

use crate::{Clock, PortError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RbacRoleRecord {
    pub role: Role,
    pub created_by: Option<UserId>,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
}

#[async_trait]
pub trait RbacRepository: Send + Sync {
    async fn roles(&self, account_id: Option<AccountId>) -> Result<Vec<RbacRoleRecord>, PortError>;
    async fn role(&self, id: RoleId) -> Result<Option<RbacRoleRecord>, PortError>;
    async fn save_role(&self, record: &RbacRoleRecord) -> Result<(), PortError>;
    async fn delete_custom_role(&self, id: RoleId) -> Result<bool, PortError>;
    async fn active_assignments(
        &self,
        principal: PrincipalId,
        now: i64,
    ) -> Result<Vec<RoleAssignment>, PortError>;
    async fn save_assignment(&self, assignment: &RoleAssignment) -> Result<(), PortError>;
    async fn revoke_assignment(&self, id: RoleAssignmentId, now: i64) -> Result<bool, PortError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCustomRole {
    pub account_id: AccountId,
    pub name: String,
    pub permissions: BTreeSet<Permission>,
    pub parent_role_ids: BTreeSet<RoleId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateCustomRole {
    pub id: RoleId,
    pub name: String,
    pub permissions: BTreeSet<Permission>,
    pub parent_role_ids: BTreeSet<RoleId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignRole {
    pub principal: PrincipalId,
    pub role_id: RoleId,
    pub scope: RoleScope,
    pub expires_at: Option<i64>,
}

pub struct RoleManagementService {
    repository: Arc<dyn RbacRepository>,
    clock: Arc<dyn Clock>,
}

#[allow(clippy::missing_errors_doc)]
impl RoleManagementService {
    #[must_use]
    pub fn new(repository: Arc<dyn RbacRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }

    pub async fn create_custom_role(
        &self,
        actor: UserId,
        command: CreateCustomRole,
    ) -> Result<RbacRoleRecord, RbacManagementError> {
        let name = validate_name(&command.name)?;
        let now = self.now()?;
        let role = Role {
            id: RoleId::new(),
            account_id: Some(command.account_id),
            name,
            kind: RoleKind::Custom,
            parent_role_ids: command.parent_role_ids,
            permissions: command.permissions,
        };
        self.authorize_definition(actor, &role, now).await?;
        let record = RbacRoleRecord {
            role,
            created_by: Some(actor),
            created_at: now,
            updated_at: now,
            version: 1,
        };
        self.repository.save_role(&record).await?;
        Ok(record)
    }

    pub async fn update_custom_role(
        &self,
        actor: UserId,
        command: UpdateCustomRole,
    ) -> Result<RbacRoleRecord, RbacManagementError> {
        let Some(existing) = self.repository.role(command.id).await? else {
            return Err(RbacManagementError::NotFound);
        };
        if existing.role.kind != RoleKind::Custom {
            return Err(RbacManagementError::BuiltInImmutable);
        }
        let now = self.now()?;
        let role = Role {
            id: command.id,
            account_id: existing.role.account_id,
            name: validate_name(&command.name)?,
            kind: RoleKind::Custom,
            parent_role_ids: command.parent_role_ids,
            permissions: command.permissions,
        };
        self.authorize_definition(actor, &role, now).await?;
        let record = RbacRoleRecord {
            role,
            created_by: existing.created_by,
            created_at: existing.created_at,
            updated_at: now,
            version: existing.version + 1,
        };
        self.repository.save_role(&record).await?;
        Ok(record)
    }

    pub async fn delete_custom_role(
        &self,
        actor: UserId,
        id: RoleId,
    ) -> Result<(), RbacManagementError> {
        let Some(role) = self.repository.role(id).await? else {
            return Err(RbacManagementError::NotFound);
        };
        if role.role.kind != RoleKind::Custom {
            return Err(RbacManagementError::BuiltInImmutable);
        }
        let account_id = role
            .role
            .account_id
            .ok_or(RbacManagementError::InvalidScope)?;
        self.require_permission(
            actor,
            Permission::RoleManage,
            RoleScope::Account(account_id),
            self.now()?,
        )
        .await?;
        if !self.repository.delete_custom_role(id).await? {
            return Err(RbacManagementError::NotFound);
        }
        Ok(())
    }

    pub async fn assign_role(
        &self,
        actor: UserId,
        command: AssignRole,
    ) -> Result<RoleAssignment, RbacManagementError> {
        let now = self.now()?;
        if command.expires_at.is_some_and(|expires| expires <= now) {
            return Err(RbacManagementError::InvalidExpiry);
        }
        let role_record = self
            .repository
            .role(command.role_id)
            .await?
            .ok_or(RbacManagementError::NotFound)?;
        validate_role_scope(&role_record.role, command.scope)?;
        let roles = self.roles_for_scope(command.scope).await?;
        let evaluator = RbacEvaluator::new(&roles);
        let assignments = self
            .repository
            .active_assignments(PrincipalId::User(actor), now)
            .await?;
        if !evaluator
            .authorize_delegation(
                PrincipalId::User(actor),
                command.role_id,
                command.scope,
                timestamp(now)?,
                &assignments,
            )
            .is_allowed()
        {
            return Err(RbacManagementError::PrivilegeEscalation);
        }
        let assignment = RoleAssignment {
            id: RoleAssignmentId::new(),
            principal: command.principal,
            role_id: command.role_id,
            scope: command.scope,
            granted_by: actor,
            granted_at: timestamp(now)?,
            expires_at: command.expires_at.map(timestamp).transpose()?,
        };
        self.repository.save_assignment(&assignment).await?;
        Ok(assignment)
    }

    pub async fn revoke_assignment(
        &self,
        actor: UserId,
        id: RoleAssignmentId,
        scope: RoleScope,
    ) -> Result<(), RbacManagementError> {
        let now = self.now()?;
        self.require_permission(actor, Permission::RoleManage, scope, now)
            .await?;
        if !self.repository.revoke_assignment(id, now).await? {
            return Err(RbacManagementError::NotFound);
        }
        Ok(())
    }

    pub async fn effective_permissions(
        &self,
        principal: PrincipalId,
        scope: RoleScope,
    ) -> Result<BTreeSet<Permission>, RbacManagementError> {
        let now = self.now()?;
        let roles = self.roles_for_scope(scope).await?;
        let evaluator = RbacEvaluator::new(&roles);
        let assignments = self.repository.active_assignments(principal, now).await?;
        let evaluated_at = timestamp(now)?;
        Ok(Permission::ALL
            .into_iter()
            .filter(|permission| {
                evaluator
                    .authorize(AccessRequest {
                        principal,
                        permission: *permission,
                        scope,
                        evaluated_at,
                        assignments: &assignments,
                    })
                    .is_allowed()
            })
            .collect())
    }

    async fn authorize_definition(
        &self,
        actor: UserId,
        candidate: &Role,
        now: i64,
    ) -> Result<(), RbacManagementError> {
        let account_id = candidate
            .account_id
            .ok_or(RbacManagementError::InvalidScope)?;
        let mut roles = self.roles_for_scope(RoleScope::Account(account_id)).await?;
        for parent in &candidate.parent_role_ids {
            let Some(parent_role) = roles.get(parent) else {
                return Err(RbacManagementError::InvalidParent);
            };
            if parent_role.account_id != Some(account_id) {
                return Err(RbacManagementError::InvalidParent);
            }
        }
        roles.insert(candidate.id, candidate.clone());
        let evaluator = RbacEvaluator::new(&roles);
        let assignments = self
            .repository
            .active_assignments(PrincipalId::User(actor), now)
            .await?;
        if evaluator
            .authorize_delegation(
                PrincipalId::User(actor),
                candidate.id,
                RoleScope::Account(account_id),
                timestamp(now)?,
                &assignments,
            )
            .is_allowed()
        {
            Ok(())
        } else {
            Err(RbacManagementError::PrivilegeEscalation)
        }
    }

    async fn require_permission(
        &self,
        actor: UserId,
        permission: Permission,
        scope: RoleScope,
        now: i64,
    ) -> Result<(), RbacManagementError> {
        let roles = self.roles_for_scope(scope).await?;
        let evaluator = RbacEvaluator::new(&roles);
        let assignments = self
            .repository
            .active_assignments(PrincipalId::User(actor), now)
            .await?;
        if evaluator
            .authorize(AccessRequest {
                principal: PrincipalId::User(actor),
                permission,
                scope,
                evaluated_at: timestamp(now)?,
                assignments: &assignments,
            })
            .is_allowed()
        {
            Ok(())
        } else {
            Err(RbacManagementError::Forbidden)
        }
    }

    async fn roles_for_scope(
        &self,
        scope: RoleScope,
    ) -> Result<HashMap<RoleId, Role>, RbacManagementError> {
        let account = match scope {
            RoleScope::Instance => None,
            RoleScope::Account(id) | RoleScope::System { account_id: id, .. } => Some(id),
        };
        let mut records = self.repository.roles(None).await?;
        if account.is_some() {
            records.extend(self.repository.roles(account).await?);
        }
        Ok(records
            .into_iter()
            .map(|record| (record.role.id, record.role))
            .collect())
    }

    fn now(&self) -> Result<i64, RbacManagementError> {
        i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| RbacManagementError::Internal("clock_out_of_range"))
    }
}

/// Stable built-in account roles seeded for each account.
#[must_use]
pub fn built_in_account_roles(
    account_id: AccountId,
    creator: UserId,
    now: i64,
) -> Vec<RbacRoleRecord> {
    [
        (BuiltInRole::AccountOwner, "owner"),
        (BuiltInRole::AccountAdministrator, "administrator"),
        (BuiltInRole::Manager, "manager"),
        (BuiltInRole::Contributor, "contributor"),
        (BuiltInRole::Viewer, "viewer"),
        (BuiltInRole::Auditor, "auditor"),
    ]
    .into_iter()
    .map(|(kind, name)| RbacRoleRecord {
        role: Role {
            id: RoleId::new(),
            account_id: Some(account_id),
            name: name.to_owned(),
            kind: RoleKind::BuiltIn(kind),
            parent_role_ids: BTreeSet::new(),
            permissions: built_in_permissions(kind),
        },
        created_by: Some(creator),
        created_at: now,
        updated_at: now,
        version: 1,
    })
    .collect()
}

fn validate_name(value: &str) -> Result<String, RbacManagementError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 {
        Err(RbacManagementError::InvalidName)
    } else {
        Ok(value.to_owned())
    }
}
fn validate_role_scope(role: &Role, scope: RoleScope) -> Result<(), RbacManagementError> {
    match (role.account_id, scope) {
        (None, RoleScope::Instance) => Ok(()),
        (
            Some(role_account),
            RoleScope::Account(account)
            | RoleScope::System {
                account_id: account,
                ..
            },
        ) if role_account == account => Ok(()),
        _ => Err(RbacManagementError::InvalidScope),
    }
}
fn timestamp(value: i64) -> Result<pvlog_domain::UtcTimestamp, RbacManagementError> {
    pvlog_domain::UtcTimestamp::from_epoch_millis(value)
        .map_err(|_| RbacManagementError::Internal("timestamp_out_of_range"))
}
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RbacManagementError {
    #[error("RBAC operation is forbidden")]
    Forbidden,
    #[error("RBAC operation would escalate privileges")]
    PrivilegeEscalation,
    #[error("role was not found")]
    NotFound,
    #[error("built-in roles are immutable")]
    BuiltInImmutable,
    #[error("role name is invalid")]
    InvalidName,
    #[error("role parent is invalid")]
    InvalidParent,
    #[error("role scope is invalid")]
    InvalidScope,
    #[error("assignment expiry is invalid")]
    InvalidExpiry,
    #[error("RBAC persistence failed")]
    Persistence,
    #[error("RBAC service failed: {0}")]
    Internal(&'static str),
}
impl From<PortError> for RbacManagementError {
    fn from(_: PortError) -> Self {
        Self::Persistence
    }
}
