//! Local-user lifecycle contracts shared by `SQLite` and `PostgreSQL` management adapters.

use std::{error::Error, sync::Arc};

use pvlog_application::{
    AcceptInvitation, AdminUserActor, CreateLocalUser, CredentialService, LifecycleCreateOutcome,
    LifecycleUserRecord, LocalCredentialRepository, LocalUserPolicy, RegisterLocalUser,
    UserLifecycleError, UserLifecycleRepository, UserLifecycleService, UserLifecycleUseCases,
};
use pvlog_application_fakes::{FakeCredentialService, FixedClock};
use pvlog_domain::{SessionId, UserId, UserStatus, UtcTimestamp};
use pvlog_storage::{
    DatabaseTarget, PostgresUserLifecycleRepository, SqliteUserLifecycleRepository,
    apply_migrations,
};
use secrecy::{ExposeSecret as _, SecretString};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

const NOW: i64 = 1_750_000_000_000;

#[tokio::test]
async fn sqlite_user_lifecycle_repository_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management = directory.path().join("management.sqlite3");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management.clone(),
        accounts_dir: directory.path().join("accounts"),
    })
    .await?;
    let repository = Arc::new(SqliteUserLifecycleRepository::new(management.clone()));
    let outcome = verify_contract(repository.clone()).await?;
    let session_id = SessionId::new();
    let mut connection = sqlite_connection(&management).await?;
    insert_sqlite_login_state(&mut connection, outcome.managed_user_id, session_id).await?;
    connection.close().await?;
    exercise_destructive_lifecycle(repository, &outcome).await?;
    let mut connection = sqlite_connection(&management).await?;
    let revoked_at: Option<i64> = sqlx::query_scalar("SELECT revoked_at FROM sessions WHERE id=?")
        .bind(session_id.as_uuid().as_bytes().as_slice())
        .fetch_one(&mut connection)
        .await?;
    let credentials: i64 =
        sqlx::query_scalar("SELECT count(*) FROM local_credentials WHERE user_id=?")
            .bind(outcome.managed_user_id.as_uuid().as_bytes().as_slice())
            .fetch_one(&mut connection)
            .await?;
    connection.close().await?;
    assert_eq!(revoked_at, Some(NOW));
    assert_eq!(credentials, 0);
    Ok(())
}

#[tokio::test]
async fn postgres_user_lifecycle_repository_contract_when_configured() -> Result<(), Box<dyn Error>>
{
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let repository = Arc::new(PostgresUserLifecycleRepository::new(url.clone()));
    let outcome = verify_contract(repository.clone()).await?;
    let session_id = SessionId::new();
    let mut connection = PgConnection::connect(&url).await?;
    insert_postgres_login_state(&mut connection, outcome.managed_user_id, session_id).await?;
    connection.close().await?;
    exercise_destructive_lifecycle(repository, &outcome).await?;
    let mut connection = PgConnection::connect(&url).await?;
    let revoked_at: Option<i64> =
        sqlx::query_scalar("SELECT revoked_at FROM management.sessions WHERE id=$1")
            .bind(session_id.as_uuid())
            .fetch_one(&mut connection)
            .await?;
    let credentials: i64 =
        sqlx::query_scalar("SELECT count(*) FROM management.local_credentials WHERE user_id=$1")
            .bind(outcome.managed_user_id.as_uuid())
            .fetch_one(&mut connection)
            .await?;
    connection.close().await?;
    assert_eq!(revoked_at, Some(NOW));
    assert_eq!(credentials, 0);
    Ok(())
}

struct ContractOutcome {
    admin_user_id: UserId,
    managed_user_id: UserId,
}

#[allow(clippy::too_many_lines)]
async fn verify_contract<R>(repository: Arc<R>) -> Result<ContractOutcome, Box<dyn Error>>
where
    R: UserLifecycleRepository + LocalCredentialRepository + 'static,
{
    let suffix = UserId::new();
    let admin = lifecycle_user(
        &format!("admin-{suffix}@example.test"),
        "Administrator",
        UserStatus::Active,
    );
    assert_eq!(
        repository.create_user(&admin).await?,
        LifecycleCreateOutcome::Created
    );
    let actor = AdminUserActor {
        user_id: admin.id,
        can_manage_users: true,
    };
    let service = UserLifecycleService::new(
        repository.clone(),
        Arc::new(FakeCredentialService),
        Arc::new(FixedClock::new(UtcTimestamp::from_epoch_millis(NOW)?)),
        LocalUserPolicy {
            allow_self_registration: true,
            require_verified_email: true,
            invitation_lifetime_seconds: 600,
            password_minimum_length: 20,
            password_maximum_length: 128,
        },
    );

    assert!(matches!(
        service
            .create_user(
                AdminUserActor {
                    can_manage_users: false,
                    ..actor
                },
                CreateLocalUser {
                    email: format!("denied-{suffix}@example.test"),
                    display_name: "Denied".to_owned(),
                    email_verified: true,
                }
            )
            .await,
        Err(UserLifecycleError::Forbidden)
    ));

    assert!(matches!(
        service
            .accept_invitation(AcceptInvitation {
                token: SecretString::from("unused-token"),
                display_name: "Too short".to_owned(),
                password: SecretString::from("short-password"),
            })
            .await,
        Err(UserLifecycleError::InvalidInput("password"))
    ));
    assert!(matches!(
        service
            .create_user(
                actor,
                CreateLocalUser {
                    email: format!("unverified-{suffix}@example.test"),
                    display_name: "Unverified".to_owned(),
                    email_verified: false,
                }
            )
            .await,
        Err(UserLifecycleError::EmailVerificationRequired)
    ));

    let managed = service
        .create_user(
            actor,
            CreateLocalUser {
                email: format!("  MANAGED-{suffix}@EXAMPLE.TEST "),
                display_name: " Managed user ".to_owned(),
                email_verified: true,
            },
        )
        .await?;
    assert_eq!(managed.email, format!("managed-{suffix}@example.test"));
    assert_eq!(managed.display_name, "Managed user");
    assert_eq!(service.own_profile(managed.id).await?, managed);
    let updated_profile = service
        .update_own_profile(managed.id, " Updated profile ".to_owned())
        .await?;
    assert_eq!(updated_profile.display_name, "Updated profile");
    assert_eq!(updated_profile.email, managed.email);
    assert!(matches!(
        service
            .update_own_profile(managed.id, "   ".to_owned())
            .await,
        Err(UserLifecycleError::InvalidInput("display_name"))
    ));
    assert!(matches!(
        service
            .create_user(
                actor,
                CreateLocalUser {
                    email: managed.email.clone(),
                    display_name: managed.display_name.clone(),
                    email_verified: true,
                }
            )
            .await,
        Err(UserLifecycleError::Conflict)
    ));

    assert!(matches!(
        service.delete(actor, actor.user_id).await,
        Err(UserLifecycleError::SelfAdministrationDenied)
    ));

    let invitation = service
        .invite_user(
            actor,
            pvlog_application::InviteLocalUser {
                email: format!("invitee-{suffix}@example.test"),
            },
        )
        .await?;
    let invitation_token = invitation.token.expose_secret().to_owned();
    let invitation_debug = format!("{invitation:?}");
    assert!(invitation_debug.contains("[REDACTED]"));
    assert!(!invitation_debug.contains(&invitation_token));
    let accepted = service
        .accept_invitation(AcceptInvitation {
            token: invitation.token,
            display_name: "Invitee".to_owned(),
            password: SecretString::from("accepted-password-longer"),
        })
        .await?;
    assert_eq!(
        accepted,
        pvlog_application::PublicLifecycleOutcome::Accepted
    );
    let accepted_credential = repository
        .credential_by_email(&format!("invitee-{suffix}@example.test"))
        .await?
        .ok_or("accepted invitation must create a local credential")?;
    assert!(
        FakeCredentialService
            .verify_password(
                &SecretString::from("accepted-password-longer"),
                &accepted_credential.password_hash,
            )
            .await?
    );
    assert_eq!(
        service
            .accept_invitation(AcceptInvitation {
                token: SecretString::from("unknown"),
                display_name: "Unknown".to_owned(),
                password: SecretString::from("accepted-password-longer"),
            })
            .await?,
        accepted
    );

    let pending = lifecycle_user(
        &format!("pending-{suffix}@example.test"),
        "Pending",
        UserStatus::Invited,
    );
    assert_eq!(
        repository.create_user(&pending).await?,
        LifecycleCreateOutcome::Created
    );
    assert!(matches!(
        service.activate(actor, pending.id, false).await,
        Err(UserLifecycleError::EmailVerificationRequired)
    ));
    assert_eq!(
        service.activate(actor, pending.id, true).await?.status,
        UserStatus::Active
    );

    let registration = RegisterLocalUser {
        email: format!("register-{suffix}@example.test"),
        display_name: "Registration".to_owned(),
    };
    let first = service.register(registration.clone()).await?;
    let duplicate = service.register(registration).await?;
    assert_eq!(first, duplicate);

    Ok(ContractOutcome {
        admin_user_id: admin.id,
        managed_user_id: managed.id,
    })
}

async fn exercise_destructive_lifecycle<R>(
    repository: Arc<R>,
    outcome: &ContractOutcome,
) -> Result<(), Box<dyn Error>>
where
    R: UserLifecycleRepository + LocalCredentialRepository + 'static,
{
    let service = UserLifecycleService::new(
        repository.clone(),
        Arc::new(FakeCredentialService),
        Arc::new(FixedClock::new(UtcTimestamp::from_epoch_millis(NOW)?)),
        LocalUserPolicy::default(),
    );
    let actor = AdminUserActor {
        user_id: outcome.admin_user_id,
        can_manage_users: true,
    };
    let disabled = service.disable(actor, outcome.managed_user_id).await?;
    assert_eq!(disabled.status, UserStatus::Disabled);
    let unlocked = service.unlock(actor, outcome.managed_user_id).await?;
    assert_eq!(unlocked.locked_until, None);
    service.delete(actor, outcome.managed_user_id).await?;
    let deleted = repository
        .user(outcome.managed_user_id)
        .await?
        .ok_or("deleted user missing")?;
    assert_eq!(deleted.status, UserStatus::Deleted);
    assert!(deleted.email.starts_with("deleted-"));
    Ok(())
}

fn lifecycle_user(email: &str, display_name: &str, status: UserStatus) -> LifecycleUserRecord {
    LifecycleUserRecord {
        id: UserId::new(),
        email: email.to_owned(),
        display_name: display_name.to_owned(),
        status,
        email_verified_at: (status == UserStatus::Active).then_some(NOW),
        disabled_at: None,
        locked_until: None,
        created_at: NOW,
        updated_at: NOW,
    }
}

async fn insert_sqlite_login_state(
    connection: &mut SqliteConnection,
    user_id: UserId,
    session_id: SessionId,
) -> Result<(), sqlx::Error> {
    let (session_digest, csrf_digest) = session_digests(session_id);
    sqlx::query("INSERT INTO local_credentials (user_id,password_hash,password_changed_at,failed_attempts,locked_until) VALUES (?,'argon2id-test',1,5,?)")
        .bind(user_id.as_uuid().as_bytes().as_slice())
        .bind(NOW + 60_000)
        .execute(&mut *connection)
        .await?;
    sqlx::query("INSERT INTO sessions (id,user_id,session_digest,csrf_digest,authentication_method,created_at,last_seen_at,idle_expires_at,absolute_expires_at) VALUES (?,?,?,?,'local',1,1,?,?)")
        .bind(session_id.as_uuid().as_bytes().as_slice())
        .bind(user_id.as_uuid().as_bytes().as_slice())
        .bind(session_digest.as_slice())
        .bind(csrf_digest.as_slice())
        .bind(NOW + 60_000)
        .bind(NOW + 120_000)
        .execute(connection)
        .await?;
    Ok(())
}

async fn insert_postgres_login_state(
    connection: &mut PgConnection,
    user_id: UserId,
    session_id: SessionId,
) -> Result<(), sqlx::Error> {
    let (session_digest, csrf_digest) = session_digests(session_id);
    sqlx::query("INSERT INTO management.local_credentials (user_id,password_hash,password_changed_at,failed_attempts,locked_until) VALUES ($1,'argon2id-test',1,5,$2)")
        .bind(user_id.as_uuid())
        .bind(NOW + 60_000)
        .execute(&mut *connection)
        .await?;
    sqlx::query("INSERT INTO management.sessions (id,user_id,session_digest,csrf_digest,authentication_method,created_at,last_seen_at,idle_expires_at,absolute_expires_at) VALUES ($1,$2,$3,$4,'local',1,1,$5,$6)")
        .bind(session_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(session_digest.as_slice())
        .bind(csrf_digest.as_slice())
        .bind(NOW + 60_000)
        .bind(NOW + 120_000)
        .execute(connection)
        .await?;
    Ok(())
}

fn session_digests(id: SessionId) -> ([u8; 32], [u8; 32]) {
    let mut session = [0_u8; 32];
    session[..16].copy_from_slice(id.as_uuid().as_bytes());
    session[16..].copy_from_slice(id.as_uuid().as_bytes());
    let mut csrf = session;
    csrf.reverse();
    (session, csrf)
}

async fn sqlite_connection(path: &std::path::Path) -> Result<SqliteConnection, sqlx::Error> {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await
}
