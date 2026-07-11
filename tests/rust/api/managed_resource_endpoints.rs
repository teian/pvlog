use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::managed_resources_router;
use pvlog_application::{
    CreateManagedResource, ManagedResource, ManagedResourceError, ManagedResourceKind,
    ManagedResourceService, ModernApiActor,
};
use pvlog_domain::{AccountId, ApiScope, SystemId, UserId};
use std::{collections::BTreeSet, error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn all_managed_resource_routes_require_scopes_and_emit_etags() -> Result<(), Box<dyn Error>> {
    let account = AccountId::new();
    let system = SystemId::new();
    let actor = ModernApiActor {
        user_id: UserId::new(),
        scopes: BTreeSet::from([ApiScope::SystemsRead, ApiScope::SystemsWrite]),
    };
    let app = managed_resources_router(Arc::new(Stub)).layer(Extension(actor));
    for path in [
        format!("/api/v1/accounts/{account}/systems/{system}/equipment"),
        format!("/api/v1/accounts/{account}/systems/{system}/tariffs"),
        format!("/api/v1/accounts/{account}/systems/{system}/channels"),
        format!("/api/v1/accounts/{account}/memberships"),
        format!("/api/v1/accounts/{account}/credentials"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":"example"}"#))?,
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
    }
    let forbidden = managed_resources_router(Arc::new(Stub))
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/accounts/{account}/memberships"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    Ok(())
}

struct Stub;
#[async_trait]
impl ManagedResourceService for Stub {
    async fn list(
        &self,
        _actor: &ModernApiActor,
        _account: AccountId,
        _system: Option<SystemId>,
        _kind: ManagedResourceKind,
    ) -> Result<Vec<ManagedResource>, ManagedResourceError> {
        Ok(Vec::new())
    }
    async fn create(
        &self,
        _actor: &ModernApiActor,
        command: CreateManagedResource,
    ) -> Result<ManagedResource, ManagedResourceError> {
        Ok(ManagedResource {
            id: uuid::Uuid::now_v7(),
            account_id: command.account_id,
            system_id: command.system_id,
            kind: command.kind,
            version: 1,
            attributes: command.attributes,
        })
    }
}
