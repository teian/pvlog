use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use pvlog_compatibility::{
    AddStatusPolicy, AddStatusServiceError, AddStatusUseCases, LegacyAuth, LegacyStatus,
    LegacyStatusEnergy, add_status_router,
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
    energy_only: HttpCase,
    net_post: HttpCase,
    invalid_net_cumulative: HttpCase,
    moon_powered: HttpCase,
}

#[derive(Deserialize)]
struct HttpCase {
    query: Option<String>,
    form: Option<String>,
    status: u16,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!("../fixtures/pvoutput/addstatus-golden.json"))
}

#[tokio::test]
async fn addstatus_derives_power_and_accepts_extended_and_battery_fields()
-> Result<(), Box<dyn Error>> {
    let service = Arc::new(FakeService::default());
    let app = add_status_router(service.clone());
    assert_case(&app, Method::GET, &golden()?.energy_only).await?;

    let saved = service.saved.lock().map_err(|_| "status lock poisoned")?;
    assert_eq!(saved[0].time, time!(12:00));
    assert_eq!(saved[0].generation_power_watts, Some(3_600));
    assert_eq!(saved[0].consumption_power_watts, Some(1_800));
    assert_eq!(saved[0].temperature_milli_celsius, Some(21_500));
    assert_eq!(saved[0].voltage_millivolts, Some(239_200));
    assert_eq!(saved[0].extended.get(&7).map(String::as_str), Some("42.25"));
    assert_eq!(saved[0].battery_state_of_charge_basis_points, Some(7_550));
    assert_eq!(saved[0].battery_state, Some(3));
    Ok(())
}

#[tokio::test]
async fn addstatus_maps_documented_net_sign_combinations() -> Result<(), Box<dyn Error>> {
    let service = Arc::new(FakeService::default());
    let app = add_status_router(service.clone());
    assert_case(&app, Method::POST, &golden()?.net_post).await?;

    let saved = service.saved.lock().map_err(|_| "status lock poisoned")?;
    assert_eq!(saved[0].net_export_power_watts, Some(1_400));
    assert_eq!(saved[0].net_import_power_watts, Some(0));
    assert_eq!(saved[0].generation_power_watts, None);
    Ok(())
}

#[tokio::test]
async fn addstatus_enforces_net_cumulative_and_daylight_restrictions() -> Result<(), Box<dyn Error>>
{
    let app = add_status_router(Arc::new(FakeService::default()));
    let cases = golden()?;
    assert_case(&app, Method::GET, &cases.invalid_net_cumulative).await?;
    assert_case(&app, Method::GET, &cases.moon_powered).await?;
    Ok(())
}

async fn assert_case(
    app: &axum::Router,
    method: Method,
    case: &HttpCase,
) -> Result<(), Box<dyn Error>> {
    let uri = case.query.as_ref().map_or_else(
        || "/service/r2/addstatus.jsp".to_owned(),
        |query| format!("/service/r2/addstatus.jsp?{query}"),
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
    saved: Mutex<Vec<LegacyStatus>>,
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
        Ok(Some(LegacyStatus {
            date: date!(2024 - 12 - 20),
            time: time!(11:55),
            generation_energy: Some(LegacyStatusEnergy {
                watt_hours: 1_000,
                cumulative: true,
            }),
            generation_power_watts: None,
            consumption_energy: Some(LegacyStatusEnergy {
                watt_hours: 550,
                cumulative: false,
            }),
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
        }))
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
