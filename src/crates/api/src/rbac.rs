//! Account role inspection endpoints protected by `RoleManage`.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch},
};
use pvlog_domain::{
    AccountId, ApiCredentialId, Permission, PrincipalId, RoleAssignmentId, RoleId, RoleScope,
    SystemId, UserId,
};
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAssignmentResponse {
    pub id: RoleAssignmentId,
    pub role_id: RoleId,
    pub principal_type: String,
    pub principal_id: uuid::Uuid,
    pub account_id: Option<AccountId>,
    pub system_id: Option<SystemId>,
    pub expires_at: Option<i64>,
}

#[async_trait]
pub trait RbacApiUseCases: Send + Sync {
    async fn instance_roles(&self) -> Result<Vec<RoleResponse>, RbacApiError>;
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
    async fn assign_role(
        &self,
        actor: UserId,
        account_id: AccountId,
        input: RoleAssignmentInput,
    ) -> Result<RoleAssignmentResponse, RbacApiError>;
    async fn assignments(
        &self,
        account_id: AccountId,
        principal: PrincipalId,
    ) -> Result<Vec<RoleAssignmentResponse>, RbacApiError>;
    async fn instance_assignments(
        &self,
        principal: PrincipalId,
    ) -> Result<Vec<RoleAssignmentResponse>, RbacApiError>;
    async fn assign_instance_role(
        &self,
        actor: UserId,
        input: RoleAssignmentInput,
    ) -> Result<RoleAssignmentResponse, RbacApiError>;
    async fn revoke_assignment(
        &self,
        actor: UserId,
        account_id: AccountId,
        assignment_id: RoleAssignmentId,
        scope: RoleScope,
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
        .route("/api/v1/admin/roles", get(instance_roles))
        .route(
            "/api/v1/admin/role-assignments",
            get(instance_assignments).post(assign_instance_role),
        )
        .route(
            "/api/v1/accounts/{account_id}/roles",
            get(roles).post(create_role),
        )
        .route(
            "/api/v1/accounts/{account_id}/role-assignments",
            get(assignments).post(assign_role),
        )
        .route(
            "/api/v1/accounts/{account_id}/role-assignments/{assignment_id}",
            delete(revoke_assignment),
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

async fn instance_roles(
    State(state): State<RbacState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<RoleResponse>>, RbacApiError> {
    authorize_instance_actor(&state, principal, "instance_role.list").await?;
    Ok(Json(state.service.instance_roles().await?))
}

async fn instance_assignments(
    State(state): State<RbacState>,
    Query(query): Query<RoleAssignmentListQuery>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<RoleAssignmentResponse>>, RbacApiError> {
    authorize_instance_actor(&state, principal, "instance_role_assignment.list").await?;
    Ok(Json(
        state
            .service
            .instance_assignments(query.principal()?)
            .await?,
    ))
}

async fn assign_instance_role(
    State(state): State<RbacState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(input): Json<RoleAssignmentInput>,
) -> Result<Response, RbacApiError> {
    let actor = authorize_instance_actor(&state, principal, "instance_role.assign").await?;
    Ok((
        StatusCode::CREATED,
        Json(state.service.assign_instance_role(actor, input).await?),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoleAssignmentListQuery {
    principal_type: AssignmentPrincipalType,
    principal_id: uuid::Uuid,
}

impl RoleAssignmentListQuery {
    fn principal(&self) -> Result<PrincipalId, RbacApiError> {
        match self.principal_type {
            AssignmentPrincipalType::User => UserId::from_uuid(self.principal_id)
                .map(PrincipalId::User)
                .map_err(|_| RbacApiError::Invalid),
            AssignmentPrincipalType::ApiCredential => ApiCredentialId::from_uuid(self.principal_id)
                .map(PrincipalId::ApiCredential)
                .map_err(|_| RbacApiError::Invalid),
        }
    }
}

async fn assignments(
    State(state): State<RbacState>,
    Path(account_id): Path<AccountId>,
    Query(query): Query<RoleAssignmentListQuery>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<RoleAssignmentResponse>>, RbacApiError> {
    let Extension(principal) = principal.ok_or(RbacApiError::Forbidden)?;
    state
        .authorizer
        .authorize_account(
            principal_identity(&principal)?,
            account_id,
            Permission::RoleManage,
            "role_assignment.list",
        )
        .await?;
    Ok(Json(
        state
            .service
            .assignments(account_id, query.principal()?)
            .await?,
    ))
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
            principal_identity(&principal)?,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentPrincipalType {
    User,
    ApiCredential,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAssignmentInput {
    pub role_id: RoleId,
    pub principal_type: AssignmentPrincipalType,
    pub principal_id: uuid::Uuid,
    pub system_id: Option<SystemId>,
    pub expires_at: Option<i64>,
}

impl RoleAssignmentInput {
    /// Converts the untrusted HTTP identifier into a typed RBAC principal.
    ///
    /// # Errors
    ///
    /// Returns [`RbacApiError::Invalid`] if the identifier is not a valid typed ID.
    pub fn principal(&self) -> Result<PrincipalId, RbacApiError> {
        match self.principal_type {
            AssignmentPrincipalType::User => UserId::from_uuid(self.principal_id)
                .map(PrincipalId::User)
                .map_err(|_| RbacApiError::Invalid),
            AssignmentPrincipalType::ApiCredential => ApiCredentialId::from_uuid(self.principal_id)
                .map(PrincipalId::ApiCredential)
                .map_err(|_| RbacApiError::Invalid),
        }
    }
    /// Builds the account or system scope implied by the request path and body.
    #[must_use]
    pub fn scope(&self, account_id: AccountId) -> RoleScope {
        self.system_id
            .map_or(RoleScope::Account(account_id), |system_id| {
                RoleScope::System {
                    account_id,
                    system_id,
                }
            })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokeAssignmentQuery {
    system_id: Option<SystemId>,
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

async fn assign_role(
    State(state): State<RbacState>,
    Path(account_id): Path<AccountId>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(input): Json<RoleAssignmentInput>,
) -> Result<Response, RbacApiError> {
    let actor = authorize_actor(&state, principal, account_id, "role.assign").await?;
    Ok((
        StatusCode::CREATED,
        Json(state.service.assign_role(actor, account_id, input).await?),
    )
        .into_response())
}

async fn revoke_assignment(
    State(state): State<RbacState>,
    Path((account_id, assignment_id)): Path<(AccountId, RoleAssignmentId)>,
    Query(query): Query<RevokeAssignmentQuery>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<StatusCode, RbacApiError> {
    let actor = authorize_actor(&state, principal, account_id, "role.revoke").await?;
    let scope = query
        .system_id
        .map_or(RoleScope::Account(account_id), |system_id| {
            RoleScope::System {
                account_id,
                system_id,
            }
        });
    state
        .service
        .revoke_assignment(actor, account_id, assignment_id, scope)
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
            principal_identity(&principal)?,
            account_id,
            Permission::RoleManage,
            action,
        )
        .await
        .map(|authorized| authorized.actor_user_id)
        .map_err(Into::into)
}

async fn authorize_instance_actor(
    state: &RbacState,
    principal: Option<Extension<RequestPrincipal>>,
    action: &'static str,
) -> Result<UserId, RbacApiError> {
    let Extension(principal) = principal.ok_or(RbacApiError::Forbidden)?;
    if !matches!(principal, RequestPrincipal::User(_)) {
        return Err(RbacApiError::Forbidden);
    }
    state
        .authorizer
        .authorize_instance(
            principal_identity(&principal)?,
            Permission::RoleManage,
            action,
        )
        .await
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
