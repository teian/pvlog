use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::{NotificationApiError, NotificationApiUseCases, notifications_router};
use pvlog_domain::{AccountId, AlertRuleId, WebhookDeliveryId, WebhookSubscriptionId};
use serde_json::{Value, json};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn modern_notification_resources_and_actions_are_exposed() -> Result<(), Box<dyn Error>> {
    let account = AccountId::new();
    let alert = AlertRuleId::new();
    let webhook = WebhookSubscriptionId::new();
    let delivery = WebhookDeliveryId::new();
    let app = notifications_router(Arc::new(Stub));
    let cases = [
        (
            Method::GET,
            format!("/api/v1/accounts/{account}/alerts"),
            None,
            200,
        ),
        (
            Method::POST,
            format!("/api/v1/accounts/{account}/alerts"),
            Some(json!({"kind":"idle"})),
            201,
        ),
        (
            Method::PATCH,
            format!("/api/v1/accounts/{account}/alerts/{alert}"),
            Some(json!({"enabled":true})),
            200,
        ),
        (
            Method::DELETE,
            format!("/api/v1/accounts/{account}/alerts/{alert}"),
            None,
            204,
        ),
        (
            Method::GET,
            format!("/api/v1/accounts/{account}/alert-events"),
            None,
            200,
        ),
        (
            Method::GET,
            format!("/api/v1/accounts/{account}/webhooks"),
            None,
            200,
        ),
        (
            Method::POST,
            format!("/api/v1/accounts/{account}/webhooks"),
            Some(json!({"endpoint":"https://example.test"})),
            201,
        ),
        (
            Method::POST,
            format!("/api/v1/accounts/{account}/webhooks/{webhook}/verify"),
            Some(json!({"challenge":"challenge"})),
            200,
        ),
        (
            Method::GET,
            format!("/api/v1/accounts/{account}/webhooks/{webhook}/attempts"),
            None,
            200,
        ),
        (
            Method::POST,
            format!("/api/v1/accounts/{account}/webhook-deliveries/{delivery}/replay"),
            Some(json!({})),
            200,
        ),
        (
            Method::DELETE,
            format!("/api/v1/accounts/{account}/webhooks/{webhook}"),
            None,
            204,
        ),
    ];
    for (method, uri, body, expected) in cases {
        let mut builder = Request::builder().method(method).uri(uri);
        let body = if let Some(value) = body {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        } else {
            Body::empty()
        };
        let response = app.clone().oneshot(builder.body(body)?).await?;
        let status = response.status();
        let _ = to_bytes(response.into_body(), 1024 * 1024).await?;
        assert_eq!(status, StatusCode::from_u16(expected)?);
    }
    Ok(())
}

struct Stub;
#[async_trait]
impl NotificationApiUseCases for Stub {
    async fn list_alerts(&self, _: AccountId) -> Result<Vec<Value>, NotificationApiError> {
        Ok(vec![json!({"id":"alert"})])
    }
    async fn create_alert(
        &self,
        _: AccountId,
        input: Value,
    ) -> Result<Value, NotificationApiError> {
        Ok(input)
    }
    async fn update_alert(
        &self,
        _: AccountId,
        _: AlertRuleId,
        input: Value,
    ) -> Result<Value, NotificationApiError> {
        Ok(input)
    }
    async fn delete_alert(&self, _: AccountId, _: AlertRuleId) -> Result<(), NotificationApiError> {
        Ok(())
    }
    async fn list_events(&self, _: AccountId) -> Result<Vec<Value>, NotificationApiError> {
        Ok(vec![json!({"state":"open"})])
    }
    async fn list_webhooks(&self, _: AccountId) -> Result<Vec<Value>, NotificationApiError> {
        Ok(vec![])
    }
    async fn create_webhook(
        &self,
        _: AccountId,
        input: Value,
    ) -> Result<Value, NotificationApiError> {
        Ok(input)
    }
    async fn verify_webhook(
        &self,
        _: AccountId,
        _: WebhookSubscriptionId,
        challenge: String,
    ) -> Result<Value, NotificationApiError> {
        Ok(json!({"challenge":challenge,"state":"active"}))
    }
    async fn delete_webhook(
        &self,
        _: AccountId,
        _: WebhookSubscriptionId,
    ) -> Result<(), NotificationApiError> {
        Ok(())
    }
    async fn attempts(
        &self,
        _: AccountId,
        _: WebhookSubscriptionId,
    ) -> Result<Vec<Value>, NotificationApiError> {
        Ok(vec![json!({"status":204})])
    }
    async fn replay(
        &self,
        _: AccountId,
        _: WebhookDeliveryId,
    ) -> Result<Value, NotificationApiError> {
        Ok(json!({"state":"pending"}))
    }
}
