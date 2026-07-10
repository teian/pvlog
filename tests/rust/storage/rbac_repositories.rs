//! Hierarchical RBAC CRUD, delegation, and escalation contracts for both engines.

use std::{collections::BTreeSet, error::Error, sync::Arc};

use pvlog_application::{
    AssignRole, CreateCustomRole, RbacManagementError, RbacRepository, RoleManagementService,
    UpdateCustomRole, built_in_account_roles,
};
use pvlog_application_fakes::FixedClock;
use pvlog_domain::{
    AccountId, BuiltInRole, MembershipId, Permission, PrincipalId, RoleAssignment,
    RoleAssignmentId, RoleKind, RoleScope, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountRecord, DatabaseTarget, ManagementRepository, MembershipRecord,
    PostgresManagementRepository, PostgresRbacRepository, SqliteManagementRepository,
    SqliteRbacRepository, UserRecord, apply_migrations,
};
use tempfile::TempDir;

const NOW: i64 = 1_770_000_000_000;

#[tokio::test]
async fn sqlite_rbac_repository_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management = directory.path().join("management.sqlite3");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management.clone(),
        accounts_dir: directory.path().join("accounts"),
    })
    .await?;
    verify_contract(
        Arc::new(SqliteRbacRepository::new(management.clone())),
        &SqliteManagementRepository::new(management),
    )
    .await
}

#[tokio::test]
async fn postgres_rbac_repository_contract_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    verify_contract(
        Arc::new(PostgresRbacRepository::new(url.clone())),
        &PostgresManagementRepository::new(url),
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract<R>(
    repository: Arc<R>,
    management: &dyn ManagementRepository,
) -> Result<(), Box<dyn Error>>
where
    R: RbacRepository + 'static,
{
    let account = AccountId::new();
    let other_account = AccountId::new();
    let owner = UserId::new();
    let manager = UserId::new();
    let target = UserId::new();
    for (id, label) in [(owner, "owner"), (manager, "manager"), (target, "target")] {
        management
            .save_user(&UserRecord {
                id,
                email: format!("{label}-{id}@example.test"),
                display_name: label.to_owned(),
                status: "active".to_owned(),
                created_at: NOW,
                updated_at: NOW,
            })
            .await?;
    }
    for (id, label) in [(account, "primary"), (other_account, "other")] {
        management
            .save_account(&AccountRecord {
                id,
                slug: format!("rbac-{label}-{id}"),
                display_name: label.to_owned(),
                status: "active".to_owned(),
                created_by: Some(owner),
                created_at: NOW,
                updated_at: NOW,
            })
            .await?;
    }
    let target_membership_id = MembershipId::new();
    for (id, user) in [
        (MembershipId::new(), owner),
        (MembershipId::new(), manager),
        (target_membership_id, target),
    ] {
        management
            .save_membership(&MembershipRecord {
                id,
                account_id: account,
                user_id: user,
                status: "active".to_owned(),
                joined_at: Some(NOW),
                created_at: NOW,
                updated_at: NOW,
            })
            .await?;
    }
    let builtins = built_in_account_roles(account, owner, NOW);
    for role in &builtins {
        repository.save_role(role).await?;
    }
    let owner_role = builtins
        .iter()
        .find(|r| r.role.kind == RoleKind::BuiltIn(BuiltInRole::AccountOwner))
        .ok_or("owner role missing")?;
    let manager_role = builtins
        .iter()
        .find(|r| r.role.kind == RoleKind::BuiltIn(BuiltInRole::Manager))
        .ok_or("manager role missing")?;
    repository
        .save_assignment(&RoleAssignment {
            id: RoleAssignmentId::new(),
            principal: PrincipalId::User(owner),
            role_id: owner_role.role.id,
            scope: RoleScope::Account(account),
            granted_by: owner,
            granted_at: UtcTimestamp::from_epoch_millis(NOW)?,
            expires_at: None,
        })
        .await?;
    let service = RoleManagementService::new(
        repository.clone(),
        Arc::new(FixedClock::new(UtcTimestamp::from_epoch_millis(NOW)?)),
    );
    let manager_assignment = service
        .assign_role(
            owner,
            AssignRole {
                principal: PrincipalId::User(manager),
                role_id: manager_role.role.id,
                scope: RoleScope::Account(account),
                expires_at: None,
            },
        )
        .await?;

    let read_role = service
        .create_custom_role(
            manager,
            CreateCustomRole {
                account_id: account,
                name: "Telemetry reader".to_owned(),
                permissions: BTreeSet::from([Permission::TelemetryRead]),
                parent_role_ids: BTreeSet::new(),
            },
        )
        .await?;
    assert!(matches!(
        service
            .create_custom_role(
                manager,
                CreateCustomRole {
                    account_id: account,
                    name: "Escalation".to_owned(),
                    permissions: BTreeSet::from([Permission::AuditRead]),
                    parent_role_ids: BTreeSet::new()
                }
            )
            .await,
        Err(RbacManagementError::PrivilegeEscalation)
    ));
    let target_assignment = service
        .assign_role(
            manager,
            AssignRole {
                principal: PrincipalId::User(target),
                role_id: read_role.role.id,
                scope: RoleScope::Account(account),
                expires_at: None,
            },
        )
        .await?;
    assert_eq!(
        service
            .effective_permissions(PrincipalId::User(target), RoleScope::Account(account))
            .await?,
        BTreeSet::from([Permission::TelemetryRead])
    );
    assert!(matches!(
        service
            .assign_role(
                manager,
                AssignRole {
                    principal: PrincipalId::User(target),
                    role_id: read_role.role.id,
                    scope: RoleScope::Account(other_account),
                    expires_at: None
                }
            )
            .await,
        Err(RbacManagementError::PrivilegeEscalation | RbacManagementError::InvalidScope)
    ));
    assert!(matches!(
        service
            .update_custom_role(
                manager,
                UpdateCustomRole {
                    id: read_role.role.id,
                    name: "Escalated".to_owned(),
                    permissions: BTreeSet::from([Permission::TelemetryRead, Permission::AuditRead]),
                    parent_role_ids: BTreeSet::new()
                }
            )
            .await,
        Err(RbacManagementError::PrivilegeEscalation)
    ));
    let updated = service
        .update_custom_role(
            owner,
            UpdateCustomRole {
                id: read_role.role.id,
                name: "Telemetry observer".to_owned(),
                permissions: BTreeSet::from([Permission::TelemetryRead, Permission::AuditRead]),
                parent_role_ids: BTreeSet::new(),
            },
        )
        .await?;
    assert_eq!(updated.version, 2);
    assert!(matches!(
        service
            .update_custom_role(
                owner,
                UpdateCustomRole {
                    id: updated.role.id,
                    name: "Cycle".to_owned(),
                    permissions: BTreeSet::new(),
                    parent_role_ids: BTreeSet::from([updated.role.id])
                }
            )
            .await,
        Err(RbacManagementError::PrivilegeEscalation | RbacManagementError::InvalidParent)
    ));
    assert!(matches!(
        service.delete_custom_role(owner, owner_role.role.id).await,
        Err(RbacManagementError::BuiltInImmutable)
    ));
    service
        .revoke_assignment(manager, target_assignment.id, RoleScope::Account(account))
        .await?;
    assert!(
        service
            .effective_permissions(PrincipalId::User(target), RoleScope::Account(account))
            .await?
            .is_empty()
    );
    service
        .revoke_assignment(owner, manager_assignment.id, RoleScope::Account(account))
        .await?;
    assert!(matches!(
        service
            .create_custom_role(
                manager,
                CreateCustomRole {
                    account_id: account,
                    name: "After revoke".to_owned(),
                    permissions: BTreeSet::from([Permission::TelemetryRead]),
                    parent_role_ids: BTreeSet::new()
                }
            )
            .await,
        Err(RbacManagementError::PrivilegeEscalation)
    ));
    service
        .assign_role(
            owner,
            AssignRole {
                principal: PrincipalId::User(target),
                role_id: updated.role.id,
                scope: RoleScope::Account(account),
                expires_at: None,
            },
        )
        .await?;
    management
        .save_membership(&MembershipRecord {
            id: target_membership_id,
            account_id: account,
            user_id: target,
            status: "suspended".to_owned(),
            joined_at: Some(NOW),
            created_at: NOW,
            updated_at: NOW + 1,
        })
        .await?;
    assert!(
        service
            .effective_permissions(PrincipalId::User(target), RoleScope::Account(account))
            .await?
            .is_empty()
    );
    service.delete_custom_role(owner, updated.role.id).await?;
    assert!(repository.role(updated.role.id).await?.is_none());
    Ok(())
}
