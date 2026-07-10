//! Runtime composition for the `PVLog` application.

#![forbid(unsafe_code)]

pub mod config;

use async_trait::async_trait;
use pvlog_application::{Clock, CredentialService, PortError};
use pvlog_domain::{CredentialDigest, PasswordHash, UtcTimestamp};
use secrecy::{ExposeSecret as _, SecretString};

/// Wall clock used by production application services.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::now_utc())
    }
}

/// Keyed bearer-token digest boundary used by lifecycle tokens before password services are wired.
pub struct LifecycleCredentialService {
    digest_key: [u8; 32],
}

impl LifecycleCredentialService {
    #[must_use]
    pub fn new(secret: &SecretString) -> Self {
        Self {
            digest_key: *blake3::hash(secret.expose_secret().as_bytes()).as_bytes(),
        }
    }
}

#[async_trait]
impl CredentialService for LifecycleCredentialService {
    async fn hash_password(&self, _password: &SecretString) -> Result<PasswordHash, PortError> {
        Err(PortError::Rejected(
            "password_service_not_configured".to_owned(),
        ))
    }

    async fn verify_password(
        &self,
        _password: &SecretString,
        _expected: &PasswordHash,
    ) -> Result<bool, PortError> {
        Err(PortError::Rejected(
            "password_service_not_configured".to_owned(),
        ))
    }

    async fn digest_bearer(
        &self,
        credential: &SecretString,
    ) -> Result<CredentialDigest, PortError> {
        Ok(CredentialDigest::new(
            *blake3::keyed_hash(&self.digest_key, credential.expose_secret().as_bytes()).as_bytes(),
        ))
    }
}
