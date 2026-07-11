use async_trait::async_trait;
use pvlog_application::{
    CreateWebhookSubscription, RotateWebhookSecret, StoredWebhookSubscription,
    WebhookEventEnvelope, WebhookService, WebhookServiceError, WebhookSubscriptionRepository,
    sign_webhook_event,
};
use pvlog_domain::{
    AccountId, UtcTimestamp, WebhookEventType, WebhookSubscriptionId, WebhookSubscriptionState,
};
use serde_json::json;
use std::{collections::BTreeSet, error::Error, sync::Mutex};
use url::Url;
use uuid::Uuid;

#[tokio::test]
async fn subscription_is_verified_disabled_and_rotated_with_overlap() -> Result<(), Box<dyn Error>>
{
    let account_id = AccountId::new();
    let repository = MemoryRepository::default();
    let service = WebhookService::new(repository, 60_000);
    let now = UtcTimestamp::from_epoch_millis(1_000_000)?;
    let pending = service
        .create(CreateWebhookSubscription {
            account_id,
            endpoint: Url::parse("https://hooks.example.test/events")?,
            events: BTreeSet::from([WebhookEventType::AlertOpened]),
            signing_key_reference: "secret:webhooks/v1".to_owned(),
            now,
        })
        .await?;
    assert_eq!(
        pending.subscription.state,
        WebhookSubscriptionState::PendingVerification
    );
    assert_eq!(
        service
            .verify(account_id, pending.subscription.id, "wrong", now)
            .await,
        Err(WebhookServiceError::VerificationFailed)
    );
    let active = service
        .verify(account_id, pending.subscription.id, &pending.challenge, now)
        .await?;
    assert_eq!(active.state, WebhookSubscriptionState::Active);
    let rotated = service
        .rotate(
            account_id,
            active.id,
            RotateWebhookSecret {
                signing_key_reference: "secret:webhooks/v2".to_owned(),
                overlap_until: UtcTimestamp::from_epoch_millis(1_300_000)?,
            },
        )
        .await?;
    assert_eq!(
        rotated.previous_signing_key_reference.as_deref(),
        Some("secret:webhooks/v1")
    );
    assert_eq!(
        rotated.subscription.signing_key_reference,
        "secret:webhooks/v2"
    );
    assert_eq!(
        service.disable(account_id, active.id).await?.state,
        WebhookSubscriptionState::Disabled
    );
    Ok(())
}

#[test]
fn event_ids_are_uuidv7_and_keyed_signatures_cover_timestamp_and_body() -> Result<(), Box<dyn Error>>
{
    let event = WebhookEventEnvelope::new(
        WebhookEventType::AlertOpened,
        UtcTimestamp::from_epoch_millis(1_700_000_000_123)?,
        json!({"rule_id": "rule-1"}),
    );
    assert_eq!(Uuid::parse_str(&event.event_id)?.get_version_num(), 7);
    let signed = sign_webhook_event(&event, &[7; 32])?;
    let repeated = sign_webhook_event(&event, &[7; 32])?;
    assert_eq!(signed, repeated);
    assert!(signed.signature.starts_with("v1="));
    assert_eq!(signed.timestamp_epoch_seconds, 1_700_000_000);
    assert_ne!(
        signed.signature,
        sign_webhook_event(&event, &[8; 32])?.signature
    );
    Ok(())
}

#[derive(Default)]
struct MemoryRepository {
    record: Mutex<Option<StoredWebhookSubscription>>,
}
#[async_trait]
impl WebhookSubscriptionRepository for MemoryRepository {
    async fn save(&self, record: &StoredWebhookSubscription) -> Result<(), WebhookServiceError> {
        *self
            .record
            .lock()
            .map_err(|_| WebhookServiceError::Unavailable)? = Some(record.clone());
        Ok(())
    }
    async fn find(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
    ) -> Result<Option<StoredWebhookSubscription>, WebhookServiceError> {
        Ok(self
            .record
            .lock()
            .map_err(|_| WebhookServiceError::Unavailable)?
            .as_ref()
            .filter(|record| {
                record.subscription.account_id == account_id && record.subscription.id == id
            })
            .cloned())
    }
}
