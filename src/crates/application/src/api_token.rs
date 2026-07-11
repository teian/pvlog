//! One-time-display, scoped modern API tokens with keyed digest storage.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, SystemId, UserId};
use secrecy::{ExposeSecret as _, SecretString};
use thiserror::Error;
use uuid::Uuid;

use crate::{Clock, PortError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiTokenRecord {
    pub id: ApiCredentialId,
    pub account_id: AccountId,
    pub owner_user_id: UserId,
    pub system_id: Option<SystemId>,
    pub name: String,
    pub digest: [u8; 32],
    pub scopes: BTreeSet<ApiScope>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ApiToken {
    pub id: ApiCredentialId,
    pub plaintext: SecretString,
    pub expires_at: Option<i64>,
}

#[async_trait]
pub trait ApiTokenRepository: Send + Sync {
    async fn save(&self, record: ApiTokenRecord) -> Result<(), PortError>;
    async fn active_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<ApiTokenRecord>, PortError>;
    async fn revoke(&self, id: ApiCredentialId, now: i64) -> Result<(), PortError>;
}

pub struct ApiTokenService {
    repository: Arc<dyn ApiTokenRepository>,
    clock: Arc<dyn Clock>,
    digest_key: [u8; 32],
}

impl ApiTokenService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn ApiTokenRepository>,
        clock: Arc<dyn Clock>,
        digest_key: [u8; 32],
    ) -> Self {
        Self {
            repository,
            clock,
            digest_key,
        }
    }

    /// Issues a new token whose plaintext is returned once.
    ///
    /// # Errors
    /// Returns an error for invalid scope/expiry input, time failure, or persistence failure.
    pub async fn issue(
        &self,
        account_id: AccountId,
        owner_user_id: UserId,
        system_id: Option<SystemId>,
        name: String,
        scopes: BTreeSet<ApiScope>,
        expires_at: Option<i64>,
    ) -> Result<ApiToken, ApiTokenError> {
        if name.trim().is_empty() || scopes.is_empty() {
            return Err(ApiTokenError::InvalidRequest);
        }
        let now = self.now()?;
        if expires_at.is_some_and(|expiry| expiry <= now) {
            return Err(ApiTokenError::InvalidRequest);
        }
        let id = ApiCredentialId::new();
        let plaintext = SecretString::from(format!(
            "pvlog_{}.{}{}",
            id,
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        self.repository
            .save(ApiTokenRecord {
                id,
                account_id,
                owner_user_id,
                system_id,
                name,
                digest: self.digest(&plaintext),
                scopes,
                created_at: now,
                expires_at,
                revoked_at: None,
            })
            .await
            .map_err(ApiTokenError::Repository)?;
        Ok(ApiToken {
            id,
            plaintext,
            expires_at,
        })
    }

    /// Verifies token integrity, lifetime, account, system, and required action scope.
    ///
    /// # Errors
    /// Returns an error when the token is invalid or persistence/time is unavailable.
    pub async fn verify(
        &self,
        plaintext: &SecretString,
        required_scope: ApiScope,
        account_id: AccountId,
        system_id: Option<SystemId>,
    ) -> Result<ApiTokenRecord, ApiTokenError> {
        let digest = self.digest(plaintext);
        let record = self
            .repository
            .active_by_digest(&digest, self.now()?)
            .await
            .map_err(ApiTokenError::Repository)?
            .ok_or(ApiTokenError::InvalidToken)?;
        if !constant_time_eq(&record.digest, &digest)
            || record.account_id != account_id
            || !record.scopes.contains(&required_scope)
            || record
                .system_id
                .is_some_and(|allowed| Some(allowed) != system_id)
        {
            return Err(ApiTokenError::InvalidToken);
        }
        Ok(record)
    }

    /// Revokes a verified token and returns its one-time replacement.
    ///
    /// # Errors
    /// Returns an error when verification, revocation, or replacement issuance fails.
    pub async fn rotate(
        &self,
        current: &SecretString,
        required_scope: ApiScope,
        account_id: AccountId,
        system_id: Option<SystemId>,
    ) -> Result<ApiToken, ApiTokenError> {
        let record = self
            .verify(current, required_scope, account_id, system_id)
            .await?;
        self.repository
            .revoke(record.id, self.now()?)
            .await
            .map_err(ApiTokenError::Repository)?;
        self.issue(
            record.account_id,
            record.owner_user_id,
            record.system_id,
            record.name,
            record.scopes,
            record.expires_at,
        )
        .await
    }

    /// Revokes a token by its non-secret identifier.
    ///
    /// # Errors
    /// Returns an error when time or persistence is unavailable.
    pub async fn revoke(&self, id: ApiCredentialId) -> Result<(), ApiTokenError> {
        self.repository
            .revoke(id, self.now()?)
            .await
            .map_err(ApiTokenError::Repository)
    }
    fn now(&self) -> Result<i64, ApiTokenError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| ApiTokenError::Time)
    }
    fn digest(&self, value: &SecretString) -> [u8; 32] {
        *blake3::keyed_hash(&self.digest_key, value.expose_secret().as_bytes()).as_bytes()
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
pub enum ApiTokenError {
    #[error("API token request is invalid")]
    InvalidRequest,
    #[error("API token is invalid, expired, revoked, or insufficiently scoped")]
    InvalidToken,
    #[error("clock value is invalid")]
    Time,
    #[error("API token persistence is unavailable")]
    Repository(PortError),
}
