//! Production web application embedded in the `PVLog` executable.

use std::borrow::Cow;
use std::path::Path;

use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse as _, Response},
    routing::get,
};
use rust_embed::RustEmbed;
use serde_json::json;

#[derive(RustEmbed)]
#[folder = "../../../embedded-ui/"]
struct UiAssets;

#[derive(Clone)]
struct UiState {
    version: &'static str,
    telemetry_enabled: bool,
    telemetry_endpoint: Option<String>,
}

/// Creates routes for runtime configuration and embedded SPA assets.
pub fn router(
    version: &'static str,
    telemetry_enabled: bool,
    telemetry_endpoint: Option<String>,
) -> Router {
    Router::new()
        .route("/runtime-config.json", get(runtime_config))
        .fallback(asset_or_spa)
        .with_state(UiState {
            version,
            telemetry_enabled,
            telemetry_endpoint,
        })
}

async fn runtime_config(State(state): State<UiState>) -> Response {
    let mut telemetry = json!({
        "enabled": state.telemetry_enabled,
        "headers": {},
        "serviceName": "pvlog-ui",
        "serviceVersion": state.version,
    });
    if let Some(endpoint) = state.telemetry_endpoint {
        telemetry["endpoint"] = json!(endpoint);
    }
    let mut response = Json(json!({
        "apiBaseUrl": "/api/v1",
        "telemetry": telemetry,
    }))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn asset_or_spa(request: Request) -> Response {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let requested = request.uri().path().trim_start_matches('/');
    let path = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    if let Some(asset) = UiAssets::get(path) {
        return embedded_response(path, asset.data, request.method() == Method::HEAD);
    }
    if !path.contains('.')
        && accepts_html(&request)
        && let Some(index) = UiAssets::get("index.html")
    {
        return embedded_response("index.html", index.data, request.method() == Method::HEAD);
    }
    StatusCode::NOT_FOUND.into_response()
}

fn embedded_response(path: &str, data: Cow<'static, [u8]>, head_only: bool) -> Response {
    let body = if head_only {
        Body::empty()
    } else {
        Body::from(data.into_owned())
    };
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(path)),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if path == "index.html" || path.starts_with("openapi/") {
            "no-cache"
        } else {
            "public, max-age=31536000, immutable"
        }),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn accepts_html(request: &Request) -> bool {
    request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| value.contains("text/html") || value.contains("*/*"))
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("yaml" | "yml") => "application/yaml; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
