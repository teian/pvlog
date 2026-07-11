//! Browser-session identity-link inspection endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{ConnectorId, ExternalIdentityId, UserId};
use serde::Serialize;

use crate::RequestPrincipal;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedIdentityResponse {
    pub id: ExternalIdentityId,
    pub connector_id: ConnectorId,
    pub subject: String,
    pub linked_at_epoch_millis: i64,
    pub last_login_at_epoch_millis: Option<i64>,
}

#[async_trait]
pub trait IdentityApiUseCases: Send + Sync {
    async fn list_identities(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LinkedIdentityResponse>, IdentityApiError>;
}

#[derive(Clone)]
struct IdentityState {
    service: Arc<dyn IdentityApiUseCases>,
}

pub fn identities_router(service: Arc<dyn IdentityApiUseCases>) -> Router {
    Router::new()
        .route("/api/v1/users/me/identities", get(list))
        .with_state(IdentityState { service })
}

async fn list(
    State(state): State<IdentityState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<LinkedIdentityResponse>>, IdentityApiError> {
    let Some(Extension(RequestPrincipal::User(user_id))) = principal else {
        return Err(IdentityApiError::Forbidden);
    };
    Ok(Json(state.service.list_identities(user_id).await?))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityApiError {
    Forbidden,
    Unavailable,
}

impl IntoResponse for IdentityApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => axum::http::StatusCode::FORBIDDEN,
            Self::Unavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
