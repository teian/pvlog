//! Runtime account API-key lifecycle backed by management persistence.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{
    AccountApiKeyError, AccountApiKeyMetadata, AccountApiKeyScope, AccountApiKeyUseCases,
    IssuedAccountApiKey,
};
use pvlog_application::{ApiTokenError, ApiTokenRecord, ApiTokenService, Clock, PortError};
use pvlog_domain::{AccountId, ApiCredentialId, ApiScope, AuditEventId, UserId};
use pvlog_storage::{AuditRecord, ManagementRepository};

/// Production account API-key use cases with lifecycle auditing.
pub struct ManagementAccountApiKeyService {
    service: ApiTokenService,
    repository: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl ManagementAccountApiKeyService {
    #[must_use]
    pub fn new(
        service: ApiTokenService,
        repository: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            service,
            repository,
            clock,
        }
    }

    async fn audit(
        &self,
        actor: UserId,
        account_id: AccountId,
        action: &'static str,
        target_id: ApiCredentialId,
    ) -> Result<(), AccountApiKeyError> {
        let id = AuditEventId::new();
        let mut event_hash = [0_u8; 32];
        event_hash[..16].copy_from_slice(id.as_uuid().as_bytes());
        event_hash[16..].copy_from_slice(id.as_uuid().as_bytes());
        let occurred_at = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| AccountApiKeyError::Unavailable)?;
        self.repository
            .append_audit(&AuditRecord {
                id,
                occurred_at,
                request_id: None,
                actor_type: "user".to_owned(),
                actor_id: Some(actor.as_uuid()),
                account_id: Some(account_id),
                action: action.to_owned(),
                target_type: "api_credential".to_owned(),
                target_id: Some(target_id.as_uuid()),
                outcome: "succeeded".to_owned(),
                previous_event_hash: None,
                event_hash,
                safe_metadata: serde_json::json!({}),
            })
            .await
            .map_err(|_| AccountApiKeyError::Unavailable)
    }
}

#[async_trait]
impl AccountApiKeyUseCases for ManagementAccountApiKeyService {
    async fn issue(
        &self,
        actor: UserId,
        account_id: AccountId,
        name: String,
        scopes: BTreeSet<ApiScope>,
        expires_at: Option<i64>,
    ) -> Result<IssuedAccountApiKey, AccountApiKeyError> {
        if scopes.contains(&ApiScope::IntegrationsManage) {
            return Err(AccountApiKeyError::Invalid);
        }
        let issued = self
            .service
            .issue(account_id, actor, None, name, scopes, expires_at)
            .await
            .map_err(map_api_token_error)?;
        if let Err(error) = self
            .audit(
                actor,
                account_id,
                "account.api_key.issued",
                issued.credential.id,
            )
            .await
        {
            let _ = self.service.revoke(account_id, issued.credential.id).await;
            return Err(error);
        }
        Ok(IssuedAccountApiKey {
            api_key: issued.plaintext,
            credential: metadata(issued.credential)?,
        })
    }

    async fn list(
        &self,
        _actor: UserId,
        account_id: AccountId,
    ) -> Result<Vec<AccountApiKeyMetadata>, AccountApiKeyError> {
        self.service
            .list(account_id)
            .await
            .map_err(map_api_token_error)?
            .into_iter()
            .map(metadata)
            .collect()
    }

    async fn revoke(
        &self,
        actor: UserId,
        account_id: AccountId,
        id: ApiCredentialId,
    ) -> Result<(), AccountApiKeyError> {
        self.service
            .revoke(account_id, id)
            .await
            .map_err(map_api_token_error)?;
        self.audit(actor, account_id, "account.api_key.revoked", id)
            .await
    }
}

fn metadata(record: ApiTokenRecord) -> Result<AccountApiKeyMetadata, AccountApiKeyError> {
    Ok(AccountApiKeyMetadata {
        id: record.id,
        name: record.name,
        scopes: record
            .scopes
            .into_iter()
            .map(AccountApiKeyScope::try_from)
            .collect::<Result<_, _>>()?,
        created_at_epoch_millis: record.created_at,
        expires_at_epoch_millis: record.expires_at,
        revoked_at_epoch_millis: record.revoked_at,
    })
}

fn map_api_token_error(error: ApiTokenError) -> AccountApiKeyError {
    match error {
        ApiTokenError::InvalidRequest | ApiTokenError::Time => AccountApiKeyError::Invalid,
        ApiTokenError::InvalidToken | ApiTokenError::NotFound => AccountApiKeyError::NotFound,
        ApiTokenError::Repository(PortError::Conflict) => AccountApiKeyError::Conflict,
        ApiTokenError::Repository(PortError::Rejected(_)) => AccountApiKeyError::Invalid,
        ApiTokenError::Repository(PortError::NotFound) => AccountApiKeyError::NotFound,
        ApiTokenError::Repository(PortError::Unavailable) => AccountApiKeyError::Unavailable,
    }
}
