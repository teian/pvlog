use std::error::Error;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use serde_json::Value;
use tower::ServiceExt as _;

#[tokio::test]
async fn embedded_ui_serves_spa_and_runtime_configuration() -> Result<(), Box<dyn Error>> {
    let app = pvlog::embedded_ui::router("1.2.3", false, None);
    let runtime = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/runtime-config.json")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(runtime.status(), StatusCode::OK);
    assert_eq!(
        runtime.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store"))
    );
    let body: Value = serde_json::from_slice(&to_bytes(runtime.into_body(), 16_384).await?)?;
    assert_eq!(body["apiBaseUrl"], "/api/v1");
    assert_eq!(body["telemetry"]["serviceVersion"], "1.2.3");
    assert!(body["telemetry"].get("endpoint").is_none());

    let spa = app
        .oneshot(
            Request::builder()
                .uri("/administration")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(spa.status(), StatusCode::OK);
    assert_eq!(
        spa.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static(
            "text/html; charset=utf-8"
        ))
    );
    Ok(())
}
