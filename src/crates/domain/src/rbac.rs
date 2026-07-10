use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    BuiltInRole, Permission, PrincipalId, Role, RoleAssignment, RoleId, RoleKind, RoleScope,
    UtcTimestamp,
};

/// One authorization question evaluated against a complete assignment snapshot.
#[derive(Clone, Copy, Debug)]
pub struct AccessRequest<'a> {
    pub principal: PrincipalId,
    pub permission: Permission,
    pub scope: RoleScope,
    pub evaluated_at: UtcTimestamp,
    pub assignments: &'a [RoleAssignment],
}

/// Explicit allow or classified deny result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessDecision {
    Allowed,
    Denied(AccessDenial),
}

impl AccessDecision {
    #[must_use]
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Safe reason for a deny-by-default decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessDenial {
    NoApplicableAssignment,
    PermissionMissing,
    InvalidRoleHierarchy,
    DelegationWouldEscalate,
}

/// Pure RBAC evaluator over an immutable role snapshot.
pub struct RbacEvaluator<'a> {
    roles: &'a HashMap<RoleId, Role>,
}

impl<'a> RbacEvaluator<'a> {
    #[must_use]
    pub const fn new(roles: &'a HashMap<RoleId, Role>) -> Self {
        Self { roles }
    }

    /// Evaluates one permission without falling back to implicit ownership or visibility.
    #[must_use]
    pub fn authorize(&self, request: AccessRequest<'_>) -> AccessDecision {
        let mut applicable = false;
        let mut invalid_hierarchy = false;

        for assignment in request.assignments {
            if assignment.principal != request.principal
                || assignment
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= request.evaluated_at)
                || !scope_covers(assignment.scope, request.scope)
            {
                continue;
            }
            let Some(role) = self.roles.get(&assignment.role_id) else {
                invalid_hierarchy = true;
                continue;
            };
            if !role_can_apply_at(role, assignment.scope) {
                continue;
            }
            applicable = true;

            match self.effective_permissions(role.id) {
                Some(permissions) if permissions.contains(&request.permission) => {
                    return AccessDecision::Allowed;
                }
                Some(_) => {}
                None => invalid_hierarchy = true,
            }
        }

        if invalid_hierarchy {
            AccessDecision::Denied(AccessDenial::InvalidRoleHierarchy)
        } else if applicable {
            AccessDecision::Denied(AccessDenial::PermissionMissing)
        } else {
            AccessDecision::Denied(AccessDenial::NoApplicableAssignment)
        }
    }

    /// Checks that a granter may assign the role at the requested scope without gaining or
    /// conveying any permission the granter does not already hold there.
    #[must_use]
    pub fn authorize_delegation(
        &self,
        principal: PrincipalId,
        role_id: RoleId,
        scope: RoleScope,
        evaluated_at: UtcTimestamp,
        assignments: &[RoleAssignment],
    ) -> AccessDecision {
        let Some(role) = self.roles.get(&role_id) else {
            return AccessDecision::Denied(AccessDenial::InvalidRoleHierarchy);
        };
        if !role_can_apply_at(role, scope) {
            return AccessDecision::Denied(AccessDenial::DelegationWouldEscalate);
        }
        let Some(proposed_permissions) = self.effective_permissions(role_id) else {
            return AccessDecision::Denied(AccessDenial::InvalidRoleHierarchy);
        };

        let role_management = self.authorize(AccessRequest {
            principal,
            permission: Permission::RoleManage,
            scope,
            evaluated_at,
            assignments,
        });
        if !role_management.is_allowed() {
            return AccessDecision::Denied(AccessDenial::DelegationWouldEscalate);
        }

        for permission in proposed_permissions {
            if !self
                .authorize(AccessRequest {
                    principal,
                    permission,
                    scope,
                    evaluated_at,
                    assignments,
                })
                .is_allowed()
            {
                return AccessDecision::Denied(AccessDenial::DelegationWouldEscalate);
            }
        }
        AccessDecision::Allowed
    }

    fn effective_permissions(&self, role_id: RoleId) -> Option<BTreeSet<Permission>> {
        self.resolve_permissions(role_id, &mut HashSet::new())
    }

    fn resolve_permissions(
        &self,
        role_id: RoleId,
        visiting: &mut HashSet<RoleId>,
    ) -> Option<BTreeSet<Permission>> {
        if !visiting.insert(role_id) {
            return None;
        }
        let role = self.roles.get(&role_id)?;
        let mut permissions = match role.kind {
            RoleKind::BuiltIn(template) => built_in_permissions(template),
            RoleKind::Custom => role.permissions.clone(),
        };

        for parent_id in &role.parent_role_ids {
            let parent = self.roles.get(parent_id)?;
            if parent.account_id != role.account_id && parent.account_id.is_some() {
                return None;
            }
            permissions.extend(self.resolve_permissions(*parent_id, visiting)?);
        }
        visiting.remove(&role_id);
        Some(permissions)
    }
}

fn scope_covers(granted: RoleScope, requested: RoleScope) -> bool {
    match (granted, requested) {
        (RoleScope::Instance, _) => true,
        (RoleScope::Account(granted_account), RoleScope::Account(requested_account)) => {
            granted_account == requested_account
        }
        (
            RoleScope::Account(granted_account),
            RoleScope::System {
                account_id: requested_account,
                ..
            },
        ) => granted_account == requested_account,
        (
            RoleScope::System {
                account_id: granted_account,
                system_id: granted_system,
            },
            RoleScope::System {
                account_id: requested_account,
                system_id: requested_system,
            },
        ) => granted_account == requested_account && granted_system == requested_system,
        (RoleScope::Account(_) | RoleScope::System { .. }, RoleScope::Instance)
        | (RoleScope::System { .. }, RoleScope::Account(_)) => false,
    }
}

fn role_can_apply_at(role: &Role, scope: RoleScope) -> bool {
    match (role.account_id, scope) {
        (None, _) => true,
        (Some(_), RoleScope::Instance) => false,
        (
            Some(role_account),
            RoleScope::Account(scope_account)
            | RoleScope::System {
                account_id: scope_account,
                ..
            },
        ) => role_account == scope_account,
    }
}

/// Returns the explicit stable permission set for a built-in role template.
#[must_use]
pub fn built_in_permissions(role: BuiltInRole) -> BTreeSet<Permission> {
    use Permission::{
        AccountManage, AccountRead, AuditRead, CredentialManage, InstanceManage, InstanceRead,
        IntegrationManage, MembershipManage, RoleManage, SystemManage, SystemRead, TelemetryRead,
        TelemetryWrite,
    };

    let values: &[Permission] = match role {
        BuiltInRole::InstanceAdministrator => &[
            InstanceRead,
            InstanceManage,
            AccountRead,
            AccountManage,
            MembershipManage,
            RoleManage,
            SystemRead,
            SystemManage,
            TelemetryRead,
            TelemetryWrite,
            CredentialManage,
            IntegrationManage,
            AuditRead,
        ],
        BuiltInRole::AccountOwner | BuiltInRole::AccountAdministrator => &[
            AccountRead,
            AccountManage,
            MembershipManage,
            RoleManage,
            SystemRead,
            SystemManage,
            TelemetryRead,
            TelemetryWrite,
            CredentialManage,
            IntegrationManage,
            AuditRead,
        ],
        BuiltInRole::Manager => &[
            AccountRead,
            MembershipManage,
            RoleManage,
            SystemRead,
            SystemManage,
            TelemetryRead,
            TelemetryWrite,
            CredentialManage,
            IntegrationManage,
        ],
        BuiltInRole::Operator => &[
            AccountRead,
            SystemRead,
            SystemManage,
            TelemetryRead,
            TelemetryWrite,
            IntegrationManage,
        ],
        BuiltInRole::Contributor | BuiltInRole::Uploader => {
            &[AccountRead, SystemRead, TelemetryRead, TelemetryWrite]
        }
        BuiltInRole::Analyst | BuiltInRole::Viewer => &[AccountRead, SystemRead, TelemetryRead],
        BuiltInRole::Auditor => &[AccountRead, SystemRead, TelemetryRead, AuditRead],
    };
    values.iter().copied().collect()
}
