//! Account role inspection endpoints protected by `RoleManage`.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use pvlog_domain::{AccountId, Permission, RoleId};
use serde::{Deserialize, Serialize};

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
    async fn create_role(
        &self,
        actor: pvlog_domain::UserId,
        account_id: AccountId,
        input: RoleInput,
    ) -> Result<RoleResponse, RbacApiError>;
    async fn update_role(
        &self,
        actor: pvlog_domain::UserId,
        account_id: AccountId,
        role_id: RoleId,
        input: RoleInput,
    ) -> Result<RoleResponse, RbacApiError>;
    async fn delete_role(
        &self,
        actor: pvlog_domain::UserId,
        account_id: AccountId,
        role_id: RoleId,
    ) -> Result<(), RbacApiError>;
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
        .route(
            "/api/v1/accounts/{account_id}/roles",
            get(roles).post(create_role),
        )
        .route(
            "/api/v1/accounts/{account_id}/roles/{role_id}",
            patch(update_role).delete(delete_role),
        )
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleInput {
    pub name: String,
    pub permissions: BTreeSet<Permission>,
    #[serde(default)]
    pub parent_role_ids: BTreeSet<RoleId>,
}

async fn create_role(
    State(state): State<RbacState>,
    Path(account_id): Path<AccountId>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(input): Json<RoleInput>,
) -> Result<Response, RbacApiError> {
    let actor = authorize_actor(&state, principal, account_id, "role.create").await?;
    Ok((
        StatusCode::CREATED,
        Json(state.service.create_role(actor, account_id, input).await?),
    )
        .into_response())
}

async fn update_role(
    State(state): State<RbacState>,
    Path((account_id, role_id)): Path<(AccountId, RoleId)>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(input): Json<RoleInput>,
) -> Result<Json<RoleResponse>, RbacApiError> {
    let actor = authorize_actor(&state, principal, account_id, "role.update").await?;
    Ok(Json(
        state
            .service
            .update_role(actor, account_id, role_id, input)
            .await?,
    ))
}

async fn delete_role(
    State(state): State<RbacState>,
    Path((account_id, role_id)): Path<(AccountId, RoleId)>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<StatusCode, RbacApiError> {
    let actor = authorize_actor(&state, principal, account_id, "role.delete").await?;
    state
        .service
        .delete_role(actor, account_id, role_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn authorize_actor(
    state: &RbacState,
    principal: Option<Extension<RequestPrincipal>>,
    account_id: AccountId,
    action: &'static str,
) -> Result<pvlog_domain::UserId, RbacApiError> {
    let Extension(principal) = principal.ok_or(RbacApiError::Forbidden)?;
    if !matches!(principal, RequestPrincipal::User(_)) {
        return Err(RbacApiError::Forbidden);
    }
    state
        .authorizer
        .authorize_account(
            principal_identity(&principal),
            account_id,
            Permission::RoleManage,
            action,
        )
        .await
        .map(|authorized| authorized.actor_user_id)
        .map_err(Into::into)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RbacApiError {
    Forbidden,
    NotFound,
    Invalid,
    Conflict,
    Unavailable,
}

impl From<RequestAuthorizationError> for RbacApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for RbacApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => axum::http::StatusCode::FORBIDDEN,
            Self::NotFound => axum::http::StatusCode::NOT_FOUND,
            Self::Invalid => axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict => axum::http::StatusCode::CONFLICT,
            Self::Unavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
