//! Authorization-before-routing boundary with mandatory audit recording.

use crate::PortError;
use async_trait::async_trait;
use pvlog_domain::{AccountId, Permission, PrincipalId, RequestId, SystemId};
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedAccountRequest {
    pub principal: PrincipalId,
    pub account_id: AccountId,
    pub system_id: Option<SystemId>,
    pub permission: Permission,
    pub request_id: RequestId,
    pub action: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedAccountRoute {
    pub account_id: AccountId,
    pub opaque_route: String,
}

#[async_trait]
pub trait AuthorizationBoundaryPorts: Send + Sync {
    async fn is_authorized(&self, request: &ProtectedAccountRequest) -> Result<bool, PortError>;
    async fn account_route(
        &self,
        account_id: AccountId,
    ) -> Result<Option<AuthorizedAccountRoute>, PortError>;
    async fn append_audit(
        &self,
        request: &ProtectedAccountRequest,
        outcome: &'static str,
    ) -> Result<(), PortError>;
}

pub struct AuthorizationBoundary {
    ports: Arc<dyn AuthorizationBoundaryPorts>,
}
impl AuthorizationBoundary {
    #[must_use]
    pub fn new(ports: Arc<dyn AuthorizationBoundaryPorts>) -> Self {
        Self { ports }
    }
    /// Authorizes an account request before resolving its opaque storage route.
    ///
    /// # Errors
    /// Returns an error for denial, unavailable routing, audit failure, or port failure.
    pub async fn authorize_and_route(
        &self,
        request: &ProtectedAccountRequest,
    ) -> Result<AuthorizedAccountRoute, AuthorizationBoundaryError> {
        if !self
            .ports
            .is_authorized(request)
            .await
            .map_err(AuthorizationBoundaryError::Port)?
        {
            self.ports
                .append_audit(request, "denied")
                .await
                .map_err(AuthorizationBoundaryError::Port)?;
            return Err(AuthorizationBoundaryError::Forbidden);
        }
        let route = self
            .ports
            .account_route(request.account_id)
            .await
            .map_err(AuthorizationBoundaryError::Port)?
            .ok_or(AuthorizationBoundaryError::AccountUnavailable)?;
        self.ports
            .append_audit(request, "succeeded")
            .await
            .map_err(AuthorizationBoundaryError::Port)?;
        Ok(route)
    }
}

#[derive(Debug, Error)]
pub enum AuthorizationBoundaryError {
    #[error("access is forbidden")]
    Forbidden,
    #[error("authorized account storage is unavailable")]
    AccountUnavailable,
    #[error("authorization boundary persistence is unavailable")]
    Port(PortError),
}
