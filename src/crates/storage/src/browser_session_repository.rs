//! Application browser-session port backed by management persistence.

use async_trait::async_trait;
use pvlog_application::{BrowserSessionRecord, BrowserSessionRepository, PortError};

use crate::{
    ManagementRepository, PostgresManagementRepository, SessionRecord, SqliteManagementRepository,
};

fn stored(record: &BrowserSessionRecord) -> SessionRecord {
    SessionRecord {
        id: record.id,
        user_id: record.user_id,
        session_digest: record.session_digest,
        csrf_digest: record.csrf_digest,
        created_at: record.created_at,
        last_seen_at: record.last_seen_at,
        idle_expires_at: record.idle_expires_at,
        absolute_expires_at: record.absolute_expires_at,
        revoked_at: record.revoked_at,
    }
}

fn browser(record: &SessionRecord) -> BrowserSessionRecord {
    BrowserSessionRecord {
        id: record.id,
        user_id: record.user_id,
        session_digest: record.session_digest,
        csrf_digest: record.csrf_digest,
        created_at: record.created_at,
        last_seen_at: record.last_seen_at,
        idle_expires_at: record.idle_expires_at,
        absolute_expires_at: record.absolute_expires_at,
        revoked_at: record.revoked_at,
    }
}

macro_rules! browser_session_repository {
    ($repository:ty) => {
        #[async_trait]
        impl BrowserSessionRepository for $repository {
            async fn save(&self, record: BrowserSessionRecord) -> Result<(), PortError> {
                self.save_session(&stored(&record))
                    .await
                    .map_err(|_| PortError::Unavailable)
            }

            async fn active_by_digest(
                &self,
                digest: &[u8; 32],
                now: i64,
            ) -> Result<Option<BrowserSessionRecord>, PortError> {
                self.active_session_by_digest(digest, now)
                    .await
                    .map(|record| record.as_ref().map(browser))
                    .map_err(|_| PortError::Unavailable)
            }

            async fn revoke(&self, id: pvlog_domain::SessionId, now: i64) -> Result<(), PortError> {
                self.revoke_session(id, now)
                    .await
                    .map_err(|_| PortError::Unavailable)
            }

            async fn revoke_oldest_above_limit(
                &self,
                user_id: pvlog_domain::UserId,
                keep: u32,
                now: i64,
            ) -> Result<(), PortError> {
                self.revoke_oldest_sessions_above_limit(user_id, keep, now)
                    .await
                    .map_err(|_| PortError::Unavailable)
            }
        }
    };
}

browser_session_repository!(SqliteManagementRepository);
browser_session_repository!(PostgresManagementRepository);
