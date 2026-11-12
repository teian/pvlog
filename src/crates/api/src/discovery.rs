//! Equipment discovery endpoints.
//!
//! A device that a source reports and that matches no inverter of the system
//! is listed here as a proposal. Adopting it creates the inverter and its
//! strings and binds the device's measurement channels; dismissing it keeps it
//! from being proposed again until the dismissal is reset.

use std::collections::BTreeSet;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_domain::{ApiScope, Permission, SystemId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// Longest name an adopted inverter or string may be given, in characters.
const MAXIMUM_NAME_CHARACTERS: usize = 120;

/// A reported device that matches no inverter of the system.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryProposalResponse {
    pub id: Uuid,
    pub device_serial: String,
    pub reported_name: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    /// `open` or `dismissed`.
    pub state: String,
    /// DC inputs that have reported power, by their tracker number.
    pub trackers: Vec<u8>,
    /// Measurement channels the device delivers.
    pub channel_count: u64,
    pub first_seen_at_epoch_millis: i64,
    pub last_seen_at_epoch_millis: i64,
}

/// How a proposed device is taken over as equipment.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptProposalInput {
    /// Name of the inverter; the reported name or the serial when absent.
    pub name: Option<String>,
    /// Strings to create. At least one is required.
    pub strings: Vec<AdoptedStringInput>,
}

/// One string to create while adopting a device.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptedStringInput {
    /// Tracker whose channels the string receives; a string without one
    /// receives no channels.
    pub tracker: Option<u8>,
    pub name: Option<String>,
    /// Installed capacity of the string in watts peak.
    pub capacity_watts: i64,
    pub orientation_degrees: Option<u16>,
    pub tilt_degrees: Option<u8>,
}

/// The equipment an adoption created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptedEquipmentResponse {
    pub inverter_id: Uuid,
    pub strings: Vec<AdoptedStringResponse>,
    /// Measurement channels that were bound to the new equipment.
    pub bound_channels: u64,
}

/// One string an adoption created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptedStringResponse {
    pub string_id: Uuid,
    pub tracker: Option<u8>,
    pub name: String,
}

#[async_trait::async_trait]
pub trait DiscoveryApiUseCases: Send + Sync {
    /// Lists the open proposals of a system, and the dismissed ones on request.
    async fn list(
        &self,
        system_id: SystemId,
        include_dismissed: bool,
    ) -> Result<Vec<DiscoveryProposalResponse>, DiscoveryApiError>;
    /// Creates the inverter and strings of an open proposal and binds the
    /// device's unassigned channels to them.
    async fn adopt(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
        input: AdoptProposalInput,
    ) -> Result<AdoptedEquipmentResponse, DiscoveryApiError>;
    /// Dismisses an open proposal.
    async fn dismiss(
        &self,
        system_id: SystemId,
        proposal_id: Uuid,
    ) -> Result<(), DiscoveryApiError>;
    /// Opens a dismissed proposal again.
    async fn reset(&self, system_id: SystemId, proposal_id: Uuid) -> Result<(), DiscoveryApiError>;
}

#[derive(Clone)]
struct DiscoveryState {
    service: Arc<dyn DiscoveryApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the discovery proposal routes of one system.
pub fn discovery_router(
    service: Arc<dyn DiscoveryApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/discovery-proposals", get(list))
        .route(
            "/api/v1/systems/{system_id}/discovery-proposals/{proposal_id}/adopt",
            post(adopt),
        )
        .route(
            "/api/v1/systems/{system_id}/discovery-proposals/{proposal_id}/dismiss",
            post(dismiss),
        )
        .route(
            "/api/v1/systems/{system_id}/discovery-proposals/{proposal_id}/reset",
            post(reset),
        )
        .with_state(DiscoveryState {
            service,
            authorizer,
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    include_dismissed: bool,
}

/// Handles the listing route after a read authorization check.
async fn list(
    State(state): State<DiscoveryState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<DiscoveryProposalResponse>>, DiscoveryApiError> {
    authorize(&state, principal, system_id, false, "system.discovery.list").await?;
    Ok(Json(
        state
            .service
            .list(system_id, query.include_dismissed)
            .await?,
    ))
}

/// Handles the adoption route: authorizes the write, validates the strings, then creates the equipment.
async fn adopt(
    State(state): State<DiscoveryState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, proposal_id)): Path<(SystemId, Uuid)>,
    Json(input): Json<AdoptProposalInput>,
) -> Result<(StatusCode, Json<AdoptedEquipmentResponse>), DiscoveryApiError> {
    authorize(&state, principal, system_id, true, "system.discovery.adopt").await?;
    validate(&input)?;
    Ok((
        StatusCode::CREATED,
        Json(state.service.adopt(system_id, proposal_id, input).await?),
    ))
}

/// Handles the dismissal route and answers `204 No Content` on success.
async fn dismiss(
    State(state): State<DiscoveryState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, proposal_id)): Path<(SystemId, Uuid)>,
) -> Result<StatusCode, DiscoveryApiError> {
    authorize(
        &state,
        principal,
        system_id,
        true,
        "system.discovery.dismiss",
    )
    .await?;
    state.service.dismiss(system_id, proposal_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Handles the reset route and answers `204 No Content` on success.
async fn reset(
    State(state): State<DiscoveryState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, proposal_id)): Path<(SystemId, Uuid)>,
) -> Result<StatusCode, DiscoveryApiError> {
    authorize(&state, principal, system_id, true, "system.discovery.reset").await?;
    state.service.reset(system_id, proposal_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Rejects an adoption without strings, with a string that has no capacity or
/// an impossible orientation, with a blank or overlong name, or with two
/// strings for one tracker.
fn validate(input: &AdoptProposalInput) -> Result<(), DiscoveryApiError> {
    let name_valid = |name: &Option<String>| {
        name.as_ref().is_none_or(|name| {
            !name.trim().is_empty() && name.chars().count() <= MAXIMUM_NAME_CHARACTERS
        })
    };
    if !name_valid(&input.name) {
        return Err(DiscoveryApiError::InvalidInput("name"));
    }
    if input.strings.is_empty() || input.strings.len() > usize::from(u8::MAX) {
        return Err(DiscoveryApiError::InvalidInput("strings"));
    }
    let mut trackers = BTreeSet::new();
    for string in &input.strings {
        if string.capacity_watts <= 0 {
            return Err(DiscoveryApiError::InvalidInput("strings.capacityWatts"));
        }
        if string.orientation_degrees.is_some_and(|value| value > 359) {
            return Err(DiscoveryApiError::InvalidInput(
                "strings.orientationDegrees",
            ));
        }
        if string.tilt_degrees.is_some_and(|value| value > 90) {
            return Err(DiscoveryApiError::InvalidInput("strings.tiltDegrees"));
        }
        if !name_valid(&string.name) {
            return Err(DiscoveryApiError::InvalidInput("strings.name"));
        }
        if string
            .tracker
            .is_some_and(|tracker| tracker == 0 || !trackers.insert(tracker))
        {
            return Err(DiscoveryApiError::InvalidInput("strings.tracker"));
        }
    }
    Ok(())
}

/// Checks the principal's API scope and system permission; a missing principal or scope is `Forbidden`.
async fn authorize(
    state: &DiscoveryState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<(), DiscoveryApiError> {
    let Extension(principal) = principal.ok_or(DiscoveryApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::SystemsRead
        };
        if !scopes.contains(&scope) {
            return Err(DiscoveryApiError::Forbidden);
        }
    }
    state
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
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryApiError {
    Forbidden,
    NotFound,
    /// A member of the request is invalid; carries its wire name.
    InvalidInput(&'static str),
    /// The proposal cannot be decided in its present state; carries the reason.
    Conflict(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for DiscoveryApiError {
    /// Maps authorization failures onto the discovery errors, hiding `NotFound` behind `Forbidden`.
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for DiscoveryApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response with a matching status code.
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "proposal_not_found"),
            Self::InvalidInput(field) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "invalid_adoption", field)
            }
            Self::Conflict(reason) => (StatusCode::CONFLICT, "conflict", reason),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "discovery_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/equipment-discovery",
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
