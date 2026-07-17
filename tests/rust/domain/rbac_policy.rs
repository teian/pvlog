use std::{collections::BTreeSet, error::Error};

use pvlog_domain::{
    AccessDecision, AccessDenial, AccessRequest, AccountId, BuiltInRole, Permission, PrincipalId,
    RbacEvaluator, Role, RoleAssignment, RoleAssignmentId, RoleId, RoleKind, RoleScope, SystemId,
    UserId, UtcTimestamp, built_in_permissions,
};

#[test]
fn instance_administrator_always_contains_the_complete_permission_catalog() {
    assert_eq!(
        built_in_permissions(BuiltInRole::InstanceAdministrator),
        Permission::ALL.into_iter().collect(),
    );
}

#[test]
fn authorization_is_deny_by_default_and_honors_permission_matrix() -> Result<(), Box<dyn Error>> {
    let now = UtcTimestamp::from_epoch_millis(1_000)?;
    let account = AccountId::new();
    let viewer = built_in_role(Some(account), BuiltInRole::Viewer);
    let user = PrincipalId::User(UserId::new());
    let roles = std::collections::HashMap::from([(viewer.id, viewer.clone())]);
    let evaluator = RbacEvaluator::new(&roles);

    assert_eq!(
        evaluator.authorize(AccessRequest {
            principal: user,
            permission: Permission::SystemRead,
            scope: RoleScope::Account(account),
            evaluated_at: now,
            assignments: &[],
        }),
        AccessDecision::Denied(AccessDenial::NoApplicableAssignment)
    );

    let assignments = [assignment(
        user,
        viewer.id,
        RoleScope::Account(account),
        now,
    )];
    assert!(
        evaluator
            .authorize(AccessRequest {
                principal: user,
                permission: Permission::TelemetryRead,
                scope: RoleScope::Account(account),
                evaluated_at: now,
                assignments: &assignments,
            })
            .is_allowed()
    );
    assert_eq!(
        evaluator.authorize(AccessRequest {
            principal: user,
            permission: Permission::TelemetryWrite,
            scope: RoleScope::Account(account),
            evaluated_at: now,
            assignments: &assignments,
        }),
        AccessDecision::Denied(AccessDenial::PermissionMissing)
    );
    Ok(())
}

#[test]
fn system_and_account_scopes_prevent_cross_account_access() -> Result<(), Box<dyn Error>> {
    let now = UtcTimestamp::from_epoch_millis(1_000)?;
    let account = AccountId::new();
    let other_account = AccountId::new();
    let system = SystemId::new();
    let other_system = SystemId::new();
    let operator = built_in_role(Some(account), BuiltInRole::Operator);
    let principal = PrincipalId::User(UserId::new());
    let roles = std::collections::HashMap::from([(operator.id, operator.clone())]);
    let evaluator = RbacEvaluator::new(&roles);
    let assignments = [assignment(
        principal,
        operator.id,
        RoleScope::System {
            account_id: account,
            system_id: system,
        },
        now,
    )];

    for scope in [
        RoleScope::System {
            account_id: account,
            system_id: other_system,
        },
        RoleScope::System {
            account_id: other_account,
            system_id: system,
        },
        RoleScope::Account(account),
    ] {
        assert!(
            !evaluator
                .authorize(AccessRequest {
                    principal,
                    permission: Permission::SystemRead,
                    scope,
                    evaluated_at: now,
                    assignments: &assignments,
                })
                .is_allowed()
        );
    }
    Ok(())
}

#[test]
fn custom_roles_inherit_but_cycles_fail_closed() -> Result<(), Box<dyn Error>> {
    let now = UtcTimestamp::from_epoch_millis(1_000)?;
    let account = AccountId::new();
    let viewer = built_in_role(Some(account), BuiltInRole::Viewer);
    let custom = Role {
        id: RoleId::new(),
        account_id: Some(account),
        name: "viewer plus uploader".to_owned(),
        kind: RoleKind::Custom,
        parent_role_ids: BTreeSet::from([viewer.id]),
        permissions: BTreeSet::from([Permission::TelemetryWrite]),
    };
    let principal = PrincipalId::User(UserId::new());
    let assignments = [assignment(
        principal,
        custom.id,
        RoleScope::Account(account),
        now,
    )];
    let roles = std::collections::HashMap::from([(viewer.id, viewer), (custom.id, custom.clone())]);
    let evaluator = RbacEvaluator::new(&roles);
    assert!(
        evaluator
            .authorize(AccessRequest {
                principal,
                permission: Permission::TelemetryRead,
                scope: RoleScope::Account(account),
                evaluated_at: now,
                assignments: &assignments,
            })
            .is_allowed()
    );

    let mut cyclic = custom;
    cyclic.parent_role_ids = BTreeSet::from([cyclic.id]);
    let cyclic_roles = std::collections::HashMap::from([(cyclic.id, cyclic)]);
    assert_eq!(
        RbacEvaluator::new(&cyclic_roles).authorize(AccessRequest {
            principal,
            permission: Permission::TelemetryWrite,
            scope: RoleScope::Account(account),
            evaluated_at: now,
            assignments: &assignments,
        }),
        AccessDecision::Denied(AccessDenial::InvalidRoleHierarchy)
    );
    Ok(())
}

#[test]
fn delegation_requires_role_management_and_every_conveyed_permission() -> Result<(), Box<dyn Error>>
{
    let now = UtcTimestamp::from_epoch_millis(1_000)?;
    let account = AccountId::new();
    let administrator = built_in_role(Some(account), BuiltInRole::AccountAdministrator);
    let operator = built_in_role(Some(account), BuiltInRole::Operator);
    let elevated = Role {
        id: RoleId::new(),
        account_id: Some(account),
        name: "membership manager".to_owned(),
        kind: RoleKind::Custom,
        parent_role_ids: BTreeSet::new(),
        permissions: BTreeSet::from([Permission::MembershipManage]),
    };
    let roles = std::collections::HashMap::from([
        (administrator.id, administrator.clone()),
        (operator.id, operator.clone()),
        (elevated.id, elevated.clone()),
    ]);
    let evaluator = RbacEvaluator::new(&roles);
    let admin = PrincipalId::User(UserId::new());
    let operator_user = PrincipalId::User(UserId::new());
    let assignments = [
        assignment(admin, administrator.id, RoleScope::Account(account), now),
        assignment(operator_user, operator.id, RoleScope::Account(account), now),
    ];

    assert!(
        evaluator
            .authorize_delegation(
                admin,
                elevated.id,
                RoleScope::Account(account),
                now,
                &assignments
            )
            .is_allowed()
    );
    assert_eq!(
        evaluator.authorize_delegation(
            operator_user,
            elevated.id,
            RoleScope::Account(account),
            now,
            &assignments,
        ),
        AccessDecision::Denied(AccessDenial::DelegationWouldEscalate)
    );
    Ok(())
}

fn built_in_role(account_id: Option<AccountId>, template: BuiltInRole) -> Role {
    Role {
        id: RoleId::new(),
        account_id,
        name: format!("{template:?}"),
        kind: RoleKind::BuiltIn(template),
        parent_role_ids: BTreeSet::new(),
        permissions: BTreeSet::new(),
    }
}

fn assignment(
    principal: PrincipalId,
    role_id: RoleId,
    scope: RoleScope,
    now: UtcTimestamp,
) -> RoleAssignment {
    RoleAssignment {
        id: RoleAssignmentId::new(),
        principal,
        role_id,
        scope,
        granted_by: UserId::new(),
        granted_at: now,
        expires_at: None,
    }
}
