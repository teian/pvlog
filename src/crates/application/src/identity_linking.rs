//! Provider-neutral external identity provisioning and explicit linking.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_domain::{ConnectorId, ExternalIdentityId, UserId};
use thiserror::Error;

use crate::{Clock, IdentityClaims, PortError};

/// Persisted identity link, uniquely keyed by connector and immutable subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedIdentityRecord {
    pub id: ExternalIdentityId,
    pub connector_id: ConnectorId,
    pub subject: String,
    pub user_id: UserId,
    pub linked_at_epoch_millis: i64,
    pub last_login_at_epoch_millis: Option<i64>,
}

/// Policy for just-in-time provisioning from a successful external callback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExternalLoginPolicy {
    pub allow_just_in_time_provisioning: bool,
}

/// Result of resolving a verified external callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalLoginOutcome {
    ExistingUser(UserId),
    ProvisionedUser(UserId),
    UnlinkedIdentity,
}

/// Explicit account-linking request; the caller must have recently reauthenticated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkExternalIdentity {
    pub user_id: UserId,
    pub connector_id: ConnectorId,
    pub claims: IdentityClaims,
    pub recently_reauthenticated: bool,
}

/// Explicit account-unlinking request; the caller must have recently reauthenticated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnlinkExternalIdentity {
    pub user_id: UserId,
    pub identity_id: ExternalIdentityId,
    pub recently_reauthenticated: bool,
}

/// Persistence and audit boundary for external identity lifecycle operations.
#[async_trait]
pub trait ExternalIdentityLinkingRepository: Send + Sync {
    async fn find_by_connector_subject(
        &self,
        connector_id: ConnectorId,
        subject: &str,
    ) -> Result<Option<LinkedIdentityRecord>, PortError>;
    async fn create_user_from_external_claims(
        &self,
        claims: &IdentityClaims,
        now_epoch_millis: i64,
    ) -> Result<UserId, PortError>;
    async fn link(&self, identity: LinkedIdentityRecord) -> Result<(), PortError>;
    async fn touch_login(
        &self,
        identity_id: ExternalIdentityId,
        now_epoch_millis: i64,
    ) -> Result<(), PortError>;
    async fn find_for_user(
        &self,
        identity_id: ExternalIdentityId,
        user_id: UserId,
    ) -> Result<Option<LinkedIdentityRecord>, PortError>;
    async fn has_local_login(&self, user_id: UserId) -> Result<bool, PortError>;
    async fn external_identity_count(&self, user_id: UserId) -> Result<u32, PortError>;
    async fn unlink(&self, identity_id: ExternalIdentityId) -> Result<(), PortError>;
    async fn audit(
        &self,
        user_id: UserId,
        action: &'static str,
        now_epoch_millis: i64,
    ) -> Result<(), PortError>;
}

/// External identity use cases called after protocol validation, never by raw callback input.
#[async_trait]
pub trait ExternalIdentityLinkingUseCases: Send + Sync {
    async fn resolve_external_login(
        &self,
        connector_id: ConnectorId,
        claims: IdentityClaims,
    ) -> Result<ExternalLoginOutcome, ExternalIdentityLinkingError>;
    async fn link_external_identity(
        &self,
        request: LinkExternalIdentity,
    ) -> Result<(), ExternalIdentityLinkingError>;
    async fn unlink_external_identity(
        &self,
        request: UnlinkExternalIdentity,
    ) -> Result<(), ExternalIdentityLinkingError>;
}

/// Provider-neutral external identity orchestration service.
pub struct ExternalIdentityLinkingService {
    repository: Arc<dyn ExternalIdentityLinkingRepository>,
    clock: Arc<dyn Clock>,
    policy: ExternalLoginPolicy,
}

impl ExternalIdentityLinkingService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn ExternalIdentityLinkingRepository>,
        clock: Arc<dyn Clock>,
        policy: ExternalLoginPolicy,
    ) -> Self {
        Self {
            repository,
            clock,
            policy,
        }
    }

    fn now(&self) -> Result<i64, ExternalIdentityLinkingError> {
        i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| ExternalIdentityLinkingError::Time)
    }

    fn valid_subject(subject: &str) -> bool {
        !subject.trim().is_empty()
    }
}

#[async_trait]
impl ExternalIdentityLinkingUseCases for ExternalIdentityLinkingService {
    async fn resolve_external_login(
        &self,
        connector_id: ConnectorId,
        claims: IdentityClaims,
    ) -> Result<ExternalLoginOutcome, ExternalIdentityLinkingError> {
        if !Self::valid_subject(&claims.subject) {
            return Err(ExternalIdentityLinkingError::InvalidSubject);
        }
        let now = self.now()?;
        if let Some(identity) = self
            .repository
            .find_by_connector_subject(connector_id, &claims.subject)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?
        {
            self.repository
                .touch_login(identity.id, now)
                .await
                .map_err(ExternalIdentityLinkingError::Repository)?;
            self.repository
                .audit(identity.user_id, "external_identity.login", now)
                .await
                .map_err(ExternalIdentityLinkingError::Repository)?;
            return Ok(ExternalLoginOutcome::ExistingUser(identity.user_id));
        }
        if !self.policy.allow_just_in_time_provisioning {
            return Ok(ExternalLoginOutcome::UnlinkedIdentity);
        }
        let user_id = self
            .repository
            .create_user_from_external_claims(&claims, now)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        self.repository
            .link(LinkedIdentityRecord {
                id: ExternalIdentityId::new(),
                connector_id,
                subject: claims.subject,
                user_id,
                linked_at_epoch_millis: now,
                last_login_at_epoch_millis: Some(now),
            })
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        self.repository
            .audit(user_id, "external_identity.provisioned", now)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        Ok(ExternalLoginOutcome::ProvisionedUser(user_id))
    }

    async fn link_external_identity(
        &self,
        request: LinkExternalIdentity,
    ) -> Result<(), ExternalIdentityLinkingError> {
        if !request.recently_reauthenticated {
            return Err(ExternalIdentityLinkingError::RecentReauthenticationRequired);
        }
        if !Self::valid_subject(&request.claims.subject) {
            return Err(ExternalIdentityLinkingError::InvalidSubject);
        }
        let now = self.now()?;
        if let Some(existing) = self
            .repository
            .find_by_connector_subject(request.connector_id, &request.claims.subject)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?
        {
            return if existing.user_id == request.user_id {
                Ok(())
            } else {
                Err(ExternalIdentityLinkingError::IdentityAlreadyLinked)
            };
        }
        self.repository
            .link(LinkedIdentityRecord {
                id: ExternalIdentityId::new(),
                connector_id: request.connector_id,
                subject: request.claims.subject,
                user_id: request.user_id,
                linked_at_epoch_millis: now,
                last_login_at_epoch_millis: None,
            })
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        self.repository
            .audit(request.user_id, "external_identity.linked", now)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)
    }

    async fn unlink_external_identity(
        &self,
        request: UnlinkExternalIdentity,
    ) -> Result<(), ExternalIdentityLinkingError> {
        if !request.recently_reauthenticated {
            return Err(ExternalIdentityLinkingError::RecentReauthenticationRequired);
        }
        let identity = self
            .repository
            .find_for_user(request.identity_id, request.user_id)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?
            .ok_or(ExternalIdentityLinkingError::IdentityNotFound)?;
        let has_local = self
            .repository
            .has_local_login(request.user_id)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        let identity_count = self
            .repository
            .external_identity_count(request.user_id)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        if !has_local && identity_count <= 1 {
            return Err(ExternalIdentityLinkingError::FinalLoginMethod);
        }
        let now = self.now()?;
        self.repository
            .unlink(identity.id)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)?;
        self.repository
            .audit(request.user_id, "external_identity.unlinked", now)
            .await
            .map_err(ExternalIdentityLinkingError::Repository)
    }
}

#[derive(Debug, Error)]
pub enum ExternalIdentityLinkingError {
    #[error("external identity subject is invalid")]
    InvalidSubject,
    #[error("recent reauthentication is required")]
    RecentReauthenticationRequired,
    #[error("external identity is already linked to another user")]
    IdentityAlreadyLinked,
    #[error("external identity was not found for this user")]
    IdentityNotFound,
    #[error("cannot remove the final viable login method")]
    FinalLoginMethod,
    #[error("clock value is invalid")]
    Time,
    #[error("external identity persistence is unavailable")]
    Repository(PortError),
}
