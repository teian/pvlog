use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_api::{
    DashboardApiError, DashboardApiUseCases, DashboardIngestionResponse, DashboardResponse,
    RequestPrincipal, dashboard_router,
};
use pvlog_domain::UserId;
use tower::ServiceExt as _;

#[tokio::test]
async fn dashboard_requires_a_session_and_returns_projection() -> Result<(), Box<dyn Error>> {
    let app = dashboard_router(Arc::new(Dashboard));
    let anonymous = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dashboard")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let authenticated = app
        .layer(Extension(RequestPrincipal::User(UserId::new())))
        .oneshot(
            Request::builder()
                .uri("/api/v1/dashboard")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(authenticated.status(), StatusCode::OK);
    let body = to_bytes(authenticated.into_body(), usize::MAX).await?;
    let document: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(document["coverageBasisPoints"], 0);
    Ok(())
}

struct Dashboard;

#[async_trait]
impl DashboardApiUseCases for Dashboard {
    async fn dashboard(&self) -> Result<DashboardResponse, DashboardApiError> {
        Ok(DashboardResponse {
            observed_at_epoch_millis: 0,
            age_seconds: 1,
            freshness_threshold_seconds: 60,
            generation_watts: 0.0,
            consumption_watts: None,
            grid_watts: None,
            battery_basis_points: None,
            coverage_basis_points: 0,
            recent_alerts: Vec::new(),
            ingestion: DashboardIngestionResponse {
                accepted_today: 0,
                rejected_today: 0,
                lag_seconds: 0,
            },
        })
    }
}
