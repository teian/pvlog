//! Argon2id password, lockout, rehash, and recovery contracts for both engines.

use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pvlog_application::{
    AdminUserActor, Argon2CredentialConfig, Argon2CredentialService, AuthenticatePassword,
    AuthenticationOutcome, ChangePassword, CommonPasswordHook, CredentialService,
    LifecycleCreateOutcome, LifecycleUserRecord, LocalCredentialRepository, LocalPasswordPolicy,
    LocalPasswordService, LocalPasswordUseCases, PasswordPolicyError, PasswordRecoveryNotifier,
    PasswordServiceError, PortError, SetInitialPassword, UserLifecycleRepository,
};
use pvlog_application_fakes::FixedClock;
use pvlog_domain::{UserId, UserStatus, UtcTimestamp};
use pvlog_storage::{
    DatabaseTarget, PostgresUserLifecycleRepository, SqliteUserLifecycleRepository,
    apply_migrations,
};
use secrecy::{ExposeSecret as _, SecretString};
use tempfile::TempDir;

const NOW: i64 = 1_760_000_000_000;

#[tokio::test]
async fn sqlite_local_password_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management = directory.path().join("management.sqlite3");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management.clone(),
        accounts_dir: directory.path().join("accounts"),
    })
    .await?;
    verify_contract(Arc::new(SqliteUserLifecycleRepository::new(management))).await
}

#[tokio::test]
async fn postgres_local_password_contract_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    verify_contract(Arc::new(PostgresUserLifecycleRepository::new(url))).await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract<R>(repository: Arc<R>) -> Result<(), Box<dyn Error>>
where
    R: UserLifecycleRepository + LocalCredentialRepository + 'static,
{
    let suffix = UserId::new();
    let admin = user(format!("admin-{suffix}@example.test"));
    let target = user(format!("password-{suffix}@example.test"));
    assert_eq!(
        repository.create_user(&admin).await?,
        LifecycleCreateOutcome::Created
    );
    assert_eq!(
        repository.create_user(&target).await?,
        LifecycleCreateOutcome::Created
    );
    let actor = AdminUserActor {
        user_id: admin.id,
        can_manage_users: true,
    };
    let clock = Arc::new(FixedClock::new(UtcTimestamp::from_epoch_millis(NOW)?));
    let notifier = Arc::new(RecordingRecoveryNotifier::default());
    let old_credentials = Arc::new(credentials(1));
    let old_service = service(
        repository.clone(),
        old_credentials.clone(),
        clock.clone(),
        notifier.clone(),
    );

    assert!(matches!(
        old_service
            .set_initial_password(
                actor,
                SetInitialPassword {
                    user_id: target.id,
                    password: SecretString::from("short")
                }
            )
            .await,
        Err(PasswordServiceError::Policy(PasswordPolicyError::TooShort))
    ));
    assert!(matches!(
        old_service
            .set_initial_password(
                actor,
                SetInitialPassword {
                    user_id: target.id,
                    password: SecretString::from("passwordpassword")
                }
            )
            .await,
        Err(PasswordServiceError::Policy(
            PasswordPolicyError::CommonOrBreached
        ))
    ));
    old_service
        .set_initial_password(
            actor,
            SetInitialPassword {
                user_id: target.id,
                password: SecretString::from("Initial-PV-Password-42"),
            },
        )
        .await?;
    let stored = repository
        .credential(target.id)
        .await?
        .ok_or("credential missing")?;
    assert!(
        stored
            .password_hash
            .expose_encoded()
            .starts_with("$argon2id$v=19$m=8192,t=1,p=1$")
    );
    assert!(!old_credentials.password_needs_rehash(&stored.password_hash)?);

    for _ in 0..3 {
        assert_eq!(
            old_service
                .authenticate(AuthenticatePassword {
                    email: target.email.clone(),
                    password: SecretString::from("wrong-password-value")
                })
                .await?,
            AuthenticationOutcome::Rejected
        );
    }
    assert_eq!(
        old_service
            .authenticate(AuthenticatePassword {
                email: target.email.to_uppercase(),
                password: SecretString::from("Initial-PV-Password-42")
            })
            .await?,
        AuthenticationOutcome::Rejected
    );
    clock.set(UtcTimestamp::from_epoch_millis(NOW + 61_000)?)?;
    assert_eq!(
        old_service
            .authenticate(AuthenticatePassword {
                email: target.email.clone(),
                password: SecretString::from("Initial-PV-Password-42")
            })
            .await?,
        AuthenticationOutcome::Authenticated(target.id)
    );
    assert_eq!(
        repository
            .credential(target.id)
            .await?
            .ok_or("credential missing")?
            .failed_attempts,
        0
    );

    let current_credentials = Arc::new(credentials(2));
    let current_service = service(
        repository.clone(),
        current_credentials.clone(),
        clock.clone(),
        notifier.clone(),
    );
    assert_eq!(
        current_service
            .authenticate(AuthenticatePassword {
                email: target.email.clone(),
                password: SecretString::from("Initial-PV-Password-42")
            })
            .await?,
        AuthenticationOutcome::Authenticated(target.id)
    );
    let rehashed = repository
        .credential(target.id)
        .await?
        .ok_or("credential missing")?;
    assert!(!current_credentials.password_needs_rehash(&rehashed.password_hash)?);

    assert!(matches!(
        current_service
            .change_password(ChangePassword {
                user_id: target.id,
                current_password: SecretString::from("wrong-password-value"),
                new_password: SecretString::from("Changed-PV-Password-42")
            })
            .await,
        Err(PasswordServiceError::CurrentCredentialRejected)
    ));
    current_service
        .change_password(ChangePassword {
            user_id: target.id,
            current_password: SecretString::from("Initial-PV-Password-42"),
            new_password: SecretString::from("Changed-PV-Password-42"),
        })
        .await?;

    let unknown = current_service
        .request_recovery(format!("unknown-{suffix}@example.test"))
        .await?;
    let known = current_service
        .request_recovery(target.email.clone())
        .await?;
    assert_eq!(unknown, known);
    let delivered = notifier.deliveries()?;
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].email, target.email);
    let recovery_token = SecretString::from(delivered[0].token.clone());
    assert_eq!(
        current_service
            .complete_recovery(
                SecretString::from("unknown-recovery-token"),
                SecretString::from("Recovered-PV-Password-42")
            )
            .await?,
        known
    );
    assert_eq!(
        current_service
            .complete_recovery(
                recovery_token,
                SecretString::from("Recovered-PV-Password-42")
            )
            .await?,
        known
    );
    assert_eq!(
        current_service
            .authenticate(AuthenticatePassword {
                email: target.email.clone(),
                password: SecretString::from("Recovered-PV-Password-42")
            })
            .await?,
        AuthenticationOutcome::Authenticated(target.id)
    );

    repository.disable_user(target.id, NOW + 62_000).await?;
    assert_eq!(
        current_service
            .authenticate(AuthenticatePassword {
                email: target.email,
                password: SecretString::from("Recovered-PV-Password-42")
            })
            .await?,
        AuthenticationOutcome::Rejected
    );
    Ok(())
}

fn service<R>(
    repository: Arc<R>,
    credentials: Arc<Argon2CredentialService>,
    clock: Arc<FixedClock>,
    notifier: Arc<RecordingRecoveryNotifier>,
) -> LocalPasswordService
where
    R: LocalCredentialRepository + 'static,
{
    LocalPasswordService::new(
        repository,
        credentials,
        clock,
        Arc::new(CommonPasswordHook::default()),
        notifier,
        LocalPasswordPolicy {
            minimum_length: 12,
            maximum_length: 128,
            maximum_failed_attempts: 3,
            lockout_seconds: 60,
            recovery_lifetime_seconds: 600,
        },
    )
}

fn credentials(time_cost: u32) -> Argon2CredentialService {
    Argon2CredentialService::new(
        Argon2CredentialConfig {
            memory_kib: 8_192,
            time_cost,
            parallelism: 1,
        },
        &SecretString::from("test-digest-key-with-at-least-32-bytes"),
    )
}

fn user(email: String) -> LifecycleUserRecord {
    LifecycleUserRecord {
        id: UserId::new(),
        email,
        display_name: "Password contract".to_owned(),
        status: UserStatus::Active,
        email_verified_at: Some(NOW),
        disabled_at: None,
        locked_until: None,
        created_at: NOW,
        updated_at: NOW,
    }
}

#[derive(Default)]
struct RecordingRecoveryNotifier {
    deliveries: Mutex<Vec<RecoveryDelivery>>,
}

impl RecordingRecoveryNotifier {
    fn deliveries(&self) -> Result<Vec<RecoveryDelivery>, PortError> {
        self.deliveries
            .lock()
            .map(|deliveries| deliveries.clone())
            .map_err(|_| PortError::Unavailable)
    }
}

#[async_trait]
impl PasswordRecoveryNotifier for RecordingRecoveryNotifier {
    async fn deliver(
        &self,
        email: &str,
        token: &SecretString,
        expires_at: i64,
    ) -> Result<(), PortError> {
        self.deliveries
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push(RecoveryDelivery {
                email: email.to_owned(),
                token: token.expose_secret().to_owned(),
                expires_at,
            });
        Ok(())
    }
}

#[derive(Clone)]
struct RecoveryDelivery {
    email: String,
    token: String,
    #[allow(dead_code)]
    expires_at: i64,
}
