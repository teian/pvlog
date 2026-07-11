//! Shared authorization contract for modern HTTP handlers.

use async_trait::async_trait;
use pvlog_domain::{AccountId, Permission, PrincipalId, SystemId, UserId};

use crate::RequestPrincipal;

/// Principal accepted by a handler after scope and RBAC checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizedRequest {
    pub actor_user_id: UserId,
    pub account_id: AccountId,
}

/// Runtime authorization service for account- and system-addressed API requests.
#[async_trait]
pub trait ModernRequestAuthorizer: Send + Sync {
    async fn authorize_account(
        &self,
        principal: PrincipalId,
        account_id: AccountId,
        permission: Permission,
        action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError>;
    async fn authorize_system(
        &self,
        principal: PrincipalId,
        system_id: SystemId,
        permission: Permission,
        action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError>;
}

/// Converts an extracted credential to its auditable RBAC identity.
#[must_use]
pub fn principal_identity(principal: &RequestPrincipal) -> PrincipalId {
    match principal {
        RequestPrincipal::User(id) => PrincipalId::User(*id),
        RequestPrincipal::ApiCredential { id, .. } => PrincipalId::ApiCredential(*id),
    }
}

/// Returns the owning user used by existing application use cases after authorization.
#[must_use]
pub fn actor_user_id(principal: &RequestPrincipal) -> UserId {
    match principal {
        RequestPrincipal::User(id) => *id,
        RequestPrincipal::ApiCredential { owner_user_id, .. } => *owner_user_id,
    }
}

/// Safe HTTP-level authorization result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestAuthorizationError {
    Forbidden,
    NotFound,
    Unavailable,
}
