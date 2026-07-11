use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use pvlog_compatibility::{
    AddOutputPolicy, AddOutputServiceError, AddOutputUseCases, DailyOutput, LegacyAuth,
    add_output_router,
};
use serde::Deserialize;
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use time::{Date, macros::date};
use tower::ServiceExt as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    minimum_get: HttpCase,
    csv_batch_post: HttpCase,
    future_date: HttpCase,
    invalid_temperature_pair: HttpCase,
}

#[derive(Deserialize)]
struct HttpCase {
    query: Option<String>,
    form: Option<String>,
    status: u16,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!("../fixtures/pvoutput/addoutput-golden.json"))
}

#[tokio::test]
async fn addoutput_supports_documented_get_post_csv_and_batch_shapes() -> Result<(), Box<dyn Error>>
{
    let service = Arc::new(FakeService::default());
    let app = add_output_router(service.clone());
    let golden = golden()?;
    assert_case(&app, Method::GET, &golden.minimum_get).await?;
    assert_case(&app, Method::POST, &golden.csv_batch_post).await?;

    let saved = service
        .saved
        .lock()
        .map_err(|_| "saved output lock poisoned")?;
    assert_eq!(saved.len(), 3);
    assert_eq!(saved[0].generated_wh, Some(12_000));
    assert_eq!(saved[1].exported_wh, Some(2_000));
    assert_eq!(saved[1].export_peak_wh, Some(1_000));
    assert_eq!(saved[1].export_off_peak_wh, Some(500));
    assert_eq!(saved[1].comments.as_deref(), Some("clear, bright"));
    Ok(())
}

#[tokio::test]
async fn addoutput_enforces_dates_temperature_pairs_and_new_output_requirements()
-> Result<(), Box<dyn Error>> {
    let app = add_output_router(Arc::new(FakeService::default()));
    let golden = golden()?;
    assert_case(&app, Method::GET, &golden.future_date).await?;
    assert_case(&app, Method::GET, &golden.invalid_temperature_pair).await?;

    let missing = HttpCase {
        query: Some("key=write-key&sid=42&d=20240229".to_owned()),
        form: None,
        status: 400,
        body: "Bad request 400: Generated or consumption must be provided for a new output"
            .to_owned(),
    };
    assert_case(&app, Method::GET, &missing).await?;
    Ok(())
}

#[tokio::test]
async fn addoutput_overwrites_total_export_with_tariff_period_sum_and_checks_capacity()
-> Result<(), Box<dyn Error>> {
    let service = Arc::new(FakeService::default());
    let app = add_output_router(service.clone());
    let case = HttpCase {
        query: Some(
            "key=write-key&sid=42&d=20240229&g=10000&e=9999&ep=1000&eo=500&es=250&eh=250"
                .to_owned(),
        ),
        form: None,
        status: 200,
        body: "Added Output".to_owned(),
    };
    assert_case(&app, Method::GET, &case).await?;
    {
        let saved = service
            .saved
            .lock()
            .map_err(|_| "saved output lock poisoned")?;
        assert_eq!(saved[0].exported_wh, Some(2_000));
    }

    let too_high = HttpCase {
        query: Some("key=write-key&sid=42&d=20240229&g=200000".to_owned()),
        form: None,
        status: 400,
        body: "Bad request 400: Generation too high for system size".to_owned(),
    };
    assert_case(&app, Method::GET, &too_high).await?;
    Ok(())
}

async fn assert_case(
    app: &axum::Router,
    method: Method,
    case: &HttpCase,
) -> Result<(), Box<dyn Error>> {
    let uri = case.query.as_ref().map_or_else(
        || "/service/r2/addoutput.jsp".to_owned(),
        |query| format!("/service/r2/addoutput.jsp?{query}"),
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
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
    let actual_body = std::str::from_utf8(&bytes)?;
    assert_eq!(status, StatusCode::from_u16(case.status)?, "{actual_body}");
    assert_eq!(actual_body, case.body);
    Ok(())
}

#[derive(Default)]
struct FakeService {
    saved: Mutex<Vec<DailyOutput>>,
}

#[async_trait]
impl AddOutputUseCases for FakeService {
    async fn policy(&self, auth: &LegacyAuth) -> Result<AddOutputPolicy, AddOutputServiceError> {
        if auth.api_key != "write-key" || auth.system_id != 42 {
            return Err(AddOutputServiceError::Unauthorized);
        }
        Ok(AddOutputPolicy {
            today: date!(2024 - 12 - 31),
            effective_capacity_watts: 6_000,
            batching_enabled: true,
            maximum_batch_size: 100,
        })
    }

    async fn output_exists(
        &self,
        _auth: &LegacyAuth,
        _date: Date,
    ) -> Result<bool, AddOutputServiceError> {
        Ok(false)
    }

    async fn upsert_outputs(
        &self,
        _auth: &LegacyAuth,
        outputs: Vec<DailyOutput>,
    ) -> Result<(), AddOutputServiceError> {
        self.saved
            .lock()
            .map_err(|_| AddOutputServiceError::Unavailable)?
            .extend(outputs);
        Ok(())
    }
}
