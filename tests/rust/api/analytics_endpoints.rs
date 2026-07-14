use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::analytics_router;
use pvlog_application::{
    AnalysisExportRequest, AnalysisExportResult, DataQualityIssue, EnergyStatistics,
    ModeledAnalysisExportMetadata, ModernAnalyticsError, ModernAnalyticsUseCases, QueryResolution,
    SeriesField, SeriesQueryResult, StatisticsPeriod,
};
use pvlog_domain::{JobId, SystemId, UserId, WeatherDataRunId, YieldCalculationRunId};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn analytics_routes_cover_queries_and_both_export_modes() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let actor = UserId::new();
    let app = analytics_router(Arc::new(Stub)).layer(Extension(actor));
    for uri in [
        format!(
            "/api/v1/systems/{system}/series?startEpochMillis=0&endEpochMillis=3600000&fields=generation_power"
        ),
        format!("/api/v1/systems/{system}/statistics?period=day"),
        format!("/api/v1/systems/{system}/data-quality?startEpochMillis=0&endEpochMillis=3600000"),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let synchronous = app.clone().oneshot(export_request(system, false)?).await?;
    assert_eq!(synchronous.status(), StatusCode::OK);
    assert_eq!(
        synchronous
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    assert!(
        synchronous
            .headers()
            .contains_key(header::CONTENT_DISPOSITION)
    );
    assert_eq!(
        synchronous.headers()["x-pvlog-model-version"],
        "pvwatts-compatible@1"
    );
    assert_eq!(
        synchronous.headers()["x-pvlog-provider-attribution"],
        "Weather Example"
    );

    let asynchronous = app.oneshot(export_request(system, true)?).await?;
    assert_eq!(asynchronous.status(), StatusCode::ACCEPTED);
    Ok(())
}

#[tokio::test]
async fn analytics_routes_require_identity_and_validate_bounds() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let app = analytics_router(Arc::new(Stub));
    let forbidden = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/systems/{system}/statistics?period=lifetime"
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let invalid = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/systems/{system}/series?startEpochMillis=1&endEpochMillis=1&fields=unknown"
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

fn export_request(
    system: SystemId,
    asynchronous: bool,
) -> Result<Request<Body>, axum::http::Error> {
    Request::builder()
        .method(Method::POST)
        .uri(format!(
            "/api/v1/systems/{system}/analysis-exports"
        ))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"startEpochMillis":0,"endEpochMillis":3600000,"fields":["forecast_power","forecast_energy","generation_energy","expected_energy","generation_performance","forecast_realization"],"includePartial":true,"format":"csv","asynchronous":{asynchronous}}}"#
        )))
}

struct Stub;

#[async_trait]
impl ModernAnalyticsUseCases for Stub {
    async fn time_series(
        &self,
        _actor: UserId,
        _system_id: SystemId,
        _request: pvlog_application::QueryPlanRequest,
    ) -> Result<SeriesQueryResult, ModernAnalyticsError> {
        Ok(SeriesQueryResult {
            actual_resolution: QueryResolution::Hourly,
            timezone: "UTC".to_owned(),
            series: Vec::new(),
        })
    }

    async fn statistics(
        &self,
        _actor: UserId,
        _system_id: SystemId,
        period: StatisticsPeriod,
    ) -> Result<EnergyStatistics, ModernAnalyticsError> {
        Ok(EnergyStatistics {
            period,
            generation_wh: None,
            consumption_wh: None,
            grid_import_wh: None,
            grid_export_wh: None,
            self_consumption_wh: None,
            efficiency_wh_per_kw: None,
            peak_generation_watts: None,
            minimum_temperature_milli_celsius: None,
            maximum_temperature_milli_celsius: None,
            battery_charge_wh: None,
            battery_discharge_wh: None,
            minimum_battery_basis_points: None,
            maximum_battery_basis_points: None,
            revenue_minor_units: None,
            cost_minor_units: None,
            net_financial_minor_units: None,
            coverage_basis_points: 0,
        })
    }

    async fn data_quality(
        &self,
        _actor: UserId,
        _system_id: SystemId,
        _start_epoch_millis: i64,
        _end_epoch_millis: i64,
    ) -> Result<Vec<DataQualityIssue>, ModernAnalyticsError> {
        Ok(Vec::new())
    }

    async fn export(
        &self,
        request: AnalysisExportRequest,
    ) -> Result<AnalysisExportResult, ModernAnalyticsError> {
        for field in [
            SeriesField::ForecastPower,
            SeriesField::ForecastEnergy,
            SeriesField::GenerationEnergy,
            SeriesField::ExpectedEnergy,
            SeriesField::GenerationPerformance,
            SeriesField::ForecastRealization,
        ] {
            assert!(request.query.fields.contains(&field));
        }
        assert_eq!(
            request
                .modeled_selection
                .as_ref()
                .map(|selection| selection.include_partial),
            Some(true)
        );
        if request.asynchronous {
            Ok(AnalysisExportResult::Queued {
                job_id: JobId::new(),
            })
        } else {
            Ok(AnalysisExportResult::Ready {
                content_type: "text/csv; charset=utf-8".to_owned(),
                filename: "generation.csv".to_owned(),
                bytes: b"interval_start,interval_end,forecast_power_watts,forecast_power_lower_watts,forecast_power_upper_watts,forecast_energy_watt_hours,actual_energy_watt_hours,expected_energy_watt_hours,generation_performance_basis_points,forecast_realization_basis_points,coverage_basis_points,model_version,provider_attribution\n".to_vec(),
                modeled_metadata: Some(Box::new(ModeledAnalysisExportMetadata {
                    weather_run_id: WeatherDataRunId::new(),
                    calculation_run_id: YieldCalculationRunId::new(),
                    model_identifier: "pvwatts-compatible".to_owned(),
                    model_revision: 1,
                    configuration_digest: "09".repeat(32),
                    provider_attribution: "Weather Example".to_owned(),
                    freshness: "fresh".to_owned(),
                    coverage_basis_points: 9_500,
                    uncertainty_available: true,
                    interval_semantics: "half_open".to_owned(),
                })),
            })
        }
    }
}
