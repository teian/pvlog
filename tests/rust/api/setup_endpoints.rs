//! Setup status and first data are readable by whoever may read the system.

use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, DataState, FirstDataResponse, ModernRequestAuthorizer,
    RequestAuthorizationError, RequestPrincipal, SetupApiError, SetupApiUseCases,
    SetupStatusResponse, VerifiedReading, setup_router,
};
use pvlog_application::{SetupFacts, setup_status};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, DataSourceId, Permission, PrincipalId, SystemId, UserId,
};
use tower::ServiceExt as _;

const NOW: i64 = 1_800_000_000_000;

#[test]
fn a_system_without_a_reading_has_no_data_whatever_the_clock_says() {
    assert_eq!(DataState::at(None, 300, NOW), DataState::None);
    assert_eq!(DataState::at(None, 300, 0), DataState::None);
}

#[test]
fn data_turns_stale_after_three_status_intervals() {
    assert_eq!(DataState::at(Some(NOW), 300, NOW), DataState::Current);
    assert_eq!(
        DataState::at(Some(NOW - 900_000), 300, NOW),
        DataState::Current
    );
    assert_eq!(
        DataState::at(Some(NOW - 900_001), 300, NOW),
        DataState::Stale
    );
    assert_eq!(
        DataState::at(Some(NOW - 900_001), 3_600, NOW),
        DataState::Current,
        "a slower reporting interval tolerates a longer silence"
    );
}

fn app(principal: RequestPrincipal) -> Router {
    setup_router(Arc::new(Stub), Arc::new(Authorizer)).layer(Extension(principal))
}

fn api_key(scope: ApiScope) -> RequestPrincipal {
    RequestPrincipal::ApiCredential {
        id: ApiCredentialId::new(),
        owner_user_id: UserId::new(),
        account_id: AccountId::new(),
        system_id: None,
        scopes: BTreeSet::from([scope]),
    }
}

async fn get(app: Router, uri: String) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
async fn the_status_lists_each_item_with_its_state_and_reason() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let (status, body) = get(
        app(RequestPrincipal::User(UserId::new())),
        format!("/api/v1/systems/{system}/setup-status"),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["systemId"], system.to_string());
    assert_eq!(body["dataState"], "none");
    assert!(body["lastReceivedAtEpochMillis"].is_null());
    assert_eq!(body["items"][2]["key"], "data_source");
    assert_eq!(body["items"][2]["state"], "recommended");
    assert_eq!(body["items"][2]["reason"], "data_source_missing");
    assert_eq!(body["items"][3]["key"], "first_data");
    assert_eq!(body["items"][3]["state"], "blocked");
    assert_eq!(body["items"][4]["key"], "channels");
    assert_eq!(body["items"][4]["state"], "optional");
    assert_eq!(body["items"][4]["count"], 2);
    assert!(
        body["items"][0].get("count").is_none(),
        "a count appears only where it means something"
    );
    Ok(())
}

#[tokio::test]
async fn first_data_shows_the_reading_and_its_source() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let (status, body) = get(
        app(api_key(ApiScope::SystemsRead)),
        format!("/api/v1/systems/{system}/first-data"),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["received"], false);
    assert_eq!(body["reading"]["test"], true);
    assert_eq!(body["reading"]["dataSourceName"], "Roof logger");
    assert_eq!(body["reading"]["generationPowerWatts"], 100);
    assert_eq!(body["reading"]["message"], "Self-test");
    Ok(())
}

#[tokio::test]
async fn an_upload_key_cannot_read_the_setup() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    for path in ["setup-status", "first-data"] {
        let (status, _) = get(
            app(api_key(ApiScope::TelemetryWrite)),
            format!("/api/v1/systems/{system}/{path}"),
        )
        .await?;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
    let anonymous = setup_router(Arc::new(Stub), Arc::new(Authorizer));
    let (status, _) = get(anonymous, format!("/api/v1/systems/{system}/setup-status")).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    Ok(())
}

struct Authorizer;

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
        _account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }

    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        assert_eq!(permission, Permission::SystemRead);
        Ok(AuthorizedRequest {
            actor_user_id: UserId::new(),
            account_id: AccountId::new(),
        })
    }
}

struct Stub;

#[async_trait]
impl SetupApiUseCases for Stub {
    async fn status(
        &self,
        _account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SetupStatusResponse, SetupApiError> {
        Ok(SetupStatusResponse {
            system_id,
            items: setup_status(&SetupFacts {
                unassigned_channels: 2,
                ..SetupFacts::default()
            }),
            data_state: DataState::None,
            last_received_at_epoch_millis: None,
        })
    }

    async fn first_data(
        &self,
        _account_id: AccountId,
        system_id: SystemId,
    ) -> Result<FirstDataResponse, SetupApiError> {
        Ok(FirstDataResponse {
            system_id,
            received: false,
            reading: Some(VerifiedReading {
                id: uuid::Uuid::now_v7(),
                measured_at_epoch_millis: NOW,
                received_at_epoch_millis: NOW,
                generation_power_watts: Some(100),
                generation_energy_wh: None,
                consumption_power_watts: None,
                consumption_energy_wh: None,
                message: Some("Self-test".to_owned()),
                data_source_id: Some(DataSourceId::new()),
                data_source_name: Some("Roof logger".to_owned()),
                test: true,
            }),
        })
    }
}
