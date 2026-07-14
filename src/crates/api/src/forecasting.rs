//! Authorized forecast settings and modeled-yield query resources.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_domain::{
    AccountId, ApiScope, ForecastCompletenessReason, InverterId, Permission, StringId, SystemId,
    UserId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForecastResourceScope {
    Account {
        account_id: AccountId,
    },
    System {
        account_id: AccountId,
        system_id: SystemId,
    },
    Inverter {
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
    },
    String {
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
        string_id: StringId,
    },
}

impl ForecastResourceScope {
    const fn account_id(self) -> AccountId {
        match self {
            Self::Account { account_id }
            | Self::System { account_id, .. }
            | Self::Inverter { account_id, .. }
            | Self::String { account_id, .. } => account_id,
        }
    }

    const fn system_id(self) -> Option<SystemId> {
        match self {
            Self::Account { .. } => None,
            Self::System { system_id, .. }
            | Self::Inverter { system_id, .. }
            | Self::String { system_id, .. } => Some(system_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastLossInput {
    pub soiling_basis_points: u16,
    pub shading_basis_points: u16,
    pub mismatch_basis_points: u16,
    pub wiring_basis_points: u16,
    pub unavailability_basis_points: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastSettingsInput {
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub model_identifier: String,
    pub model_revision: u16,
    pub losses: ForecastLossInput,
    pub calibration_basis_points: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastSettingsResponse {
    pub scope: ForecastResourceScope,
    pub effective_from: i64,
    pub effective_to: Option<i64>,
    pub model_identifier: String,
    pub model_revision: u16,
    pub losses: ForecastLossInput,
    pub calibration_basis_points: i32,
    pub version: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastInputCompletenessResponse {
    pub scope: ForecastResourceScope,
    pub effective_at: i64,
    pub included_capacity_watts: i64,
    pub total_effective_capacity_watts: i64,
    pub complete: bool,
    pub reasons: Vec<ForecastCompletenessReason>,
    pub version: u64,
}

#[async_trait]
pub trait ForecastApiUseCases: Send + Sync {
    async fn settings(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastSettingsResponse, ForecastApiError>;

    async fn update_settings(
        &self,
        actor: UserId,
        scope: ForecastResourceScope,
        expected_version: u64,
        input: ForecastSettingsInput,
    ) -> Result<ForecastSettingsResponse, ForecastApiError>;

    async fn input_completeness(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastInputCompletenessResponse, ForecastApiError>;
}

#[derive(Clone)]
struct ForecastState {
    service: Arc<dyn ForecastApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates account, system, inverter, and string forecast administration routes.
pub fn forecasting_router(
    service: Arc<dyn ForecastApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    const SETTINGS_ROUTES: [&str; 4] = [
        "/api/v1/accounts/{account_id}/forecast-settings",
        "/api/v1/accounts/{account_id}/systems/{system_id}/forecast-settings",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/forecast-settings",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/strings/{string_id}/forecast-settings",
    ];
    const COMPLETENESS_ROUTES: [&str; 4] = [
        "/api/v1/accounts/{account_id}/forecast-input-completeness",
        "/api/v1/accounts/{account_id}/systems/{system_id}/forecast-input-completeness",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/forecast-input-completeness",
        "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/strings/{string_id}/forecast-input-completeness",
    ];
    let mut router = Router::new();
    for route in SETTINGS_ROUTES {
        router = router.route(route, get(settings).put(update_settings));
    }
    for route in COMPLETENESS_ROUTES {
        router = router.route(route, get(input_completeness));
    }
    router.with_state(ForecastState {
        service,
        authorizer,
    })
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct ForecastPath {
    #[serde(rename = "account_id")]
    account: AccountId,
    #[serde(rename = "system_id")]
    system: Option<SystemId>,
    #[serde(rename = "inverter_id")]
    inverter: Option<InverterId>,
    #[serde(rename = "string_id")]
    string: Option<StringId>,
}

impl ForecastPath {
    fn scope(self) -> Result<ForecastResourceScope, ForecastApiError> {
        match (self.system, self.inverter, self.string) {
            (None, None, None) => Ok(ForecastResourceScope::Account {
                account_id: self.account,
            }),
            (Some(system_id), None, None) => Ok(ForecastResourceScope::System {
                account_id: self.account,
                system_id,
            }),
            (Some(system_id), Some(inverter_id), None) => Ok(ForecastResourceScope::Inverter {
                account_id: self.account,
                system_id,
                inverter_id,
            }),
            (Some(system_id), Some(inverter_id), Some(string_id)) => {
                Ok(ForecastResourceScope::String {
                    account_id: self.account,
                    system_id,
                    inverter_id,
                    string_id,
                })
            }
            _ => Err(ForecastApiError::InvalidPath),
        }
    }
}

async fn settings(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
) -> Result<Response, ForecastApiError> {
    let scope = path.scope()?;
    authorize(&state, principal, scope, false).await?;
    Ok(with_etag(
        StatusCode::OK,
        state.service.settings(scope).await?,
    ))
}

async fn update_settings(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
    headers: HeaderMap,
    Json(input): Json<ForecastSettingsInput>,
) -> Result<Response, ForecastApiError> {
    validate_settings(&input)?;
    let scope = path.scope()?;
    let actor = authorize(&state, principal, scope, true).await?;
    let expected_version = expected_version(&headers)?;
    let response = state
        .service
        .update_settings(actor, scope, expected_version, input)
        .await?;
    Ok(with_etag(StatusCode::OK, response))
}

async fn input_completeness(
    State(state): State<ForecastState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(path): Path<ForecastPath>,
) -> Result<Response, ForecastApiError> {
    let scope = path.scope()?;
    authorize(&state, principal, scope, false).await?;
    let response = state.service.input_completeness(scope).await?;
    let version = response.version;
    Ok(json_with_etag(StatusCode::OK, response, version))
}

async fn authorize(
    state: &ForecastState,
    principal: Option<Extension<RequestPrincipal>>,
    scope: ForecastResourceScope,
    write: bool,
) -> Result<UserId, ForecastApiError> {
    let Extension(principal) = principal.ok_or(ForecastApiError::Forbidden)?;
    require_scope(&principal, write)?;
    let identity = principal_identity(&principal)?;
    let authorized = if let Some(system_id) = scope.system_id() {
        state
            .authorizer
            .authorize_system(
                identity,
                system_id,
                if write {
                    Permission::SystemManage
                } else {
                    Permission::SystemRead
                },
                if write {
                    "forecast.settings.update"
                } else {
                    "forecast.settings.read"
                },
            )
            .await?
    } else {
        state
            .authorizer
            .authorize_account(
                identity,
                scope.account_id(),
                if write {
                    Permission::AccountManage
                } else {
                    Permission::AccountRead
                },
                if write {
                    "forecast.settings.update"
                } else {
                    "forecast.settings.read"
                },
            )
            .await?
    };
    if authorized.account_id != scope.account_id() {
        return Err(ForecastApiError::Forbidden);
    }
    Ok(authorized.actor_user_id)
}

fn require_scope(principal: &RequestPrincipal, write: bool) -> Result<(), ForecastApiError> {
    match principal {
        RequestPrincipal::User(_) => Ok(()),
        RequestPrincipal::ApiCredential { scopes, .. }
            if scopes.contains(if write {
                &ApiScope::SystemsWrite
            } else {
                &ApiScope::SystemsRead
            }) =>
        {
            Ok(())
        }
        RequestPrincipal::ApiCredential { .. } | RequestPrincipal::SystemIngestion(_) => {
            Err(ForecastApiError::Forbidden)
        }
    }
}

fn validate_settings(input: &ForecastSettingsInput) -> Result<(), ForecastApiError> {
    if input.model_identifier.trim().is_empty() || input.model_identifier.len() > 64 {
        return Err(ForecastApiError::Validation(
            "modelIdentifier",
            "invalid_model_identifier",
        ));
    }
    if input.model_revision == 0 {
        return Err(ForecastApiError::Validation(
            "modelRevision",
            "invalid_model_revision",
        ));
    }
    if input
        .effective_to
        .is_some_and(|effective_to| effective_to <= input.effective_from)
    {
        return Err(ForecastApiError::Validation(
            "effectiveTo",
            "invalid_effective_period",
        ));
    }
    for (field, value) in [
        (
            "losses.soilingBasisPoints",
            input.losses.soiling_basis_points,
        ),
        (
            "losses.shadingBasisPoints",
            input.losses.shading_basis_points,
        ),
        (
            "losses.mismatchBasisPoints",
            input.losses.mismatch_basis_points,
        ),
        ("losses.wiringBasisPoints", input.losses.wiring_basis_points),
        (
            "losses.unavailabilityBasisPoints",
            input.losses.unavailability_basis_points,
        ),
    ] {
        if value > 10_000 {
            return Err(ForecastApiError::Validation(field, "loss_out_of_range"));
        }
    }
    if !(-5_000..=5_000).contains(&input.calibration_basis_points) {
        return Err(ForecastApiError::Validation(
            "calibrationBasisPoints",
            "calibration_out_of_range",
        ));
    }
    Ok(())
}

fn expected_version(headers: &HeaderMap) -> Result<u64, ForecastApiError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse().ok())
        .ok_or(ForecastApiError::PreconditionRequired)
}

fn with_etag(status: StatusCode, response: ForecastSettingsResponse) -> Response {
    let version = response.version;
    json_with_etag(status, response, version)
}

fn json_with_etag<T: Serialize>(status: StatusCode, body: T, version: u64) -> Response {
    let mut response = (status, Json(body)).into_response();
    if let Ok(value) = HeaderValue::from_str(&format!("\"{version}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ForecastApiError {
    #[error("forecast resource path is invalid")]
    InvalidPath,
    #[error("forecast access is forbidden")]
    Forbidden,
    #[error("forecast resource was not found")]
    NotFound,
    #[error("forecast resource version precondition is required")]
    PreconditionRequired,
    #[error("forecast resource version conflicts")]
    Conflict,
    #[error("forecast setting is invalid")]
    Validation(&'static str, &'static str),
    #[error("forecast service is unavailable")]
    Unavailable,
}

impl From<RequestAuthorizationError> for ForecastApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForecastValidationProblem {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: &'static str,
    field: &'static str,
}

impl IntoResponse for ForecastApiError {
    fn into_response(self) -> Response {
        if let Self::Validation(field, detail) = self {
            let mut response = (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ForecastValidationProblem {
                    problem_type: "https://pvlog.example/problems/forecast-validation",
                    title: "invalid_forecast_setting",
                    status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                    detail,
                    field,
                }),
            )
                .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/problem+json"),
            );
            return response;
        }
        match self {
            Self::InvalidPath | Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::Conflict => StatusCode::PRECONDITION_FAILED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Validation(_, _) => unreachable!(),
        }
        .into_response()
    }
}
