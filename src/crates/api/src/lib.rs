//! Modern HTTP API adapter for `PVLog`.

#![forbid(unsafe_code)]

use axum::{Json, Router, routing::get};
use serde::Serialize;
use utoipa::ToSchema;

mod user_lifecycle;

pub use user_lifecycle::user_lifecycle_router;

/// Creates the versioned HTTP application.
pub fn router(version: &'static str) -> Router {
    Router::new().route(
        "/api/v1/health/live",
        get(move || async move {
            Json(HealthStatus {
                status: "ok",
                version,
            })
        }),
    )
}

/// Successful process liveness response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthStatus {
    /// Stable machine-readable status.
    pub status: &'static str,
    /// Running application version.
    pub version: &'static str,
}
