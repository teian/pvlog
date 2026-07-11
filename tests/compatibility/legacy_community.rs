use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use pvlog_compatibility::{
    LegacyArrayDetails, LegacyAuth, LegacyCommunityError, LegacyCommunityUseCases,
    LegacyFavouriteSystem, LegacyLadderSummary, LegacySearchQuery, LegacySearchSystem,
    LegacySystemDetails, LegacySystemOptions, LegacySystemUpdate, legacy_community_router,
};
use serde::Deserialize;
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use time::macros::date;
use tower::ServiceExt as _;

#[derive(Deserialize)]
struct Golden {
    system: HttpCase,
    update: HttpCase,
    search: HttpCase,
    favourite: HttpCase,
    ladder: HttpCase,
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
    serde_json::from_str(include_str!("../fixtures/pvoutput/community-golden.json"))
}

#[tokio::test]
async fn community_services_preserve_legacy_fields_and_options() -> Result<(), Box<dyn Error>> {
    let service = Arc::new(FakeCommunity::default());
    let app = legacy_community_router(service.clone());
    let cases = golden()?;
    for case in [
        cases.system,
        cases.update,
        cases.search,
        cases.favourite,
        cases.ladder,
    ] {
        assert_case(&app, &case).await?;
    }

    let options = service.options.lock().map_err(|_| "options lock")?.clone();
    assert_eq!(
        options,
        Some(LegacySystemOptions {
            include_array_two: true,
            include_timezone: true,
            include_tariffs: true,
            include_teams: true,
            include_estimates: true,
            include_donations: true,
            include_extended: true,
            target_system_id: Some(99),
            ..LegacySystemOptions::default()
        })
    );
    let update = service
        .update
        .lock()
        .map_err(|_| "update lock")?
        .clone()
        .ok_or("missing update")?;
    assert_eq!(update.name.as_deref(), Some("Roof Array"));
    assert_eq!(
        update.extended.get(&7).map(|field| field.label.as_str()),
        Some("Temperature")
    );
    let search = service
        .search
        .lock()
        .map_err(|_| "search lock")?
        .clone()
        .ok_or("missing search")?;
    assert_eq!(search.origin_microdegrees, Some((-33_875_000, 151_200_000)));
    assert!(search.country_only);
    assert_eq!(search.country_code.as_deref(), Some("AU"));
    assert_eq!(search.seen_days, Some(30));
    Ok(())
}

#[tokio::test]
async fn post_system_requires_paired_extended_label_and_unit() -> Result<(), Box<dyn Error>> {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/service/r2/postsystem.jsp")
        .header("x-pvoutput-apikey", "write-key")
        .header("x-pvoutput-systemid", "42")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("v7l=Temperature"))?;
    let response = legacy_community_router(Arc::new(FakeCommunity::default()))
        .oneshot(request)
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
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
struct FakeCommunity {
    options: Mutex<Option<LegacySystemOptions>>,
    update: Mutex<Option<LegacySystemUpdate>>,
    search: Mutex<Option<LegacySearchQuery>>,
}

#[async_trait]
impl LegacyCommunityUseCases for FakeCommunity {
    async fn system(
        &self,
        auth: &LegacyAuth,
        options: &LegacySystemOptions,
    ) -> Result<LegacySystemDetails, LegacyCommunityError> {
        authorize(auth)?;
        *self
            .options
            .lock()
            .map_err(|_| LegacyCommunityError::Unavailable)? = Some(options.clone());
        Ok(system())
    }

    async fn update_system(
        &self,
        auth: &LegacyAuth,
        update: LegacySystemUpdate,
    ) -> Result<(), LegacyCommunityError> {
        authorize(auth)?;
        *self
            .update
            .lock()
            .map_err(|_| LegacyCommunityError::Unavailable)? = Some(update);
        Ok(())
    }

    async fn search(
        &self,
        auth: &LegacyAuth,
        query: &LegacySearchQuery,
    ) -> Result<Vec<LegacySearchSystem>, LegacyCommunityError> {
        authorize(auth)?;
        *self
            .search
            .lock()
            .map_err(|_| LegacyCommunityError::Unavailable)? = Some(query.clone());
        Ok(vec![LegacySearchSystem {
            name: "Solar House".to_owned(),
            size_watts: Some(6_000),
            postcode: "1234".to_owned(),
            orientation: "North".to_owned(),
            outputs: 365,
            last_output: "20241231".to_owned(),
            system_id: 99,
            panel: "PanelCo".to_owned(),
            inverter: "InvertCo".to_owned(),
            distance_kilometres: Some(12),
            latitude_microdegrees: Some(-33_875_000),
            longitude_microdegrees: Some(151_200_000),
        }])
    }

    async fn favourites(
        &self,
        auth: &LegacyAuth,
        target_system_id: Option<u64>,
    ) -> Result<Vec<LegacyFavouriteSystem>, LegacyCommunityError> {
        authorize(auth)?;
        assert_eq!(target_system_id, Some(99));
        Ok(vec![system()])
    }

    async fn ladder(
        &self,
        auth: &LegacyAuth,
        target_system_id: Option<u64>,
    ) -> Result<LegacyLadderSummary, LegacyCommunityError> {
        authorize(auth)?;
        assert_eq!(target_system_id, Some(99));
        Ok(LegacyLadderSummary {
            ranking_date: date!(2024 - 12 - 31),
            generation_rank: Some(4),
            efficiency_rank: Some(6),
            average_efficiency_milli_kwh_per_kw: Some(4_250),
            total_outputs: 365,
            last_output: Some(date!(2024 - 12 - 30)),
            total_generation_wh: Some(5_000_000),
            total_consumption_wh: Some(4_200_000),
            average_generation_wh: Some(14_000),
            average_consumption_wh: Some(12_000),
            maximum_generation_wh: Some(30_000),
            maximum_consumption_wh: Some(25_000),
            system_age_days: 1_825,
        })
    }
}

fn system() -> LegacySystemDetails {
    LegacySystemDetails {
        system_id: 99,
        name: "Solar House".to_owned(),
        size_watts: Some(6_000),
        postcode: "1234".to_owned(),
        panels: Some(15),
        panel_power_watts: Some(400),
        panel_brand: "PanelCo".to_owned(),
        inverters: Some(1),
        inverter_power_watts: Some(5_000),
        inverter_brand: "InvertCo".to_owned(),
        orientation: "North".to_owned(),
        tilt_milli_degrees: Some(22_500),
        shade: "None".to_owned(),
        install_date: Some(date!(2020 - 01 - 02)),
        latitude_microdegrees: Some(-33_875_000),
        longitude_microdegrees: Some(151_200_000),
        status_interval_minutes: Some(5),
        array_two: Some(LegacyArrayDetails {
            panels: Some(10),
            panel_power_watts: Some(300),
            orientation: Some("West".to_owned()),
            tilt_milli_degrees: Some(15_000),
        }),
        array_three: None,
        timezone: Some("Australia/Sydney".to_owned()),
        export_tariff_milli_cents: Some(250),
        import_peak_tariff_milli_cents: Some(300),
        import_off_peak_tariff_milli_cents: Some(100),
        import_shoulder_tariff_milli_cents: Some(200),
        import_high_shoulder_tariff_milli_cents: Some(225),
        import_daily_charge_milli_cents: Some(1_100),
        team_ids: vec![7, 9],
        donations: 3,
        extended_config_fields: vec![
            "Temperature".to_owned(),
            "C".to_owned(),
            "ff0000".to_owned(),
            "1".to_owned(),
            "line".to_owned(),
        ],
        monthly_estimates_kwh: (10..=21).map(|value| format!("{value}0")).collect(),
    }
}

fn authorize(auth: &LegacyAuth) -> Result<(), LegacyCommunityError> {
    if auth.api_key == "write-key" && auth.system_id == 42 {
        Ok(())
    } else {
        Err(LegacyCommunityError::Unauthorized)
    }
}
