//! Modern alert, event, webhook, delivery-attempt, and replay endpoints.

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_domain::{AccountId, AlertRuleId, WebhookDeliveryId, WebhookSubscriptionId};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

#[async_trait]
pub trait NotificationApiUseCases: Send + Sync {
    async fn list_alerts(&self, account_id: AccountId) -> Result<Vec<Value>, NotificationApiError>;
    async fn create_alert(
        &self,
        account_id: AccountId,
        input: Value,
    ) -> Result<Value, NotificationApiError>;
    async fn update_alert(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
        input: Value,
    ) -> Result<Value, NotificationApiError>;
    async fn delete_alert(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<(), NotificationApiError>;
    async fn list_events(&self, account_id: AccountId) -> Result<Vec<Value>, NotificationApiError>;
    async fn list_webhooks(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Value>, NotificationApiError>;
    async fn create_webhook(
        &self,
        account_id: AccountId,
        input: Value,
    ) -> Result<Value, NotificationApiError>;
    async fn verify_webhook(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
        challenge: String,
    ) -> Result<Value, NotificationApiError>;
    async fn delete_webhook(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
    ) -> Result<(), NotificationApiError>;
    async fn attempts(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
    ) -> Result<Vec<Value>, NotificationApiError>;
    async fn replay(
        &self,
        account_id: AccountId,
        id: WebhookDeliveryId,
    ) -> Result<Value, NotificationApiError>;
}

#[derive(Clone)]
struct NotificationState {
    service: Arc<dyn NotificationApiUseCases>,
}
pub fn notifications_router(service: Arc<dyn NotificationApiUseCases>) -> Router {
    Router::new()
        .route(
            "/api/v1/accounts/{account_id}/alerts",
            get(list_alerts).post(create_alert),
        )
        .route(
            "/api/v1/accounts/{account_id}/alerts/{alert_id}",
            axum::routing::patch(update_alert).delete(delete_alert),
        )
        .route(
            "/api/v1/accounts/{account_id}/alert-events",
            get(list_events),
        )
        .route(
            "/api/v1/accounts/{account_id}/webhooks",
            get(list_webhooks).post(create_webhook),
        )
        .route(
            "/api/v1/accounts/{account_id}/webhooks/{webhook_id}/verify",
            post(verify_webhook),
        )
        .route(
            "/api/v1/accounts/{account_id}/webhooks/{webhook_id}",
            axum::routing::delete(delete_webhook),
        )
        .route(
            "/api/v1/accounts/{account_id}/webhooks/{webhook_id}/attempts",
            get(attempts),
        )
        .route(
            "/api/v1/accounts/{account_id}/webhook-deliveries/{delivery_id}/replay",
            post(replay),
        )
        .with_state(NotificationState { service })
}

async fn list_alerts(
    State(state): State<NotificationState>,
    Path(account): Path<AccountId>,
) -> Result<Json<Vec<Value>>, NotificationApiError> {
    Ok(Json(state.service.list_alerts(account).await?))
}
async fn create_alert(
    State(state): State<NotificationState>,
    Path(account): Path<AccountId>,
    Json(input): Json<Value>,
) -> Result<Response, NotificationApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.service.create_alert(account, input).await?),
    )
        .into_response())
}
async fn update_alert(
    State(state): State<NotificationState>,
    Path((account, id)): Path<(AccountId, AlertRuleId)>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, NotificationApiError> {
    Ok(Json(state.service.update_alert(account, id, input).await?))
}
async fn delete_alert(
    State(state): State<NotificationState>,
    Path((account, id)): Path<(AccountId, AlertRuleId)>,
) -> Result<StatusCode, NotificationApiError> {
    state.service.delete_alert(account, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn list_events(
    State(state): State<NotificationState>,
    Path(account): Path<AccountId>,
) -> Result<Json<Vec<Value>>, NotificationApiError> {
    Ok(Json(state.service.list_events(account).await?))
}
async fn list_webhooks(
    State(state): State<NotificationState>,
    Path(account): Path<AccountId>,
) -> Result<Json<Vec<Value>>, NotificationApiError> {
    Ok(Json(state.service.list_webhooks(account).await?))
}
async fn create_webhook(
    State(state): State<NotificationState>,
    Path(account): Path<AccountId>,
    Json(input): Json<Value>,
) -> Result<Response, NotificationApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.service.create_webhook(account, input).await?),
    )
        .into_response())
}
#[derive(Deserialize)]
struct VerifyBody {
    challenge: String,
}
async fn verify_webhook(
    State(state): State<NotificationState>,
    Path((account, id)): Path<(AccountId, WebhookSubscriptionId)>,
    Json(body): Json<VerifyBody>,
) -> Result<Json<Value>, NotificationApiError> {
    Ok(Json(
        state
            .service
            .verify_webhook(account, id, body.challenge)
            .await?,
    ))
}
async fn delete_webhook(
    State(state): State<NotificationState>,
    Path((account, id)): Path<(AccountId, WebhookSubscriptionId)>,
) -> Result<StatusCode, NotificationApiError> {
    state.service.delete_webhook(account, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn attempts(
    State(state): State<NotificationState>,
    Path((account, id)): Path<(AccountId, WebhookSubscriptionId)>,
) -> Result<Json<Vec<Value>>, NotificationApiError> {
    Ok(Json(state.service.attempts(account, id).await?))
}
async fn replay(
    State(state): State<NotificationState>,
    Path((account, id)): Path<(AccountId, WebhookDeliveryId)>,
) -> Result<Json<Value>, NotificationApiError> {
    Ok(Json(state.service.replay(account, id).await?))
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NotificationApiError {
    #[error("notification resource was not found")]
    NotFound,
    #[error("notification input is invalid")]
    Invalid,
    #[error("notification operation is forbidden")]
    Forbidden,
    #[error("notification service is unavailable")]
    Unavailable,
}
impl IntoResponse for NotificationApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
