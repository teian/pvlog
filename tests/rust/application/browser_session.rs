use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pvlog_application::{
    BrowserSessionPolicy, BrowserSessionRecord, BrowserSessionRepository, BrowserSessionService,
    BrowserSessionUseCases, Clock, PortError,
};
use pvlog_domain::{SessionId, UserId, UtcTimestamp};
use secrecy::{ExposeSecret as _, SecretString};

const NOW: i64 = 1_780_000_000_000;

#[tokio::test]
async fn sessions_enforce_cookie_csrf_rotation_expiry_limit_and_logout()
-> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = BrowserSessionService::new(
        repository.clone(),
        Arc::new(FixedClock),
        [7; 32],
        BrowserSessionPolicy {
            idle_lifetime_seconds: 300,
            absolute_lifetime_seconds: 3_600,
            max_concurrent_sessions: 2,
            secure_cookies: true,
        },
    );
    let session = service.issue(UserId::new()).await?;
    assert_eq!(session.session_cookie.name, "__Host-pvlog_session");
    assert!(session.session_cookie.http_only && session.session_cookie.secure);
    assert_eq!(session.session_cookie.same_site, "Lax");
    assert_eq!(session.session_cookie.path, "/");
    assert_eq!(session.session_cookie.max_age_seconds, 3_600);
    assert!(!repository.contains_plaintext(session.session_cookie.value.expose_secret())?);
    assert!(
        service
            .authenticate(&session.session_cookie.value, None, true)
            .await
            .is_err()
    );
    service
        .authenticate(
            &session.session_cookie.value,
            Some(&session.csrf_token),
            true,
        )
        .await?;
    let old_token = SecretString::from(session.session_cookie.value.expose_secret().to_owned());
    let rotated = service.rotate(&old_token).await?;
    assert!(service.authenticate(&old_token, None, false).await.is_err());
    service
        .authenticate(&rotated.session_cookie.value, None, false)
        .await?;
    service.logout(&rotated.session_cookie.value).await?;
    assert!(
        service
            .authenticate(&rotated.session_cookie.value, None, false)
            .await
            .is_err()
    );
    assert_eq!(repository.last_limit()?, Some(2));

    let development_service = BrowserSessionService::new(
        Arc::new(FakeRepository::default()),
        Arc::new(FixedClock),
        [8; 32],
        BrowserSessionPolicy {
            idle_lifetime_seconds: 300,
            absolute_lifetime_seconds: 3_600,
            max_concurrent_sessions: 2,
            secure_cookies: false,
        },
    );
    let development_session = development_service.issue(UserId::new()).await?;
    assert_eq!(development_session.session_cookie.name, "pvlog_session");
    assert!(!development_session.session_cookie.secure);
    Ok(())
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(NOW))
    }
}

#[derive(Default)]
struct FakeRepository {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    sessions: Vec<BrowserSessionRecord>,
    last_limit: Option<u32>,
}
impl FakeRepository {
    fn contains_plaintext(&self, token: &str) -> Result<bool, Box<dyn Error>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "poisoned")?
            .sessions
            .iter()
            .any(|record| {
                record
                    .session_digest
                    .windows(token.len())
                    .any(|window| window == token.as_bytes())
            }))
    }
    fn last_limit(&self) -> Result<Option<u32>, Box<dyn Error>> {
        Ok(self.state.lock().map_err(|_| "poisoned")?.last_limit)
    }
}
#[async_trait]
impl BrowserSessionRepository for FakeRepository {
    async fn save(&self, record: BrowserSessionRecord) -> Result<(), PortError> {
        self.state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .sessions
            .push(record);
        Ok(())
    }
    async fn active_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<BrowserSessionRecord>, PortError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .sessions
            .iter()
            .find(|record| {
                &record.session_digest == digest
                    && record.revoked_at.is_none()
                    && record.idle_expires_at > now
                    && record.absolute_expires_at > now
            })
            .cloned())
    }
    async fn revoke(&self, id: SessionId, now: i64) -> Result<(), PortError> {
        if let Some(record) = self
            .state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .sessions
            .iter_mut()
            .find(|record| record.id == id)
        {
            record.revoked_at = Some(now);
        }
        Ok(())
    }
    async fn revoke_oldest_above_limit(
        &self,
        _user_id: UserId,
        keep: u32,
        _now: i64,
    ) -> Result<(), PortError> {
        self.state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .last_limit = Some(keep);
        Ok(())
    }
}
