//! Account role inspection endpoints protected by `RoleManage`.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{AccountId, Permission, RoleId};
use serde::Serialize;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleResponse {
    pub id: RoleId,
    pub name: String,
    pub kind: String,
    pub permissions: BTreeSet<Permission>,
    pub parent_role_ids: BTreeSet<RoleId>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[async_trait]
pub trait RbacApiUseCases: Send + Sync {
    async fn roles(&self, account_id: AccountId) -> Result<Vec<RoleResponse>, RbacApiError>;
}

#[derive(Clone)]
struct RbacState {
    service: Arc<dyn RbacApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

pub fn rbac_router(
    service: Arc<dyn RbacApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/accounts/{account_id}/roles", get(roles))
        .with_state(RbacState {
            service,
            authorizer,
        })
}

async fn roles(
    State(state): State<RbacState>,
    Path(account_id): Path<AccountId>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<RoleResponse>>, RbacApiError> {
    let Extension(principal) = principal.ok_or(RbacApiError::Forbidden)?;
    state
        .authorizer
        .authorize_account(
            principal_identity(&principal),
            account_id,
            Permission::RoleManage,
            "role.list",
        )
        .await?;
    Ok(Json(state.service.roles(account_id).await?))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RbacApiError {
    Forbidden,
    Unavailable,
}

impl From<RequestAuthorizationError> for RbacApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for RbacApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => axum::http::StatusCode::FORBIDDEN,
            Self::Unavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
