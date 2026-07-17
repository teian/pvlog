//! Production notification administration backed by account storage.

use async_trait::async_trait;
use pvlog_api::{NotificationApiError, NotificationApiUseCases};
use pvlog_domain::{AccountId, AlertRuleId, SystemId, WebhookDeliveryId, WebhookSubscriptionId};
use pvlog_storage::{
    AlertRuleRecord, DatabaseTarget, OperationalRepository, PostgresOperationalRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteOperationalRepository,
    WebhookSubscriptionRecord,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct ManagementNotificationApi {
    target: DatabaseTarget,
}

impl ManagementNotificationApi {
    #[must_use]
    pub const fn new(target: DatabaseTarget) -> Self {
        Self { target }
    }

    async fn repository(
        &self,
        account_id: AccountId,
    ) -> Result<Box<dyn OperationalRepository>, NotificationApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let router = SqliteAccountPoolRouter::new(
                        management_path.clone(),
                        accounts_dir.clone(),
                        SqliteAccountPoolConfig::default(),
                    )
                    .map_err(|_| NotificationApiError::Unavailable)?;
                    let account = router
                        .route(account_id)
                        .await
                        .map_err(|_| NotificationApiError::Unavailable)?;
                    Ok(Box::new(SqliteOperationalRepository::new(
                        management_path.clone(),
                        account,
                    )))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir);
                    Err(NotificationApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Ok(Box::new(PostgresOperationalRepository::new(
                        url.clone(),
                        account_id,
                    )))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = url;
                    Err(NotificationApiError::Unavailable)
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertInput {
    name: String,
    kind: String,
    timezone: String,
    enabled: bool,
    #[serde(default)]
    condition: Value,
    #[serde(default)]
    system_id: Option<SystemId>,
}

#[async_trait]
impl NotificationApiUseCases for ManagementNotificationApi {
    async fn list_alerts(&self, account_id: AccountId) -> Result<Vec<Value>, NotificationApiError> {
        let records = self
            .repository(account_id)
            .await?
            .alerts()
            .await
            .map_err(|_| NotificationApiError::Unavailable)?;
        Ok(records.iter().map(alert_response).collect())
    }

    async fn create_alert(
        &self,
        account_id: AccountId,
        input: Value,
    ) -> Result<Value, NotificationApiError> {
        let input: AlertInput =
            serde_json::from_value(input).map_err(|_| NotificationApiError::Invalid)?;
        let system_id = input.system_id.ok_or(NotificationApiError::Invalid)?;
        let now = now();
        let record = alert_record(AlertRuleId::new(), system_id, input, now, now)?;
        self.repository(account_id)
            .await?
            .save_alert(&record)
            .await
            .map_err(|_| NotificationApiError::Unavailable)?;
        Ok(alert_response(&record))
    }

    async fn update_alert(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
        input: Value,
    ) -> Result<Value, NotificationApiError> {
        let repository = self.repository(account_id).await?;
        let existing = repository
            .alert(id)
            .await
            .map_err(|_| NotificationApiError::Unavailable)?
            .ok_or(NotificationApiError::NotFound)?;
        let input: AlertInput =
            serde_json::from_value(input).map_err(|_| NotificationApiError::Invalid)?;
        let record = alert_record(
            id,
            input.system_id.unwrap_or(existing.system_id),
            input,
            existing.created_at,
            now(),
        )?;
        repository
            .save_alert(&record)
            .await
            .map_err(|_| NotificationApiError::Unavailable)?;
        Ok(alert_response(&record))
    }

    async fn delete_alert(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<(), NotificationApiError> {
        if self
            .repository(account_id)
            .await?
            .delete_alert(id)
            .await
            .map_err(|_| NotificationApiError::Unavailable)?
        {
            Ok(())
        } else {
            Err(NotificationApiError::NotFound)
        }
    }

    async fn list_events(
        &self,
        _account_id: AccountId,
    ) -> Result<Vec<Value>, NotificationApiError> {
        Ok(Vec::new())
    }

    async fn list_webhooks(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Value>, NotificationApiError> {
        let records = self
            .repository(account_id)
            .await?
            .webhooks()
            .await
            .map_err(|_| NotificationApiError::Unavailable)?;
        Ok(records.iter().map(webhook_response).collect())
    }

    async fn create_webhook(
        &self,
        _account_id: AccountId,
        _input: Value,
    ) -> Result<Value, NotificationApiError> {
        Err(NotificationApiError::Unavailable)
    }

    async fn verify_webhook(
        &self,
        _account_id: AccountId,
        _id: WebhookSubscriptionId,
        _challenge: String,
    ) -> Result<Value, NotificationApiError> {
        Err(NotificationApiError::Unavailable)
    }

    async fn delete_webhook(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
    ) -> Result<(), NotificationApiError> {
        if self
            .repository(account_id)
            .await?
            .delete_webhook(id)
            .await
            .map_err(|_| NotificationApiError::Unavailable)?
        {
            Ok(())
        } else {
            Err(NotificationApiError::NotFound)
        }
    }

    async fn attempts(
        &self,
        _account_id: AccountId,
        _id: WebhookSubscriptionId,
    ) -> Result<Vec<Value>, NotificationApiError> {
        Ok(Vec::new())
    }

    async fn replay(
        &self,
        _account_id: AccountId,
        _id: WebhookDeliveryId,
    ) -> Result<Value, NotificationApiError> {
        Err(NotificationApiError::Unavailable)
    }
}

fn alert_record(
    id: AlertRuleId,
    system_id: SystemId,
    input: AlertInput,
    created_at: i64,
    updated_at: i64,
) -> Result<AlertRuleRecord, NotificationApiError> {
    if input.name.trim().is_empty() || input.timezone.trim().is_empty() {
        return Err(NotificationApiError::Invalid);
    }
    Ok(AlertRuleRecord {
        id,
        system_id,
        name: input.name,
        alert_kind: storage_kind(&input.kind)?.to_owned(),
        enabled: input.enabled,
        condition: input.condition,
        schedule: json!({"timezone": input.timezone}),
        debounce_seconds: 0,
        cooldown_seconds: 0,
        created_at,
        updated_at,
    })
}

fn alert_response(record: &AlertRuleRecord) -> Value {
    json!({
        "id": record.id,
        "systemId": record.system_id,
        "name": record.name,
        "kind": api_kind(&record.alert_kind, &record.condition),
        "timezone": record.schedule.get("timezone").and_then(Value::as_str).unwrap_or("UTC"),
        "enabled": record.enabled,
        "condition": record.condition,
    })
}

fn webhook_response(record: &WebhookSubscriptionRecord) -> Value {
    json!({
        "id": record.id,
        "endpoint": record.endpoint_url,
        "events": record.event_types,
        "state": record.state,
    })
}

fn storage_kind(kind: &str) -> Result<&'static str, NotificationApiError> {
    match kind {
        "idle" => Ok("idle"),
        "generation_below" => Ok("generation"),
        "consumption_above" => Ok("consumption"),
        "net_power_above" => Ok("net_power"),
        "standby_cost_above" => Ok("standby_cost"),
        "performance_below" => Ok("performance"),
        "battery_below" => Ok("battery"),
        "extended_below" | "extended_above" => Ok("extended_channel"),
        _ => Err(NotificationApiError::Invalid),
    }
}

fn api_kind(kind: &str, condition: &Value) -> &'static str {
    match kind {
        "generation" => "generation_below",
        "consumption" => "consumption_above",
        "net_power" => "net_power_above",
        "standby_cost" => "standby_cost_above",
        "performance" => "performance_below",
        "battery" => "battery_below",
        "extended_channel"
            if condition.get("direction").and_then(Value::as_str) == Some("above") =>
        {
            "extended_above"
        }
        "extended_channel" => "extended_below",
        _ => "idle",
    }
}

fn now() -> i64 {
    let value = time::OffsetDateTime::now_utc();
    value.unix_timestamp() * 1_000 + i64::from(value.nanosecond() / 1_000_000)
}
