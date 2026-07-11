use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode},
};
use pvlog_api::{
    ConnectorAdminError, ConnectorAdminResponse, ConnectorAdminUseCases, ModernRequestAuthorizer,
    RequestAuthorizationError, RequestPrincipal, connectors_router,
};
use pvlog_domain::{AccountId, Permission, PrincipalId, SystemId, UserId};
use tower::ServiceExt as _;

#[tokio::test]
async fn connector_catalog_requires_instance_administration() -> Result<(), Box<dyn Error>> {
    let app = connectors_router(Arc::new(Connectors), Arc::new(Authorizer));
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/auth-connectors")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let allowed = app
        .layer(Extension(RequestPrincipal::User(UserId::new())))
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/auth-connectors")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(allowed.status(), StatusCode::OK);
    Ok(())
}

struct Connectors;

#[async_trait]
impl ConnectorAdminUseCases for Connectors {
    async fn connectors(&self) -> Result<Vec<ConnectorAdminResponse>, ConnectorAdminError> {
        Ok(vec![ConnectorAdminResponse {
            id: "company-sso".to_owned(),
            display_name: "Company SSO".to_owned(),
            protocol: "oidc".to_owned(),
            enabled: true,
            authorization_endpoint: Some("https://identity.example/authorize".to_owned()),
            scopes: vec!["openid".to_owned()],
        }])
    }
}

struct Authorizer;

#[async_trait]
impl ModernRequestAuthorizer for Authorizer {
    async fn authorize_instance(
        &self,
        principal: PrincipalId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        let PrincipalId::User(user_id) = principal else {
            return Err(RequestAuthorizationError::Forbidden);
        };
        (permission == Permission::InstanceManage)
            .then_some(user_id)
            .ok_or(RequestAuthorizationError::Forbidden)
    }

    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        _account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<pvlog_api::AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }

    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<pvlog_api::AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
}
