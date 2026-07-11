use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode},
};
use pvlog_api::{
    AuditApiError, AuditApiUseCases, AuditEventResponse, AuthorizedRequest,
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, audit_router,
};
use pvlog_domain::{AccountId, Permission, PrincipalId, SystemId, UserId};
use tower::ServiceExt as _;

#[tokio::test]
async fn account_audit_requires_auditor_permission() -> Result<(), Box<dyn Error>> {
    let account_id = AccountId::new();
    let app = audit_router(Arc::new(Audit), Arc::new(Authorizer { account_id }))
        .layer(Extension(RequestPrincipal::User(UserId::new())));
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{account_id}/audit-events?limit=999"
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

struct Audit;
#[async_trait]
impl AuditApiUseCases for Audit {
    async fn account_audit(
        &self,
        _account_id: AccountId,
        _limit: u32,
    ) -> Result<Vec<AuditEventResponse>, AuditApiError> {
        Ok(Vec::new())
    }
}

struct Authorizer {
    account_id: AccountId,
}
#[async_trait]
impl ModernRequestAuthorizer for Authorizer {
    async fn authorize_instance(
        &self,
        _principal: PrincipalId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        account_id: AccountId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        if account_id == self.account_id && permission == Permission::AuditRead {
            Ok(AuthorizedRequest {
                actor_user_id: UserId::new(),
                account_id,
            })
        } else {
            Err(RequestAuthorizationError::Forbidden)
        }
    }
    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
}
