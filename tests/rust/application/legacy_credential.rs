use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use pvlog_application::{
    LegacyCredentialError, LegacyCredentialInput, LegacyCredentialPolicy, LegacyCredentialRecord,
    LegacyCredentialRepository, LegacyCredentialService, PortError,
};
use pvlog_domain::{AccountId, SystemId};
use secrecy::SecretString;

#[tokio::test]
async fn legacy_keys_map_header_and_opt_in_query_auth_to_scoped_principals()
-> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let account = AccountId::new();
    let key = SecretString::from("legacy-secret");
    let digest = *blake3::keyed_hash(&[3; 32], b"legacy-secret").as_bytes();
    let service = LegacyCredentialService::new(
        Arc::new(FakeRepository {
            record: LegacyCredentialRecord {
                account_id: account,
                system_id: system,
                digest,
                policy: LegacyCredentialPolicy::ReadOnly,
                revoked: false,
            },
        }),
        [3; 32],
        true,
    );
    let input = LegacyCredentialInput {
        header_key: Some(key),
        header_system_id: Some(system),
        query_key: None,
        query_system_id: None,
    };
    assert_eq!(
        service.authenticate(&input, false).await?.account_id,
        account
    );
    assert!(matches!(
        service.authenticate(&input, true).await,
        Err(LegacyCredentialError::WriteForbidden)
    ));
    let ambiguous = LegacyCredentialInput {
        header_key: Some(SecretString::from("legacy-secret")),
        header_system_id: Some(system),
        query_key: Some(SecretString::from("legacy-secret")),
        query_system_id: Some(system),
    };
    assert!(matches!(
        service.authenticate(&ambiguous, false).await,
        Err(LegacyCredentialError::AmbiguousCredentials)
    ));
    Ok(())
}

struct FakeRepository {
    record: LegacyCredentialRecord,
}
#[async_trait]
impl LegacyCredentialRepository for FakeRepository {
    async fn credential(
        &self,
        system_id: SystemId,
        digest: &[u8; 32],
    ) -> Result<Option<LegacyCredentialRecord>, PortError> {
        Ok(
            (self.record.system_id == system_id && &self.record.digest == digest)
                .then(|| self.record.clone()),
        )
    }
}
