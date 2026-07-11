//! Verified webhook subscription lifecycle and signed versioned event envelopes.

use async_trait::async_trait;
use pvlog_domain::{
    AccountId, UtcTimestamp, WebhookEventType, WebhookSubscription, WebhookSubscriptionId,
    WebhookSubscriptionState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateWebhookSubscription {
    pub account_id: AccountId,
    pub endpoint: Url,
    pub events: BTreeSet<WebhookEventType>,
    pub signing_key_reference: String,
    pub now: UtcTimestamp,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingWebhookVerification {
    pub subscription: WebhookSubscription,
    pub challenge: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredWebhookSubscription {
    pub subscription: WebhookSubscription,
    pub challenge_digest: Option<[u8; 32]>,
    pub challenge_expires_at: Option<UtcTimestamp>,
    pub previous_signing_key_reference: Option<String>,
    pub previous_key_expires_at: Option<UtcTimestamp>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotateWebhookSecret {
    pub signing_key_reference: String,
    pub overlap_until: UtcTimestamp,
}

#[async_trait]
pub trait WebhookSubscriptionRepository: Send + Sync {
    async fn save(&self, record: &StoredWebhookSubscription) -> Result<(), WebhookServiceError>;
    async fn find(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
    ) -> Result<Option<StoredWebhookSubscription>, WebhookServiceError>;
}

pub struct WebhookService<R> {
    repository: R,
    verification_ttl_milliseconds: u64,
}
#[allow(clippy::missing_errors_doc)]
impl<R: WebhookSubscriptionRepository> WebhookService<R> {
    #[must_use]
    pub const fn new(repository: R, verification_ttl_milliseconds: u64) -> Self {
        Self {
            repository,
            verification_ttl_milliseconds,
        }
    }
    pub async fn create(
        &self,
        input: CreateWebhookSubscription,
    ) -> Result<PendingWebhookVerification, WebhookServiceError> {
        if input.events.is_empty() || input.signing_key_reference.trim().is_empty() {
            return Err(WebhookServiceError::Invalid);
        }
        let challenge = Uuid::now_v7().to_string();
        let subscription = WebhookSubscription {
            id: WebhookSubscriptionId::new(),
            account_id: input.account_id,
            endpoint: input.endpoint,
            events: input.events,
            state: WebhookSubscriptionState::PendingVerification,
            signing_key_reference: input.signing_key_reference,
            created_at: input.now,
        };
        let expires = add_milliseconds(input.now, self.verification_ttl_milliseconds)?;
        self.repository
            .save(&StoredWebhookSubscription {
                subscription: subscription.clone(),
                challenge_digest: Some(*blake3::hash(challenge.as_bytes()).as_bytes()),
                challenge_expires_at: Some(expires),
                previous_signing_key_reference: None,
                previous_key_expires_at: None,
            })
            .await?;
        Ok(PendingWebhookVerification {
            subscription,
            challenge,
        })
    }
    pub async fn verify(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
        challenge: &str,
        now: UtcTimestamp,
    ) -> Result<WebhookSubscription, WebhookServiceError> {
        let mut record = self
            .repository
            .find(account_id, id)
            .await?
            .ok_or(WebhookServiceError::NotFound)?;
        if record
            .challenge_expires_at
            .is_none_or(|expires| expires < now)
            || record.challenge_digest != Some(*blake3::hash(challenge.as_bytes()).as_bytes())
        {
            return Err(WebhookServiceError::VerificationFailed);
        }
        record.subscription.state = WebhookSubscriptionState::Active;
        record.challenge_digest = None;
        record.challenge_expires_at = None;
        self.repository.save(&record).await?;
        Ok(record.subscription)
    }
    pub async fn disable(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
    ) -> Result<WebhookSubscription, WebhookServiceError> {
        let mut record = self
            .repository
            .find(account_id, id)
            .await?
            .ok_or(WebhookServiceError::NotFound)?;
        record.subscription.state = WebhookSubscriptionState::Disabled;
        self.repository.save(&record).await?;
        Ok(record.subscription)
    }
    pub async fn rotate(
        &self,
        account_id: AccountId,
        id: WebhookSubscriptionId,
        rotation: RotateWebhookSecret,
    ) -> Result<StoredWebhookSubscription, WebhookServiceError> {
        let mut record = self
            .repository
            .find(account_id, id)
            .await?
            .ok_or(WebhookServiceError::NotFound)?;
        record.previous_signing_key_reference =
            Some(record.subscription.signing_key_reference.clone());
        record.previous_key_expires_at = Some(rotation.overlap_until);
        record.subscription.signing_key_reference = rotation.signing_key_reference;
        self.repository.save(&record).await?;
        Ok(record)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookEventEnvelope {
    pub schema_version: u16,
    pub event_id: String,
    pub event_type: WebhookEventType,
    pub occurred_at_epoch_millis: i128,
    pub data: Value,
}
impl WebhookEventEnvelope {
    #[must_use]
    pub fn new(event_type: WebhookEventType, occurred_at: UtcTimestamp, data: Value) -> Self {
        Self {
            schema_version: 1,
            event_id: Uuid::now_v7().to_string(),
            event_type,
            occurred_at_epoch_millis: occurred_at.epoch_millis(),
            data,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedWebhookEvent {
    pub body: Vec<u8>,
    pub event_id: String,
    pub timestamp_epoch_seconds: i64,
    pub signature: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebhookReplayPolicy {
    pub maximum_age_seconds: u32,
    pub require_unique_event_id: bool,
}
impl Default for WebhookReplayPolicy {
    fn default() -> Self {
        Self {
            maximum_age_seconds: 300,
            require_unique_event_id: true,
        }
    }
}

/// Serializes and signs an event using `timestamp.body`, suitable for replay-window validation.
///
/// # Errors
///
/// Returns an error when the envelope cannot be serialized or the timestamp is out of range.
pub fn sign_webhook_event(
    envelope: &WebhookEventEnvelope,
    signing_key: &[u8; 32],
) -> Result<SignedWebhookEvent, WebhookServiceError> {
    let body = serde_json::to_vec(envelope).map_err(|_| WebhookServiceError::Serialization)?;
    let timestamp_epoch_seconds = i64::try_from(envelope.occurred_at_epoch_millis / 1_000)
        .map_err(|_| WebhookServiceError::Serialization)?;
    let signed = [
        timestamp_epoch_seconds.to_string().as_bytes(),
        b".",
        body.as_slice(),
    ]
    .concat();
    let signature = blake3::keyed_hash(signing_key, &signed)
        .to_hex()
        .to_string();
    Ok(SignedWebhookEvent {
        body,
        event_id: envelope.event_id.clone(),
        timestamp_epoch_seconds,
        signature: format!("v1={signature}"),
    })
}

fn add_milliseconds(
    value: UtcTimestamp,
    milliseconds: u64,
) -> Result<UtcTimestamp, WebhookServiceError> {
    let milliseconds = i128::from(milliseconds);
    let target = value
        .epoch_millis()
        .checked_add(milliseconds)
        .ok_or(WebhookServiceError::Invalid)?;
    UtcTimestamp::from_epoch_millis(
        i64::try_from(target).map_err(|_| WebhookServiceError::Invalid)?,
    )
    .map_err(|_| WebhookServiceError::Invalid)
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WebhookServiceError {
    #[error("webhook subscription was not found")]
    NotFound,
    #[error("webhook subscription is invalid")]
    Invalid,
    #[error("webhook verification failed")]
    VerificationFailed,
    #[error("webhook repository is unavailable")]
    Unavailable,
    #[error("webhook event serialization failed")]
    Serialization,
}
