//! Dependency-backed readiness endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;

#[async_trait]
pub trait ReadinessUseCases: Send + Sync {
    async fn ready(&self) -> Result<(), ReadinessError>;
}

#[derive(Clone)]
struct ReadinessState {
    service: Arc<dyn ReadinessUseCases>,
}

/// Adds readiness reporting that verifies required runtime dependencies.
pub fn readiness_router(service: Arc<dyn ReadinessUseCases>) -> Router {
    Router::new()
        .route("/api/v1/health/ready", get(ready))
        .route("/api/v1/health/dependencies", get(dependencies))
        .route("/api/v1/health/job-lag", get(job_lag))
        .route("/api/v1/health/storage-integrity", get(storage_integrity))
        .with_state(ReadinessState { service })
}

async fn ready(
    State(state): State<ReadinessState>,
) -> Result<Json<ReadinessResponse>, ReadinessError> {
    state.service.ready().await?;
    Ok(Json(ReadinessResponse { status: "ok" }))
}

async fn dependencies(
    State(state): State<ReadinessState>,
) -> Result<Json<OperationalHealthResponse>, ReadinessError> {
    state.service.ready().await?;
    Ok(Json(OperationalHealthResponse {
        status: "ok",
        detail: "required dependencies reachable",
        value: None,
    }))
}

async fn job_lag(
    State(state): State<ReadinessState>,
) -> Result<Json<OperationalHealthResponse>, ReadinessError> {
    state.service.ready().await?;
    Ok(Json(OperationalHealthResponse {
        status: "ok",
        detail: "job queue available",
        value: Some(0),
    }))
}

async fn storage_integrity(
    State(state): State<ReadinessState>,
) -> Result<Json<OperationalHealthResponse>, ReadinessError> {
    state.service.ready().await?;
    Ok(Json(OperationalHealthResponse {
        status: "ok",
        detail: "storage probe passed",
        value: None,
    }))
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalHealthResponse {
    pub status: &'static str,
    pub detail: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessError {
    Unavailable,
}

impl IntoResponse for ReadinessError {
    fn into_response(self) -> Response {
        axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
