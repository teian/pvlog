//! Measurement-channel binding endpoints.
//!
//! Readings arrive under source-defined keys such as `mppt-1` or `wr1.mppt2`.
//! A key that nobody has bound yet is still retained, reported as `unassigned`,
//! and shown to operators so they can point it at the equipment it meters.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_application::PortError;
use pvlog_domain::{ApiScope, Permission, SystemId, UserId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// One measurement channel, what its values mean and the equipment it is bound to.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelResponse {
    pub id: Uuid,
    pub channel_key: String,
    pub display_name: String,
    /// `integer`, `decimal`, `boolean`, or `counter`.
    pub data_type: String,
    pub unit: String,
    /// Decimal places of `last_value`: a value of 1234 at scale 2 is 12.34.
    pub scale: i32,
    /// `mean`, `sum`, `min`, `max`, `last`, or `counter_delta`.
    pub aggregation: String,
    /// `unassigned`, `plant`, `inverter`, or `string`.
    pub target_kind: String,
    /// Equipment identifier for the `inverter` and `string` targets.
    pub target_id: Option<Uuid>,
    pub discovered_at_epoch_millis: Option<i64>,
    pub last_value_at_epoch_millis: Option<i64>,
    /// Newest value as a scaled integer.
    pub last_value: Option<i64>,
}

/// Requested binding for one channel key.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelBindingInput {
    pub target_kind: String,
    pub target_id: Option<Uuid>,
    pub display_name: Option<String>,
    /// How a period of values is reduced; the stored kind is kept when absent.
    pub aggregation: Option<String>,
}

/// Which aggregates of bound channels to read.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAggregateQuery {
    /// `plant`, `inverter`, or `string`.
    pub target_kind: String,
    /// Equipment identifier for the `inverter` and `string` targets.
    pub target_id: Option<Uuid>,
    /// `fifteen_minute`, `hour`, `day`, `month`, or `year`.
    pub resolution: String,
    /// Start of the range in epoch milliseconds, inclusive.
    pub from: i64,
    /// End of the range in epoch milliseconds, exclusive.
    pub to: i64,
}

/// The aggregates of one channel bound to the requested target.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAggregateResponse {
    pub channel_key: String,
    pub display_name: String,
    pub unit: String,
    /// Decimal places of every value in `buckets`.
    pub scale: i32,
    pub aggregation: String,
    pub buckets: Vec<ChannelAggregateBucket>,
}

/// What a channel reports for one local-calendar bucket.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAggregateBucket {
    pub start_epoch_millis: i64,
    pub end_epoch_millis: i64,
    /// The bucket reduced by the channel's aggregation kind. Absent for a
    /// counter whose first value in the bucket has no predecessor.
    pub value: Option<i64>,
    pub minimum: i64,
    pub maximum: i64,
    pub point_count: i64,
}

#[async_trait::async_trait]
pub trait ChannelApiUseCases: Send + Sync {
    /// Reads the aggregates of the channels bound to one target, per channel.
    async fn aggregates(
        &self,
        system_id: SystemId,
        query: ChannelAggregateQuery,
    ) -> Result<Vec<ChannelAggregateResponse>, ChannelApiError>;
    /// Lists the active channels of a system.
    async fn list(&self, system_id: SystemId) -> Result<Vec<ChannelResponse>, ChannelApiError>;
    /// Binds a channel to a target (plant, inverter, string, or unassigned) and returns the updated channel.
    async fn bind(
        &self,
        actor: UserId,
        system_id: SystemId,
        channel_key: &str,
        input: ChannelBindingInput,
    ) -> Result<ChannelResponse, ChannelApiError>;
    /// Retires a channel; unknown channels yield `NotFound`.
    async fn remove(&self, system_id: SystemId, channel_key: &str) -> Result<(), ChannelApiError>;
}

#[derive(Clone)]
struct ChannelState {
    service: Arc<dyn ChannelApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the channel listing and binding routes for one system.
pub fn channels_router(
    service: Arc<dyn ChannelApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/channels", get(list))
        .route(
            "/api/v1/systems/{system_id}/channel-aggregates",
            get(aggregates),
        )
        .route(
            "/api/v1/systems/{system_id}/channels/{channel_key}",
            axum::routing::put(bind).delete(remove),
        )
        .with_state(ChannelState {
            service,
            authorizer,
        })
}

/// Handles the channel listing route after a read authorization check.
async fn list(
    State(state): State<ChannelState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<Vec<ChannelResponse>>, ChannelApiError> {
    authorize(&state, principal, system_id, false, "system.channel.list").await?;
    Ok(Json(state.service.list(system_id).await?))
}

/// Handles the aggregate route after a read authorization check, refusing an
/// unknown resolution, a target without its identifier and an empty range.
async fn aggregates(
    State(state): State<ChannelState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(query): Query<ChannelAggregateQuery>,
) -> Result<Json<Vec<ChannelAggregateResponse>>, ChannelApiError> {
    authorize(
        &state,
        principal,
        system_id,
        false,
        "system.channel.aggregates",
    )
    .await?;
    if !matches!(
        query.resolution.as_str(),
        "fifteen_minute" | "hour" | "day" | "month" | "year"
    ) {
        return Err(ChannelApiError::InvalidInput("unknown_resolution"));
    }
    match (query.target_kind.as_str(), query.target_id) {
        ("plant", None) | ("inverter" | "string", Some(_)) => {}
        ("plant", Some(_)) => return Err(ChannelApiError::InvalidInput("target_id_not_allowed")),
        ("inverter" | "string", None) => {
            return Err(ChannelApiError::InvalidInput("target_id_required"));
        }
        _ => return Err(ChannelApiError::InvalidInput("unknown_target_kind")),
    }
    if query.to <= query.from {
        return Err(ChannelApiError::InvalidInput("empty_range"));
    }
    Ok(Json(state.service.aggregates(system_id, query).await?))
}

/// Handles the channel binding route: authorizes the write, validates the target, then stores it.
async fn bind(
    State(state): State<ChannelState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, channel_key)): Path<(SystemId, String)>,
    Json(input): Json<ChannelBindingInput>,
) -> Result<Json<ChannelResponse>, ChannelApiError> {
    let authorized = authorize(&state, principal, system_id, true, "system.channel.bind").await?;
    validate(&input)?;
    Ok(Json(
        state
            .service
            .bind(authorized, system_id, &channel_key, input)
            .await?,
    ))
}

/// Handles the channel removal route and answers `204 No Content` on success.
async fn remove(
    State(state): State<ChannelState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, channel_key)): Path<(SystemId, String)>,
) -> Result<StatusCode, ChannelApiError> {
    authorize(&state, principal, system_id, true, "system.channel.remove").await?;
    state.service.remove(system_id, &channel_key).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Rejects a binding whose target kind and identifier contradict each other
/// or whose aggregation kind is unknown.
fn validate(input: &ChannelBindingInput) -> Result<(), ChannelApiError> {
    if input.aggregation.as_deref().is_some_and(|kind| {
        !matches!(
            kind,
            "mean" | "sum" | "min" | "max" | "last" | "counter_delta"
        )
    }) {
        return Err(ChannelApiError::InvalidInput("unknown_aggregation"));
    }
    match input.target_kind.as_str() {
        "inverter" | "string" if input.target_id.is_none() => {
            Err(ChannelApiError::InvalidInput("target_id_required"))
        }
        "unassigned" | "plant" if input.target_id.is_some() => {
            Err(ChannelApiError::InvalidInput("target_id_not_allowed"))
        }
        "unassigned" | "plant" | "inverter" | "string" => Ok(()),
        _ => Err(ChannelApiError::InvalidInput("unknown_target_kind")),
    }
}

/// Checks the principal's API scope and system permission, returning the acting user; a missing principal or scope is `Forbidden`.
async fn authorize(
    state: &ChannelState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<UserId, ChannelApiError> {
    let Extension(principal) = principal.ok_or(ChannelApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::SystemsRead
        };
        if !scopes.contains(&scope) {
            return Err(ChannelApiError::Forbidden);
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
pub enum ChannelApiError {
    Forbidden,
    NotFound,
    InvalidInput(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for ChannelApiError {
    /// Maps authorization failures onto the channel API errors, hiding `NotFound` behind `Forbidden`.
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<PortError> for ChannelApiError {
    /// Maps any port failure to `Unavailable`.
    fn from(_: PortError) -> Self {
        Self::Unavailable
    }
}

impl IntoResponse for ChannelApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response with a matching status code.
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "channel_not_found"),
            Self::InvalidInput(code) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_channel_binding",
                code,
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "channel_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/channel-binding",
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
