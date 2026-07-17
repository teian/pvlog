use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, ModernRequestAuthorizer, RequestAuthorizationError, systems_router,
};
use pvlog_application::{
    CreateSystem, SystemLifecycleError, SystemLifecycleRecord, SystemLifecycleUseCases,
    UpdateSystem,
};
use pvlog_domain::{
    AccountId, Permission, PrincipalId, SystemId, SystemLifecycle, UserId, Visibility,
};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn system_mutations_require_actor_and_etag_preconditions() -> Result<(), Box<dyn Error>> {
    let actor = UserId::new();
    let app = systems_router(Arc::new(Stub), Arc::new(AllowAuthorizer { actor }))
        .layer(Extension(pvlog_api::RequestPrincipal::User(actor)));
    let system_id = SystemId::new();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/systems/{system_id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok()),
        Some("\"1\"")
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/systems")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Roof","timezone":"Europe/Berlin"}"#))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok()),
        Some("\"1\"")
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/systems/{system_id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"Roof","timezone":"Europe/Berlin","visibility":"private"}"#,
                ))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    Ok(())
}

struct Stub;
struct AllowAuthorizer {
    actor: UserId,
}

#[async_trait]
impl ModernRequestAuthorizer for AllowAuthorizer {
    async fn authorize_instance(
        &self,
        _principal: PrincipalId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        Ok(self.actor)
    }

    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Ok(AuthorizedRequest {
            actor_user_id: self.actor,
            account_id,
        })
    }

    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Ok(AuthorizedRequest {
            actor_user_id: self.actor,
            account_id: AccountId::new(),
        })
    }
}
fn record(
    id: SystemId,
    account_id: AccountId,
    name: String,
    timezone: String,
    version: u64,
) -> SystemLifecycleRecord {
    SystemLifecycleRecord {
        id,
        account_id,
        name,
        timezone,
        visibility: Visibility::Private,
        lifecycle: SystemLifecycle::Active,
        version,
        created_at: 0,
        updated_at: 0,
    }
}
#[async_trait]
impl SystemLifecycleUseCases for Stub {
    async fn system(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        Ok(record(
            id,
            AccountId::new(),
            "x".to_owned(),
            "UTC".to_owned(),
            1,
        ))
    }

    async fn create_system(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        Ok(record(
            SystemId::new(),
            request.account_id,
            request.name,
            request.timezone,
            1,
        ))
    }
    async fn update_system(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        Ok(record(
            request.id,
            AccountId::new(),
            request.name,
            request.timezone,
            request.expected_version + 1,
        ))
    }
    async fn archive_system(
        &self,
        id: SystemId,
        _actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        Ok(record(
            id,
            AccountId::new(),
            "x".to_owned(),
            "UTC".to_owned(),
            version + 1,
        ))
    }
    async fn restore_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.archive_system(id, actor, version).await
    }
    async fn delete_system(
        &self,
        _id: SystemId,
        _actor: UserId,
        _version: u64,
        _confirmed: bool,
    ) -> Result<(), SystemLifecycleError> {
        Ok(())
    }
}
