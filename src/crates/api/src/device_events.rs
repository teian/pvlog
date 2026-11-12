//! Device event endpoints.
//!
//! A logger uploads the events its devices report, such as a fault or a
//! changed setting. An event is identified by its device and by the source's
//! event identifier, or by its time and code when the source has none, so
//! uploading it again stores nothing new.
//!
//! The archive energy samples a logger delivered through the ingestion API
//! are read here as well, since they are kept per device like its events.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{ApiCredentialId, ApiScope, Permission, SystemId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity, problem::Problem,
};

/// Most events one request may carry.
pub const MAXIMUM_DEVICE_EVENTS_PER_REQUEST: usize = 1_000;
/// Events returned when a listing names no limit.
pub const DEFAULT_DEVICE_EVENT_LIMIT: u32 = 100;
/// Most events one listing returns.
pub const MAXIMUM_DEVICE_EVENT_LIMIT: u32 = 1_000;
/// Energy samples returned when a listing names no limit.
pub const DEFAULT_ENERGY_SAMPLE_LIMIT: u32 = 1_000;
/// Most energy samples one listing returns.
pub const MAXIMUM_ENERGY_SAMPLE_LIMIT: u32 = 10_000;

const MAXIMUM_DEVICE_ID_CHARACTERS: usize = 64;
const MAXIMUM_EVENT_ID_CHARACTERS: usize = 128;
const MAXIMUM_MESSAGE_CHARACTERS: usize = 500;
const MAXIMUM_DETAIL_CHARACTERS: usize = 500;

/// What a device says about an event beyond its code and message. An absent
/// member was not reported.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEventDetails {
    /// Kind of event as the device names it.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// Severity category, such as an error or a warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Value a changed setting had before the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    /// Value a changed setting has since the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    /// User group that caused the event, such as an installer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_group: Option<String>,
}

/// One event as a source uploads it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEventInput {
    /// The device as the source names it; for an inverter its serial.
    pub device_id: String,
    /// The source's own identifier of the event within the device.
    pub event_id: Option<String>,
    pub occurred_at_epoch_millis: i64,
    pub code: Option<i64>,
    pub message: Option<String>,
    #[serde(flatten)]
    pub details: DeviceEventDetails,
}

#[derive(Deserialize)]
struct DeviceEventsBody {
    events: Vec<DeviceEventInput>,
}

/// What became of the events of one upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEventsAccepted {
    /// Events that were new, or replaced a stored event with other content.
    pub stored: u64,
    /// Events that were already stored as uploaded.
    pub unchanged: u64,
}

/// One stored event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEventResponse {
    pub id: Uuid,
    pub device_id: String,
    pub event_id: Option<String>,
    pub occurred_at_epoch_millis: i64,
    pub code: Option<i64>,
    pub message: Option<String>,
    #[serde(flatten)]
    pub details: DeviceEventDetails,
    pub data_source_id: Option<Uuid>,
    pub received_at_epoch_millis: i64,
}

/// Which events to list.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEventQuery {
    /// Only events of this device.
    pub device_id: Option<String>,
    /// Only events that occurred at or after this instant, in epoch milliseconds.
    pub from: Option<i64>,
    /// Only events that occurred before this instant, in epoch milliseconds.
    pub to: Option<i64>,
    /// Largest number of events to return.
    pub limit: Option<u32>,
}

/// One stored archive energy sample of a device.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnergySampleResponse {
    /// The device as the source names it; for an inverter its serial.
    pub device_id: String,
    pub measured_at_epoch_millis: i64,
    /// The device's lifetime energy counter; absent when the sample has none.
    pub energy_lifetime_wh: Option<i64>,
    /// Mean power of the sample interval; absent when the sample has none.
    pub power_watts: Option<i64>,
    pub data_source_id: Option<Uuid>,
    pub received_at_epoch_millis: i64,
}

#[async_trait::async_trait]
pub trait DeviceEventApiUseCases: Send + Sync {
    /// Stores uploaded events, attributing them to the data source of the credential.
    async fn store(
        &self,
        system_id: SystemId,
        credential_id: Option<ApiCredentialId>,
        events: Vec<DeviceEventInput>,
    ) -> Result<DeviceEventsAccepted, DeviceEventApiError>;
    /// Lists the events of a system, newest first.
    async fn list(
        &self,
        system_id: SystemId,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError>;
    /// Lists the events of one inverter, found by its serial, newest first.
    /// An unknown inverter is `NotFound`.
    async fn list_for_inverter(
        &self,
        system_id: SystemId,
        inverter_id: Uuid,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError>;
    /// Lists the archive energy samples of a system, oldest first.
    async fn list_energy_samples(
        &self,
        system_id: SystemId,
        query: DeviceEventQuery,
    ) -> Result<Vec<EnergySampleResponse>, DeviceEventApiError>;
}

#[derive(Clone)]
struct DeviceEventState {
    service: Arc<dyn DeviceEventApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the device event routes of one system.
pub fn device_events_router(
    service: Arc<dyn DeviceEventApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/systems/{system_id}/device-events",
            get(list).post(store),
        )
        .route(
            "/api/v1/systems/{system_id}/inverters/{inverter_id}/device-events",
            get(list_for_inverter),
        )
        .route(
            "/api/v1/systems/{system_id}/energy-samples",
            get(list_energy_samples),
        )
        .with_state(DeviceEventState {
            service,
            authorizer,
        })
}

/// Handles the upload route: a credential needs the telemetry write scope and
/// must not be restricted to another system.
async fn store(
    State(state): State<DeviceEventState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Json(body): Json<DeviceEventsBody>,
) -> Result<Json<DeviceEventsAccepted>, DeviceEventApiError> {
    let Extension(principal) = principal.ok_or(DeviceEventApiError::Forbidden)?;
    let credential_id = match &principal {
        RequestPrincipal::ApiCredential {
            id,
            system_id: allowed_system,
            scopes,
            ..
        } => {
            if !scopes.contains(&ApiScope::TelemetryWrite)
                || allowed_system.is_some_and(|allowed| allowed != system_id)
            {
                return Err(DeviceEventApiError::Forbidden);
            }
            Some(*id)
        }
        RequestPrincipal::User(_) => None,
    };
    state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            Permission::TelemetryWrite,
            "telemetry.device_events.store",
        )
        .await?;
    let events = validated(body.events)?;
    Ok(Json(
        state
            .service
            .store(system_id, credential_id, events)
            .await?,
    ))
}

/// Handles the listing route of a system after a read authorization check.
async fn list(
    State(state): State<DeviceEventState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(query): Query<DeviceEventQuery>,
) -> Result<Json<Vec<DeviceEventResponse>>, DeviceEventApiError> {
    authorize_read(&state, principal, system_id, "telemetry.device_events.list").await?;
    Ok(Json(state.service.list(system_id, bounded(query)?).await?))
}

/// Handles the listing route of one inverter after a read authorization check.
async fn list_for_inverter(
    State(state): State<DeviceEventState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, inverter_id)): Path<(SystemId, Uuid)>,
    Query(query): Query<DeviceEventQuery>,
) -> Result<Json<Vec<DeviceEventResponse>>, DeviceEventApiError> {
    authorize_read(
        &state,
        principal,
        system_id,
        "telemetry.device_events.list_for_inverter",
    )
    .await?;
    Ok(Json(
        state
            .service
            .list_for_inverter(system_id, inverter_id, bounded(query)?)
            .await?,
    ))
}

/// Handles the energy sample listing of a system after a read authorization check.
async fn list_energy_samples(
    State(state): State<DeviceEventState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(query): Query<DeviceEventQuery>,
) -> Result<Json<Vec<EnergySampleResponse>>, DeviceEventApiError> {
    authorize_read(
        &state,
        principal,
        system_id,
        "telemetry.energy_samples.list",
    )
    .await?;
    let query = bounded_by(
        query,
        DEFAULT_ENERGY_SAMPLE_LIMIT,
        MAXIMUM_ENERGY_SAMPLE_LIMIT,
    )?;
    Ok(Json(
        state.service.list_energy_samples(system_id, query).await?,
    ))
}

/// Checks an upload and trims what it carries. A blank message or event
/// identifier counts as absent.
fn validated(events: Vec<DeviceEventInput>) -> Result<Vec<DeviceEventInput>, DeviceEventApiError> {
    if events.is_empty() || events.len() > MAXIMUM_DEVICE_EVENTS_PER_REQUEST {
        return Err(DeviceEventApiError::InvalidInput("events"));
    }
    let printable = |text: &str, maximum: usize| {
        !text.is_empty() && text.chars().count() <= maximum && !text.chars().any(char::is_control)
    };
    events
        .into_iter()
        .map(|event| {
            let device_id = event.device_id.trim().to_owned();
            if !printable(&device_id, MAXIMUM_DEVICE_ID_CHARACTERS) {
                return Err(DeviceEventApiError::InvalidInput("events.deviceId"));
            }
            let event_id = event
                .event_id
                .map(|id| id.trim().to_owned())
                .filter(|id| !id.is_empty());
            if event_id
                .as_ref()
                .is_some_and(|id| !printable(id, MAXIMUM_EVENT_ID_CHARACTERS))
            {
                return Err(DeviceEventApiError::InvalidInput("events.eventId"));
            }
            if event.occurred_at_epoch_millis < 0 {
                return Err(DeviceEventApiError::InvalidInput(
                    "events.occurredAtEpochMillis",
                ));
            }
            let message = event
                .message
                .map(|message| message.trim().to_owned())
                .filter(|message| !message.is_empty());
            if message
                .as_ref()
                .is_some_and(|message| message.chars().count() > MAXIMUM_MESSAGE_CHARACTERS)
            {
                return Err(DeviceEventApiError::InvalidInput("events.message"));
            }
            // A blank detail counts as absent, like a blank message.
            let mut details = event.details;
            for (text, member) in [
                (&mut details.event_type, "events.type"),
                (&mut details.category, "events.category"),
                (&mut details.group, "events.group"),
                (&mut details.old_value, "events.oldValue"),
                (&mut details.new_value, "events.newValue"),
                (&mut details.user_group, "events.userGroup"),
            ] {
                *text = text
                    .take()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty());
                if text.as_ref().is_some_and(|value| {
                    value.chars().count() > MAXIMUM_DETAIL_CHARACTERS
                        || value.chars().any(char::is_control)
                }) {
                    return Err(DeviceEventApiError::InvalidInput(member));
                }
            }
            Ok(DeviceEventInput {
                device_id,
                event_id,
                message,
                details,
                ..event
            })
        })
        .collect()
}

/// Applies the default limit and refuses a limit or range that cannot be served.
fn bounded(query: DeviceEventQuery) -> Result<DeviceEventQuery, DeviceEventApiError> {
    bounded_by(
        query,
        DEFAULT_DEVICE_EVENT_LIMIT,
        MAXIMUM_DEVICE_EVENT_LIMIT,
    )
}

/// Applies a default limit and refuses a limit above `maximum` or an empty range.
fn bounded_by(
    query: DeviceEventQuery,
    default: u32,
    maximum: u32,
) -> Result<DeviceEventQuery, DeviceEventApiError> {
    let limit = query.limit.unwrap_or(default);
    if limit == 0 || limit > maximum {
        return Err(DeviceEventApiError::InvalidInput("limit"));
    }
    if query
        .from
        .zip(query.to)
        .is_some_and(|(from, to)| to <= from)
    {
        return Err(DeviceEventApiError::InvalidInput("to"));
    }
    Ok(DeviceEventQuery {
        limit: Some(limit),
        ..query
    })
}

/// Checks the principal's read scope and system permission; a missing principal or scope is `Forbidden`.
async fn authorize_read(
    state: &DeviceEventState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    action: &'static str,
) -> Result<(), DeviceEventApiError> {
    let Extension(principal) = principal.ok_or(DeviceEventApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential { scopes, .. } = &principal
        && !scopes.contains(&ApiScope::TelemetryRead)
    {
        return Err(DeviceEventApiError::Forbidden);
    }
    state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            Permission::TelemetryRead,
            action,
        )
        .await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceEventApiError {
    Forbidden,
    NotFound,
    /// A member of the request is invalid; carries its wire name.
    InvalidInput(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for DeviceEventApiError {
    /// Maps authorization failures onto the device event errors, hiding `NotFound` behind `Forbidden`.
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for DeviceEventApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response with a matching status code.
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "inverter_not_found"),
            Self::InvalidInput(field) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_device_events",
                field,
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "device_event_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(Problem {
                problem_type: "https://pvlog.example/problems/device-events",
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
