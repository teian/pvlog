use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, InverterApiError, InverterApiUseCases, InverterInput, InverterResponse,
    ModernRequestAuthorizer, PvStringResponse, RequestAuthorizationError, RequestPrincipal,
    inverters_router, managed_resources_router,
};
use pvlog_application::{
    CreateManagedResource, ManagedResource, ManagedResourceError, ManagedResourceKind,
    ManagedResourceService, ModernApiActor,
};
use pvlog_domain::{
    AccountId, ApiScope, InverterId, Permission, PrincipalId, StringId, SystemId, UserId,
};
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

#[tokio::test]
async fn inverter_routes_authorize_system_and_emit_nested_aggregate() -> Result<(), Box<dyn Error>>
{
    let account = AccountId::new();
    let system = SystemId::new();
    let user = UserId::new();
    let app = inverters_router(
        Arc::new(InverterStub { system }),
        Arc::new(Authorizer { account, user }),
    )
    .layer(Extension(RequestPrincipal::User(user)));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/accounts/{account}/systems/{system}/inverters"
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"Roof inverter","ratedPowerWatts":8000,"effectiveFrom":1,"strings":[{"name":"South roof","panelCount":20,"ratedPowerWatts":8000,"orientationDegrees":180,"tiltDegrees":35,"effectiveFrom":1}]}"#,
                ))?,
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

struct InverterStub {
    system: SystemId,
}
#[async_trait]
impl InverterApiUseCases for InverterStub {
    async fn list(
        &self,
        _account_id: AccountId,
        _system_id: SystemId,
        _at: i64,
    ) -> Result<Vec<InverterResponse>, InverterApiError> {
        Ok(Vec::new())
    }

    async fn create(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        system_id: SystemId,
        input: InverterInput,
    ) -> Result<InverterResponse, InverterApiError> {
        assert_eq!(system_id, self.system);
        let inverter_id = InverterId::new();
        Ok(InverterResponse {
            id: inverter_id,
            system_id,
            name: input.name,
            manufacturer: input.manufacturer,
            model: input.model,
            serial_reference: input.serial_reference,
            rated_power_watts: input.rated_power_watts,
            effective_from: input.effective_from,
            effective_to: input.effective_to,
            version: 1,
            strings: input
                .strings
                .into_iter()
                .map(|string| PvStringResponse {
                    id: StringId::new(),
                    inverter_id,
                    name: string.name,
                    panel_count: string.panel_count,
                    panel_manufacturer: string.panel_manufacturer,
                    panel_model: string.panel_model,
                    rated_power_watts: string.rated_power_watts,
                    orientation_degrees: string.orientation_degrees,
                    tilt_degrees: string.tilt_degrees,
                    effective_from: string.effective_from,
                    effective_to: string.effective_to,
                })
                .collect(),
        })
    }
}

struct Authorizer {
    account: AccountId,
    user: UserId,
}
#[async_trait]
impl ModernRequestAuthorizer for Authorizer {
    async fn authorize_instance(
        &self,
        _principal: PrincipalId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        Ok(self.user)
    }
    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        _account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Ok(AuthorizedRequest {
            actor_user_id: self.user,
            account_id: self.account,
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
            actor_user_id: self.user,
            account_id: self.account,
        })
    }
}
