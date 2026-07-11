use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_application::{
    CreateManagedResource, ManagedResource, ManagedResourceError, ManagedResourceKind,
    ManagedResourceService, ModernApiActor,
};
use pvlog_domain::{AccountId, ApiScope, SystemId};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
struct ResourceState {
    service: Arc<dyn ManagedResourceService>,
}

pub fn managed_resources_router(service: Arc<dyn ManagedResourceService>) -> Router {
    Router::new()
        .route(
            "/api/v1/accounts/{account_id}/systems/{system_id}/equipment",
            get(list_equipment).post(create_equipment),
        )
        .route(
            "/api/v1/accounts/{account_id}/systems/{system_id}/tariffs",
            get(list_tariffs).post(create_tariff),
        )
        .route(
            "/api/v1/accounts/{account_id}/systems/{system_id}/channels",
            get(list_channels).post(create_channel),
        )
        .route(
            "/api/v1/accounts/{account_id}/memberships",
            get(list_memberships).post(create_membership),
        )
        .route(
            "/api/v1/accounts/{account_id}/credentials",
            get(list_credentials).post(create_credential),
        )
        .with_state(ResourceState { service })
}

macro_rules! system_handlers {
    ($list:ident, $create:ident, $kind:expr) => {
        async fn $list(
            State(state): State<ResourceState>,
            actor: Option<Extension<ModernApiActor>>,
            Path((account_id, system_id)): Path<(AccountId, SystemId)>,
        ) -> Result<Json<Vec<ManagedResource>>, ResourceApiError> {
            let actor = require_actor(actor, ApiScope::SystemsRead)?;
            Ok(Json(
                state
                    .service
                    .list(&actor, account_id, Some(system_id), $kind)
                    .await?,
            ))
        }
        async fn $create(
            State(state): State<ResourceState>,
            actor: Option<Extension<ModernApiActor>>,
            Path((account_id, system_id)): Path<(AccountId, SystemId)>,
            Json(attributes): Json<Value>,
        ) -> Result<Response, ResourceApiError> {
            create(
                state,
                require_actor(actor, ApiScope::SystemsWrite)?,
                account_id,
                Some(system_id),
                $kind,
                attributes,
            )
            .await
        }
    };
}
macro_rules! account_handlers {
    ($list:ident, $create:ident, $kind:expr) => {
        async fn $list(
            State(state): State<ResourceState>,
            actor: Option<Extension<ModernApiActor>>,
            Path(account_id): Path<AccountId>,
        ) -> Result<Json<Vec<ManagedResource>>, ResourceApiError> {
            let actor = require_actor(actor, ApiScope::SystemsRead)?;
            Ok(Json(
                state.service.list(&actor, account_id, None, $kind).await?,
            ))
        }
        async fn $create(
            State(state): State<ResourceState>,
            actor: Option<Extension<ModernApiActor>>,
            Path(account_id): Path<AccountId>,
            Json(attributes): Json<Value>,
        ) -> Result<Response, ResourceApiError> {
            create(
                state,
                require_actor(actor, ApiScope::SystemsWrite)?,
                account_id,
                None,
                $kind,
                attributes,
            )
            .await
        }
    };
}
system_handlers!(
    list_equipment,
    create_equipment,
    ManagedResourceKind::Equipment
);
system_handlers!(list_tariffs, create_tariff, ManagedResourceKind::Tariff);
system_handlers!(list_channels, create_channel, ManagedResourceKind::Channel);
account_handlers!(
    list_memberships,
    create_membership,
    ManagedResourceKind::Membership
);
account_handlers!(
    list_credentials,
    create_credential,
    ManagedResourceKind::Credential
);

fn require_actor(
    actor: Option<Extension<ModernApiActor>>,
    scope: ApiScope,
) -> Result<ModernApiActor, ResourceApiError> {
    let Extension(actor) = actor.ok_or(ResourceApiError::Forbidden)?;
    actor
        .scopes
        .contains(&scope)
        .then_some(actor)
        .ok_or(ResourceApiError::Forbidden)
}
async fn create(
    state: ResourceState,
    actor: ModernApiActor,
    account_id: AccountId,
    system_id: Option<SystemId>,
    kind: ManagedResourceKind,
    attributes: Value,
) -> Result<Response, ResourceApiError> {
    let resource = state
        .service
        .create(
            &actor,
            CreateManagedResource {
                account_id,
                system_id,
                kind,
                attributes,
            },
        )
        .await?;
    let version = resource.version;
    let mut response = (StatusCode::CREATED, Json(resource)).into_response();
    if let Ok(value) = HeaderValue::from_str(&format!("\"{version}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    Ok(response)
}

enum ResourceApiError {
    Forbidden,
    Domain(ManagedResourceError),
}
impl From<ManagedResourceError> for ResourceApiError {
    fn from(value: ManagedResourceError) -> Self {
        Self::Domain(value)
    }
}
impl IntoResponse for ResourceApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden | Self::Domain(ManagedResourceError::Forbidden) => {
                StatusCode::FORBIDDEN
            }
            Self::Domain(ManagedResourceError::InvalidInput) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Domain(ManagedResourceError::Repository(_)) => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
