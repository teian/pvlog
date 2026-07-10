//! Argon2id local-password authentication, policy, lockout, and recovery use cases.

use std::{collections::BTreeSet, sync::Arc};

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash as PhcPasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString,
    },
};
use async_trait::async_trait;
use pvlog_domain::{CredentialDigest, PasswordHash, PasswordRecoveryId, UserId, UserStatus};
use secrecy::{ExposeSecret as _, SecretString};
use thiserror::Error;
use uuid::Uuid;

use crate::{AdminUserActor, Clock, CredentialService, PortError, PublicLifecycleOutcome};

/// Versioned Argon2id parameters used for new password verifiers and rehash decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Argon2CredentialConfig {
    pub memory_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
}

impl Default for Argon2CredentialConfig {
    fn default() -> Self {
        Self {
            memory_kib: 19_456,
            time_cost: 2,
            parallelism: 1,
        }
    }
}

/// Production Argon2id password service plus keyed bearer-token digests.
pub struct Argon2CredentialService {
    config: Argon2CredentialConfig,
    digest_key: [u8; 32],
}

impl Argon2CredentialService {
    #[must_use]
    pub fn new(config: Argon2CredentialConfig, digest_secret: &SecretString) -> Self {
        Self {
            config,
            digest_key: *blake3::hash(digest_secret.expose_secret().as_bytes()).as_bytes(),
        }
    }

    fn argon2(&self) -> Result<Argon2<'static>, PortError> {
        let params = Params::new(
            self.config.memory_kib,
            self.config.time_cost,
            self.config.parallelism,
            None,
        )
        .map_err(|_| PortError::Rejected("invalid_argon2_parameters".to_owned()))?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

#[async_trait]
impl CredentialService for Argon2CredentialService {
    async fn hash_password(&self, password: &SecretString) -> Result<PasswordHash, PortError> {
        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
            .map_err(|_| PortError::Rejected("password_salt_generation_failed".to_owned()))?;
        let encoded = self
            .argon2()?
            .hash_password(password.expose_secret().as_bytes(), &salt)
            .map_err(|_| PortError::Rejected("password_hashing_failed".to_owned()))?
            .to_string();
        PasswordHash::new(encoded)
            .map_err(|_| PortError::Rejected("invalid_password_hash".to_owned()))
    }

    async fn verify_password(
        &self,
        password: &SecretString,
        expected: &PasswordHash,
    ) -> Result<bool, PortError> {
        let parsed = PhcPasswordHash::new(expected.expose_encoded())
            .map_err(|_| PortError::Rejected("invalid_password_hash".to_owned()))?;
        Ok(self
            .argon2()?
            .verify_password(password.expose_secret().as_bytes(), &parsed)
            .is_ok())
    }

    async fn digest_bearer(
        &self,
        credential: &SecretString,
    ) -> Result<CredentialDigest, PortError> {
        Ok(CredentialDigest::new(
            *blake3::keyed_hash(&self.digest_key, credential.expose_secret().as_bytes()).as_bytes(),
        ))
    }

    fn password_needs_rehash(&self, encoded: &PasswordHash) -> Result<bool, PortError> {
        let parsed = PhcPasswordHash::new(encoded.expose_encoded())
            .map_err(|_| PortError::Rejected("invalid_password_hash".to_owned()))?;
        Ok(parsed.algorithm.as_str() != "argon2id"
            || parsed.version != Some(0x13)
            || parsed.params.get_decimal("m") != Some(self.config.memory_kib)
            || parsed.params.get_decimal("t") != Some(self.config.time_cost)
            || parsed.params.get_decimal("p") != Some(self.config.parallelism))
    }
}

/// Configurable local-password and brute-force policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalPasswordPolicy {
    pub minimum_length: u16,
    pub maximum_length: u16,
    pub maximum_failed_attempts: u16,
    pub lockout_seconds: u32,
    pub recovery_lifetime_seconds: u32,
}

impl Default for LocalPasswordPolicy {
    fn default() -> Self {
        Self {
            minimum_length: 12,
            maximum_length: 128,
            maximum_failed_attempts: 5,
            lockout_seconds: 900,
            recovery_lifetime_seconds: 1_800,
        }
    }
}

/// Hook for deployment-specific common/breached-password policy.
#[async_trait]
pub trait PasswordPolicyHook: Send + Sync {
    async fn check(&self, password: &SecretString) -> Result<(), PasswordPolicyError>;
}

/// Small built-in common-password deny set; deployments can replace it with a breach service.
pub struct CommonPasswordHook {
    normalized: BTreeSet<String>,
}

impl Default for CommonPasswordHook {
    fn default() -> Self {
        Self {
            normalized: [
                "123456789012",
                "correcthorsebatterystaple",
                "letmeinletmein",
                "passwordpassword",
                "qwertyqwerty",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

#[async_trait]
impl PasswordPolicyHook for CommonPasswordHook {
    async fn check(&self, password: &SecretString) -> Result<(), PasswordPolicyError> {
        if self
            .normalized
            .contains(&password.expose_secret().to_lowercase())
        {
            Err(PasswordPolicyError::CommonOrBreached)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCredentialRecord {
    pub user_id: UserId,
    pub email: String,
    pub user_status: UserStatus,
    pub password_hash: PasswordHash,
    pub failed_attempts: u32,
    pub locked_until: Option<i64>,
    pub rehash_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasswordRecoveryRecord {
    pub id: PasswordRecoveryId,
    pub user_id: UserId,
    pub token_digest: CredentialDigest,
    pub expires_at: i64,
    pub created_at: i64,
}

#[async_trait]
pub trait LocalCredentialRepository: Send + Sync {
    async fn credential_by_email(
        &self,
        normalized_email: &str,
    ) -> Result<Option<LocalCredentialRecord>, PortError>;
    async fn credential(&self, user_id: UserId)
    -> Result<Option<LocalCredentialRecord>, PortError>;
    async fn save_password(
        &self,
        user_id: UserId,
        hash: &PasswordHash,
        changed_at: i64,
        rehash_required: bool,
    ) -> Result<bool, PortError>;
    async fn record_failed_attempt(
        &self,
        user_id: UserId,
        maximum_attempts: u16,
        locked_until: i64,
    ) -> Result<(), PortError>;
    async fn clear_failed_attempts(&self, user_id: UserId) -> Result<(), PortError>;
    async fn create_recovery(&self, record: &PasswordRecoveryRecord) -> Result<(), PortError>;
    async fn consume_recovery(
        &self,
        digest: &CredentialDigest,
        new_hash: &PasswordHash,
        changed_at: i64,
    ) -> Result<bool, PortError>;
}

#[async_trait]
pub trait PasswordRecoveryNotifier: Send + Sync {
    async fn deliver(
        &self,
        email: &str,
        token: &SecretString,
        expires_at: i64,
    ) -> Result<(), PortError>;
}

/// Safe default until an installation configures an email notification adapter.
pub struct DiscardingRecoveryNotifier;

#[async_trait]
impl PasswordRecoveryNotifier for DiscardingRecoveryNotifier {
    async fn deliver(
        &self,
        _email: &str,
        _token: &SecretString,
        _expires_at: i64,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct AuthenticatePassword {
    pub email: String,
    pub password: SecretString,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationOutcome {
    Authenticated(UserId),
    Rejected,
}

#[derive(Debug)]
pub struct SetInitialPassword {
    pub user_id: UserId,
    pub password: SecretString,
}

#[derive(Debug)]
pub struct ChangePassword {
    pub user_id: UserId,
    pub current_password: SecretString,
    pub new_password: SecretString,
}

#[async_trait]
pub trait LocalPasswordUseCases: Send + Sync {
    async fn set_initial_password(
        &self,
        actor: AdminUserActor,
        command: SetInitialPassword,
    ) -> Result<(), PasswordServiceError>;
    async fn authenticate(
        &self,
        command: AuthenticatePassword,
    ) -> Result<AuthenticationOutcome, PasswordServiceError>;
    async fn change_password(&self, command: ChangePassword) -> Result<(), PasswordServiceError>;
    async fn request_recovery(
        &self,
        email: String,
    ) -> Result<PublicLifecycleOutcome, PasswordServiceError>;
    async fn complete_recovery(
        &self,
        token: SecretString,
        new_password: SecretString,
    ) -> Result<PublicLifecycleOutcome, PasswordServiceError>;
}

pub struct LocalPasswordService {
    repository: Arc<dyn LocalCredentialRepository>,
    credentials: Arc<dyn CredentialService>,
    clock: Arc<dyn Clock>,
    policy_hook: Arc<dyn PasswordPolicyHook>,
    notifier: Arc<dyn PasswordRecoveryNotifier>,
    policy: LocalPasswordPolicy,
}

impl LocalPasswordService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn LocalCredentialRepository>,
        credentials: Arc<dyn CredentialService>,
        clock: Arc<dyn Clock>,
        policy_hook: Arc<dyn PasswordPolicyHook>,
        notifier: Arc<dyn PasswordRecoveryNotifier>,
        policy: LocalPasswordPolicy,
    ) -> Self {
        Self {
            repository,
            credentials,
            clock,
            policy_hook,
            notifier,
            policy,
        }
    }

    fn now(&self) -> Result<i64, PasswordServiceError> {
        i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| PasswordServiceError::Internal("clock_out_of_range"))
    }

    async fn validate_password(&self, password: &SecretString) -> Result<(), PasswordServiceError> {
        let length = password.expose_secret().chars().count();
        if length < usize::from(self.policy.minimum_length) {
            return Err(PasswordPolicyError::TooShort.into());
        }
        if length > usize::from(self.policy.maximum_length) {
            return Err(PasswordPolicyError::TooLong.into());
        }
        self.policy_hook.check(password).await?;
        Ok(())
    }

    async fn reject_with_work(
        &self,
        password: &SecretString,
    ) -> Result<AuthenticationOutcome, PasswordServiceError> {
        let _ = self.credentials.hash_password(password).await?;
        Ok(AuthenticationOutcome::Rejected)
    }
}

#[async_trait]
impl LocalPasswordUseCases for LocalPasswordService {
    async fn set_initial_password(
        &self,
        actor: AdminUserActor,
        command: SetInitialPassword,
    ) -> Result<(), PasswordServiceError> {
        if !actor.can_manage_users {
            return Err(PasswordServiceError::Forbidden);
        }
        self.validate_password(&command.password).await?;
        let hash = self.credentials.hash_password(&command.password).await?;
        if !self
            .repository
            .save_password(command.user_id, &hash, self.now()?, false)
            .await?
        {
            return Err(PasswordServiceError::NotFound);
        }
        Ok(())
    }

    async fn authenticate(
        &self,
        command: AuthenticatePassword,
    ) -> Result<AuthenticationOutcome, PasswordServiceError> {
        let normalized = normalize_email_for_lookup(&command.email);
        let Some(stored) = self.repository.credential_by_email(&normalized).await? else {
            return self.reject_with_work(&command.password).await;
        };
        let now = self.now()?;
        let verified = self
            .credentials
            .verify_password(&command.password, &stored.password_hash)
            .await?;
        if !verified || stored.user_status != UserStatus::Active {
            if stored.user_status != UserStatus::Deleted {
                let locked_until = now
                    .checked_add(i64::from(self.policy.lockout_seconds) * 1_000)
                    .ok_or(PasswordServiceError::Internal("lockout_overflow"))?;
                self.repository
                    .record_failed_attempt(
                        stored.user_id,
                        self.policy.maximum_failed_attempts,
                        locked_until,
                    )
                    .await?;
            }
            return Ok(AuthenticationOutcome::Rejected);
        }
        if stored.locked_until.is_some_and(|until| until > now) {
            return Ok(AuthenticationOutcome::Rejected);
        }
        self.repository
            .clear_failed_attempts(stored.user_id)
            .await?;
        if stored.rehash_required
            || self
                .credentials
                .password_needs_rehash(&stored.password_hash)?
        {
            let hash = self.credentials.hash_password(&command.password).await?;
            self.repository
                .save_password(stored.user_id, &hash, now, false)
                .await?;
        }
        Ok(AuthenticationOutcome::Authenticated(stored.user_id))
    }

    async fn change_password(&self, command: ChangePassword) -> Result<(), PasswordServiceError> {
        let Some(stored) = self.repository.credential(command.user_id).await? else {
            return Err(PasswordServiceError::CurrentCredentialRejected);
        };
        if stored.user_status != UserStatus::Active
            || !self
                .credentials
                .verify_password(&command.current_password, &stored.password_hash)
                .await?
        {
            return Err(PasswordServiceError::CurrentCredentialRejected);
        }
        self.validate_password(&command.new_password).await?;
        let new_hash = self
            .credentials
            .hash_password(&command.new_password)
            .await?;
        self.repository
            .save_password(command.user_id, &new_hash, self.now()?, false)
            .await?;
        Ok(())
    }

    async fn request_recovery(
        &self,
        email: String,
    ) -> Result<PublicLifecycleOutcome, PasswordServiceError> {
        let normalized = normalize_email_for_lookup(&email);
        let Some(stored) = self.repository.credential_by_email(&normalized).await? else {
            return Ok(PublicLifecycleOutcome::Accepted);
        };
        if stored.user_status != UserStatus::Active {
            return Ok(PublicLifecycleOutcome::Accepted);
        }
        let now = self.now()?;
        let expires_at = now
            .checked_add(i64::from(self.policy.recovery_lifetime_seconds) * 1_000)
            .ok_or(PasswordServiceError::Internal("recovery_expiry_overflow"))?;
        let token = SecretString::from(format!("{}.{}", Uuid::new_v4(), Uuid::new_v4()));
        let digest = self.credentials.digest_bearer(&token).await?;
        self.repository
            .create_recovery(&PasswordRecoveryRecord {
                id: PasswordRecoveryId::new(),
                user_id: stored.user_id,
                token_digest: digest,
                expires_at,
                created_at: now,
            })
            .await?;
        self.notifier
            .deliver(&stored.email, &token, expires_at)
            .await?;
        Ok(PublicLifecycleOutcome::Accepted)
    }

    async fn complete_recovery(
        &self,
        token: SecretString,
        new_password: SecretString,
    ) -> Result<PublicLifecycleOutcome, PasswordServiceError> {
        self.validate_password(&new_password).await?;
        let digest = self.credentials.digest_bearer(&token).await?;
        let hash = self.credentials.hash_password(&new_password).await?;
        let _ = self
            .repository
            .consume_recovery(&digest, &hash, self.now()?)
            .await?;
        Ok(PublicLifecycleOutcome::Accepted)
    }
}

fn normalize_email_for_lookup(value: &str) -> String {
    value.trim().to_lowercase()
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasswordPolicyError {
    #[error("password is shorter than policy permits")]
    TooShort,
    #[error("password is longer than policy permits")]
    TooLong,
    #[error("password is present in the common or breached password policy")]
    CommonOrBreached,
    #[error("password was rejected by policy")]
    Rejected,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PasswordServiceError {
    #[error("password administration is forbidden")]
    Forbidden,
    #[error("requested user was not found")]
    NotFound,
    #[error("current credential was rejected")]
    CurrentCredentialRejected,
    #[error(transparent)]
    Policy(#[from] PasswordPolicyError),
    #[error("password persistence is temporarily unavailable")]
    Persistence,
    #[error("password service failed: {0}")]
    Internal(&'static str),
}

impl From<PortError> for PasswordServiceError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::NotFound => Self::NotFound,
            PortError::Conflict | PortError::Unavailable | PortError::Rejected(_) => {
                Self::Persistence
            }
        }
    }
}
