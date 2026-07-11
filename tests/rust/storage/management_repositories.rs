//! Shared management repository authorization and isolation contracts.

use std::{collections::BTreeSet, error::Error, sync::Arc};

use pvlog_application::{
    BrowserSessionPolicy, BrowserSessionService, BrowserSessionUseCases, Clock,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, AuditEventId, MembershipId, Permission, PrincipalId, SessionId,
    SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountRecord, ApiCredentialRecord, AuditRecord, AuthorizationGrant, DatabaseTarget,
    ManagementRepository, MembershipRecord, PostgresManagementRepository, RoutingBackend,
    SessionRecord, SqliteAccountProvisioner, SqliteManagementRepository, SystemRegistryRecord,
    UserRecord, apply_migrations,
};
use sqlx::{Connection as _, PgConnection};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_management_repository_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    })
    .await?;
    let repository = SqliteManagementRepository::new(management_path.clone());
    let fixture = seed_contract(&repository).await?;
    SqliteAccountProvisioner::new(management_path, accounts_dir)
        .provision(fixture.account_a)
        .await?;
    verify_contract(&repository, fixture, RoutingBackend::Sqlite).await
}

#[tokio::test]
async fn postgres_management_repository_contract_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let repository = PostgresManagementRepository::new(url.clone());
    let fixture = seed_contract(&repository).await?;
    let mut connection = PgConnection::connect(&url).await?;
    sqlx::query(
        "INSERT INTO management.account_storage_registry \
         (account_id, storage_kind, schema_version, migration_state, created_at, updated_at) \
         VALUES ($1, 'postgres', 4, 'ready', 1, 1)",
    )
    .bind(fixture.account_a.as_uuid())
    .execute(&mut connection)
    .await?;
    connection.close().await?;
    verify_contract(&repository, fixture, RoutingBackend::Postgres).await
}

#[tokio::test]
async fn sqlite_browser_sessions_are_revocable_and_limited() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir,
    })
    .await?;
    let repository = Arc::new(SqliteManagementRepository::new(management_path));
    let user_id = UserId::new();
    repository
        .save_user(&UserRecord {
            id: user_id,
            email: "session@example.test".to_owned(),
            display_name: "Session".to_owned(),
            status: "active".to_owned(),
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    let service = BrowserSessionService::new(
        repository,
        Arc::new(SessionClock),
        [5; 32],
        BrowserSessionPolicy {
            idle_lifetime_seconds: 300,
            absolute_lifetime_seconds: 3_600,
            max_concurrent_sessions: 1,
            secure_cookies: true,
        },
    );
    let first = service.issue(user_id).await?;
    let second = service.issue(user_id).await?;
    assert!(
        service
            .authenticate(&first.session_cookie.value, None, false)
            .await
            .is_err()
    );
    service
        .authenticate(&second.session_cookie.value, None, false)
        .await?;
    service.logout(&second.session_cookie.value).await?;
    assert!(
        service
            .authenticate(&second.session_cookie.value, None, false)
            .await
            .is_err()
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct ContractFixture {
    user_id: UserId,
    account_a: AccountId,
    account_b: AccountId,
    system_a: SystemId,
    system_b: SystemId,
    credential_id: ApiCredentialId,
    session_digest: [u8; 32],
    credential_digest: [u8; 32],
}

#[allow(clippy::too_many_lines)]
async fn seed_contract(
    repository: &dyn ManagementRepository,
) -> Result<ContractFixture, Box<dyn Error>> {
    let suffix = Uuid::now_v7();
    let user_id = UserId::new();
    repository
        .save_user(&UserRecord {
            id: user_id,
            email: format!("repository-{suffix}@example.test"),
            display_name: "Repository User".to_owned(),
            status: "active".to_owned(),
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    let account_a = AccountId::new();
    let account_b = AccountId::new();
    for (account_id, label) in [(account_a, "a"), (account_b, "b")] {
        repository
            .save_account(&AccountRecord {
                id: account_id,
                slug: format!("repository-{label}-{suffix}"),
                display_name: format!("Account {label}"),
                status: "active".to_owned(),
                created_by: Some(user_id),
                created_at: 1,
                updated_at: 1,
            })
            .await?;
    }
    repository
        .save_membership(&MembershipRecord {
            id: MembershipId::new(),
            account_id: account_a,
            user_id,
            status: "active".to_owned(),
            joined_at: Some(1),
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    let session_digest = test_digest(suffix, 3);
    repository
        .save_session(&SessionRecord {
            id: SessionId::new(),
            user_id,
            session_digest,
            csrf_digest: [4_u8; 32],
            created_at: 1,
            last_seen_at: 10,
            idle_expires_at: 100,
            absolute_expires_at: 200,
            revoked_at: None,
        })
        .await?;
    let credential_id = ApiCredentialId::new();
    let credential_digest = test_digest(suffix, 5);
    repository
        .save_api_credential(&ApiCredentialRecord {
            id: credential_id,
            account_id: account_a,
            owner_user_id: user_id,
            system_id: None,
            name: format!("contract-{suffix}"),
            credential_digest,
            scopes: BTreeSet::from(["systems_read".to_owned(), "telemetry_write".to_owned()]),
            created_at: 1,
            expires_at: Some(200),
            revoked_at: None,
        })
        .await?;
    let system_a = SystemId::new();
    let system_b = SystemId::new();
    repository
        .save_system_registry(&SystemRegistryRecord {
            system_id: system_a,
            account_id: account_a,
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    for grant in [
        AuthorizationGrant {
            account_id: account_a,
            user_id,
            permission: Permission::AccountRead,
            system_id: None,
            granted_by: user_id,
            created_at: 1,
            expires_at: None,
        },
        AuthorizationGrant {
            account_id: account_a,
            user_id,
            permission: Permission::TelemetryWrite,
            system_id: Some(system_a),
            granted_by: user_id,
            created_at: 1,
            expires_at: Some(200),
        },
    ] {
        repository.grant_user_permission(&grant).await?;
    }
    append_audit(repository, account_a, "account.a").await?;
    append_audit(repository, account_b, "account.b").await?;
    Ok(ContractFixture {
        user_id,
        account_a,
        account_b,
        system_a,
        system_b,
        credential_id,
        session_digest,
        credential_digest,
    })
}

#[allow(clippy::too_many_lines)]
async fn verify_contract(
    repository: &dyn ManagementRepository,
    fixture: ContractFixture,
    backend: RoutingBackend,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        repository.user(fixture.user_id).await?.map(|user| user.id),
        Some(fixture.user_id)
    );
    assert!(
        repository
            .active_membership(fixture.account_a, fixture.user_id)
            .await?
            .is_some()
    );
    assert!(
        repository
            .active_membership(fixture.account_b, fixture.user_id)
            .await?
            .is_none()
    );
    assert_eq!(
        repository
            .active_accounts_for_user(fixture.user_id)
            .await?
            .into_iter()
            .map(|account| account.id)
            .collect::<Vec<_>>(),
        vec![fixture.account_a]
    );
    assert!(
        repository
            .active_session_by_digest(&fixture.session_digest, 50)
            .await?
            .is_some()
    );
    assert!(
        repository
            .active_session_by_digest(&fixture.session_digest, 250)
            .await?
            .is_none()
    );
    assert!(
        repository
            .api_credential(fixture.account_a, fixture.credential_id)
            .await?
            .is_some()
    );
    assert!(
        repository
            .api_credential(fixture.account_b, fixture.credential_id)
            .await?
            .is_none()
    );
    let Some(credential) = repository
        .active_api_credential_by_digest(&fixture.credential_digest, 50)
        .await?
    else {
        return Err("seeded active credential did not resolve".into());
    };
    assert_eq!(credential.account_id, fixture.account_a);
    assert_eq!(credential.scopes.len(), 2);
    assert_eq!(
        repository
            .system_registry(fixture.system_a)
            .await?
            .map(|record| record.account_id),
        Some(fixture.account_a)
    );
    assert!(
        repository
            .system_registry(fixture.system_b)
            .await?
            .is_none()
    );
    assert_eq!(
        repository.systems_for_account(fixture.account_a).await?,
        vec![fixture.system_a]
    );
    assert!(
        repository
            .user_is_authorized(
                fixture.user_id,
                fixture.account_a,
                Some(fixture.system_b),
                Permission::AccountRead,
                50,
            )
            .await?
    );
    assert!(
        repository
            .principal_is_authorized(
                PrincipalId::User(fixture.user_id),
                fixture.account_a,
                Some(fixture.system_a),
                Permission::TelemetryWrite,
                50,
            )
            .await?
    );
    assert!(
        !repository
            .principal_is_authorized(
                PrincipalId::User(fixture.user_id),
                fixture.account_a,
                Some(fixture.system_b),
                Permission::TelemetryWrite,
                50,
            )
            .await?
    );
    assert!(
        repository
            .user_is_authorized(
                fixture.user_id,
                fixture.account_a,
                Some(fixture.system_a),
                Permission::TelemetryWrite,
                50,
            )
            .await?
    );
    assert!(
        !repository
            .user_is_authorized(
                fixture.user_id,
                fixture.account_a,
                Some(fixture.system_b),
                Permission::TelemetryWrite,
                50,
            )
            .await?
    );
    assert!(
        !repository
            .user_is_authorized(
                fixture.user_id,
                fixture.account_b,
                Some(fixture.system_a),
                Permission::AccountRead,
                50,
            )
            .await?
    );
    let Some(route) = repository.routing(fixture.account_a).await? else {
        return Err("seeded account route does not exist".into());
    };
    assert_eq!(route.backend, backend);
    assert_eq!(route.account_id, fixture.account_a);
    let audit = repository.account_audit(fixture.account_a, 10).await?;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, "account.a");

    let Some(mut disabled_user) = repository.user(fixture.user_id).await? else {
        return Err("seeded user does not exist".into());
    };
    "disabled".clone_into(&mut disabled_user.status);
    disabled_user.updated_at = 2;
    repository.save_user(&disabled_user).await?;
    assert!(
        repository
            .active_session_by_digest(&fixture.session_digest, 50)
            .await?
            .is_none()
    );
    assert!(
        repository
            .active_api_credential_by_digest(&fixture.credential_digest, 50)
            .await?
            .is_none()
    );
    assert!(
        !repository
            .user_is_authorized(
                fixture.user_id,
                fixture.account_a,
                Some(fixture.system_a),
                Permission::TelemetryWrite,
                50,
            )
            .await?
    );
    Ok(())
}

async fn append_audit(
    repository: &dyn ManagementRepository,
    account_id: AccountId,
    action: &str,
) -> Result<(), Box<dyn Error>> {
    let id = AuditEventId::new();
    let mut event_hash = [0_u8; 32];
    event_hash[..16].copy_from_slice(id.as_uuid().as_bytes());
    event_hash[16..].copy_from_slice(id.as_uuid().as_bytes());
    repository
        .append_audit(&AuditRecord {
            id,
            occurred_at: 1,
            request_id: Some(Uuid::now_v7()),
            actor_type: "user".to_owned(),
            actor_id: Some(Uuid::now_v7()),
            account_id: Some(account_id),
            action: action.to_owned(),
            target_type: "account".to_owned(),
            target_id: Some(account_id.as_uuid()),
            outcome: "succeeded".to_owned(),
            previous_event_hash: None,
            event_hash,
            safe_metadata: serde_json::json!({"source": "contract"}),
        })
        .await?;
    Ok(())
}

fn test_digest(seed: Uuid, discriminator: u8) -> [u8; 32] {
    let mut digest = [0_u8; 32];
    digest[..16].copy_from_slice(seed.as_bytes());
    digest[16..].copy_from_slice(seed.as_bytes());
    digest[31] ^= discriminator;
    digest
}

struct SessionClock;
impl Clock for SessionClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(1_000))
    }
}
