use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use pvlog_api::{ReadinessError, ReadinessUseCases, readiness_router};
use tower::ServiceExt as _;

#[tokio::test]
async fn readiness_reports_a_healthy_dependency() -> Result<(), Box<dyn Error>> {
    let response = readiness_router(Arc::new(Ready))
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/ready")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn readiness_reports_an_unavailable_dependency() -> Result<(), Box<dyn Error>> {
    let response = readiness_router(Arc::new(Unavailable))
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/ready")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    Ok(())
}

struct Ready;
#[async_trait]
impl ReadinessUseCases for Ready {
    async fn ready(&self) -> Result<(), ReadinessError> {
        Ok(())
    }
}

struct Unavailable;
#[async_trait]
impl ReadinessUseCases for Unavailable {
    async fn ready(&self) -> Result<(), ReadinessError> {
        Err(ReadinessError::Unavailable)
    }
}
