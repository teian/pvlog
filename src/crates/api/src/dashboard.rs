//! Authenticated operational dashboard projection endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{Extension, Json, Router, http::StatusCode, response::Response, routing::get};
use serde::Serialize;

use crate::RequestPrincipal;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardResponse {
    pub observed_at_epoch_millis: i64,
    pub age_seconds: u64,
    pub freshness_threshold_seconds: u64,
    pub generation_watts: f64,
    pub consumption_watts: Option<f64>,
    pub grid_watts: Option<f64>,
    pub battery_basis_points: Option<i64>,
    pub coverage_basis_points: u16,
    pub recent_alerts: Vec<DashboardAlertResponse>,
    pub ingestion: DashboardIngestionResponse,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardAlertResponse {
    pub id: String,
    pub title: String,
    pub state: String,
    pub opened_at_epoch_millis: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardIngestionResponse {
    pub accepted_today: u64,
    pub rejected_today: u64,
    pub lag_seconds: u64,
}

#[async_trait]
pub trait DashboardApiUseCases: Send + Sync {
    async fn dashboard(&self) -> Result<DashboardResponse, DashboardApiError>;
}

#[derive(Clone, Copy, Debug)]
pub enum DashboardApiError {
    Unavailable,
}

pub fn dashboard_router(service: Arc<dyn DashboardApiUseCases>) -> Router {
    Router::new()
        .route("/api/v1/dashboard", get(dashboard))
        .with_state(service)
}

async fn dashboard(
    axum::extract::State(service): axum::extract::State<Arc<dyn DashboardApiUseCases>>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<DashboardResponse>, DashboardHttpError> {
    if principal.is_none() {
        return Err(DashboardHttpError::Unauthorized);
    }
    service
        .dashboard()
        .await
        .map(Json)
        .map_err(|_| DashboardHttpError::Unavailable)
}

enum DashboardHttpError {
    Unauthorized,
    Unavailable,
}

impl axum::response::IntoResponse for DashboardHttpError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
