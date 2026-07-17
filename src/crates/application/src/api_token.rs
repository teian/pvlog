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
    pub plaintext: SecretString,
    pub credential: ApiTokenRecord,
}

#[async_trait]
pub trait ApiTokenRepository: Send + Sync {
    async fn save(&self, record: ApiTokenRecord) -> Result<(), PortError>;
    async fn active_by_digest(
        &self,
        digest: &[u8; 32],
        now: i64,
    ) -> Result<Option<ApiTokenRecord>, PortError>;
    async fn list_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<ApiTokenRecord>, PortError>;
    async fn revoke(
        &self,
        account_id: AccountId,
        id: ApiCredentialId,
        now: i64,
    ) -> Result<bool, PortError>;
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
        let credential = ApiTokenRecord {
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
        };
        self.repository
            .save(credential.clone())
            .await
            .map_err(ApiTokenError::Repository)?;
        Ok(ApiToken {
            plaintext,
            credential,
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
        let revoked = self
            .repository
            .revoke(record.account_id, record.id, self.now()?)
            .await
            .map_err(ApiTokenError::Repository)?;
        if !revoked {
            return Err(ApiTokenError::NotFound);
        }
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

    /// Lists safe token metadata for an account.
    ///
    /// # Errors
    /// Returns an error when persistence is unavailable.
    pub async fn list(&self, account_id: AccountId) -> Result<Vec<ApiTokenRecord>, ApiTokenError> {
        self.repository
            .list_for_account(account_id)
            .await
            .map_err(ApiTokenError::Repository)
    }

    /// Revokes a token owned by the specified account.
    ///
    /// # Errors
    /// Returns not found for an unknown or foreign token and a persistence error on failure.
    pub async fn revoke(
        &self,
        account_id: AccountId,
        id: ApiCredentialId,
    ) -> Result<(), ApiTokenError> {
        let revoked = self
            .repository
            .revoke(account_id, id, self.now()?)
            .await
            .map_err(ApiTokenError::Repository)?;
        if revoked {
            Ok(())
        } else {
            Err(ApiTokenError::NotFound)
        }
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
    #[error("API token was not found")]
    NotFound,
    #[error("clock value is invalid")]
    Time,
    #[error("API token persistence is unavailable")]
    Repository(PortError),
}
