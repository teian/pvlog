use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{patch, post},
};
use pvlog_application::{
    BatchIngestionMode, CorrectObservation, EnergyInput, ModernTelemetryError,
    ModernTelemetryUseCases, NormalizeObservation, PowerUnit,
};
use pvlog_domain::{
    ApiScope, BatteryFlowState, MeasurementValues, ObservationId, ObservationSource,
    ObservationSourceKind, Permission, QualityFlags, SystemId, UserId, UtcTimestamp, Watts,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
struct TelemetryState {
    service: Arc<dyn ModernTelemetryUseCases>,
    authorizer: Arc<dyn crate::ModernRequestAuthorizer>,
}
pub fn telemetry_router(
    service: Arc<dyn ModernTelemetryUseCases>,
    authorizer: Arc<dyn crate::ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/observations", post(single))
        .route(
            "/api/v1/systems/{system_id}/observations/batch",
            post(batch),
        )
        .route(
            "/api/v1/systems/{system_id}/observations/{observation_id}",
            patch(correct).delete(remove),
        )
        .with_state(TelemetryState {
            service,
            authorizer,
        })
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationBody {
    observed_at_epoch_millis: i64,
    generation_power_watts: Option<i64>,
    generation_energy_wh: Option<i64>,
    consumption_power_watts: Option<i64>,
    consumption_energy_wh: Option<i64>,
    voltage_millivolts: Option<u32>,
    temperature_millidegrees_celsius: Option<i32>,
    source_reference: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum BatchModeBody {
    Atomic,
    Partial,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchItemBody {
    idempotency_key: String,
    #[serde(flatten)]
    observation: ObservationBody,
}
#[derive(Deserialize)]
struct BatchBody {
    mode: BatchModeBody,
    items: Vec<BatchItemBody>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrectionBody {
    expected_version: u64,
    reason: String,
    generation_power_watts: Option<i64>,
    delete: bool,
}

async fn single(
    State(state): State<TelemetryState>,
    principal: Option<Extension<crate::RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    headers: HeaderMap,
    Json(body): Json<ObservationBody>,
) -> Result<Response, TelemetryApiError> {
    let (_, identity) =
        authorize_telemetry_write(&state, principal, system_id, "telemetry.ingest").await?;
    let command = command(system_id, identity, idempotency_key(&headers)?, body)?;
    let observation = state.service.ingest(command).await?;
    Ok((StatusCode::CREATED, Json(observation)).into_response())
}
async fn batch(
    State(state): State<TelemetryState>,
    principal: Option<Extension<crate::RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Json(body): Json<BatchBody>,
) -> Result<Response, TelemetryApiError> {
    let (_, identity) =
        authorize_telemetry_write(&state, principal, system_id, "telemetry.ingest_batch").await?;
    let commands = body
        .items
        .into_iter()
        .map(|item| {
            command(
                system_id,
                identity.clone(),
                item.idempotency_key,
                item.observation,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mode = match body.mode {
        BatchModeBody::Atomic => BatchIngestionMode::Atomic,
        BatchModeBody::Partial => BatchIngestionMode::Partial,
    };
    Ok((
        StatusCode::OK,
        Json(state.service.ingest_batch(commands, mode).await?),
    )
        .into_response())
}
async fn correct(
    State(state): State<TelemetryState>,
    principal: Option<Extension<crate::RequestPrincipal>>,
    Path((system_id, observation_id)): Path<(SystemId, ObservationId)>,
    Json(body): Json<CorrectionBody>,
) -> Result<Response, TelemetryApiError> {
    let replacement = (!body.delete).then(|| MeasurementValues {
        generation_power: body.generation_power_watts.map(Watts::new),
        ..MeasurementValues::default()
    });
    let (actor, _) =
        authorize_telemetry_write(&state, principal, system_id, "telemetry.correct").await?;
    let visible = state
        .service
        .correct(CorrectObservation {
            observation_id,
            system_id,
            actor,
            expected_version: body.expected_version,
            replacement,
            reason: body.reason,
        })
        .await?;
    Ok((StatusCode::OK, Json(visible)).into_response())
}
async fn remove(
    State(state): State<TelemetryState>,
    principal: Option<Extension<crate::RequestPrincipal>>,
    Path((system_id, observation_id)): Path<(SystemId, ObservationId)>,
    headers: HeaderMap,
) -> Result<StatusCode, TelemetryApiError> {
    let version = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse().ok())
        .ok_or(TelemetryApiError::Invalid)?;
    let reason = headers
        .get("x-correction-reason")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
        .ok_or(TelemetryApiError::Invalid)?;
    let (actor, _) =
        authorize_telemetry_write(&state, principal, system_id, "telemetry.delete").await?;
    state
        .service
        .delete(CorrectObservation {
            observation_id,
            system_id,
            actor,
            expected_version: version,
            replacement: None,
            reason,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn authorize_telemetry_write(
    state: &TelemetryState,
    principal: Option<Extension<crate::RequestPrincipal>>,
    system_id: SystemId,
    action: &'static str,
) -> Result<(UserId, String), TelemetryApiError> {
    let Extension(principal) = principal.ok_or(TelemetryApiError::Forbidden)?;
    if let crate::RequestPrincipal::ApiCredential {
        system_id: allowed_system,
        scopes,
        ..
    } = &principal
        && (!scopes.contains(&ApiScope::TelemetryWrite)
            || allowed_system.is_some_and(|allowed| allowed != system_id))
    {
        return Err(TelemetryApiError::Forbidden);
    }
    let identity = principal.safe_ingestion_identity();
    let authorized = state
        .authorizer
        .authorize_system(
            crate::principal_identity(&principal)?,
            system_id,
            Permission::TelemetryWrite,
            action,
        )
        .await?;
    Ok((authorized.actor_user_id, identity))
}
fn idempotency_key(headers: &HeaderMap) -> Result<String, TelemetryApiError> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or(TelemetryApiError::Invalid)
}
fn command(
    system_id: SystemId,
    idempotency_namespace: String,
    idempotency_key: String,
    body: ObservationBody,
) -> Result<NormalizeObservation, TelemetryApiError> {
    let observed_at = UtcTimestamp::from_epoch_millis(body.observed_at_epoch_millis)
        .map_err(|_| TelemetryApiError::Invalid)?;
    Ok(NormalizeObservation {
        system_id,
        observed_at,
        received_at: observed_at,
        generation_power: body
            .generation_power_watts
            .map(|value| (value, PowerUnit::Watts)),
        generation_energy: body.generation_energy_wh.map(|value| EnergyInput {
            value,
            unit: pvlog_application::EnergyUnit::WattHours,
            cumulative: false,
            reset_sequence: 0,
        }),
        consumption_power: body
            .consumption_power_watts
            .map(|value| (value, PowerUnit::Watts)),
        consumption_energy: body.consumption_energy_wh.map(|value| EnergyInput {
            value,
            unit: pvlog_application::EnergyUnit::WattHours,
            cumulative: false,
            reset_sequence: 0,
        }),
        voltage_millivolts: body.voltage_millivolts,
        temperature_millidegrees_celsius: body.temperature_millidegrees_celsius,
        battery_energy: None,
        battery_power: None,
        battery_state_of_charge_basis_points: None,
        battery_flow_state: BatteryFlowState::Unknown,
        extended: BTreeMap::new(),
        registered_channels: BTreeSet::new(),
        source: ObservationSource {
            kind: ObservationSourceKind::ModernApi,
            source_reference: body.source_reference,
        },
        idempotency_namespace,
        idempotency_key,
        quality: QualityFlags::NONE,
    })
}

enum TelemetryApiError {
    Forbidden,
    NotFound,
    Unavailable,
    Invalid,
    Domain(ModernTelemetryError),
}
impl From<ModernTelemetryError> for TelemetryApiError {
    fn from(value: ModernTelemetryError) -> Self {
        Self::Domain(value)
    }
}
impl From<crate::RequestAuthorizationError> for TelemetryApiError {
    fn from(value: crate::RequestAuthorizationError) -> Self {
        match value {
            crate::RequestAuthorizationError::Forbidden => Self::Forbidden,
            crate::RequestAuthorizationError::NotFound => Self::NotFound,
            crate::RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}
impl IntoResponse for TelemetryApiError {
    fn into_response(self) -> Response {
        let (status, retry) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, None),
            Self::NotFound | Self::Domain(ModernTelemetryError::NotFound) => {
                (StatusCode::NOT_FOUND, None)
            }
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, None),
            Self::Invalid | Self::Domain(ModernTelemetryError::Invalid) => {
                (StatusCode::UNPROCESSABLE_ENTITY, None)
            }
            Self::Domain(ModernTelemetryError::Conflict) => (StatusCode::CONFLICT, None),
            Self::Domain(ModernTelemetryError::Overloaded {
                retry_after_seconds,
            }) => (StatusCode::SERVICE_UNAVAILABLE, Some(retry_after_seconds)),
            Self::Domain(ModernTelemetryError::Repository(_)) => {
                (StatusCode::SERVICE_UNAVAILABLE, None)
            }
        };
        let mut response = status.into_response();
        if let Some(retry) = retry
            && let Ok(value) = HeaderValue::from_str(&retry.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}
