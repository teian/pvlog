//! Modern HTTP API adapter for `PVLog`.

#![forbid(unsafe_code)]

use std::time::Duration;

use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::{
    Json, Router, middleware,
    middleware::Next,
    response::Response,
    routing::{any, get},
};
use opentelemetry::KeyValue;
use serde::Serialize;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::ToSchema;

mod analytics;
mod audit;
mod authentication;
mod authorization;
mod connectors;
mod dashboard;
mod identities;
mod inverters;
mod local_password;
mod managed_resources;
mod notifications;
mod problem;
mod rbac;
mod readiness;
mod sessions;
mod systems;
mod telemetry;
mod user_lifecycle;

pub use analytics::analytics_router;
pub use audit::{AuditApiError, AuditApiUseCases, AuditEventResponse, audit_router};
pub use authentication::{
    RequestAuthenticationError, RequestAuthenticator, RequestPrincipal, session_cookie_token,
    with_request_authentication,
};
pub use authorization::{
    AuthorizedRequest, ModernRequestAuthorizer, RequestAuthorizationError, actor_user_id,
    principal_identity,
};
pub use connectors::{
    ConnectorAdminError, ConnectorAdminResponse, ConnectorAdminUseCases, connectors_router,
};
pub use dashboard::{
    DashboardAlertResponse, DashboardApiError, DashboardApiUseCases, DashboardIngestionResponse,
    DashboardResponse, dashboard_router,
};
pub use identities::{
    IdentityApiError, IdentityApiUseCases, LinkedIdentityResponse, identities_router,
};
pub use inverters::{
    InverterApiError, InverterApiUseCases, InverterInput, InverterResponse, PvStringInput,
    PvStringResponse, inverters_router,
};
pub use local_password::local_password_router;
pub use managed_resources::managed_resources_router;
pub use notifications::{NotificationApiError, NotificationApiUseCases, notifications_router};
pub use problem::Problem;
pub use rbac::{
    AssignmentPrincipalType, RbacApiError, RbacApiUseCases, RoleAssignmentInput,
    RoleAssignmentResponse, RoleInput, RoleResponse, rbac_router,
};
pub use readiness::{ReadinessError, ReadinessResponse, ReadinessUseCases, readiness_router};
pub use sessions::{
    SessionApiError, SessionBootstrap, SessionBootstrapUseCases, SessionConnector, SessionUser,
    sessions_router,
};
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
        .route(
            "/api/v1/health/version",
            get(move || async move {
                Json(HealthStatus {
                    status: "ok",
                    version,
                })
            }),
        )
        .route("/api/v1", any(problem::not_found))
        .route("/api/v1/{*path}", any(problem::not_found))
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
        .layer(middleware::from_fn(record_http_metrics))
        .layer(SetSensitiveRequestHeadersLayer::new([
            header::AUTHORIZATION,
            header::COOKIE,
        ]))
        .layer(TraceLayer::new_for_http().make_span_with(
            |request: &axum::http::Request<axum::body::Body>| {
                let request_id = request
                    .headers()
                    .get("x-request-id")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("missing");
                tracing::info_span!(
                    "http.request",
                    method = %request.method(),
                    uri = %request.uri(),
                    request_id
                )
            },
        ))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}

async fn record_http_metrics(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    let meter = opentelemetry::global::meter("pvlog-http");
    let attributes = [
        KeyValue::new("http.request.method", method),
        KeyValue::new(
            "http.response.status_code",
            i64::from(response.status().as_u16()),
        ),
    ];
    meter
        .u64_counter("http.server.request.count")
        .build()
        .add(1, &attributes);
    meter
        .f64_histogram("http.server.request.duration")
        .with_unit("s")
        .build()
        .record(started.elapsed().as_secs_f64(), &attributes);
    response
}

/// Successful process liveness response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthStatus {
    /// Stable machine-readable status.
    pub status: &'static str,
    /// Running application version.
    pub version: &'static str,
}
