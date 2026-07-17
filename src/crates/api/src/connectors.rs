//! Read-only, secret-safe connector administration endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::Permission;
use serde::Serialize;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorAdminResponse {
    pub id: String,
    pub display_name: String,
    pub protocol: String,
    pub enabled: bool,
    pub authorization_endpoint: Option<String>,
    pub scopes: Vec<String>,
}

#[async_trait]
pub trait ConnectorAdminUseCases: Send + Sync {
    async fn connectors(&self) -> Result<Vec<ConnectorAdminResponse>, ConnectorAdminError>;
}

#[derive(Clone)]
struct ConnectorState {
    service: Arc<dyn ConnectorAdminUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Exposes non-secret connector metadata to authorized browser-session instance administrators.
pub fn connectors_router(
    service: Arc<dyn ConnectorAdminUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/admin/auth-connectors", get(list))
        .with_state(ConnectorState {
            service,
            authorizer,
        })
}

async fn list(
    State(state): State<ConnectorState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<ConnectorAdminResponse>>, ConnectorAdminError> {
    let Extension(principal) = principal.ok_or(ConnectorAdminError::Forbidden)?;
    state
        .authorizer
        .authorize_instance(
            principal_identity(&principal)?,
            Permission::InstanceManage,
            "connector.list",
        )
        .await?;
    Ok(Json(state.service.connectors().await?))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectorAdminError {
    Forbidden,
    Unavailable,
}

impl From<RequestAuthorizationError> for ConnectorAdminError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for ConnectorAdminError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => axum::http::StatusCode::FORBIDDEN,
            Self::Unavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
