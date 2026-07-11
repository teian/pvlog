//! Modern HTTP API adapter for `PVLog`.

#![forbid(unsafe_code)]

use std::time::Duration;

use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::{Json, Router, middleware, routing::get};
use serde::Serialize;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
};
use utoipa::ToSchema;

mod local_password;
mod managed_resources;
mod problem;
mod systems;
mod telemetry;
mod user_lifecycle;

pub use local_password::local_password_router;
pub use managed_resources::managed_resources_router;
pub use problem::Problem;
pub use systems::systems_router;
pub use telemetry::telemetry_router;
pub use user_lifecycle::user_lifecycle_router;

/// Creates the versioned HTTP application.
pub fn router(version: &'static str) -> Router {
    let request_id = HeaderName::from_static("x-request-id");
    Router::new()
        .route(
            "/api/v1/health/live",
            get(move || async move {
                Json(HealthStatus {
                    status: "ok",
                    version,
                })
            }),
        )
        .fallback(problem::not_found)
        .layer(middleware::from_fn(problem::negotiate))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
            ),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(
            CorsLayer::new()
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                ])
                .allow_headers([
                    header::AUTHORIZATION,
                    header::CONTENT_TYPE,
                    HeaderName::from_static("x-csrf-token"),
                ]),
        )
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(ConcurrencyLimitLayer::new(256))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(CompressionLayer::new())
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}

/// Successful process liveness response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthStatus {
    /// Stable machine-readable status.
    pub status: &'static str,
    /// Running application version.
    pub version: &'static str,
}
