//! Leased webhook delivery, retry scheduling, dead letters, and administrative replay.

use crate::{Clock, WebhookRequest, WebhookSender};
use async_trait::async_trait;
use pvlog_domain::{
    DeliveryAttempt, UtcTimestamp, WebhookDelivery, WebhookDeliveryId, WebhookDeliveryState,
};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryWorkItem {
    pub delivery: WebhookDelivery,
    pub request: WebhookRequest,
    pub maximum_attempts: u16,
    pub lease_expires_at: UtcTimestamp,
}

#[async_trait]
pub trait DeliveryRepository: Send + Sync {
    async fn claim(
        &self,
        worker_id: &str,
        now: UtcTimestamp,
        lease_until: UtcTimestamp,
    ) -> Result<Option<DeliveryWorkItem>, DeliveryServiceError>;
    async fn save(&self, item: &DeliveryWorkItem) -> Result<(), DeliveryServiceError>;
    async fn find(
        &self,
        id: WebhookDeliveryId,
    ) -> Result<Option<DeliveryWorkItem>, DeliveryServiceError>;
}
pub trait JitterSource: Send + Sync {
    fn milliseconds(&self, upper_exclusive: u64) -> u64;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeliveryMetrics {
    pub attempts: u64,
    pub delivered: u64,
    pub retries: u64,
    pub dead_letters: u64,
    pub replays: u64,
}

pub struct DeliveryService<R, S, C, J> {
    repository: Arc<R>,
    sender: Arc<S>,
    clock: Arc<C>,
    jitter: Arc<J>,
    lease_milliseconds: u64,
    base_backoff_milliseconds: u64,
    maximum_backoff_milliseconds: u64,
    metrics: Mutex<DeliveryMetrics>,
}
#[allow(clippy::missing_errors_doc)]
impl<R: DeliveryRepository, S: WebhookSender, C: Clock, J: JitterSource>
    DeliveryService<R, S, C, J>
{
    #[must_use]
    pub fn new(repository: Arc<R>, sender: Arc<S>, clock: Arc<C>, jitter: Arc<J>) -> Self {
        Self {
            repository,
            sender,
            clock,
            jitter,
            lease_milliseconds: 30_000,
            base_backoff_milliseconds: 1_000,
            maximum_backoff_milliseconds: 3_600_000,
            metrics: Mutex::new(DeliveryMetrics::default()),
        }
    }
    pub async fn run_once(
        &self,
        worker_id: &str,
    ) -> Result<Option<WebhookDeliveryState>, DeliveryServiceError> {
        let now = self.clock.now();
        let lease_until = add(now, self.lease_milliseconds)?;
        let Some(mut item) = self.repository.claim(worker_id, now, lease_until).await? else {
            return Ok(None);
        };
        let started = self.clock.now();
        let response = self.sender.send(item.request.clone()).await;
        let finished = self.clock.now();
        let status = response.as_ref().ok().map(|value| value.status);
        item.delivery.attempts.push(DeliveryAttempt {
            attempted_at: started,
            response_status: status,
            error_class: response.as_ref().err().map(|_| "delivery_error".to_owned()),
            duration_milliseconds: u32::try_from(elapsed(started, finished)).unwrap_or(u32::MAX),
        });
        self.metric(|metrics| metrics.attempts = metrics.attempts.saturating_add(1))?;
        if status.is_some_and(|value| (200..300).contains(&value)) {
            item.delivery.state = WebhookDeliveryState::Delivered;
            item.delivery.next_attempt_at = None;
            self.metric(|metrics| metrics.delivered = metrics.delivered.saturating_add(1))?;
        } else if item.delivery.attempts.len() >= usize::from(item.maximum_attempts) {
            item.delivery.state = WebhookDeliveryState::DeadLetter;
            item.delivery.next_attempt_at = None;
            self.metric(|metrics| metrics.dead_letters = metrics.dead_letters.saturating_add(1))?;
        } else {
            item.delivery.state = WebhookDeliveryState::Retrying;
            let exponent = u32::try_from(item.delivery.attempts.len().saturating_sub(1))
                .unwrap_or(u32::MAX)
                .min(31);
            let backoff = self
                .base_backoff_milliseconds
                .saturating_mul(1_u64 << exponent)
                .min(self.maximum_backoff_milliseconds);
            let jitter = self.jitter.milliseconds(backoff / 4 + 1);
            item.delivery.next_attempt_at = Some(add(finished, backoff.saturating_add(jitter))?);
            self.metric(|metrics| metrics.retries = metrics.retries.saturating_add(1))?;
        }
        let state = item.delivery.state;
        self.repository.save(&item).await?;
        Ok(Some(state))
    }
    pub async fn replay(
        &self,
        id: WebhookDeliveryId,
        now: UtcTimestamp,
    ) -> Result<WebhookDelivery, DeliveryServiceError> {
        let mut item = self
            .repository
            .find(id)
            .await?
            .ok_or(DeliveryServiceError::NotFound)?;
        if item.delivery.state != WebhookDeliveryState::DeadLetter {
            return Err(DeliveryServiceError::NotDeadLetter);
        }
        item.delivery.state = WebhookDeliveryState::Pending;
        item.delivery.next_attempt_at = Some(now);
        item.delivery.attempts.clear();
        self.repository.save(&item).await?;
        self.metric(|metrics| metrics.replays = metrics.replays.saturating_add(1))?;
        Ok(item.delivery)
    }
    pub fn metrics(&self) -> Result<DeliveryMetrics, DeliveryServiceError> {
        self.metrics
            .lock()
            .map(|value| value.clone())
            .map_err(|_| DeliveryServiceError::Unavailable)
    }
    fn metric(
        &self,
        update: impl FnOnce(&mut DeliveryMetrics),
    ) -> Result<(), DeliveryServiceError> {
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|_| DeliveryServiceError::Unavailable)?;
        update(&mut metrics);
        Ok(())
    }
}
fn elapsed(start: UtcTimestamp, end: UtcTimestamp) -> u64 {
    u64::try_from(end.epoch_millis().saturating_sub(start.epoch_millis())).unwrap_or_default()
}
fn add(value: UtcTimestamp, milliseconds: u64) -> Result<UtcTimestamp, DeliveryServiceError> {
    let target = value
        .epoch_millis()
        .checked_add(i128::from(milliseconds))
        .ok_or(DeliveryServiceError::Unavailable)?;
    UtcTimestamp::from_epoch_millis(
        i64::try_from(target).map_err(|_| DeliveryServiceError::Unavailable)?,
    )
    .map_err(|_| DeliveryServiceError::Unavailable)
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DeliveryServiceError {
    #[error("delivery was not found")]
    NotFound,
    #[error("delivery is not a dead letter")]
    NotDeadLetter,
    #[error("delivery repository is unavailable")]
    Unavailable,
}
