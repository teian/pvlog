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

async fn post_system(
    app: axum::Router,
    body: &'static str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/systems")
                .header("content-type", "application/json")
                .body(Body::from(body))?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
async fn a_system_is_created_from_a_name_and_optional_setup_fields() -> Result<(), Box<dyn Error>> {
    let actor = UserId::new();
    let app = systems_router(Arc::new(Stub), Arc::new(AllowAuthorizer { actor }))
        .layer(Extension(pvlog_api::RequestPrincipal::User(actor)));

    let (status, body) = post_system(
        app.clone(),
        r#"{"name":"Roof","clientTimezone":"Europe/Vienna"}"#,
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["timezone"], "Europe/Vienna");
    assert_eq!(body["statusIntervalSeconds"], 300);
    assert!(body["declaredCapacityWatts"].is_null());

    let (status, body) = post_system(
        app.clone(),
        r#"{"name":"Roof","timezone":"Europe/Berlin","locationLabel":"Leipzig","countryCode":"DE",
            "latitudeMicrodegrees":51340000,"longitudeMicrodegrees":12370000,
            "statusIntervalSeconds":600,"declaredCapacityWatts":9900}"#,
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["locationLabel"], "Leipzig");
    assert_eq!(body["countryCode"], "DE");
    assert_eq!(body["latitudeMicrodegrees"], 51_340_000);
    assert_eq!(body["longitudeMicrodegrees"], 12_370_000);
    assert_eq!(body["statusIntervalSeconds"], 600);
    assert_eq!(body["declaredCapacityWatts"], 9_900);

    let (status, body) = post_system(app, r#"{"name":" ","clientTimezone":"UTC"}"#).await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"], "name_required");
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
        description: String::new(),
        commissioning_date: None,
        location_label: None,
        country_code: None,
        latitude_microdegrees: None,
        longitude_microdegrees: None,
        status_interval_seconds: 300,
        declared_capacity_watts: None,
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
        if request.name.trim().is_empty() {
            return Err(SystemLifecycleError::InvalidField("name_required"));
        }
        let mut created = record(
            SystemId::new(),
            request.account_id,
            request.name,
            request
                .timezone
                .or(request.client_timezone)
                .unwrap_or_default(),
            1,
        );
        created.location_label = request.details.location_label;
        created.country_code = request.details.country_code;
        created.latitude_microdegrees = request.details.latitude_microdegrees;
        created.longitude_microdegrees = request.details.longitude_microdegrees;
        created.declared_capacity_watts = request.details.declared_capacity_watts;
        if let Some(interval) = request.details.status_interval_seconds {
            created.status_interval_seconds = interval;
        }
        Ok(created)
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
