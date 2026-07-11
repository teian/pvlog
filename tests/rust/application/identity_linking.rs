use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pvlog_application::{
    Clock, ExternalIdentityLinkingError, ExternalIdentityLinkingRepository,
    ExternalIdentityLinkingService, ExternalIdentityLinkingUseCases, ExternalLoginOutcome,
    ExternalLoginPolicy, IdentityClaims, LinkExternalIdentity, LinkedIdentityRecord, PortError,
    UnlinkExternalIdentity,
};
use pvlog_domain::{ConnectorId, ExternalIdentityId, UserId, UtcTimestamp};

#[tokio::test]
async fn external_identity_linking_prevents_takeover_and_final_login_removal()
-> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = ExternalIdentityLinkingService::new(
        repository.clone(),
        Arc::new(FixedClock),
        ExternalLoginPolicy {
            allow_just_in_time_provisioning: true,
        },
    );
    let connector = ConnectorId::new();
    let owner = match service
        .resolve_external_login(connector, claims("immutable-subject", "owner@example.test"))
        .await?
    {
        ExternalLoginOutcome::ProvisionedUser(user) => user,
        outcome => return Err(format!("unexpected outcome: {outcome:?}").into()),
    };
    assert_eq!(repository.created_user_count()?, 1);
    assert_eq!(
        service
            .resolve_external_login(
                connector,
                claims("immutable-subject", "attacker@example.test")
            )
            .await?,
        ExternalLoginOutcome::ExistingUser(owner)
    );
    assert_eq!(
        repository.created_user_count()?,
        1,
        "matching email must not create or merge a user"
    );

    let attacker = UserId::new();
    assert!(matches!(
        service
            .link_external_identity(LinkExternalIdentity {
                user_id: attacker,
                connector_id: connector,
                claims: claims("immutable-subject", "owner@example.test"),
                recently_reauthenticated: true
            })
            .await,
        Err(ExternalIdentityLinkingError::IdentityAlreadyLinked)
    ));
    let identity = repository.identity_for(owner)?.ok_or("identity missing")?;
    assert!(matches!(
        service
            .unlink_external_identity(UnlinkExternalIdentity {
                user_id: owner,
                identity_id: identity.id,
                recently_reauthenticated: true
            })
            .await,
        Err(ExternalIdentityLinkingError::FinalLoginMethod)
    ));
    assert!(matches!(
        service
            .link_external_identity(LinkExternalIdentity {
                user_id: owner,
                connector_id: ConnectorId::new(),
                claims: claims("second-subject", "owner@example.test"),
                recently_reauthenticated: false
            })
            .await,
        Err(ExternalIdentityLinkingError::RecentReauthenticationRequired)
    ));
    Ok(())
}

fn claims(subject: &str, email: &str) -> IdentityClaims {
    IdentityClaims {
        subject: subject.to_owned(),
        display_name: Some("Solar owner".to_owned()),
        email: Some(email.to_owned()),
        email_verified: Some(true),
        avatar_url: None,
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(
            time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(1_780_000_000_000),
        )
    }
}

#[derive(Default)]
struct FakeRepository {
    state: Mutex<FakeState>,
}
#[derive(Default)]
struct FakeState {
    identities: HashMap<ExternalIdentityId, LinkedIdentityRecord>,
    users: u32,
}
impl FakeRepository {
    fn created_user_count(&self) -> Result<u32, Box<dyn Error>> {
        Ok(self.state.lock().map_err(|_| "poisoned")?.users)
    }
    fn identity_for(
        &self,
        user_id: UserId,
    ) -> Result<Option<LinkedIdentityRecord>, Box<dyn Error>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "poisoned")?
            .identities
            .values()
            .find(|identity| identity.user_id == user_id)
            .cloned())
    }
}
#[async_trait]
impl ExternalIdentityLinkingRepository for FakeRepository {
    async fn list_for_user(&self, user_id: UserId) -> Result<Vec<LinkedIdentityRecord>, PortError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .identities
            .values()
            .filter(|identity| identity.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn find_by_connector_subject(
        &self,
        connector_id: ConnectorId,
        subject: &str,
    ) -> Result<Option<LinkedIdentityRecord>, PortError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .identities
            .values()
            .find(|identity| identity.connector_id == connector_id && identity.subject == subject)
            .cloned())
    }
    async fn create_user_from_external_claims(
        &self,
        _claims: &IdentityClaims,
        _now: i64,
    ) -> Result<UserId, PortError> {
        let mut state = self.state.lock().map_err(|_| PortError::Unavailable)?;
        state.users += 1;
        Ok(UserId::new())
    }
    async fn link(&self, identity: LinkedIdentityRecord) -> Result<(), PortError> {
        self.state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .identities
            .insert(identity.id, identity);
        Ok(())
    }
    async fn touch_login(
        &self,
        identity_id: ExternalIdentityId,
        now: i64,
    ) -> Result<(), PortError> {
        self.state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .identities
            .get_mut(&identity_id)
            .ok_or(PortError::NotFound)?
            .last_login_at_epoch_millis = Some(now);
        Ok(())
    }
    async fn find_for_user(
        &self,
        identity_id: ExternalIdentityId,
        user_id: UserId,
    ) -> Result<Option<LinkedIdentityRecord>, PortError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .identities
            .get(&identity_id)
            .filter(|identity| identity.user_id == user_id)
            .cloned())
    }
    async fn has_local_login(&self, _user_id: UserId) -> Result<bool, PortError> {
        Ok(false)
    }
    async fn external_identity_count(&self, user_id: UserId) -> Result<u32, PortError> {
        u32::try_from(
            self.state
                .lock()
                .map_err(|_| PortError::Unavailable)?
                .identities
                .values()
                .filter(|identity| identity.user_id == user_id)
                .count(),
        )
        .map_err(|_| PortError::Unavailable)
    }
    async fn unlink(&self, identity_id: ExternalIdentityId) -> Result<(), PortError> {
        self.state
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .identities
            .remove(&identity_id);
        Ok(())
    }
    async fn audit(
        &self,
        _user_id: UserId,
        _action: &'static str,
        _now: i64,
    ) -> Result<(), PortError> {
        Ok(())
    }
}
