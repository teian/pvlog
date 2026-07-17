//! Account audit-log read endpoint with RBAC authorization.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{AccountId, Permission};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventResponse {
    pub id: pvlog_domain::AuditEventId,
    pub occurred_at: i64,
    pub actor_type: String,
    pub actor_id: Option<uuid::Uuid>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<uuid::Uuid>,
    pub outcome: String,
    pub safe_metadata: Value,
}

#[async_trait]
pub trait AuditApiUseCases: Send + Sync {
    async fn account_audit(
        &self,
        account_id: AccountId,
        limit: u32,
    ) -> Result<Vec<AuditEventResponse>, AuditApiError>;
}

#[derive(Clone)]
struct AuditState {
    service: Arc<dyn AuditApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

pub fn audit_router(
    service: Arc<dyn AuditApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/accounts/{account_id}/audit-events",
            get(account_audit),
        )
        .with_state(AuditState {
            service,
            authorizer,
        })
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<u32>,
}

async fn account_audit(
    State(state): State<AuditState>,
    Path(account_id): Path<AccountId>,
    Query(query): Query<AuditQuery>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<AuditEventResponse>>, AuditApiError> {
    let Extension(principal) = principal.ok_or(AuditApiError::Forbidden)?;
    state
        .authorizer
        .authorize_account(
            principal_identity(&principal)?,
            account_id,
            Permission::AuditRead,
            "audit.list",
        )
        .await?;
    Ok(Json(
        state
            .service
            .account_audit(account_id, query.limit.unwrap_or(100).clamp(1, 500))
            .await?,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditApiError {
    Forbidden,
    Unavailable,
}

impl From<RequestAuthorizationError> for AuditApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for AuditApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => axum::http::StatusCode::FORBIDDEN,
            Self::Unavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
