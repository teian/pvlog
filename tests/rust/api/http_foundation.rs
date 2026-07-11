use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use std::error::Error;
use tower::ServiceExt as _;

#[tokio::test]
async fn api_foundation_emits_request_security_and_problem_contracts() -> Result<(), Box<dyn Error>>
{
    let response = pvlog_api::router("test")
        .oneshot(
            Request::builder()
                .uri("/api/v1/missing")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    assert!(response.headers().contains_key("x-request-id"));
    assert!(
        response
            .headers()
            .contains_key(header::CONTENT_SECURITY_POLICY)
    );
    let response = pvlog_api::router("test")
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/live")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    let response = pvlog_api::router("test-version")
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/version")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}
