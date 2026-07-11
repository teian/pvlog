use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, ModernRequestAuthorizer, RbacApiError, RbacApiUseCases,
    RequestAuthorizationError, RequestPrincipal, RoleInput, RoleResponse, rbac_router,
};
use pvlog_domain::{AccountId, Permission, PrincipalId, RoleId, SystemId, UserId};
use tower::ServiceExt as _;

#[tokio::test]
async fn role_catalog_requires_role_manage_permission() -> Result<(), Box<dyn Error>> {
    let account_id = AccountId::new();
    let app = rbac_router(Arc::new(Roles), Arc::new(Authorizer { account_id }))
        .layer(Extension(RequestPrincipal::User(UserId::new())));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/accounts/{account_id}/roles"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/accounts/{account_id}/roles"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"operator","permissions":["role_manage"],"parentRoleIds":[]}"#,
                ))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    Ok(())
}

struct Roles;
#[async_trait]
impl RbacApiUseCases for Roles {
    async fn roles(&self, _account_id: AccountId) -> Result<Vec<RoleResponse>, RbacApiError> {
        Ok(vec![RoleResponse {
            id: RoleId::new(),
            name: "operator".to_owned(),
            kind: "custom".to_owned(),
            permissions: BTreeSet::from([Permission::RoleManage]),
            parent_role_ids: BTreeSet::new(),
            version: 1,
            created_at: 0,
            updated_at: 0,
        }])
    }
    async fn create_role(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        _input: RoleInput,
    ) -> Result<RoleResponse, RbacApiError> {
        Ok(RoleResponse {
            id: RoleId::new(),
            name: "operator".to_owned(),
            kind: "custom".to_owned(),
            permissions: BTreeSet::from([Permission::RoleManage]),
            parent_role_ids: BTreeSet::new(),
            version: 1,
            created_at: 0,
            updated_at: 0,
        })
    }
    async fn update_role(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        _role_id: RoleId,
        _input: RoleInput,
    ) -> Result<RoleResponse, RbacApiError> {
        Err(RbacApiError::Unavailable)
    }
    async fn delete_role(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        _role_id: RoleId,
    ) -> Result<(), RbacApiError> {
        Err(RbacApiError::Unavailable)
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
        if account_id == self.account_id && permission == Permission::RoleManage {
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
