use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use pvlog_compatibility::{
    LegacyAuth, LegacyDailyExtended, LegacyDailyOutputRecord, LegacyOutputQuery,
    LegacyOutputUseCases, LegacyOutputsError, legacy_outputs_router,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{Arc, Mutex},
};
use time::{
    Date, Time,
    macros::{date, time},
};
use tower::ServiceExt as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    output_options: HttpCase,
    extended: HttpCase,
    missing: HttpCase,
    delete_post: HttpCase,
    delete_old: HttpCase,
}

#[derive(Deserialize)]
struct HttpCase {
    method: String,
    path: String,
    form: Option<String>,
    status: u16,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!("../fixtures/pvoutput/outputs-golden.json"))
}

#[tokio::test]
async fn output_extended_and_missing_services_preserve_legacy_field_order()
-> Result<(), Box<dyn Error>> {
    let app = legacy_outputs_router(Arc::new(FakeOutputs::default()));
    let cases = golden()?;
    for case in [cases.output_options, cases.extended, cases.missing] {
        assert_case(&app, &case).await?;
    }
    Ok(())
}

#[tokio::test]
async fn delete_status_supports_post_and_enforces_fourteen_day_window() -> Result<(), Box<dyn Error>>
{
    let service = Arc::new(FakeOutputs::default());
    let app = legacy_outputs_router(service.clone());
    let cases = golden()?;
    assert_case(&app, &cases.delete_post).await?;
    assert_case(&app, &cases.delete_old).await?;
    assert_eq!(service.deleted.lock().map_err(|_| "delete lock")?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn aggregate_team_combination_is_rejected() -> Result<(), Box<dyn Error>> {
    let case = HttpCase {
        method: "GET".to_owned(),
        path: "/service/r2/getoutput.jsp?key=write-key&sid=42&a=m&tid=7".to_owned(),
        form: None,
        status: 400,
        body: "Bad request 400: Aggregated team output is not supported".to_owned(),
    };
    assert_case(
        &legacy_outputs_router(Arc::new(FakeOutputs::default())),
        &case,
    )
    .await
}

async fn assert_case(app: &axum::Router, case: &HttpCase) -> Result<(), Box<dyn Error>> {
    let method = Method::from_bytes(case.method.as_bytes())?;
    let mut builder = Request::builder().method(method).uri(&case.path);
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
    let response_body = std::str::from_utf8(&bytes)?;
    assert_eq!(
        status,
        StatusCode::from_u16(case.status)?,
        "{response_body}"
    );
    assert_eq!(response_body, case.body);
    Ok(())
}

#[derive(Default)]
struct FakeOutputs {
    deleted: Mutex<Vec<(Date, Option<Time>)>>,
}

#[async_trait]
impl LegacyOutputUseCases for FakeOutputs {
    async fn outputs(
        &self,
        auth: &LegacyAuth,
        query: &LegacyOutputQuery,
    ) -> Result<Vec<LegacyDailyOutputRecord>, LegacyOutputsError> {
        authorize(auth)?;
        assert!(query.include_insolation && query.include_time_of_export);
        Ok(vec![LegacyDailyOutputRecord {
            date: date!(2024 - 12 - 20),
            generated_wh: Some(12_000),
            efficiency_milli_kwh_per_kw: Some(2_500),
            exported_wh: Some(3_000),
            used_wh: Some(9_000),
            peak_power_watts: Some(6_000),
            peak_time: Some(time!(12:30)),
            condition: Some("Fine".to_owned()),
            minimum_temperature_milli_celsius: Some(5_500),
            maximum_temperature_milli_celsius: Some(24_500),
            import_peak_wh: Some(5_000),
            import_off_peak_wh: Some(1_000),
            import_shoulder_wh: Some(4_000),
            import_high_shoulder_wh: Some(800),
            export_peak_wh: Some(1_000),
            export_off_peak_wh: Some(500),
            export_shoulder_wh: Some(250),
            export_high_shoulder_wh: Some(250),
            insolation_wh: Some(14_000),
        }])
    }

    async fn extended(
        &self,
        auth: &LegacyAuth,
        _date_from: Option<Date>,
        _date_to: Option<Date>,
        limit: u16,
    ) -> Result<Vec<LegacyDailyExtended>, LegacyOutputsError> {
        authorize(auth)?;
        assert_eq!(limit, 50);
        Ok(vec![LegacyDailyExtended {
            date: date!(2024 - 12 - 20),
            values_milli: BTreeMap::from([(7, 1_000), (9, 3_000), (12, -12_300)]),
        }])
    }

    async fn missing_dates(
        &self,
        auth: &LegacyAuth,
        _date_from: Option<Date>,
        _date_to: Option<Date>,
    ) -> Result<Vec<Date>, LegacyOutputsError> {
        authorize(auth)?;
        Ok(vec![date!(2024 - 12 - 03), date!(2024 - 12 - 01)])
    }

    async fn deletion_today(&self, auth: &LegacyAuth) -> Result<Date, LegacyOutputsError> {
        authorize(auth)?;
        Ok(date!(2024 - 12 - 31))
    }

    async fn delete_status(
        &self,
        auth: &LegacyAuth,
        date: Date,
        time: Option<Time>,
    ) -> Result<bool, LegacyOutputsError> {
        authorize(auth)?;
        self.deleted
            .lock()
            .map_err(|_| LegacyOutputsError::Unavailable)?
            .push((date, time));
        Ok(true)
    }
}

fn authorize(auth: &LegacyAuth) -> Result<(), LegacyOutputsError> {
    if auth.api_key == "write-key" && auth.system_id == 42 {
        Ok(())
    } else {
        Err(LegacyOutputsError::Unauthorized)
    }
}
