use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use pvlog_compatibility::{
    AddBatchStatusUseCases, AddStatusPolicy, AddStatusServiceError, AddStatusUseCases,
    BatchStatusOutcome, LegacyAuth, LegacyStatus, LegacyStatusEnergy, add_batch_status_router,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{Arc, Mutex},
};
use time::{
    PrimitiveDateTime,
    macros::{date, time},
};
use tower::ServiceExt as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    mixed_outcomes: HttpCase,
    net_batch: HttpCase,
    different_net_dates: HttpCase,
}

#[derive(Deserialize)]
struct HttpCase {
    query: Option<String>,
    form: Option<String>,
    status: u16,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../fixtures/pvoutput/addbatchstatus-golden.json"
    ))
}

#[tokio::test]
async fn batch_status_returns_stable_indexed_outcomes_and_completes_last_day_once()
-> Result<(), Box<dyn Error>> {
    let service = Arc::new(FakeService::default());
    let app = add_batch_status_router(service.clone());
    assert_case(&app, Method::GET, &golden()?.mixed_outcomes).await?;

    assert_eq!(
        *service
            .daily_completions
            .lock()
            .map_err(|_| "completion lock poisoned")?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn batch_status_supports_net_shape_and_requires_one_local_date() -> Result<(), Box<dyn Error>>
{
    let app = add_batch_status_router(Arc::new(FakeService::default()));
    let cases = golden()?;
    assert_case(&app, Method::POST, &cases.net_batch).await?;
    assert_case(&app, Method::GET, &cases.different_net_dates).await?;
    Ok(())
}

#[tokio::test]
async fn retryable_item_sets_retry_after_without_losing_item_position() -> Result<(), Box<dyn Error>>
{
    let service = Arc::new(FakeService {
        retry_minute: Some(10),
        ..FakeService::default()
    });
    let app = add_batch_status_router(service);
    let case = HttpCase {
        query: Some("key=write-key&sid=42&data=20241220%2C12%3A10%2C1200%2C1400".to_owned()),
        form: None,
        status: 200,
        body: "20241220,12:10,0".to_owned(),
    };
    let response = request_case(&app, Method::GET, &case).await?;
    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1, case.body);
    assert_eq!(response.2.as_deref(), Some("60"));
    Ok(())
}

async fn assert_case(
    app: &axum::Router,
    method: Method,
    case: &HttpCase,
) -> Result<(), Box<dyn Error>> {
    let (status, body, _) = request_case(app, method, case).await?;
    assert_eq!(status, StatusCode::from_u16(case.status)?, "{body}");
    assert_eq!(body, case.body);
    Ok(())
}

async fn request_case(
    app: &axum::Router,
    method: Method,
    case: &HttpCase,
) -> Result<(StatusCode, String, Option<String>), Box<dyn Error>> {
    let uri = case.query.as_ref().map_or_else(
        || "/service/r2/addbatchstatus.jsp".to_owned(),
        |query| format!("/service/r2/addbatchstatus.jsp?{query}"),
    );
    let mut builder = Request::builder().method(method).uri(uri);
    let body = if let Some(form) = &case.form {
        builder = builder
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("x-pvoutput-apikey", "write-key")
            .header("x-pvoutput-systemid", "42");
        Body::from(form.clone())
    } else {
        Body::empty()
    };
    let response = app.clone().oneshot(builder.body(body)?).await?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, std::str::from_utf8(&bytes)?.to_owned(), retry_after))
}

#[derive(Default)]
struct FakeService {
    saved: Mutex<Vec<LegacyStatus>>,
    daily_completions: Mutex<u32>,
    retry_minute: Option<u8>,
}

#[async_trait]
impl AddStatusUseCases for FakeService {
    async fn policy(&self, auth: &LegacyAuth) -> Result<AddStatusPolicy, AddStatusServiceError> {
        if auth.api_key != "write-key" || auth.system_id != 42 {
            return Err(AddStatusServiceError::Unauthorized);
        }
        Ok(AddStatusPolicy {
            today: date!(2024 - 12 - 31),
            effective_capacity_watts: 6_000,
            status_interval_minutes: 5,
            daylight_start: time!(06:00),
            daylight_end: time!(20:00),
            extended_enabled: true,
            battery_enabled: true,
        })
    }

    async fn previous_status(
        &self,
        _auth: &LegacyAuth,
        _before: PrimitiveDateTime,
    ) -> Result<Option<LegacyStatus>, AddStatusServiceError> {
        Ok(Some(previous()))
    }

    async fn accept_status(
        &self,
        _auth: &LegacyAuth,
        status: LegacyStatus,
    ) -> Result<(), AddStatusServiceError> {
        self.saved
            .lock()
            .map_err(|_| AddStatusServiceError::Unavailable)?
            .push(status);
        Ok(())
    }
}

#[async_trait]
impl AddBatchStatusUseCases for FakeService {
    async fn accept_batch_status(
        &self,
        _auth: &LegacyAuth,
        status: LegacyStatus,
    ) -> Result<BatchStatusOutcome, AddStatusServiceError> {
        if self.retry_minute == Some(status.time.minute()) {
            return Ok(BatchStatusOutcome::Retryable);
        }
        let outcome = if status.time.minute() == 5 {
            BatchStatusOutcome::Unchanged
        } else {
            BatchStatusOutcome::Added
        };
        self.saved
            .lock()
            .map_err(|_| AddStatusServiceError::Unavailable)?
            .push(status);
        Ok(outcome)
    }

    async fn complete_daily_output(
        &self,
        _auth: &LegacyAuth,
        _last_successful: &LegacyStatus,
    ) -> Result<(), AddStatusServiceError> {
        let mut completions = self
            .daily_completions
            .lock()
            .map_err(|_| AddStatusServiceError::Unavailable)?;
        *completions += 1;
        Ok(())
    }
}

fn previous() -> LegacyStatus {
    LegacyStatus {
        date: date!(2024 - 12 - 20),
        time: time!(11:55),
        generation_energy: Some(LegacyStatusEnergy {
            watt_hours: 900,
            cumulative: false,
        }),
        generation_power_watts: None,
        consumption_energy: None,
        consumption_power_watts: None,
        net_export_power_watts: None,
        net_import_power_watts: None,
        temperature_milli_celsius: None,
        voltage_millivolts: None,
        extended: BTreeMap::default(),
        message: None,
        battery_power_watts: None,
        battery_state_of_charge_basis_points: None,
        battery_size_wh: None,
        battery_lifetime_charge_wh: None,
        battery_lifetime_discharge_wh: None,
        battery_state: None,
    }
}
