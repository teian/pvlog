//! Secure server-side browser sessions shared by every interactive login method.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_domain::{SessionId, UserId};
use secrecy::{ExposeSecret as _, SecretString};
use thiserror::Error;
use uuid::Uuid;

use crate::{Clock, PortError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserSessionPolicy {
    pub idle_lifetime_seconds: u32,
    pub absolute_lifetime_seconds: u32,
    pub max_concurrent_sessions: u32,
    pub secure_cookies: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSessionRecord {
    pub id: SessionId,
    pub user_id: UserId,
    pub session_digest: [u8; 32],
    pub csrf_digest: [u8; 32],
    pub created_at: i64,
    pub last_seen_at: i64,
    pub idle_expires_at: i64,
    pub absolute_expires_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct SessionCookie {
    pub name: &'static str,
    pub value: SecretString,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: &'static str,
    pub path: &'static str,
}

#[derive(Clone, Debug)]
pub struct BrowserSession {
    pub user_id: UserId,
    pub session_cookie: SessionCookie,
    pub csrf_token: SecretString,
    pub idle_expires_at: i64,
    pub absolute_expires_at: i64,
}

#[async_trait]
pub trait BrowserSessionRepository: Send + Sync {
    async fn save(&self, record: BrowserSessionRecord) -> Result<(), PortError>;
    async fn active_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<BrowserSessionRecord>, PortError>;
    async fn revoke(&self, id: SessionId, now: i64) -> Result<(), PortError>;
    async fn revoke_oldest_above_limit(
        &self,
        user_id: UserId,
        keep: u32,
        now: i64,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait BrowserSessionUseCases: Send + Sync {
    async fn issue(&self, user_id: UserId) -> Result<BrowserSession, BrowserSessionError>;
    async fn authenticate(
        &self,
        session_token: &SecretString,
        csrf_token: Option<&SecretString>,
        state_changing: bool,
    ) -> Result<BrowserSessionRecord, BrowserSessionError>;
    async fn rotate(
        &self,
        session_token: &SecretString,
    ) -> Result<BrowserSession, BrowserSessionError>;
    async fn logout(&self, session_token: &SecretString) -> Result<(), BrowserSessionError>;
}

pub struct BrowserSessionService {
    repository: Arc<dyn BrowserSessionRepository>,
    clock: Arc<dyn Clock>,
    digest_key: [u8; 32],
    policy: BrowserSessionPolicy,
}

impl BrowserSessionService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn BrowserSessionRepository>,
        clock: Arc<dyn Clock>,
        digest_key: [u8; 32],
        policy: BrowserSessionPolicy,
    ) -> Self {
        Self {
            repository,
            clock,
            digest_key,
            policy,
        }
    }

    fn now(&self) -> Result<i64, BrowserSessionError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| BrowserSessionError::Time)
    }
    fn digest(&self, value: &SecretString) -> [u8; 32] {
        *blake3::keyed_hash(&self.digest_key, value.expose_secret().as_bytes()).as_bytes()
    }
    fn token() -> SecretString {
        SecretString::from(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ))
    }

    async fn issue_at(
        &self,
        user_id: UserId,
        now: i64,
    ) -> Result<BrowserSession, BrowserSessionError> {
        let session_token = Self::token();
        let csrf_token = Self::token();
        let idle_expires_at = now
            .checked_add(i64::from(self.policy.idle_lifetime_seconds) * 1_000)
            .ok_or(BrowserSessionError::Time)?;
        let absolute_expires_at = now
            .checked_add(i64::from(self.policy.absolute_lifetime_seconds) * 1_000)
            .ok_or(BrowserSessionError::Time)?;
        self.repository
            .save(BrowserSessionRecord {
                id: SessionId::new(),
                user_id,
                session_digest: self.digest(&session_token),
                csrf_digest: self.digest(&csrf_token),
                created_at: now,
                last_seen_at: now,
                idle_expires_at,
                absolute_expires_at,
                revoked_at: None,
            })
            .await
            .map_err(BrowserSessionError::Repository)?;
        self.repository
            .revoke_oldest_above_limit(user_id, self.policy.max_concurrent_sessions, now)
            .await
            .map_err(BrowserSessionError::Repository)?;
        Ok(BrowserSession {
            user_id,
            session_cookie: SessionCookie {
                name: if self.policy.secure_cookies {
                    "__Host-pvlog_session"
                } else {
                    "pvlog_session"
                },
                value: session_token,
                http_only: true,
                secure: self.policy.secure_cookies,
                same_site: "Lax",
                path: "/",
            },
            csrf_token,
            idle_expires_at,
            absolute_expires_at,
        })
    }
}

#[async_trait]
impl BrowserSessionUseCases for BrowserSessionService {
    async fn issue(&self, user_id: UserId) -> Result<BrowserSession, BrowserSessionError> {
        self.issue_at(user_id, self.now()?).await
    }
    async fn authenticate(
        &self,
        session_token: &SecretString,
        csrf_token: Option<&SecretString>,
        state_changing: bool,
    ) -> Result<BrowserSessionRecord, BrowserSessionError> {
        let now = self.now()?;
        let record = self
            .repository
            .active_by_digest(&self.digest(session_token), now)
            .await
            .map_err(BrowserSessionError::Repository)?
            .ok_or(BrowserSessionError::InvalidSession)?;
        if state_changing
            && !csrf_token
                .is_some_and(|token| constant_time_eq(&record.csrf_digest, &self.digest(token)))
        {
            return Err(BrowserSessionError::InvalidCsrf);
        }
        Ok(record)
    }
    async fn rotate(
        &self,
        session_token: &SecretString,
    ) -> Result<BrowserSession, BrowserSessionError> {
        let record = self.authenticate(session_token, None, false).await?;
        let now = self.now()?;
        self.repository
            .revoke(record.id, now)
            .await
            .map_err(BrowserSessionError::Repository)?;
        self.issue_at(record.user_id, now).await
    }
    async fn logout(&self, session_token: &SecretString) -> Result<(), BrowserSessionError> {
        if let Some(record) = self
            .repository
            .active_by_digest(&self.digest(session_token), self.now()?)
            .await
            .map_err(BrowserSessionError::Repository)?
        {
            self.repository
                .revoke(record.id, self.now()?)
                .await
                .map_err(BrowserSessionError::Repository)?;
        }
        Ok(())
    }
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[derive(Debug, Error)]
pub enum BrowserSessionError {
    #[error("browser session is invalid or expired")]
    InvalidSession,
    #[error("CSRF proof is invalid")]
    InvalidCsrf,
    #[error("clock value is invalid")]
    Time,
    #[error("browser session persistence is unavailable")]
    Repository(PortError),
}
