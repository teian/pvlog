//! The ingestion API checks a request completely before it reaches the service.

use std::{
    collections::BTreeSet,
    error::Error,
    io::Write as _,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::{
    AuthorizedRequest, IngestApiError, IngestApiUseCases, IngestCounts, IngestMark, IngestMarks,
    IngestResponse, IngestWriter, ModernRequestAuthorizer, RequestAuthorizationError,
    RequestPrincipal, ingest_router,
};
use pvlog_application::IngestRequest;
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, Permission, PrincipalId, SystemId, UserId,
};
use serde_json::{Value, json};
use tower::ServiceExt as _;

/// The requests that reached the service, with who sent them.
type Received = Arc<Mutex<Vec<(IngestWriter, IngestRequest)>>>;

fn app(principal: Option<RequestPrincipal>, received: &Received) -> Router {
    let router = ingest_router(Arc::new(Stub(received.clone())), Arc::new(Authorizer));
    match principal {
        Some(principal) => router.layer(Extension(principal)),
        None => router,
    }
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

fn user() -> Option<RequestPrincipal> {
    Some(RequestPrincipal::User(UserId::new()))
}

async fn send(
    app: Router,
    method: Method,
    uri: String,
    encoding: Option<&str>,
    body: Vec<u8>,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(encoding) = encoding {
        request = request.header(header::CONTENT_ENCODING, encoding);
    }
    let response = app.oneshot(request.body(Body::from(body))?).await?;
    let status = response.status();
    let problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value == "application/problem+json");
    assert_eq!(
        problem,
        !status.is_success(),
        "errors are problem documents"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

/// Posts a request body as a signed-in user.
async fn post(
    received: &Received,
    system: SystemId,
    body: &Value,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    send(
        app(user(), received),
        Method::POST,
        format!("/api/v1/systems/{system}/ingest"),
        None,
        serde_json::to_vec(body)?,
    )
    .await
}

fn cycle() -> Value {
    json!({
        "schemaVersion": 1,
        "source": { "name": "smalog", "version": "0.3.0" },
        "devices": [{ "id": "2000123456", "model": "SB 3000TL-21" }],
        "measurements": [{
            "measuredAt": 1_800_000_000,
            "devices": [{
                "id": "2000123456",
                "ac": [{ "phase": 1, "powerW": 800 }],
                "battery": { "stateOfChargePermille": 815 },
            }],
        }],
        "events": [{ "deviceId": "2000123456", "eventId": 8812, "occurredAt": 1_800_000_000 }],
    })
}

#[tokio::test]
async fn a_checked_request_reaches_the_service_with_its_writer() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let received = Received::default();
    let (credential, key) = api_key(ApiScope::TelemetryWrite, Some(system));
    let (status, body) = send(
        app(Some(key), &received),
        Method::POST,
        format!("/api/v1/systems/{system}/ingest"),
        None,
        serde_json::to_vec(&cycle())?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    // The answer counts each list and carries the marks to continue from.
    assert_eq!(
        body["measurements"],
        json!({ "stored": 1, "unchanged": 0, "rejected": [] })
    );
    assert_eq!(body["events"]["unchanged"], 1);
    assert_eq!(
        body["marks"],
        json!({
            "system": { "measuredAt": 1_800_000_000 },
            "devices": [{ "deviceId": "2000123456", "measuredAt": 1_800_000_000 }],
        })
    );
    let received = received.lock().map_err(|_| "poisoned")?;
    let (writer, request) = received.first().ok_or("the request reached the service")?;
    assert_eq!(writer.credential_id, Some(credential));
    assert_eq!(request.measurements[0].devices[0].ac[0].power_w, Some(800));
    assert_eq!(request.measurements[0].devices[0].ac[0].voltage_mv, None);
    // An event identifier may be a number, as a logger stores it.
    assert_eq!(request.events[0].event_id.as_deref(), Some("8812"));
    Ok(())
}

#[tokio::test]
async fn a_compressed_request_is_read_like_a_plain_one() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let uri = format!("/api/v1/systems/{system}/ingest");
    let plain = serde_json::to_vec(&cycle())?;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&plain)?;
    let gzip = gzip.finish()?;
    let zstd = zstd::stream::encode_all(plain.as_slice(), 3)?;

    let mut requests = Vec::new();
    for (encoding, body) in [
        (None, plain.clone()),
        (Some("identity"), plain.clone()),
        (Some("gzip"), gzip.clone()),
        (Some("GZIP"), gzip),
        (Some("zstd"), zstd),
    ] {
        let received = Received::default();
        let (status, _) = send(
            app(user(), &received),
            Method::POST,
            uri.clone(),
            encoding,
            body,
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{encoding:?}");
        let request = received.lock().map_err(|_| "poisoned")?[0].1.clone();
        requests.push(request);
    }
    assert!(requests.iter().all(|request| *request == requests[0]));

    let received = Received::default();
    // An encoding the server does not accept is named with those it does.
    let (status, problem) = send(
        app(user(), &received),
        Method::POST,
        uri.clone(),
        Some("br"),
        plain.clone(),
    )
    .await?;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        problem["contentEncodings"],
        json!(["identity", "gzip", "zstd"])
    );
    // A body that claims an encoding it does not have is refused.
    let (status, problem) = send(
        app(user(), &received),
        Method::POST,
        uri.clone(),
        Some("gzip"),
        plain,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["title"], "malformed_request");

    // A small body that expands beyond the limit is stopped at the limit.
    let mut bomb = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![b' '; 1 << 20];
    for _ in 0..33 {
        bomb.write_all(&chunk)?;
    }
    let bomb = bomb.finish()?;
    assert!(bomb.len() < 1 << 20);
    let (status, problem) = send(
        app(user(), &received),
        Method::POST,
        uri,
        Some("gzip"),
        bomb,
    )
    .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(problem["limit"], 32 * 1024 * 1024);
    assert!(received.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_unknown_member_is_refused_by_its_path() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let received = Received::default();
    let mut nested = cycle();
    nested["measurements"][0]["devices"][0]["ac"][0]["powerWatts"] = json!(800);
    let mut top = cycle();
    top["readings"] = json!([]);
    for (body, path) in [
        (nested, "measurements[0].devices[0].ac[0].powerWatts"),
        (top, "readings"),
    ] {
        let (status, problem) = post(&received, system, &body).await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(problem["title"], "unknown_member");
        assert_eq!(problem["detail"], path);
    }
    // A value of the wrong kind and a body that is no JSON are malformed.
    let mut wrong_kind = cycle();
    wrong_kind["measurements"][0]["measuredAt"] = json!("now");
    let (status, problem) = post(&received, system, &wrong_kind).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["title"], "malformed_request");
    let (status, _) = send(
        app(user(), &received),
        Method::POST,
        format!("/api/v1/systems/{system}/ingest"),
        None,
        b"{".to_vec(),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(received.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_invalid_item_refuses_the_whole_request() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let received = Received::default();
    let mut body = cycle();
    body["measurements"][0]["devices"][0]["battery"]["stateOfChargePermille"] = json!(1_001);
    body["events"][0]["occurredAt"] = json!(-5);
    let (status, problem) = post(&received, system, &body).await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem["title"], "items_rejected");
    // Every item that cannot be stored is named by its list, position and reason.
    assert_eq!(
        problem["rejected"],
        json!([
            { "category": "measurements", "index": 0,
              "member": "devices[0].battery.stateOfChargePermille", "reason": "out_of_range" },
            { "category": "events", "index": 0, "member": "occurredAt", "reason": "out_of_range" },
        ])
    );
    assert!(
        received.lock().map_err(|_| "poisoned")?.is_empty(),
        "nothing of the request is stored"
    );
    Ok(())
}

#[tokio::test]
async fn the_limits_are_enforced_and_published() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let received = Received::default();
    let (status, capabilities) = send(
        app(
            Some(api_key(ApiScope::TelemetryWrite, Some(system)).1),
            &received,
        ),
        Method::GET,
        "/api/v1/ingest/capabilities".to_owned(),
        None,
        Vec::new(),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(capabilities["schemaVersions"], json!([1]));
    assert_eq!(
        capabilities["contentEncodings"],
        json!(["identity", "gzip", "zstd"])
    );
    assert_eq!(
        capabilities["limits"],
        json!({
            "measurements": 5_000, "energySamples": 20_000, "dailySummaries": 5_000,
            "events": 5_000, "devices": 64, "extraValuesPerDevice": 32,
            "bodyBytes": 1_048_576, "decodedBodyBytes": 33_554_432,
        })
    );
    let (status, _) = send(
        app(None, &received),
        Method::GET,
        "/api/v1/ingest/capabilities".to_owned(),
        None,
        Vec::new(),
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // One item more than the published limit states the limit.
    let events: Vec<Value> = (0..5_001)
        .map(|second| json!({ "deviceId": "1", "occurredAt": second }))
        .collect();
    let (status, problem) = post(
        &received,
        system,
        &json!({ "schemaVersion": 1, "events": events }),
    )
    .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(problem["title"], "limit_exceeded");
    assert_eq!(problem["detail"], "events: at most 5000 items");
    assert_eq!(problem["limit"], 5_000);

    // A schema version the server does not know is refused with those it does.
    let (status, problem) = post(&received, system, &json!({ "schemaVersion": 2 })).await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem["title"], "unsupported_schema_version");
    assert_eq!(problem["schemaVersions"], json!([1]));
    assert!(received.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn only_a_writer_of_the_system_may_ingest_and_ask_for_its_state() -> Result<(), Box<dyn Error>>
{
    let system = SystemId::new();
    let received = Received::default();
    let body = serde_json::to_vec(&cycle())?;
    for principal in [
        None,
        Some(api_key(ApiScope::TelemetryRead, Some(system)).1),
        Some(api_key(ApiScope::TelemetryWrite, Some(SystemId::new())).1),
    ] {
        for (method, path) in [(Method::POST, "ingest"), (Method::GET, "ingest/state")] {
            let (status, _) = send(
                app(principal.clone(), &received),
                method,
                format!("/api/v1/systems/{system}/{path}"),
                None,
                body.clone(),
            )
            .await?;
            assert_eq!(status, StatusCode::FORBIDDEN);
        }
    }
    assert!(received.lock().map_err(|_| "poisoned")?.is_empty());

    // The upload key of a data source, which may read nothing else, reads its marks.
    let (status, marks) = send(
        app(
            Some(api_key(ApiScope::TelemetryWrite, Some(system)).1),
            &received,
        ),
        Method::GET,
        format!("/api/v1/systems/{system}/ingest/state"),
        None,
        Vec::new(),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(marks["devices"][0]["deviceId"], "2000123456");
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
        assert_eq!(permission, Permission::TelemetryWrite);
        Ok(AuthorizedRequest {
            actor_user_id: UserId::new(),
            account_id: AccountId::new(),
        })
    }
}

struct Stub(Received);

fn marks() -> IngestMarks {
    IngestMarks {
        system: IngestMark {
            measured_at: Some(1_800_000_000),
            ..IngestMark::default()
        },
        devices: vec![IngestMark {
            device_id: Some("2000123456".to_owned()),
            measured_at: Some(1_800_000_000),
            ..IngestMark::default()
        }],
    }
}

#[async_trait]
impl IngestApiUseCases for Stub {
    async fn ingest(
        &self,
        _system_id: SystemId,
        writer: IngestWriter,
        request: IngestRequest,
    ) -> Result<IngestResponse, IngestApiError> {
        self.0
            .lock()
            .map_err(|_| IngestApiError::Unavailable)?
            .push((writer, request));
        Ok(IngestResponse {
            measurements: IngestCounts {
                stored: 1,
                ..IngestCounts::default()
            },
            events: IngestCounts {
                unchanged: 1,
                ..IngestCounts::default()
            },
            marks: marks(),
            ..IngestResponse::default()
        })
    }

    async fn state(
        &self,
        _system_id: SystemId,
        _writer: IngestWriter,
    ) -> Result<IngestMarks, IngestApiError> {
        Ok(marks())
    }
}
