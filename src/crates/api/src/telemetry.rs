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
    BatteryFlowState, MeasurementValues, ObservationId, ObservationSource, ObservationSourceKind,
    QualityFlags, SystemId, UserId, UtcTimestamp, Watts,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
struct TelemetryState {
    service: Arc<dyn ModernTelemetryUseCases>,
}
pub fn telemetry_router(service: Arc<dyn ModernTelemetryUseCases>) -> Router {
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
        .with_state(TelemetryState { service })
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
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
    headers: HeaderMap,
    Json(body): Json<ObservationBody>,
) -> Result<Response, TelemetryApiError> {
    let _actor = actor_id(actor)?;
    let command = command(system_id, idempotency_key(&headers)?, body)?;
    let observation = state.service.ingest(command).await?;
    Ok((StatusCode::CREATED, Json(observation)).into_response())
}
async fn batch(
    State(state): State<TelemetryState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
    Json(body): Json<BatchBody>,
) -> Result<Response, TelemetryApiError> {
    let _actor = actor_id(actor)?;
    let commands = body
        .items
        .into_iter()
        .map(|item| command(system_id, item.idempotency_key, item.observation))
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
    actor: Option<Extension<UserId>>,
    Path((system_id, observation_id)): Path<(SystemId, ObservationId)>,
    Json(body): Json<CorrectionBody>,
) -> Result<Response, TelemetryApiError> {
    let replacement = (!body.delete).then(|| MeasurementValues {
        generation_power: body.generation_power_watts.map(Watts::new),
        ..MeasurementValues::default()
    });
    let visible = state
        .service
        .correct(CorrectObservation {
            observation_id,
            system_id,
            actor: actor_id(actor)?,
            expected_version: body.expected_version,
            replacement,
            reason: body.reason,
        })
        .await?;
    Ok((StatusCode::OK, Json(visible)).into_response())
}
async fn remove(
    State(state): State<TelemetryState>,
    actor: Option<Extension<UserId>>,
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
    state
        .service
        .delete(CorrectObservation {
            observation_id,
            system_id,
            actor: actor_id(actor)?,
            expected_version: version,
            replacement: None,
            reason,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn actor_id(actor: Option<Extension<UserId>>) -> Result<UserId, TelemetryApiError> {
    actor
        .map(|Extension(actor)| actor)
        .ok_or(TelemetryApiError::Forbidden)
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
        idempotency_namespace: "modern_api".to_owned(),
        idempotency_key,
        quality: QualityFlags::NONE,
    })
}

enum TelemetryApiError {
    Forbidden,
    Invalid,
    Domain(ModernTelemetryError),
}
impl From<ModernTelemetryError> for TelemetryApiError {
    fn from(value: ModernTelemetryError) -> Self {
        Self::Domain(value)
    }
}
impl IntoResponse for TelemetryApiError {
    fn into_response(self) -> Response {
        let (status, retry) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, None),
            Self::Invalid | Self::Domain(ModernTelemetryError::Invalid) => {
                (StatusCode::UNPROCESSABLE_ENTITY, None)
            }
            Self::Domain(ModernTelemetryError::NotFound) => (StatusCode::NOT_FOUND, None),
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
