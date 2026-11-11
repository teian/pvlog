//! Data source endpoints: the uploaders and imports that feed a system.
//!
//! Creating a data source issues an upload credential that works for this one
//! system only and returns, exactly once, a configuration the owner can paste
//! into the logger. Afterwards the source reports whether readings arrive.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{ConfigurationSnippet, ConnectionState, PortError, SourceProfile};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, DataSourceId, Permission, SystemId, UserId,
};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};

use crate::{
    AuthorizedRequest, ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// One data source with its derived connection health.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceResponse {
    pub id: DataSourceId,
    pub system_id: SystemId,
    pub name: String,
    pub profile: String,
    pub state: ConnectionState,
    /// Address uploaders send to; never contains a credential.
    pub server_url: String,
    /// Identifier of the credential issued for this source, without its secret.
    pub credential_id: Option<ApiCredentialId>,
    pub last_reading_at_epoch_millis: Option<i64>,
    pub last_received_at_epoch_millis: Option<i64>,
    pub readings_last_day: u64,
    pub created_at_epoch_millis: i64,
    pub updated_at_epoch_millis: i64,
}

/// Uploader configuration containing the cleartext credential; returned once.
#[derive(Debug)]
pub struct DataSourceConfiguration {
    pub server_url: String,
    pub system_id: SystemId,
    pub api_key: SecretString,
    pub snippets: Vec<ConfigurationSnippet>,
}

/// A data source together with the configuration issued for it, if any.
#[derive(Debug)]
pub struct IssuedDataSource {
    pub data_source: DataSourceResponse,
    /// Absent for profiles that need no credential.
    pub configuration: Option<DataSourceConfiguration>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigurationBody {
    server_url: String,
    system_id: SystemId,
    api_key: String,
    snippets: Vec<ConfigurationSnippet>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IssuedBody {
    data_source: DataSourceResponse,
    configuration: Option<ConfigurationBody>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataSourceInput {
    pub name: String,
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameDataSourceInput {
    pub name: String,
}

/// Data source lifecycle boundary implemented by the runtime composition layer.
#[async_trait::async_trait]
pub trait DataSourceApiUseCases: Send + Sync {
    fn profiles(&self) -> Vec<SourceProfile>;
    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<DataSourceResponse>, DataSourceApiError>;
    async fn read(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<DataSourceResponse, DataSourceApiError>;
    async fn create(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        input: CreateDataSourceInput,
    ) -> Result<IssuedDataSource, DataSourceApiError>;
    async fn rename(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
        input: RenameDataSourceInput,
    ) -> Result<DataSourceResponse, DataSourceApiError>;
    /// Invalidates the current credential and issues a new configuration.
    async fn regenerate(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<IssuedDataSource, DataSourceApiError>;
    async fn delete(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<(), DataSourceApiError>;
}

#[derive(Clone)]
struct DataSourceState {
    service: Arc<dyn DataSourceApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the source profile catalog and the per-system data source routes.
pub fn data_sources_router(
    service: Arc<dyn DataSourceApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/data-source-profiles", get(profiles))
        .route(
            "/api/v1/systems/{system_id}/data-sources",
            get(list).post(create),
        )
        .route(
            "/api/v1/systems/{system_id}/data-sources/{data_source_id}",
            get(read).put(rename).delete(remove),
        )
        .route(
            "/api/v1/systems/{system_id}/data-sources/{data_source_id}/credential",
            post(regenerate),
        )
        .with_state(DataSourceState {
            service,
            authorizer,
        })
}

async fn profiles(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<Vec<SourceProfile>>, DataSourceApiError> {
    // The catalog is not secret, but it is only useful to a signed-in client.
    let _ = principal.ok_or(DataSourceApiError::Forbidden)?;
    Ok(Json(state.service.profiles()))
}

async fn list(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<Vec<DataSourceResponse>>, DataSourceApiError> {
    let authorized = authorize(
        &state,
        principal,
        system_id,
        false,
        "system.data_source.list",
    )
    .await?;
    Ok(Json(
        state.service.list(authorized.account_id, system_id).await?,
    ))
}

async fn read(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, id)): Path<(SystemId, DataSourceId)>,
) -> Result<Json<DataSourceResponse>, DataSourceApiError> {
    let authorized = authorize(
        &state,
        principal,
        system_id,
        false,
        "system.data_source.read",
    )
    .await?;
    Ok(Json(
        state
            .service
            .read(authorized.account_id, system_id, id)
            .await?,
    ))
}

async fn create(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Json(input): Json<CreateDataSourceInput>,
) -> Result<Response, DataSourceApiError> {
    let authorized = authorize(
        &state,
        principal,
        system_id,
        true,
        "system.data_source.create",
    )
    .await?;
    let issued = state
        .service
        .create(
            authorized.actor_user_id,
            authorized.account_id,
            system_id,
            input,
        )
        .await?;
    Ok(one_time(StatusCode::CREATED, issued))
}

async fn rename(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, id)): Path<(SystemId, DataSourceId)>,
    Json(input): Json<RenameDataSourceInput>,
) -> Result<Json<DataSourceResponse>, DataSourceApiError> {
    let authorized = authorize(
        &state,
        principal,
        system_id,
        true,
        "system.data_source.rename",
    )
    .await?;
    Ok(Json(
        state
            .service
            .rename(
                authorized.actor_user_id,
                authorized.account_id,
                system_id,
                id,
                input,
            )
            .await?,
    ))
}

async fn regenerate(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, id)): Path<(SystemId, DataSourceId)>,
) -> Result<Response, DataSourceApiError> {
    let authorized = authorize(
        &state,
        principal,
        system_id,
        true,
        "system.data_source.credential.regenerate",
    )
    .await?;
    let issued = state
        .service
        .regenerate(
            authorized.actor_user_id,
            authorized.account_id,
            system_id,
            id,
        )
        .await?;
    Ok(one_time(StatusCode::OK, issued))
}

async fn remove(
    State(state): State<DataSourceState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, id)): Path<(SystemId, DataSourceId)>,
) -> Result<StatusCode, DataSourceApiError> {
    let authorized = authorize(
        &state,
        principal,
        system_id,
        true,
        "system.data_source.delete",
    )
    .await?;
    state
        .service
        .delete(
            authorized.actor_user_id,
            authorized.account_id,
            system_id,
            id,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Serializes a response that carries a cleartext credential and forbids caching it.
fn one_time(status: StatusCode, issued: IssuedDataSource) -> Response {
    let body = IssuedBody {
        data_source: issued.data_source,
        configuration: issued.configuration.map(|configuration| ConfigurationBody {
            server_url: configuration.server_url,
            system_id: configuration.system_id,
            api_key: configuration.api_key.expose_secret().to_owned(),
            snippets: configuration.snippets,
        }),
    };
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Requires system read or management permission, and the matching scope for an API key.
///
/// A credential restricted to one system never manages data sources: it could
/// otherwise mint further credentials for itself.
async fn authorize(
    state: &DataSourceState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<AuthorizedRequest, DataSourceApiError> {
    let Extension(principal) = principal.ok_or(DataSourceApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential {
        scopes,
        system_id: restricted,
        ..
    } = &principal
    {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::SystemsRead
        };
        if !scopes.contains(&scope) || restricted.is_some() {
            return Err(DataSourceApiError::Forbidden);
        }
    }
    Ok(state
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
        .await?)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataSourceApiError {
    Forbidden,
    NotFound,
    /// Names the invalid input.
    InvalidInput(&'static str),
    /// A live data source of the system already has this name.
    NameTaken,
    Unavailable,
}

impl From<RequestAuthorizationError> for DataSourceApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<PortError> for DataSourceApiError {
    fn from(_: PortError) -> Self {
        Self::Unavailable
    }
}

impl IntoResponse for DataSourceApiError {
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "data_source_not_found"),
            Self::InvalidInput(code) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_data_source",
                code,
            ),
            Self::NameTaken => (StatusCode::CONFLICT, "conflict", "data_source_name_taken"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "data_source_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/data-source",
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
