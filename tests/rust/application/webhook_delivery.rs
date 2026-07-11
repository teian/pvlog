use async_trait::async_trait;
use pvlog_application::{
    Clock, DeliveryRepository, DeliveryService, DeliveryServiceError, DeliveryWorkItem,
    JitterSource, PortError, WebhookRequest, WebhookResponse, WebhookSender,
};
use pvlog_domain::{
    UtcTimestamp, WebhookDelivery, WebhookDeliveryId, WebhookDeliveryState, WebhookSubscriptionId,
};
use std::{
    collections::VecDeque,
    error::Error,
    sync::{Arc, Mutex},
};
use url::Url;

#[tokio::test]
async fn attempts_back_off_dead_letter_replay_and_report_metrics() -> Result<(), Box<dyn Error>> {
    let clock = Arc::new(TestClock(Mutex::new(at(1_000)?)));
    let repository = Arc::new(MemoryRepository {
        item: Mutex::new(Some(item(at(1_000)?))),
    });
    let sender = Arc::new(FakeSender(Mutex::new(VecDeque::from([
        Ok(response(500)),
        Ok(response(500)),
        Ok(response(500)),
        Ok(response(204)),
    ]))));
    let service = DeliveryService::new(
        repository.clone(),
        sender,
        clock.clone(),
        Arc::new(FixedJitter),
    );

    assert_eq!(
        service.run_once("worker-1").await?,
        Some(WebhookDeliveryState::Retrying)
    );
    clock.set(at(2_100)?)?;
    assert_eq!(
        service.run_once("worker-1").await?,
        Some(WebhookDeliveryState::Retrying)
    );
    clock.set(at(4_200)?)?;
    assert_eq!(
        service.run_once("worker-1").await?,
        Some(WebhookDeliveryState::DeadLetter)
    );
    let delivery_id = repository
        .item
        .lock()
        .map_err(|_| "item lock")?
        .as_ref()
        .ok_or("item")?
        .delivery
        .id;
    assert_eq!(
        service.replay(delivery_id, at(5_000)?).await?.state,
        WebhookDeliveryState::Pending
    );
    clock.set(at(5_000)?)?;
    assert_eq!(
        service.run_once("worker-2").await?,
        Some(WebhookDeliveryState::Delivered)
    );
    let metrics = service.metrics()?;
    assert_eq!(
        (
            metrics.attempts,
            metrics.retries,
            metrics.dead_letters,
            metrics.replays,
            metrics.delivered
        ),
        (4, 2, 1, 1, 1)
    );
    Ok(())
}

fn item(now: UtcTimestamp) -> DeliveryWorkItem {
    DeliveryWorkItem {
        delivery: WebhookDelivery {
            id: WebhookDeliveryId::new(),
            subscription_id: WebhookSubscriptionId::new(),
            event_id: "event-1".to_owned(),
            schema_version: 1,
            state: WebhookDeliveryState::Pending,
            attempts: Vec::new(),
            next_attempt_at: Some(now),
        },
        request: WebhookRequest {
            endpoint: Url::parse("https://hooks.example.test/events")
                .unwrap_or_else(|_| unreachable!()),
            headers: Vec::new(),
            body: Vec::new(),
        },
        maximum_attempts: 3,
        lease_expires_at: now,
    }
}
fn at(value: i64) -> Result<UtcTimestamp, pvlog_domain::ValidationError> {
    UtcTimestamp::from_epoch_millis(value)
}
fn response(status: u16) -> WebhookResponse {
    WebhookResponse {
        status,
        retry_after_seconds: None,
    }
}

struct TestClock(Mutex<UtcTimestamp>);
impl TestClock {
    fn set(&self, value: UtcTimestamp) -> Result<(), &'static str> {
        *self.0.lock().map_err(|_| "clock lock")? = value;
        Ok(())
    }
}
impl Clock for TestClock {
    fn now(&self) -> UtcTimestamp {
        self.0.lock().map_or_else(
            |_| UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH),
            |value| *value,
        )
    }
}
struct FixedJitter;
impl JitterSource for FixedJitter {
    fn milliseconds(&self, _upper_exclusive: u64) -> u64 {
        0
    }
}
struct FakeSender(Mutex<VecDeque<Result<WebhookResponse, PortError>>>);
#[async_trait]
impl WebhookSender for FakeSender {
    async fn send(&self, _request: WebhookRequest) -> Result<WebhookResponse, PortError> {
        self.0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .pop_front()
            .ok_or(PortError::Unavailable)?
    }
}

struct MemoryRepository {
    item: Mutex<Option<DeliveryWorkItem>>,
}
#[async_trait]
impl DeliveryRepository for MemoryRepository {
    async fn claim(
        &self,
        _worker_id: &str,
        now: UtcTimestamp,
        lease_until: UtcTimestamp,
    ) -> Result<Option<DeliveryWorkItem>, DeliveryServiceError> {
        let mut item = self
            .item
            .lock()
            .map_err(|_| DeliveryServiceError::Unavailable)?;
        let available = item.as_ref().is_some_and(|value| {
            value
                .delivery
                .next_attempt_at
                .is_none_or(|scheduled| scheduled <= now)
                && !matches!(
                    value.delivery.state,
                    WebhookDeliveryState::Delivered | WebhookDeliveryState::DeadLetter
                )
        });
        if available {
            let mut claimed = item.clone().ok_or(DeliveryServiceError::Unavailable)?;
            claimed.lease_expires_at = lease_until;
            *item = Some(claimed.clone());
            Ok(Some(claimed))
        } else {
            Ok(None)
        }
    }
    async fn save(&self, item: &DeliveryWorkItem) -> Result<(), DeliveryServiceError> {
        *self
            .item
            .lock()
            .map_err(|_| DeliveryServiceError::Unavailable)? = Some(item.clone());
        Ok(())
    }
    async fn find(
        &self,
        id: WebhookDeliveryId,
    ) -> Result<Option<DeliveryWorkItem>, DeliveryServiceError> {
        Ok(self
            .item
            .lock()
            .map_err(|_| DeliveryServiceError::Unavailable)?
            .as_ref()
            .filter(|item| item.delivery.id == id)
            .cloned())
    }
}
