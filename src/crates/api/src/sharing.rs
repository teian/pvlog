//! Public sharing links and the unauthenticated views they open.
//!
//! A shared system is readable without a session, so these responses carry only
//! what a stranger may see: the name, production, and a location no more precise
//! than the owner allowed. Serial numbers, channel keys, equipment identifiers,
//! and anything about people stay out by construction — the response types below
//! have no field to put them in.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{ApiScope, Permission, SystemId, UserId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// Sharing state of one system, as its owner sees it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingState {
    pub system_id: SystemId,
    pub link_enabled: bool,
    /// Present once a link has ever been issued, even while disabled.
    pub public_token: Option<String>,
    pub visibility: String,
}

/// Requested change to a system's sharing.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingInput {
    pub link_enabled: bool,
    /// Issues a fresh token, invalidating any link already handed out.
    #[serde(default)]
    pub rotate_token: bool,
    /// Optional new listing visibility: `private`, `unlisted`, or `public`.
    pub visibility: Option<String>,
}

/// A system as an anonymous visitor sees it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSystem {
    pub name: String,
    /// Reduced to the precision the owner configured; absent when hidden.
    pub location: Option<String>,
    pub commissioning_date: Option<String>,
    pub capacity_watts: Option<i64>,
    pub timezone: String,
    pub generation_today_wh: Option<i64>,
    pub generation_total_wh: Option<i64>,
    pub last_seen_epoch_millis: Option<i64>,
}

/// One entry of the public directory.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicDirectoryEntry {
    pub name: String,
    pub location: Option<String>,
    pub capacity_watts: Option<i64>,
    pub public_token: String,
}

#[async_trait::async_trait]
pub trait SharingApiUseCases: Send + Sync {
    /// Reads the current sharing state of a system.
    async fn state(&self, system_id: SystemId) -> Result<SharingState, SharingApiError>;
    /// Applies a sharing update (link, token rotation, visibility) and returns the new state.
    async fn update(
        &self,
        actor: UserId,
        system_id: SystemId,
        input: SharingInput,
    ) -> Result<SharingState, SharingApiError>;
    /// Resolves a sharing token to its public system view; unknown or revoked tokens yield `NotFound`.
    async fn public_system(&self, token: &str) -> Result<PublicSystem, SharingApiError>;
    /// Lists up to `limit` systems whose owners made them public.
    async fn public_directory(
        &self,
        limit: u32,
    ) -> Result<Vec<PublicDirectoryEntry>, SharingApiError>;
}

#[derive(Clone)]
struct SharingRouterState {
    service: Arc<dyn SharingApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

#[derive(Deserialize)]
struct DirectoryParameters {
    limit: Option<u32>,
}

/// Creates the owner-facing sharing routes and the two anonymous read routes.
pub fn sharing_router(
    service: Arc<dyn SharingApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/systems/{system_id}/sharing",
            get(read_sharing).put(update_sharing),
        )
        .route("/api/v1/public/systems/{token}", get(public_system))
        .route("/api/v1/public/directory", get(public_directory))
        .with_state(SharingRouterState {
            service,
            authorizer,
        })
}

/// Handles the sharing read route for owners.
async fn read_sharing(
    State(state): State<SharingRouterState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<SharingState>, SharingApiError> {
    authorize(&state, principal, system_id, false, "system.sharing.read").await?;
    Ok(Json(state.service.state(system_id).await?))
}

/// Handles the sharing update route, rejecting a visibility other than `private`, `unlisted`, or `public`.
async fn update_sharing(
    State(state): State<SharingRouterState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Json(input): Json<SharingInput>,
) -> Result<Json<SharingState>, SharingApiError> {
    let actor = authorize(&state, principal, system_id, true, "system.sharing.update").await?;
    if let Some(visibility) = input.visibility.as_deref()
        && !matches!(visibility, "private" | "unlisted" | "public")
    {
        return Err(SharingApiError::InvalidInput("visibility"));
    }
    Ok(Json(state.service.update(actor, system_id, input).await?))
}

/// Anonymous: resolves a sharing token to its system.
async fn public_system(
    State(state): State<SharingRouterState>,
    Path(token): Path<String>,
) -> Result<Json<PublicSystem>, SharingApiError> {
    Ok(Json(state.service.public_system(&token).await?))
}

/// Anonymous: lists systems their owners marked public.
async fn public_directory(
    State(state): State<SharingRouterState>,
    Query(parameters): Query<DirectoryParameters>,
) -> Result<Json<Vec<PublicDirectoryEntry>>, SharingApiError> {
    let limit = parameters.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(state.service.public_directory(limit).await?))
}

/// Checks the principal's API scope and system permission, returning the acting user; a missing principal or scope is `Forbidden`.
async fn authorize(
    state: &SharingRouterState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<UserId, SharingApiError> {
    let Extension(principal) = principal.ok_or(SharingApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::SystemsRead
        };
        if !scopes.contains(&scope) {
            return Err(SharingApiError::Forbidden);
        }
    }
    let authorized = state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            if write {
                Permission::SystemManage
            } else {
                Permission::SystemRead
            },
            action,
        )
        .await?;
    Ok(authorized.actor_user_id)
}

#[derive(Clone, Copy, Debug)]
pub enum SharingApiError {
    Forbidden,
    NotFound,
    InvalidInput(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for SharingApiError {
    /// Maps authorization failures onto the sharing API errors, hiding `NotFound` behind `Forbidden`.
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for SharingApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response with a matching status code.
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            // An unknown or revoked token is indistinguishable from a system
            // that never existed, so guessing tokens reveals nothing.
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource_not_found"),
            Self::InvalidInput(code) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", code),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "sharing_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/sharing",
                title,
                status: status.as_u16(),
                detail,
                request_id: None,
            }),
        )
            .into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
