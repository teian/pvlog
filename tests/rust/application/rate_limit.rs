use async_trait::async_trait;
use pvlog_application::{
    Clock, PortError, PrincipalQuota, RateLimitDecision, RateLimitError, RateLimitRepository,
    RateLimitService,
};
use pvlog_domain::UtcTimestamp;
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn quota_metadata_and_retry_timing_are_deterministic() -> Result<(), Box<dyn Error>> {
    let service = RateLimitService::new(Arc::new(FakeRepository::default()), Arc::new(FixedClock));
    let quota = PrincipalQuota {
        requests: 2,
        window_seconds: 60,
    };
    let first = service.admit("user:1", quota).await?;
    assert_eq!((first.limit, first.remaining), (2, 1));
    assert_eq!(RateLimitService::legacy_headers(first, false), []);
    assert_eq!(RateLimitService::legacy_headers(first, true).len(), 3);
    assert_eq!(service.admit("user:1", quota).await?.remaining, 0);
    match service.admit("user:1", quota).await {
        Err(RateLimitError::Exceeded(metadata)) => {
            assert_eq!(metadata.retry_after_seconds, Some(20));
        }
        other => return Err(format!("unexpected: {other:?}").into()),
    }
    Ok(())
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(
            time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(1_780_000_000_000),
        )
    }
}
#[derive(Default)]
struct FakeRepository(Mutex<HashMap<String, u32>>);
#[async_trait]
impl RateLimitRepository for FakeRepository {
    async fn increment(
        &self,
        key: &str,
        started: i64,
        seconds: u32,
    ) -> Result<RateLimitDecision, PortError> {
        let mut values = self.0.lock().map_err(|_| PortError::Unavailable)?;
        let used = values
            .entry(format!("{key}:{started}"))
            .and_modify(|value| *value += 1)
            .or_insert(1);
        Ok(RateLimitDecision {
            used: *used,
            resets_at: started + i64::from(seconds) * 1_000,
        })
    }
}
