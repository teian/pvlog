use std::{collections::BTreeSet, error::Error, sync::Arc};

use pvlog::authentication::{ManagementRequestAuthorizer, ManagementSessionBootstrap};
use pvlog_api::{ModernRequestAuthorizer, RequestAuthorizationError, SessionBootstrapUseCases};
use pvlog_application::{Clock, RbacRepository, RbacRoleRecord};
use pvlog_domain::{
    AccountId, ApiCredentialId, BuiltInRole, MembershipId, Permission, PrincipalId, Role,
    RoleAssignment, RoleAssignmentId, RoleId, RoleKind, RoleScope, SystemId, UserId, UtcTimestamp,
    built_in_permissions,
};
use pvlog_storage::{
    AccountRecord, ApiCredentialRecord, AuthorizationGrant, DatabaseTarget, ManagementRepository,
    MembershipRecord, SqliteAccountProvisioner, SqliteManagementRepository, SqliteRbacRepository,
    SystemRegistryRecord, UserRecord, apply_migrations,
};
use tempfile::TempDir;

const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
async fn instance_administrator_receives_every_permission_in_session_bootstrap()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let target = DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: directory.path().join("accounts"),
    };
    apply_migrations(&target).await?;
    let management = Arc::new(SqliteManagementRepository::new(management_path.clone()));
    let rbac = Arc::new(SqliteRbacRepository::new(management_path));
    let user_id = UserId::new();
    management
        .save_user(&UserRecord {
            id: user_id,
            email: "admin@example.test".to_owned(),
            display_name: "Administrator".to_owned(),
            status: "active".to_owned(),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    let role = Role {
        id: RoleId::new(),
        account_id: None,
        name: "instance_administrator".to_owned(),
        kind: RoleKind::BuiltIn(BuiltInRole::InstanceAdministrator),
        parent_role_ids: std::collections::BTreeSet::default(),
        permissions: built_in_permissions(BuiltInRole::InstanceAdministrator),
    };
    rbac.save_role(&RbacRoleRecord {
        role: role.clone(),
        created_by: Some(user_id),
        created_at: NOW,
        updated_at: NOW,
        version: 1,
    })
    .await?;
    rbac.save_assignment(&RoleAssignment {
        id: RoleAssignmentId::new(),
        principal: PrincipalId::User(user_id),
        role_id: role.id,
        scope: RoleScope::Instance,
        granted_by: user_id,
        granted_at: UtcTimestamp::from_epoch_millis(NOW)?,
        expires_at: None,
    })
    .await?;

    let session = ManagementSessionBootstrap::new(
        management.clone(),
        rbac.clone(),
        Arc::new(FixedClock),
        target,
    )
    .bootstrap(user_id)
    .await
    .map_err(|error| std::io::Error::other(format!("session bootstrap failed: {error:?}")))?;

    let personal_account_id = AccountId::from_uuid(user_id.as_uuid())?;
    assert_eq!(session.account_id, Some(personal_account_id));
    assert!(
        management
            .active_membership(personal_account_id, user_id)
            .await?
            .is_some()
    );
    let owner_role = rbac
        .roles(Some(personal_account_id))
        .await?
        .into_iter()
        .find(|role| role.role.kind == RoleKind::BuiltIn(BuiltInRole::AccountOwner))
        .ok_or("personal owner role missing")?;
    assert!(
        rbac.active_assignments(PrincipalId::User(user_id), NOW + 1)
            .await?
            .iter()
            .any(|assignment| {
                assignment.role_id == owner_role.role.id
                    && assignment.scope == RoleScope::Account(personal_account_id)
            })
    );
    assert_eq!(session.permissions.len(), Permission::ALL.len());
    for permission in [
        "instance_manage",
        "account_manage",
        "membership_manage",
        "role_manage",
        "system_manage",
        "telemetry_read",
        "telemetry_write",
        "credential_manage",
        "integration_manage",
        "audit_read",
    ] {
        assert!(
            session
                .permissions
                .iter()
                .any(|current| current == permission)
        );
    }
    Ok(())
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn system_requests_resolve_registry_then_authorize_before_routing()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    })
    .await?;
    let repository = Arc::new(SqliteManagementRepository::new(management_path.clone()));
    let user_id = UserId::new();
    let account_id = AccountId::new();
    let system_id = SystemId::new();
    repository
        .save_user(&UserRecord {
            id: user_id,
            email: "operator@example.test".to_owned(),
            display_name: "Operator".to_owned(),
            status: "active".to_owned(),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    repository
        .save_account(&AccountRecord {
            id: account_id,
            slug: "operator-account".to_owned(),
            display_name: "Operator account".to_owned(),
            status: "active".to_owned(),
            created_by: Some(user_id),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    repository
        .save_membership(&MembershipRecord {
            id: MembershipId::new(),
            account_id,
            user_id,
            status: "active".to_owned(),
            joined_at: Some(NOW),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    SqliteAccountProvisioner::new(management_path, accounts_dir)
        .provision(account_id)
        .await?;
    repository
        .save_system_registry(&SystemRegistryRecord {
            system_id,
            account_id,
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    repository
        .grant_user_permission(&AuthorizationGrant {
            account_id,
            user_id,
            permission: Permission::SystemManage,
            system_id: Some(system_id),
            granted_by: user_id,
            created_at: NOW,
            expires_at: None,
        })
        .await?;

    let authorizer = ManagementRequestAuthorizer::new(repository.clone(), Arc::new(FixedClock));
    let allowed = authorizer
        .authorize_system(
            PrincipalId::User(user_id),
            system_id,
            Permission::SystemManage,
            "system.update",
        )
        .await
        .map_err(|error| std::io::Error::other(format!("authorization failed: {error:?}")))?;
    assert_eq!(allowed.account_id, account_id);
    assert_eq!(allowed.actor_user_id, user_id);
    assert!(matches!(
        authorizer
            .authorize_system(
                PrincipalId::User(user_id),
                SystemId::new(),
                Permission::SystemManage,
                "system.update",
            )
            .await,
        Err(RequestAuthorizationError::NotFound)
    ));
    assert_eq!(repository.account_audit(account_id, 10).await?.len(), 1);

    for permission in [Permission::TelemetryWrite, Permission::TelemetryRead] {
        repository
            .grant_user_permission(&AuthorizationGrant {
                account_id,
                user_id,
                permission,
                system_id: Some(system_id),
                granted_by: user_id,
                created_at: NOW,
                expires_at: None,
            })
            .await?;
    }
    let credential_id = ApiCredentialId::new();
    repository
        .save_api_credential(&ApiCredentialRecord {
            id: credential_id,
            account_id,
            owner_user_id: user_id,
            system_id: None,
            name: "upload-only".to_owned(),
            credential_digest: [9; 32],
            scopes: BTreeSet::from(["telemetry_write".to_owned()]),
            created_at: NOW,
            expires_at: None,
            revoked_at: None,
        })
        .await?;
    let principal = PrincipalId::ApiCredential(credential_id);
    assert!(
        authorizer
            .authorize_system(
                principal,
                system_id,
                Permission::TelemetryWrite,
                "telemetry.create",
            )
            .await
            .is_ok()
    );
    for forbidden in [Permission::TelemetryRead, Permission::SystemManage] {
        assert!(matches!(
            authorizer
                .authorize_system(principal, system_id, forbidden, "scope.denied")
                .await,
            Err(RequestAuthorizationError::Forbidden)
        ));
    }
    Ok(())
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(NOW))
    }
}
