//! Daily summaries are supplied by whoever may write telemetry and read by
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
    AuthorizedRequest, DailySummaryApiError, DailySummaryApiUseCases, DailyValueResponse,
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, SummarySource,
    daily_summaries_router,
};
use pvlog_application::{DayBasis, SuppliedDailySummary};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, Permission, PrincipalId, SystemId, UserId,
};
use tower::ServiceExt as _;

type Supplied = Arc<Mutex<Vec<(SummarySource, SuppliedDailySummary)>>>;

fn app(principal: RequestPrincipal, supplied: &Supplied) -> Router {
    daily_summaries_router(Arc::new(Stub(supplied.clone())), Arc::new(Authorizer))
        .layer(Extension(principal))
}

fn api_key(scope: ApiScope, system_id: Option<SystemId>) -> RequestPrincipal {
    RequestPrincipal::ApiCredential {
        id: ApiCredentialId::new(),
        owner_user_id: UserId::new(),
        account_id: AccountId::new(),
        system_id,
        scopes: BTreeSet::from([scope]),
    }
}

async fn send(
    app: Router,
    method: Method,
    uri: String,
    body: Option<serde_json::Value>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))?,
        None => request.body(Body::empty())?,
    };
    let response = app.oneshot(request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    ))
}

#[tokio::test]
async fn an_upload_key_supplies_a_summary_for_its_system() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let supplied = Supplied::default();
    let (status, body) = send(
        app(api_key(ApiScope::TelemetryWrite, Some(system)), &supplied),
        Method::PUT,
        format!("/api/v1/systems/{system}/daily-summaries/2026-06-10"),
        Some(serde_json::json!({
            "generationEnergyWh": 12_000,
            "complete": true,
            "peakTime": "12:35",
            "importEnergyWhByBand": { "offPeak": 800 },
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["date"], "2026-06-10");
    assert_eq!(body["basis"], "supplied_complete");
    assert_eq!(body["generationEnergyWh"], 12_000);
    {
        let stored = supplied.lock().map_err(|_| "poisoned")?;
        let (source, summary) = stored.first().ok_or("the summary reached the service")?;
        assert!(source.credential_id.is_some());
        assert_eq!(summary.date, "2026-06-10");
        assert!(summary.complete);
        assert_eq!(summary.import_energy_wh_by_band.get("offPeak"), Some(&800));
        assert_eq!(summary.device_id, None);
    }

    // The yield of one inverter, with its lifetime counter.
    let (status, _) = send(
        app(api_key(ApiScope::TelemetryWrite, Some(system)), &supplied),
        Method::PUT,
        format!("/api/v1/systems/{system}/daily-summaries/2026-06-10"),
        Some(serde_json::json!({
            "deviceId": "2000123456",
            "generationEnergyWh": 5_200,
            "generationLifetimeWh": 10_000_000,
            "complete": true,
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let stored = supplied.lock().map_err(|_| "poisoned")?;
    let (_, summary) = stored.last().ok_or("the summary reached the service")?;
    assert_eq!(summary.device_id.as_deref(), Some("2000123456"));
    assert_eq!(summary.generation_lifetime_wh, Some(10_000_000));
    Ok(())
}

#[tokio::test]
async fn a_rejected_member_is_named() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let supplied = Supplied::default();
    for (date, body, field) in [
        (
            "2026-06-10",
            serde_json::json!({ "generationEnergyWh": -1 }),
            "generationEnergyWh",
        ),
        ("2026-13-01", serde_json::json!({}), "date"),
        (
            "2026-06-10",
            serde_json::json!({ "deviceId": "roof inverter" }),
            "deviceId",
        ),
        (
            "2026-06-10",
            serde_json::json!({ "peakTime": "noon" }),
            "peakTime",
        ),
    ] {
        let (status, problem) = send(
            app(RequestPrincipal::User(UserId::new()), &supplied),
            Method::PUT,
            format!("/api/v1/systems/{system}/daily-summaries/{date}"),
            Some(body),
        )
        .await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        assert_eq!(problem["title"], "invalid_daily_summary");
        assert_eq!(problem["detail"], field);
    }
    // A member the resource does not know is refused rather than ignored.
    let (status, _) = send(
        app(RequestPrincipal::User(UserId::new()), &supplied),
        Method::PUT,
        format!("/api/v1/systems/{system}/daily-summaries/2026-06-10"),
        Some(serde_json::json!({ "generationWh": 5 })),
    )
    .await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(supplied.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_key_for_another_system_or_without_write_scope_is_refused() -> Result<(), Box<dyn Error>>
{
    let system = SystemId::new();
    let supplied = Supplied::default();
    for principal in [
        api_key(ApiScope::TelemetryWrite, Some(SystemId::new())),
        api_key(ApiScope::TelemetryRead, Some(system)),
    ] {
        let (status, _) = send(
            app(principal, &supplied),
            Method::PUT,
            format!("/api/v1/systems/{system}/daily-summaries/2026-06-10"),
            Some(serde_json::json!({ "generationEnergyWh": 1 })),
        )
        .await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    assert!(supplied.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_range_is_listed_and_checked() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let supplied = Supplied::default();
    let (status, body) = send(
        app(RequestPrincipal::User(UserId::new()), &supplied),
        Method::GET,
        format!("/api/v1/systems/{system}/daily-summaries?from=2026-06-01&to=2026-06-30"),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["date"], "2026-06-01");
    assert_eq!(body[0]["basis"], "computed");

    let (status, problem) = send(
        app(RequestPrincipal::User(UserId::new()), &supplied),
        Method::GET,
        format!("/api/v1/systems/{system}/daily-summaries?from=2026-06-30&to=2026-06-01"),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem["detail"], "to");
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
        // Supplying needs the write permission, reading the read permission.
        assert_eq!(
            permission,
            if action.ends_with(".supply") {
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

struct Stub(Supplied);

fn value(date: String, basis: DayBasis, generation: Option<i64>) -> DailyValueResponse {
    DailyValueResponse {
        date,
        basis,
        generation_energy_wh: generation,
        consumption_energy_wh: None,
        export_energy_wh: None,
        supplied: None,
        supplied_devices: Vec::new(),
        computed: None,
    }
}

#[async_trait]
impl DailySummaryApiUseCases for Stub {
    async fn supply(
        &self,
        _account_id: AccountId,
        _system_id: SystemId,
        source: SummarySource,
        summary: SuppliedDailySummary,
    ) -> Result<DailyValueResponse, DailySummaryApiError> {
        let response = value(
            summary.date.clone(),
            if summary.complete {
                DayBasis::SuppliedComplete
            } else {
                DayBasis::SuppliedIncomplete
            },
            summary.generation_energy_wh,
        );
        self.0
            .lock()
            .map_err(|_| DailySummaryApiError::Unavailable)?
            .push((source, summary));
        Ok(response)
    }

    async fn list(
        &self,
        _account_id: AccountId,
        _system_id: SystemId,
        from: String,
        _to: String,
    ) -> Result<Vec<DailyValueResponse>, DailySummaryApiError> {
        Ok(vec![value(from, DayBasis::Computed, Some(300))])
    }
}
