//! Authenticated forward-geocoding boundary for the system wizard.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::ApiScope;
use serde::{Deserialize, Serialize};

use crate::RequestPrincipal;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeocodingResult {
    pub display_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub attribution: String,
}

#[async_trait]
pub trait GeocodingApi: Send + Sync {
    async fn search(
        &self,
        query: &str,
        language: Option<&str>,
        limit: u8,
    ) -> Result<Vec<GeocodingResult>, GeocodingError>;
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    language: Option<String>,
    limit: Option<u8>,
}

pub fn geocoding_router(service: Arc<dyn GeocodingApi>) -> Router {
    Router::new()
        .route("/api/v1/geocoding/search", get(search))
        .with_state(service)
}

async fn search(
    State(service): State<Arc<dyn GeocodingApi>>,
    principal: Option<Extension<RequestPrincipal>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<GeocodingResult>>, GeocodingError> {
    match principal.map(|Extension(value)| value) {
        Some(RequestPrincipal::User(_)) => {}
        Some(RequestPrincipal::ApiCredential { scopes, .. })
            if scopes.contains(&ApiScope::SystemsRead)
                || scopes.contains(&ApiScope::SystemsWrite) => {}
        Some(RequestPrincipal::ApiCredential { .. }) | None => {
            return Err(GeocodingError::Forbidden);
        }
    }
    let value = query.q.trim();
    if value.is_empty() || value.len() > 160 {
        return Err(GeocodingError::InvalidInput);
    }
    Ok(Json(
        service
            .search(
                value,
                query.language.as_deref(),
                query.limit.unwrap_or(5).clamp(1, 5),
            )
            .await?,
    ))
}

#[derive(Debug)]
pub enum GeocodingError {
    Forbidden,
    InvalidInput,
    Unavailable,
}

impl IntoResponse for GeocodingError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unavailable => StatusCode::BAD_GATEWAY,
        }
        .into_response()
    }
}
