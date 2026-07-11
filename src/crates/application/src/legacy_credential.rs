//! PVOutput-compatible per-system credentials mapped to canonical principals.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_domain::{AccountId, SystemId};
use secrecy::{ExposeSecret as _, SecretString};
use thiserror::Error;

use crate::PortError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyCredentialPolicy {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyCredentialRecord {
    pub account_id: AccountId,
    pub system_id: SystemId,
    pub digest: [u8; 32],
    pub policy: LegacyCredentialPolicy,
    pub revoked: bool,
}

#[derive(Clone, Debug)]
pub struct LegacyCredentialInput {
    pub header_key: Option<SecretString>,
    pub header_system_id: Option<SystemId>,
    pub query_key: Option<SecretString>,
    pub query_system_id: Option<SystemId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyPrincipal {
    pub account_id: AccountId,
    pub system_id: SystemId,
    pub can_read: bool,
    pub can_write: bool,
}

#[async_trait]
pub trait LegacyCredentialRepository: Send + Sync {
    async fn credential(
        &self,
        system_id: SystemId,
        digest: &[u8; 32],
    ) -> Result<Option<LegacyCredentialRecord>, PortError>;
}

pub struct LegacyCredentialService {
    repository: Arc<dyn LegacyCredentialRepository>,
    digest_key: [u8; 32],
    allow_query_authentication: bool,
}

impl LegacyCredentialService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn LegacyCredentialRepository>,
        digest_key: [u8; 32],
        allow_query_authentication: bool,
    ) -> Self {
        Self {
            repository,
            digest_key,
            allow_query_authentication,
        }
    }

    /// Authenticates one unambiguous legacy credential source for a requested access mode.
    ///
    /// # Errors
    /// Returns an error for invalid, ambiguous, revoked, read-only, or unavailable credentials.
    pub async fn authenticate(
        &self,
        input: &LegacyCredentialInput,
        write: bool,
    ) -> Result<LegacyPrincipal, LegacyCredentialError> {
        let header = input.header_key.as_ref().zip(input.header_system_id);
        let query = input.query_key.as_ref().zip(input.query_system_id);
        if header.is_some() && query.is_some() {
            return Err(LegacyCredentialError::AmbiguousCredentials);
        }
        let (key, system_id) = header
            .or_else(|| self.allow_query_authentication.then_some(query).flatten())
            .ok_or(LegacyCredentialError::InvalidCredentials)?;
        let digest =
            *blake3::keyed_hash(&self.digest_key, key.expose_secret().as_bytes()).as_bytes();
        let credential = self
            .repository
            .credential(system_id, &digest)
            .await
            .map_err(LegacyCredentialError::Repository)?
            .filter(|credential| !credential.revoked)
            .ok_or(LegacyCredentialError::InvalidCredentials)?;
        let can_write = credential.policy == LegacyCredentialPolicy::ReadWrite;
        if write && !can_write {
            return Err(LegacyCredentialError::WriteForbidden);
        }
        Ok(LegacyPrincipal {
            account_id: credential.account_id,
            system_id: credential.system_id,
            can_read: true,
            can_write,
        })
    }
}

#[derive(Debug, Error)]
pub enum LegacyCredentialError {
    #[error("legacy credentials are invalid")]
    InvalidCredentials,
    #[error("legacy credentials must not be supplied in multiple locations")]
    AmbiguousCredentials,
    #[error("legacy credential is read-only")]
    WriteForbidden,
    #[error("legacy credential persistence is unavailable")]
    Repository(PortError),
}
