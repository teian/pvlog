use std::{error::Error, sync::Arc};

use pvlog::authentication::ManagementRequestAuthorizer;
use pvlog_api::{ModernRequestAuthorizer, RequestAuthorizationError};
use pvlog_application::Clock;
use pvlog_domain::{
    AccountId, MembershipId, Permission, PrincipalId, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountRecord, AuthorizationGrant, DatabaseTarget, ManagementRepository, MembershipRecord,
    SqliteAccountProvisioner, SqliteManagementRepository, SystemRegistryRecord, UserRecord,
    apply_migrations,
};
use tempfile::TempDir;

const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
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
    Ok(())
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(NOW))
    }
}
