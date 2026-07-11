use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::analytics_router;
use pvlog_application::{
    AnalysisExportRequest, AnalysisExportResult, ComparisonEntry, ComparisonMetric,
    DataQualityIssue, EnergyStatistics, ModernAnalyticsError, ModernAnalyticsUseCases,
    QueryResolution, SeriesQueryResult, StatisticsPeriod,
};
use pvlog_domain::{JobId, SystemId, TeamId, UserId};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn analytics_routes_cover_queries_rankings_and_both_export_modes()
-> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let actor = UserId::new();
    let app = analytics_router(Arc::new(Stub)).layer(Extension(actor));
    for uri in [
        format!(
            "/api/v1/systems/{system}/series?startEpochMillis=0&endEpochMillis=3600000&fields=generation_power"
        ),
        format!("/api/v1/systems/{system}/statistics?period=day"),
        format!("/api/v1/systems/{system}/data-quality?startEpochMillis=0&endEpochMillis=3600000"),
        "/api/v1/ladders?metric=normalized_generation".to_owned(),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let comparison = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/comparisons")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"systemIds":["{system}","{}"],"metric":"total_generation"}}"#,
                    SystemId::new()
                )))?,
        )
        .await?;
    assert_eq!(comparison.status(), StatusCode::OK);

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
            r#"{{"startEpochMillis":0,"endEpochMillis":3600000,"fields":["generation_power"],"format":"csv","asynchronous":{asynchronous}}}"#
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

    async fn compare(
        &self,
        _actor: UserId,
        _system_ids: Vec<SystemId>,
        _metric: ComparisonMetric,
    ) -> Result<Vec<ComparisonEntry>, ModernAnalyticsError> {
        Ok(Vec::new())
    }

    async fn ladder(
        &self,
        _actor: UserId,
        _team_id: Option<TeamId>,
        _metric: ComparisonMetric,
    ) -> Result<Vec<ComparisonEntry>, ModernAnalyticsError> {
        Ok(Vec::new())
    }

    async fn export(
        &self,
        request: AnalysisExportRequest,
    ) -> Result<AnalysisExportResult, ModernAnalyticsError> {
        if request.asynchronous {
            Ok(AnalysisExportResult::Queued {
                job_id: JobId::new(),
            })
        } else {
            Ok(AnalysisExportResult::Ready {
                content_type: "text/csv; charset=utf-8".to_owned(),
                filename: "generation.csv".to_owned(),
                bytes: b"timestamp,generation_power_watts\n".to_vec(),
            })
        }
    }
}
