//! Daily summaries a source supplies, and the daily values a system reports.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use pvlog_application::{DayBasis, SuppliedDailySummary};
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, DataSourceId, Permission, SystemId};
use serde::{Deserialize, Serialize};

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

/// The value a system reports for one local day, with what it is based on.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyValueResponse {
    /// Local calendar date in the system's timezone.
    pub date: String,
    /// Whether the reported value is the source's summary or was computed.
    pub basis: DayBasis,
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
    /// The summary a source supplied for the day, when there is one.
    pub supplied: Option<SuppliedSummaryResponse>,
    /// The value computed from interval readings, when there is one. It stays
    /// available also while the supplied summary is the one reported.
    pub computed: Option<ComputedSummaryResponse>,
}

/// A daily summary as its source supplied it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppliedSummaryResponse {
    pub data_source_id: Option<DataSourceId>,
    pub generation_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub generation_lifetime_wh: Option<i64>,
    pub complete: bool,
    pub peak_power_watts: Option<i64>,
    pub peak_time: Option<String>,
    pub weather_condition: Option<String>,
    pub temperature_min_millidegrees_celsius: Option<i64>,
    pub temperature_max_millidegrees_celsius: Option<i64>,
    pub import_energy_wh_by_band: BTreeMap<String, i64>,
    pub received_at_epoch_millis: i64,
}

/// A day as computed from interval readings.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedSummaryResponse {
    pub generation_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub export_energy_wh: Option<i64>,
    pub coverage_basis_points: u16,
}

/// Who supplies a summary: decides which earlier summary it replaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SummarySource {
    /// API credential used, when the request was not made by a signed-in user.
    pub credential_id: Option<ApiCredentialId>,
    /// Non-secret identity of the principal.
    pub identity: String,
}

#[async_trait]
pub trait DailySummaryApiUseCases: Send + Sync {
    /// Stores a supplied summary, replacing the one the same source sent
    /// earlier for that date, and returns the day as it is now reported.
    async fn supply(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        source: SummarySource,
        summary: SuppliedDailySummary,
    ) -> Result<DailyValueResponse, DailySummaryApiError>;
    /// Lists the days in `from..=to` that have a supplied or computed value.
    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        from: String,
        to: String,
    ) -> Result<Vec<DailyValueResponse>, DailySummaryApiError>;
}

#[derive(Clone)]
struct DailySummaryState {
    service: Arc<dyn DailySummaryApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the daily summary routes.
pub fn daily_summaries_router(
    service: Arc<dyn DailySummaryApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/systems/{system_id}/daily-summaries", get(list))
        .route(
            "/api/v1/systems/{system_id}/daily-summaries/{date}",
            put(supply),
        )
        .with_state(DailySummaryState {
            service,
            authorizer,
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SummaryBody {
    generation_energy_wh: Option<i64>,
    export_energy_wh: Option<i64>,
    consumption_energy_wh: Option<i64>,
    generation_lifetime_wh: Option<i64>,
    #[serde(default)]
    complete: bool,
    peak_power_watts: Option<i64>,
    peak_time: Option<String>,
    weather_condition: Option<String>,
    temperature_min_millidegrees_celsius: Option<i64>,
    temperature_max_millidegrees_celsius: Option<i64>,
    #[serde(default)]
    import_energy_wh_by_band: BTreeMap<String, i64>,
}

#[derive(Deserialize)]
struct RangeQuery {
    from: String,
    to: String,
}

async fn supply(
    State(state): State<DailySummaryState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, date)): Path<(SystemId, String)>,
    Json(body): Json<SummaryBody>,
) -> Result<Json<DailyValueResponse>, DailySummaryApiError> {
    let Extension(principal) = principal.ok_or(DailySummaryApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential {
        system_id: allowed_system,
        scopes,
        ..
    } = &principal
        && (!scopes.contains(&ApiScope::TelemetryWrite)
            || allowed_system.is_some_and(|allowed| allowed != system_id))
    {
        return Err(DailySummaryApiError::Forbidden);
    }
    let source = SummarySource {
        credential_id: match &principal {
            RequestPrincipal::ApiCredential { id, .. } => Some(*id),
            RequestPrincipal::User(_) => None,
        },
        identity: principal.safe_ingestion_identity(),
    };
    let account_id = state
        .authorizer
        .authorize_system(
            principal_identity(&principal).map_err(|_| DailySummaryApiError::Forbidden)?,
            system_id,
            Permission::TelemetryWrite,
            "telemetry.daily_summary.supply",
        )
        .await?
        .account_id;
    let summary = SuppliedDailySummary {
        date,
        generation_energy_wh: body.generation_energy_wh,
        export_energy_wh: body.export_energy_wh,
        consumption_energy_wh: body.consumption_energy_wh,
        generation_lifetime_wh: body.generation_lifetime_wh,
        complete: body.complete,
        peak_power_watts: body.peak_power_watts,
        peak_time: body.peak_time,
        weather_condition: body.weather_condition,
        temperature_min_millidegrees_celsius: body.temperature_min_millidegrees_celsius,
        temperature_max_millidegrees_celsius: body.temperature_max_millidegrees_celsius,
        import_energy_wh_by_band: body.import_energy_wh_by_band,
    };
    summary
        .validate()
        .map_err(DailySummaryApiError::InvalidField)?;
    Ok(Json(
        state
            .service
            .supply(account_id, system_id, source, summary)
            .await?,
    ))
}

async fn list(
    State(state): State<DailySummaryState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    Query(range): Query<RangeQuery>,
) -> Result<Json<Vec<DailyValueResponse>>, DailySummaryApiError> {
    let Extension(principal) = principal.ok_or(DailySummaryApiError::Forbidden)?;
    let account_id = state
        .authorizer
        .authorize_system(
            principal_identity(&principal).map_err(|_| DailySummaryApiError::Forbidden)?,
            system_id,
            Permission::TelemetryRead,
            "telemetry.daily_summary.read",
        )
        .await?
        .account_id;
    for (value, field) in [(&range.from, "from"), (&range.to, "to")] {
        let probe = SuppliedDailySummary {
            date: value.clone(),
            ..SuppliedDailySummary::default()
        };
        if probe.validate().is_err() {
            return Err(DailySummaryApiError::InvalidField(field));
        }
    }
    if range.from > range.to {
        return Err(DailySummaryApiError::InvalidField("to"));
    }
    Ok(Json(
        state
            .service
            .list(account_id, system_id, range.from, range.to)
            .await?,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DailySummaryApiError {
    Forbidden,
    NotFound,
    /// A member is out of range; carries its wire name.
    InvalidField(&'static str),
    Unavailable,
}

impl From<RequestAuthorizationError> for DailySummaryApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for DailySummaryApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response.
    /// A rejected member is named in `detail` by its wire name.
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "system_access_denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "system_not_found"),
            Self::InvalidField(field) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_daily_summary",
                field,
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "daily_summary_service_unavailable",
            ),
        };
        let mut response = (
            status,
            Json(serde_json::json!({
                "type": "https://pvlog.example/problems/daily-summaries",
                "title": title,
                "status": status.as_u16(),
                "detail": detail,
            })),
        )
            .into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
