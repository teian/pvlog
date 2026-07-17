//! Policy-controlled local-user lifecycle use cases.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_domain::{CredentialDigest, PasswordHash, UserId, UserInvitationId, UserStatus};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use crate::{Clock, CredentialService, PortError};

/// Persisted management-plane view needed by local-user administration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleUserRecord {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub status: UserStatus,
    pub email_verified_at: Option<i64>,
    pub disabled_at: Option<i64>,
    pub locked_until: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Single-use invitation state; only a credential digest is retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvitationRecord {
    pub id: UserInvitationId,
    pub email: String,
    pub token_digest: CredentialDigest,
    pub invited_by: UserId,
    pub expires_at: i64,
    pub created_at: i64,
}

/// Policy governing public registration and activation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalUserPolicy {
    pub allow_self_registration: bool,
    pub require_verified_email: bool,
    pub invitation_lifetime_seconds: u32,
    /// Minimum accepted initial-password length for invitation activation.
    pub password_minimum_length: u16,
    /// Maximum accepted initial-password length for invitation activation.
    pub password_maximum_length: u16,
}

impl Default for LocalUserPolicy {
    fn default() -> Self {
        Self {
            allow_self_registration: false,
            require_verified_email: true,
            invitation_lifetime_seconds: 86_400,
            password_minimum_length: 12,
            password_maximum_length: 128,
        }
    }
}

/// Authenticated actor supplied by the authorization boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUserActor {
    pub user_id: UserId,
    pub can_manage_users: bool,
}

/// Administrator request to create a local user directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateLocalUser {
    pub email: String,
    pub display_name: String,
    pub email_verified: bool,
}

/// Administrator request to create a single-use invitation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InviteLocalUser {
    pub email: String,
}

/// Public self-registration request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterLocalUser {
    pub email: String,
    pub display_name: String,
}

/// Public, single-use invitation acceptance request including the initial local password.
pub struct AcceptInvitation {
    pub token: SecretString,
    pub display_name: String,
    pub password: SecretString,
}

impl std::fmt::Debug for AcceptInvitation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AcceptInvitation")
            .field("token", &"[REDACTED]")
            .field("display_name", &self.display_name)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// Result of an atomic create attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleCreateOutcome {
    Created,
    Existing,
}

/// One-time invitation token and its persisted metadata.
pub struct InvitationResult {
    pub invitation_id: UserInvitationId,
    pub token: SecretString,
    pub expires_at: i64,
}

impl std::fmt::Debug for InvitationResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InvitationResult")
            .field("invitation_id", &self.invitation_id)
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Deliberately uniform public lifecycle response.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicLifecycleOutcome {
    Accepted,
}

/// Persistence boundary with atomic lifecycle operations.
#[async_trait]
pub trait UserLifecycleRepository: Send + Sync {
    async fn users(&self, limit: u32) -> Result<Vec<LifecycleUserRecord>, PortError>;
    async fn user(&self, id: UserId) -> Result<Option<LifecycleUserRecord>, PortError>;
    async fn create_user(
        &self,
        record: &LifecycleUserRecord,
    ) -> Result<LifecycleCreateOutcome, PortError>;
    async fn update_display_name(
        &self,
        id: UserId,
        display_name: &str,
        now: i64,
    ) -> Result<bool, PortError>;
    async fn create_invitation(&self, invitation: &InvitationRecord) -> Result<(), PortError>;
    async fn accept_invitation(
        &self,
        digest: &CredentialDigest,
        display_name: &str,
        password_hash: &PasswordHash,
        activated: bool,
        now: i64,
    ) -> Result<bool, PortError>;
    async fn activate_user(
        &self,
        id: UserId,
        email_verified_at: Option<i64>,
        now: i64,
    ) -> Result<bool, PortError>;
    async fn disable_user(&self, id: UserId, now: i64) -> Result<bool, PortError>;
    async fn unlock_user(&self, id: UserId, now: i64) -> Result<bool, PortError>;
    async fn delete_user(&self, id: UserId, now: i64) -> Result<bool, PortError>;
}

/// Object-safe local-user lifecycle use cases consumed by HTTP adapters.
#[async_trait]
pub trait UserLifecycleUseCases: Send + Sync {
    async fn own_profile(&self, id: UserId) -> Result<LifecycleUserRecord, UserLifecycleError>;
    async fn update_own_profile(
        &self,
        id: UserId,
        display_name: String,
    ) -> Result<LifecycleUserRecord, UserLifecycleError>;
    async fn users(
        &self,
        actor: AdminUserActor,
        limit: u32,
    ) -> Result<Vec<LifecycleUserRecord>, UserLifecycleError>;
    async fn create_user(
        &self,
        actor: AdminUserActor,
        command: CreateLocalUser,
    ) -> Result<LifecycleUserRecord, UserLifecycleError>;
    async fn invite_user(
        &self,
        actor: AdminUserActor,
        command: InviteLocalUser,
    ) -> Result<InvitationResult, UserLifecycleError>;
    async fn register(
        &self,
        command: RegisterLocalUser,
    ) -> Result<PublicLifecycleOutcome, UserLifecycleError>;
    async fn accept_invitation(
        &self,
        command: AcceptInvitation,
    ) -> Result<PublicLifecycleOutcome, UserLifecycleError>;
    async fn activate(
        &self,
        actor: AdminUserActor,
        id: UserId,
        email_verified: bool,
    ) -> Result<LifecycleUserRecord, UserLifecycleError>;
    async fn disable(
        &self,
        actor: AdminUserActor,
        id: UserId,
    ) -> Result<LifecycleUserRecord, UserLifecycleError>;
    async fn unlock(
        &self,
        actor: AdminUserActor,
        id: UserId,
    ) -> Result<LifecycleUserRecord, UserLifecycleError>;
    async fn delete(&self, actor: AdminUserActor, id: UserId) -> Result<(), UserLifecycleError>;
}

/// Local-user lifecycle service independent of storage and HTTP frameworks.
pub struct UserLifecycleService {
    repository: Arc<dyn UserLifecycleRepository>,
    credentials: Arc<dyn CredentialService>,
    clock: Arc<dyn Clock>,
    policy: LocalUserPolicy,
}

impl UserLifecycleService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn UserLifecycleRepository>,
        credentials: Arc<dyn CredentialService>,
        clock: Arc<dyn Clock>,
        policy: LocalUserPolicy,
    ) -> Self {
        Self {
            repository,
            credentials,
            clock,
            policy,
        }
    }

    fn now(&self) -> Result<i64, UserLifecycleError> {
        i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| UserLifecycleError::Internal("clock_out_of_range"))
    }

    async fn changed_user(&self, id: UserId) -> Result<LifecycleUserRecord, UserLifecycleError> {
        self.repository
            .user(id)
            .await?
            .ok_or(UserLifecycleError::NotFound)
    }
}

#[async_trait]
impl UserLifecycleUseCases for UserLifecycleService {
    async fn own_profile(&self, id: UserId) -> Result<LifecycleUserRecord, UserLifecycleError> {
        let user = self.changed_user(id).await?;
        if user.status != UserStatus::Active {
            return Err(UserLifecycleError::NotFound);
        }
        Ok(user)
    }

    async fn update_own_profile(
        &self,
        id: UserId,
        display_name: String,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        let display_name = normalize_display_name(&display_name)?;
        if !self
            .repository
            .update_display_name(id, &display_name, self.now()?)
            .await?
        {
            return Err(UserLifecycleError::NotFound);
        }
        self.changed_user(id).await
    }

    async fn users(
        &self,
        actor: AdminUserActor,
        limit: u32,
    ) -> Result<Vec<LifecycleUserRecord>, UserLifecycleError> {
        authorize(actor)?;
        if !(1..=500).contains(&limit) {
            return Err(UserLifecycleError::InvalidInput("limit"));
        }
        Ok(self.repository.users(limit).await?)
    }

    async fn create_user(
        &self,
        actor: AdminUserActor,
        command: CreateLocalUser,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        authorize(actor)?;
        let email = normalize_email(&command.email)?;
        let display_name = normalize_display_name(&command.display_name)?;
        if self.policy.require_verified_email && !command.email_verified {
            return Err(UserLifecycleError::EmailVerificationRequired);
        }
        let now = self.now()?;
        let record = LifecycleUserRecord {
            id: UserId::new(),
            email,
            display_name,
            status: UserStatus::Active,
            email_verified_at: command.email_verified.then_some(now),
            disabled_at: None,
            locked_until: None,
            created_at: now,
            updated_at: now,
        };
        match self.repository.create_user(&record).await? {
            LifecycleCreateOutcome::Created => Ok(record),
            LifecycleCreateOutcome::Existing => Err(UserLifecycleError::Conflict),
        }
    }

    async fn invite_user(
        &self,
        actor: AdminUserActor,
        command: InviteLocalUser,
    ) -> Result<InvitationResult, UserLifecycleError> {
        authorize(actor)?;
        let email = normalize_email(&command.email)?;
        let now = self.now()?;
        let expires_at = now
            .checked_add(i64::from(self.policy.invitation_lifetime_seconds) * 1_000)
            .ok_or(UserLifecycleError::Internal("invitation_expiry_overflow"))?;
        let token = SecretString::from(format!("{}.{}", Uuid::new_v4(), Uuid::new_v4()));
        let digest = self.credentials.digest_bearer(&token).await?;
        let id = UserInvitationId::new();
        self.repository
            .create_invitation(&InvitationRecord {
                id,
                email,
                token_digest: digest,
                invited_by: actor.user_id,
                expires_at,
                created_at: now,
            })
            .await?;
        Ok(InvitationResult {
            invitation_id: id,
            token,
            expires_at,
        })
    }

    async fn register(
        &self,
        command: RegisterLocalUser,
    ) -> Result<PublicLifecycleOutcome, UserLifecycleError> {
        if !self.policy.allow_self_registration {
            return Err(UserLifecycleError::RegistrationDisabled);
        }
        let email = normalize_email(&command.email)?;
        let display_name = normalize_display_name(&command.display_name)?;
        let now = self.now()?;
        let active = !self.policy.require_verified_email;
        let record = LifecycleUserRecord {
            id: UserId::new(),
            email,
            display_name,
            status: if active {
                UserStatus::Active
            } else {
                UserStatus::Invited
            },
            email_verified_at: None,
            disabled_at: None,
            locked_until: None,
            created_at: now,
            updated_at: now,
        };
        let _ = self.repository.create_user(&record).await?;
        Ok(PublicLifecycleOutcome::Accepted)
    }

    async fn accept_invitation(
        &self,
        command: AcceptInvitation,
    ) -> Result<PublicLifecycleOutcome, UserLifecycleError> {
        let display_name = normalize_display_name(&command.display_name)?;
        let password_length = command.password.expose_secret().chars().count();
        if !(usize::from(self.policy.password_minimum_length)
            ..=usize::from(self.policy.password_maximum_length))
            .contains(&password_length)
        {
            return Err(UserLifecycleError::InvalidInput("password"));
        }
        let digest = self.credentials.digest_bearer(&command.token).await?;
        let password_hash = self.credentials.hash_password(&command.password).await?;
        let now = self.now()?;
        // Possession of the single-use token delivered to the invited address verifies that
        // address, so invitation acceptance may activate under either email policy.
        let activated = true;
        let _ = self
            .repository
            .accept_invitation(&digest, &display_name, &password_hash, activated, now)
            .await?;
        Ok(PublicLifecycleOutcome::Accepted)
    }

    async fn activate(
        &self,
        actor: AdminUserActor,
        id: UserId,
        email_verified: bool,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        authorize(actor)?;
        if self.policy.require_verified_email && !email_verified {
            return Err(UserLifecycleError::EmailVerificationRequired);
        }
        let now = self.now()?;
        if !self
            .repository
            .activate_user(id, email_verified.then_some(now), now)
            .await?
        {
            return Err(UserLifecycleError::NotFound);
        }
        self.changed_user(id).await
    }

    async fn disable(
        &self,
        actor: AdminUserActor,
        id: UserId,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        authorize(actor)?;
        if actor.user_id == id {
            return Err(UserLifecycleError::SelfAdministrationDenied);
        }
        if !self.repository.disable_user(id, self.now()?).await? {
            return Err(UserLifecycleError::NotFound);
        }
        self.changed_user(id).await
    }

    async fn unlock(
        &self,
        actor: AdminUserActor,
        id: UserId,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        authorize(actor)?;
        if !self.repository.unlock_user(id, self.now()?).await? {
            return Err(UserLifecycleError::NotFound);
        }
        self.changed_user(id).await
    }

    async fn delete(&self, actor: AdminUserActor, id: UserId) -> Result<(), UserLifecycleError> {
        authorize(actor)?;
        if actor.user_id == id {
            return Err(UserLifecycleError::SelfAdministrationDenied);
        }
        if !self.repository.delete_user(id, self.now()?).await? {
            return Err(UserLifecycleError::NotFound);
        }
        Ok(())
    }
}

fn authorize(actor: AdminUserActor) -> Result<(), UserLifecycleError> {
    if actor.can_manage_users {
        Ok(())
    } else {
        Err(UserLifecycleError::Forbidden)
    }
}

fn normalize_email(value: &str) -> Result<String, UserLifecycleError> {
    let value = value.trim().to_lowercase();
    let valid = value.len() <= 254
        && value
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    if valid {
        Ok(value)
    } else {
        Err(UserLifecycleError::InvalidInput("email"))
    }
}

fn normalize_display_name(value: &str) -> Result<String, UserLifecycleError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 {
        Err(UserLifecycleError::InvalidInput("display_name"))
    } else {
        Ok(value.to_owned())
    }
}

/// Safe use-case failures suitable for adapter mapping.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UserLifecycleError {
    #[error("user administration is forbidden")]
    Forbidden,
    #[error("requested user was not found")]
    NotFound,
    #[error("user already exists")]
    Conflict,
    #[error("self-registration is disabled")]
    RegistrationDisabled,
    #[error("verified email is required for activation")]
    EmailVerificationRequired,
    #[error("administrators cannot disable or delete their own user through this operation")]
    SelfAdministrationDenied,
    #[error("invalid {0}")]
    InvalidInput(&'static str),
    #[error("local-user persistence failed")]
    Persistence,
    #[error("local-user lifecycle failed: {0}")]
    Internal(&'static str),
}

impl From<PortError> for UserLifecycleError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::NotFound => Self::NotFound,
            PortError::Conflict => Self::Conflict,
            PortError::Rejected(_) | PortError::Unavailable => Self::Persistence,
        }
    }
}
