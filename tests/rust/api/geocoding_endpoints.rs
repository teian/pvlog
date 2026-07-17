use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_api::{
    GeocodingApi, GeocodingError, GeocodingResult, RequestPrincipal, geocoding_router,
};
use pvlog_domain::UserId;
use tower::ServiceExt as _;

struct Stub;

#[async_trait]
impl GeocodingApi for Stub {
    async fn search(
        &self,
        query: &str,
        language: Option<&str>,
        limit: u8,
    ) -> Result<Vec<GeocodingResult>, GeocodingError> {
        assert_eq!(query, "Marienplatz 1, Munich");
        assert_eq!(language, Some("en"));
        assert_eq!(limit, 5);
        Ok(vec![GeocodingResult {
            display_name: "Marienplatz 1, Munich, Germany".to_owned(),
            latitude: 48.1373932,
            longitude: 11.5754485,
            attribution: "© OpenStreetMap contributors".to_owned(),
        }])
    }
}

#[tokio::test]
async fn authenticated_search_returns_provider_coordinates() -> Result<(), Box<dyn Error>> {
    let app =
        geocoding_router(Arc::new(Stub)).layer(Extension(RequestPrincipal::User(UserId::new())));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/geocoding/search?q=Marienplatz%201%2C%20Munich&language=en")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(to_bytes(response.into_body(), 8_192).await?.to_vec())?;
    assert!(body.contains("48.1373932"));
    assert!(body.contains("11.5754485"));
    Ok(())
}
