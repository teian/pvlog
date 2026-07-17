//! Cookie-authenticated lifecycle endpoints for current-account bearer API keys.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, Permission, PrincipalId, UserId};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};

use crate::{ModernRequestAuthorizer, Problem, RequestAuthorizationError, RequestPrincipal};

/// Safe API-key metadata returned after creation and during listing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountApiKeyMetadata {
    pub id: ApiCredentialId,
    pub name: String,
    pub scopes: BTreeSet<AccountApiKeyScope>,
    pub created_at_epoch_millis: i64,
    pub expires_at_epoch_millis: Option<i64>,
    pub revoked_at_epoch_millis: Option<i64>,
}

/// Public least-privilege permission vocabulary for account API keys.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AccountApiKeyScope {
    #[serde(rename = "systems:read")]
    SystemsRead,
    #[serde(rename = "systems:write")]
    SystemsWrite,
    #[serde(rename = "telemetry:read")]
    TelemetryRead,
    #[serde(rename = "telemetry:write")]
    TelemetryWrite,
}

impl From<AccountApiKeyScope> for ApiScope {
    fn from(value: AccountApiKeyScope) -> Self {
        match value {
            AccountApiKeyScope::SystemsRead => Self::SystemsRead,
            AccountApiKeyScope::SystemsWrite => Self::SystemsWrite,
            AccountApiKeyScope::TelemetryRead => Self::TelemetryRead,
            AccountApiKeyScope::TelemetryWrite => Self::TelemetryWrite,
        }
    }
}

impl TryFrom<ApiScope> for AccountApiKeyScope {
    type Error = AccountApiKeyError;

    fn try_from(value: ApiScope) -> Result<Self, Self::Error> {
        match value {
            ApiScope::SystemsRead => Ok(Self::SystemsRead),
            ApiScope::SystemsWrite => Ok(Self::SystemsWrite),
            ApiScope::TelemetryRead => Ok(Self::TelemetryRead),
            ApiScope::TelemetryWrite => Ok(Self::TelemetryWrite),
            ApiScope::IntegrationsManage => Err(AccountApiKeyError::Invalid),
        }
    }
}

/// One-time creation result; the cleartext value is never used in list responses.
#[derive(Debug)]
pub struct IssuedAccountApiKey {
    pub api_key: SecretString,
    pub credential: AccountApiKeyMetadata,
}

/// Account API-key lifecycle boundary implemented by the runtime composition layer.
#[async_trait]
pub trait AccountApiKeyUseCases: Send + Sync {
    async fn issue(
        &self,
        actor: UserId,
        account_id: AccountId,
        name: String,
        scopes: BTreeSet<ApiScope>,
        expires_at: Option<i64>,
    ) -> Result<IssuedAccountApiKey, AccountApiKeyError>;
    async fn list(
        &self,
        actor: UserId,
        account_id: AccountId,
    ) -> Result<Vec<AccountApiKeyMetadata>, AccountApiKeyError>;
    async fn revoke(
        &self,
        actor: UserId,
        account_id: AccountId,
        id: ApiCredentialId,
    ) -> Result<(), AccountApiKeyError>;
}

#[derive(Clone)]
struct AccountApiKeyState {
    service: Arc<dyn AccountApiKeyUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Builds the current-account API-key lifecycle router.
pub fn account_api_keys_router(
    service: Arc<dyn AccountApiKeyUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/account/api-keys", post(issue).get(list))
        .route(
            "/api/v1/account/api-keys/{api_key_id}",
            axum::routing::delete(revoke),
        )
        .with_state(AccountApiKeyState {
            service,
            authorizer,
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IssueBody {
    name: String,
    scopes: BTreeSet<AccountApiKeyScope>,
    expires_at_epoch_millis: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IssueResponse {
    api_key: String,
    credential: AccountApiKeyMetadata,
}

async fn issue(
    State(state): State<AccountApiKeyState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(body): Json<IssueBody>,
) -> Result<Response, AccountApiKeyError> {
    let (actor, account_id) = authorize(&state, principal, "account.api_key.issue").await?;
    if body.name.trim().is_empty() || body.name.chars().count() > 120 || body.scopes.is_empty() {
        return Err(AccountApiKeyError::Invalid);
    }
    let issued = state
        .service
        .issue(
            actor,
            account_id,
            body.name,
            body.scopes.into_iter().map(ApiScope::from).collect(),
            body.expires_at_epoch_millis,
        )
        .await?;
    let mut response = (
        StatusCode::CREATED,
        Json(IssueResponse {
            api_key: issued.api_key.expose_secret().to_owned(),
            credential: issued.credential,
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn list(
    State(state): State<AccountApiKeyState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<AccountApiKeyMetadata>>, AccountApiKeyError> {
    let (actor, account_id) = authorize(&state, principal, "account.api_key.list").await?;
    Ok(Json(state.service.list(actor, account_id).await?))
}

async fn revoke(
    State(state): State<AccountApiKeyState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(api_key_id): Path<ApiCredentialId>,
) -> Result<StatusCode, AccountApiKeyError> {
    let (actor, account_id) = authorize(&state, principal, "account.api_key.revoke").await?;
    state.service.revoke(actor, account_id, api_key_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn authorize(
    state: &AccountApiKeyState,
    principal: Option<Extension<RequestPrincipal>>,
    action: &'static str,
) -> Result<(UserId, AccountId), AccountApiKeyError> {
    let Extension(RequestPrincipal::User(user_id)) =
        principal.ok_or(AccountApiKeyError::Forbidden)?
    else {
        return Err(AccountApiKeyError::Forbidden);
    };
    let account_id =
        AccountId::from_uuid(user_id.as_uuid()).map_err(|_| AccountApiKeyError::Forbidden)?;
    let authorized = state
        .authorizer
        .authorize_account(
            PrincipalId::User(user_id),
            account_id,
            Permission::CredentialManage,
            action,
        )
        .await?;
    Ok((authorized.actor_user_id, authorized.account_id))
}

/// Safe account API-key lifecycle failure.
#[derive(Debug)]
pub enum AccountApiKeyError {
    Forbidden,
    Invalid,
    Conflict,
    NotFound,
    Unavailable,
}

impl From<RequestAuthorizationError> for AccountApiKeyError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for AccountApiKeyError {
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "api_key_access_denied"),
            Self::Invalid => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_api_key_request",
                "api_key_name_scopes_or_expiry_invalid",
            ),
            Self::Conflict => (StatusCode::CONFLICT, "conflict", "api_key_name_conflict"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "api_key_not_found"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "api_key_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/account-api-key",
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
