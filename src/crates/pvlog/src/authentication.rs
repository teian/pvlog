//! Runtime adapter that verifies HTTP credentials against management persistence.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{RequestAuthenticationError, RequestAuthenticator, RequestPrincipal};
use pvlog_application::Clock;
use pvlog_domain::ApiScope;
use pvlog_storage::ManagementRepository;
use secrecy::{ExposeSecret as _, SecretString};

/// Verifies bearer credentials and browser sessions from the management plane.
pub struct ManagementRequestAuthenticator {
    repository: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
    digest_key: [u8; 32],
}

impl ManagementRequestAuthenticator {
    #[must_use]
    pub fn new(
        repository: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
        session_secret: &SecretString,
    ) -> Self {
        Self {
            repository,
            clock,
            digest_key: blake3::derive_key(
                "pvlog/http-credential-digest/v1",
                session_secret.expose_secret().as_bytes(),
            ),
        }
    }

    fn digest(&self, value: &SecretString) -> [u8; 32] {
        *blake3::keyed_hash(&self.digest_key, value.expose_secret().as_bytes()).as_bytes()
    }

    fn now(&self) -> Result<i64, RequestAuthenticationError> {
        i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| RequestAuthenticationError::Unavailable)
    }
}

#[async_trait]
impl RequestAuthenticator for ManagementRequestAuthenticator {
    async fn authenticate_bearer(
        &self,
        token: SecretString,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        let digest = self.digest(&token);
        let record = self
            .repository
            .active_api_credential_by_digest(&digest, self.now()?)
            .await
            .map_err(map_port)?
            .ok_or(RequestAuthenticationError::Invalid)?;
        let scopes = record
            .scopes
            .iter()
            .map(|scope| parse_scope(scope))
            .collect::<Result<BTreeSet<_>, _>>()?;
        Ok(RequestPrincipal::ApiCredential {
            id: record.id,
            owner_user_id: record.owner_user_id,
            account_id: record.account_id,
            system_id: record.system_id,
            scopes,
        })
    }

    async fn authenticate_session(
        &self,
        session_token: SecretString,
        csrf_token: Option<SecretString>,
        state_changing: bool,
    ) -> Result<RequestPrincipal, RequestAuthenticationError> {
        let digest = self.digest(&session_token);
        let record = self
            .repository
            .active_session_by_digest(&digest, self.now()?)
            .await
            .map_err(map_port)?
            .ok_or(RequestAuthenticationError::Invalid)?;
        if state_changing
            && !csrf_token
                .is_some_and(|token| constant_time_eq(&record.csrf_digest, &self.digest(&token)))
        {
            return Err(RequestAuthenticationError::Invalid);
        }
        Ok(RequestPrincipal::User(record.user_id))
    }
}

fn parse_scope(scope: &str) -> Result<ApiScope, RequestAuthenticationError> {
    match scope {
        "systems_read" => Ok(ApiScope::SystemsRead),
        "systems_write" => Ok(ApiScope::SystemsWrite),
        "telemetry_read" => Ok(ApiScope::TelemetryRead),
        "telemetry_write" => Ok(ApiScope::TelemetryWrite),
        "integrations_manage" => Ok(ApiScope::IntegrationsManage),
        _ => Err(RequestAuthenticationError::Invalid),
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

fn map_port(_: pvlog_storage::ManagementRepositoryError) -> RequestAuthenticationError {
    RequestAuthenticationError::Unavailable
}
