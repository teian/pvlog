//! Authenticated operational dashboard projection endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{Extension, Json, Router, http::StatusCode, response::Response, routing::get};
use serde::Serialize;

use crate::{DataState, RequestPrincipal};
use pvlog_domain::{AccountId, ApiScope, SystemId, UserId};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardResponse {
    /// Whether the visible systems have data at all, and whether it is current.
    /// A client must not show live values unless this is `current`.
    pub data_state: DataState,
    /// Systems the principal may see.
    pub system_count: u64,
    /// Measurement time of the newest reading; zero when `data_state` is `none`.
    pub observed_at_epoch_millis: i64,
    pub age_seconds: u64,
    pub freshness_threshold_seconds: u64,
    pub generation_watts: f64,
    pub consumption_watts: Option<f64>,
    pub grid_watts: Option<f64>,
    pub battery_basis_points: Option<i64>,
    pub coverage_basis_points: u16,
    pub recent_alerts: Vec<DashboardAlertResponse>,
    pub ingestion: DashboardIngestionResponse,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardAlertResponse {
    pub id: String,
    pub title: String,
    pub state: String,
    pub opened_at_epoch_millis: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardIngestionResponse {
    pub accepted_today: u64,
    pub rejected_today: u64,
    pub lag_seconds: u64,
}

/// Narrowing applied to the systems a dashboard may aggregate.
///
/// Browser sessions are unrestricted; API credentials carry the account and the
/// optional system they were issued for.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DashboardRestriction {
    pub account_id: Option<AccountId>,
    pub system_id: Option<SystemId>,
}

#[async_trait]
pub trait DashboardApiUseCases: Send + Sync {
    /// Projects the operational dashboard for one authenticated principal.
    async fn dashboard(
        &self,
        actor: UserId,
        restriction: DashboardRestriction,
    ) -> Result<DashboardResponse, DashboardApiError>;
}

#[derive(Clone, Copy, Debug)]
pub enum DashboardApiError {
    Unavailable,
}

pub fn dashboard_router(service: Arc<dyn DashboardApiUseCases>) -> Router {
    Router::new()
        .route("/api/v1/dashboard", get(dashboard))
        .with_state(service)
}

async fn dashboard(
    axum::extract::State(service): axum::extract::State<Arc<dyn DashboardApiUseCases>>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<DashboardResponse>, DashboardHttpError> {
    let Some(Extension(principal)) = principal else {
        return Err(DashboardHttpError::Unauthorized);
    };
    let (actor, restriction) = match principal {
        RequestPrincipal::User(user_id) => (user_id, DashboardRestriction::default()),
        RequestPrincipal::ApiCredential {
            owner_user_id,
            account_id,
            system_id,
            scopes,
            ..
        } => {
            if !scopes.contains(&ApiScope::TelemetryRead) {
                return Err(DashboardHttpError::Forbidden);
            }
            (
                owner_user_id,
                DashboardRestriction {
                    account_id: Some(account_id),
                    system_id,
                },
            )
        }
    };
    service
        .dashboard(actor, restriction)
        .await
        .map(Json)
        .map_err(|_| DashboardHttpError::Unavailable)
}

enum DashboardHttpError {
    Unauthorized,
    Forbidden,
    Unavailable,
}

impl axum::response::IntoResponse for DashboardHttpError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
