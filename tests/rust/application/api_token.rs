use std::{
    collections::BTreeSet,
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pvlog_application::{ApiTokenRecord, ApiTokenRepository, ApiTokenService, Clock, PortError};
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, UserId, UtcTimestamp};
use secrecy::ExposeSecret as _;

#[tokio::test]
async fn api_tokens_are_one_time_scoped_rotatable_and_revocable() -> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = ApiTokenService::new(repository.clone(), Arc::new(FixedClock), [9; 32]);
    let account = AccountId::new();
    let issued = service
        .issue(
            account,
            UserId::new(),
            None,
            "uploader".to_owned(),
            BTreeSet::from([ApiScope::TelemetryWrite]),
            None,
        )
        .await?;
    assert!(issued.plaintext.expose_secret().starts_with("pvlog_"));
    assert!(!repository.contains_plaintext(issued.plaintext.expose_secret())?);
    service
        .verify(&issued.plaintext, ApiScope::TelemetryWrite, account, None)
        .await?;
    assert!(
        service
            .verify(&issued.plaintext, ApiScope::SystemsWrite, account, None)
            .await
            .is_err()
    );
    let rotated = service
        .rotate(&issued.plaintext, ApiScope::TelemetryWrite, account, None)
        .await?;
    assert!(
        service
            .verify(&issued.plaintext, ApiScope::TelemetryWrite, account, None)
            .await
            .is_err()
    );
    service.revoke(rotated.id).await?;
    assert!(
        service
            .verify(&rotated.plaintext, ApiScope::TelemetryWrite, account, None)
            .await
            .is_err()
    );
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
struct FakeRepository(Mutex<Vec<ApiTokenRecord>>);
impl FakeRepository {
    fn contains_plaintext(&self, value: &str) -> Result<bool, Box<dyn Error>> {
        Ok(self.0.lock().map_err(|_| "poisoned")?.iter().any(|record| {
            record
                .digest
                .windows(value.len())
                .any(|window| window == value.as_bytes())
        }))
    }
}
#[async_trait]
impl ApiTokenRepository for FakeRepository {
    async fn save(&self, record: ApiTokenRecord) -> Result<(), PortError> {
        self.0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push(record);
        Ok(())
    }
    async fn active_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<ApiTokenRecord>, PortError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .iter()
            .find(|record| {
                &record.digest == digest
                    && record.revoked_at.is_none()
                    && record.expires_at.is_none_or(|expiry| expiry > now)
            })
            .cloned())
    }
    async fn revoke(&self, id: ApiCredentialId, now: i64) -> Result<(), PortError> {
        if let Some(record) = self
            .0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .iter_mut()
            .find(|record| record.id == id)
        {
            record.revoked_at = Some(now);
        }
        Ok(())
    }
}
