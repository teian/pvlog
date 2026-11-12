//! Device events are uploaded by whoever may write telemetry and listed by
//! whoever may read it.

use std::{
    collections::BTreeSet,
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::{
    AuthorizedRequest, DeviceEventApiError, DeviceEventApiUseCases, DeviceEventDetails,
    DeviceEventInput, DeviceEventQuery, DeviceEventResponse, DeviceEventsAccepted,
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, device_events_router,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, Permission, PrincipalId, SystemId, UserId,
};
use tower::ServiceExt as _;
use uuid::Uuid;

/// What the service was asked to do.
#[derive(Clone, Debug, PartialEq)]
enum Call {
    Store(Option<ApiCredentialId>, Vec<DeviceEventInput>),
    List(Option<Uuid>, DeviceEventQuery),
}

type Calls = Arc<Mutex<Vec<Call>>>;

fn app(principal: RequestPrincipal, calls: &Calls) -> Router {
    device_events_router(Arc::new(Stub(calls.clone())), Arc::new(Authorizer))
        .layer(Extension(principal))
}

fn api_key(scope: ApiScope, system_id: Option<SystemId>) -> (ApiCredentialId, RequestPrincipal) {
    let id = ApiCredentialId::new();
    (
        id,
        RequestPrincipal::ApiCredential {
            id,
            owner_user_id: UserId::new(),
            account_id: AccountId::new(),
            system_id,
            scopes: BTreeSet::from([scope]),
        },
    )
}

async fn send(
    app: Router,
    method: Method,
    uri: String,
    body: Option<&str>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_owned())))?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 65_536).await?;
    Ok((
        status,
        if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        },
    ))
}

#[tokio::test]
async fn an_upload_is_checked_trimmed_and_handed_to_the_service() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let calls = Calls::default();
    let uri = format!("/api/v1/systems/{system}/device-events");
    let (credential, key) = api_key(ApiScope::TelemetryWrite, Some(system));

    let (status, body) = send(
        app(key, &calls),
        Method::POST,
        uri.clone(),
        Some(
            r#"{"events":[
                {"deviceId":" 2000123456 ","eventId":"8812","occurredAtEpochMillis":1780000000000,
                 "code":3501,"message":" Insulation failure ","type":"Event","category":" Error ",
                 "group":"Insulation","oldValue":"230.0 V","newValue":"235.0 V",
                 "userGroup":"Installer"},
                {"deviceId":"2000123456","eventId":"  ","occurredAtEpochMillis":1780000300000,
                 "message":"","category":" "}]}"#,
        ),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({"stored": 2, "unchanged": 0}));
    assert_eq!(
        calls.lock().map_err(|_| "poisoned")?.clone(),
        vec![Call::Store(
            Some(credential),
            vec![
                DeviceEventInput {
                    device_id: "2000123456".to_owned(),
                    event_id: Some("8812".to_owned()),
                    occurred_at_epoch_millis: 1_780_000_000_000,
                    code: Some(3_501),
                    message: Some("Insulation failure".to_owned()),
                    details: DeviceEventDetails {
                        event_type: Some("Event".to_owned()),
                        category: Some("Error".to_owned()),
                        group: Some("Insulation".to_owned()),
                        old_value: Some("230.0 V".to_owned()),
                        new_value: Some("235.0 V".to_owned()),
                        user_group: Some("Installer".to_owned()),
                    },
                },
                // A blank identifier and a blank message are absent.
                DeviceEventInput {
                    device_id: "2000123456".to_owned(),
                    event_id: None,
                    occurred_at_epoch_millis: 1_780_000_300_000,
                    code: None,
                    message: None,
                    details: DeviceEventDetails::default(),
                },
            ]
        )]
    );

    // A signed-in user uploads without a key.
    let (status, _) = send(
        app(RequestPrincipal::User(UserId::new()), &calls),
        Method::POST,
        uri.clone(),
        Some(r#"{"events":[{"deviceId":"a","occurredAtEpochMillis":0}]}"#),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let long_message = "m".repeat(501);
    let too_many = format!(
        r#"{{"events":[{}]}}"#,
        vec![r#"{"deviceId":"a","occurredAtEpochMillis":0}"#; 1_001].join(",")
    );
    let oversized = format!(
        r#"{{"events":[{{"deviceId":"a","occurredAtEpochMillis":0,"message":"{long_message}"}}]}}"#
    );
    for (malformed, field) in [
        (r#"{"events":[]}"#, "events"),
        (too_many.as_str(), "events"),
        (
            r#"{"events":[{"deviceId":"  ","occurredAtEpochMillis":0}]}"#,
            "events.deviceId",
        ),
        (
            r#"{"events":[{"deviceId":"a","occurredAtEpochMillis":-1}]}"#,
            "events.occurredAtEpochMillis",
        ),
        (oversized.as_str(), "events.message"),
    ] {
        let (_, key) = api_key(ApiScope::TelemetryWrite, None);
        let (status, body) =
            send(app(key, &calls), Method::POST, uri.clone(), Some(malformed)).await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        assert_eq!(body["detail"], field);
    }
    assert_eq!(calls.lock().map_err(|_| "poisoned")?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_key_needs_the_right_scope_and_system() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let calls = Calls::default();
    let uri = format!("/api/v1/systems/{system}/device-events");
    let upload = r#"{"events":[{"deviceId":"a","occurredAtEpochMillis":0}]}"#;

    for principal in [
        api_key(ApiScope::TelemetryRead, None).1,
        api_key(ApiScope::TelemetryWrite, Some(SystemId::new())).1,
    ] {
        let (status, _) = send(
            app(principal, &calls),
            Method::POST,
            uri.clone(),
            Some(upload),
        )
        .await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _) = send(
        app(api_key(ApiScope::TelemetryWrite, None).1, &calls),
        Method::GET,
        uri.clone(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN, "a write key cannot list");
    assert!(calls.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn events_are_listed_per_system_and_per_inverter() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let inverter = Uuid::now_v7();
    let calls = Calls::default();
    let reader = || api_key(ApiScope::TelemetryRead, None).1;

    let (status, body) = send(
        app(reader(), &calls),
        Method::GET,
        format!("/api/v1/systems/{system}/device-events?deviceId=2000123456&from=10&to=20&limit=5"),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["deviceId"], "2000123456");
    assert_eq!(body[0]["eventId"], "8812");
    assert_eq!(body[0]["occurredAtEpochMillis"], 15);
    // Old and new value are returned; a detail the device did not report is absent.
    assert_eq!(body[0]["oldValue"], "230.0 V");
    assert_eq!(body[0]["newValue"], "235.0 V");
    assert!(body[0].get("userGroup").is_none());

    let (status, _) = send(
        app(reader(), &calls),
        Method::GET,
        format!("/api/v1/systems/{system}/inverters/{inverter}/device-events"),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        calls.lock().map_err(|_| "poisoned")?.clone(),
        vec![
            Call::List(
                None,
                DeviceEventQuery {
                    device_id: Some("2000123456".to_owned()),
                    from: Some(10),
                    to: Some(20),
                    limit: Some(5),
                }
            ),
            // Without a limit the default applies.
            Call::List(
                Some(inverter),
                DeviceEventQuery {
                    limit: Some(100),
                    ..DeviceEventQuery::default()
                }
            ),
        ]
    );

    for (query, field) in [
        ("limit=0", "limit"),
        ("limit=1001", "limit"),
        ("from=20&to=20", "to"),
    ] {
        let (status, body) = send(
            app(reader(), &calls),
            Method::GET,
            format!("/api/v1/systems/{system}/device-events?{query}"),
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
        assert_eq!(body["detail"], field);
    }
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
        action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        // Uploading needs the write permission, listing the read permission.
        assert_eq!(
            permission,
            if action.ends_with(".store") {
                Permission::TelemetryWrite
            } else {
                Permission::TelemetryRead
            }
        );
        Ok(AuthorizedRequest {
            actor_user_id: UserId::new(),
            account_id: AccountId::new(),
        })
    }
}

struct Stub(Calls);

impl Stub {
    fn record(&self, call: Call) -> Result<(), DeviceEventApiError> {
        self.0
            .lock()
            .map_err(|_| DeviceEventApiError::Unavailable)?
            .push(call);
        Ok(())
    }
}

#[async_trait]
impl DeviceEventApiUseCases for Stub {
    async fn store(
        &self,
        _system_id: SystemId,
        credential_id: Option<ApiCredentialId>,
        events: Vec<DeviceEventInput>,
    ) -> Result<DeviceEventsAccepted, DeviceEventApiError> {
        let stored = u64::try_from(events.len()).unwrap_or_default();
        self.record(Call::Store(credential_id, events))?;
        Ok(DeviceEventsAccepted {
            stored,
            unchanged: 0,
        })
    }

    async fn list(
        &self,
        _system_id: SystemId,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError> {
        self.record(Call::List(None, query))?;
        Ok(vec![DeviceEventResponse {
            id: Uuid::now_v7(),
            device_id: "2000123456".to_owned(),
            event_id: Some("8812".to_owned()),
            occurred_at_epoch_millis: 15,
            code: Some(3_501),
            message: None,
            details: DeviceEventDetails {
                old_value: Some("230.0 V".to_owned()),
                new_value: Some("235.0 V".to_owned()),
                ..DeviceEventDetails::default()
            },
            data_source_id: None,
            received_at_epoch_millis: 16,
        }])
    }

    async fn list_for_inverter(
        &self,
        _system_id: SystemId,
        inverter_id: Uuid,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError> {
        self.record(Call::List(Some(inverter_id), query))?;
        Ok(Vec::new())
    }
}
