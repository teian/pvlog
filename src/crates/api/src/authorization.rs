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
    /// Authorizes any active user of the account that owns `system_id`.
    async fn authorize_system_account_user(
        &self,
        _user_id: UserId,
        _system_id: SystemId,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
    async fn authorize_instance(
        &self,
        principal: PrincipalId,
        permission: Permission,
        action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError>;
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
///
/// # Errors
///
pub fn principal_identity(
    principal: &RequestPrincipal,
) -> Result<PrincipalId, RequestAuthorizationError> {
    match principal {
        RequestPrincipal::User(id) => Ok(PrincipalId::User(*id)),
        RequestPrincipal::ApiCredential { id, .. } => Ok(PrincipalId::ApiCredential(*id)),
    }
}

/// Returns the owning user used by existing application use cases after authorization.
///
/// # Errors
///
/// Returns [`RequestAuthorizationError::Forbidden`] when the principal has no owning user.
pub fn actor_user_id(principal: &RequestPrincipal) -> Result<UserId, RequestAuthorizationError> {
    match principal {
        RequestPrincipal::User(id) => Ok(*id),
        RequestPrincipal::ApiCredential { owner_user_id, .. } => Ok(*owner_user_id),
    }
}

/// Safe HTTP-level authorization result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestAuthorizationError {
    Forbidden,
    NotFound,
    Unavailable,
}
