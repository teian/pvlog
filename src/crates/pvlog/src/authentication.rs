//! Runtime adapter that verifies HTTP credentials against management persistence.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{RequestAuthenticationError, RequestAuthenticator, RequestPrincipal};
use pvlog_application::{
    AuthorizationBoundary, AuthorizationBoundaryError, AuthorizationBoundaryPorts,
    AuthorizedAccountRoute, Clock, ProtectedAccountRequest, ProtectedSystemRequest,
};
use pvlog_domain::{ApiScope, AuditEventId, Permission, PrincipalId, RequestId, SystemId, UserId};
use pvlog_storage::{AuditRecord, ManagementRepository, RoutingRecord};
use secrecy::{ExposeSecret as _, SecretString};

/// Verifies bearer credentials and browser sessions from the management plane.
pub struct ManagementRequestAuthenticator {
    repository: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
    digest_key: [u8; 32],
}

/// Derives the keyed digest material shared by browser-session issuance and verification.
#[must_use]
pub fn session_digest_key(session_secret: &SecretString) -> [u8; 32] {
    blake3::derive_key(
        "pvlog/http-credential-digest/v1",
        session_secret.expose_secret().as_bytes(),
    )
}

/// Production authorization bridge from HTTP principals to management-plane RBAC and routing.
pub struct ManagementRequestAuthorizer {
    repository: Arc<dyn ManagementRepository>,
    boundary: AuthorizationBoundary,
    clock: Arc<dyn Clock>,
}

impl ManagementRequestAuthorizer {
    #[must_use]
    pub fn new(repository: Arc<dyn ManagementRepository>, clock: Arc<dyn Clock>) -> Self {
        let ports = Arc::new(ManagementAuthorizationPorts {
            repository: repository.clone(),
            clock: clock.clone(),
        });
        Self {
            repository,
            boundary: AuthorizationBoundary::new(ports),
            clock,
        }
    }

    async fn actor(
        &self,
        principal: PrincipalId,
        account_id: pvlog_domain::AccountId,
    ) -> Result<UserId, pvlog_api::RequestAuthorizationError> {
        match principal {
            PrincipalId::User(user_id) => Ok(user_id),
            PrincipalId::ApiCredential(id) => self
                .repository
                .api_credential(account_id, id)
                .await
                .map_err(|_| pvlog_api::RequestAuthorizationError::Unavailable)?
                .map(|credential| credential.owner_user_id)
                .ok_or(pvlog_api::RequestAuthorizationError::Forbidden),
        }
    }
}

#[async_trait]
impl pvlog_api::ModernRequestAuthorizer for ManagementRequestAuthorizer {
    async fn authorize_instance(
        &self,
        principal: PrincipalId,
        permission: Permission,
        action: &'static str,
    ) -> Result<UserId, pvlog_api::RequestAuthorizationError> {
        let PrincipalId::User(user_id) = principal else {
            return Err(pvlog_api::RequestAuthorizationError::Forbidden);
        };
        let now = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| pvlog_api::RequestAuthorizationError::Unavailable)?;
        let authorized = self
            .repository
            .user_is_instance_authorized(user_id, permission, now)
            .await
            .map_err(|_| pvlog_api::RequestAuthorizationError::Unavailable)?;
        append_instance_audit(&*self.repository, user_id, action, authorized, now)
            .await
            .map_err(|_| pvlog_api::RequestAuthorizationError::Unavailable)?;
        authorized
            .then_some(user_id)
            .ok_or(pvlog_api::RequestAuthorizationError::Forbidden)
    }

    async fn authorize_account(
        &self,
        principal: PrincipalId,
        account_id: pvlog_domain::AccountId,
        permission: Permission,
        action: &'static str,
    ) -> Result<pvlog_api::AuthorizedRequest, pvlog_api::RequestAuthorizationError> {
        let route = self
            .boundary
            .authorize_and_route(&ProtectedAccountRequest {
                principal,
                account_id,
                system_id: None,
                permission,
                request_id: RequestId::new(),
                action,
            })
            .await
            .map_err(|error| map_authorization(&error))?;
        Ok(pvlog_api::AuthorizedRequest {
            actor_user_id: self.actor(principal, route.account_id).await?,
            account_id: route.account_id,
        })
    }

    async fn authorize_system(
        &self,
        principal: PrincipalId,
        system_id: SystemId,
        permission: Permission,
        action: &'static str,
    ) -> Result<pvlog_api::AuthorizedRequest, pvlog_api::RequestAuthorizationError> {
        let route = self
            .boundary
            .authorize_system_and_route(&ProtectedSystemRequest {
                principal,
                system_id,
                permission,
                request_id: RequestId::new(),
                action,
            })
            .await
            .map_err(|error| map_authorization(&error))?;
        Ok(pvlog_api::AuthorizedRequest {
            actor_user_id: self.actor(principal, route.account_id).await?,
            account_id: route.account_id,
        })
    }
}

async fn append_instance_audit(
    repository: &dyn ManagementRepository,
    actor: UserId,
    action: &'static str,
    authorized: bool,
    now: i64,
) -> Result<(), pvlog_storage::ManagementRepositoryError> {
    let id = AuditEventId::new();
    let mut event_hash = [0_u8; 32];
    event_hash[..16].copy_from_slice(id.as_uuid().as_bytes());
    event_hash[16..].copy_from_slice(id.as_uuid().as_bytes());
    repository
        .append_audit(&AuditRecord {
            id,
            occurred_at: now,
            request_id: None,
            actor_type: "user".to_owned(),
            actor_id: Some(actor.as_uuid()),
            account_id: None,
            action: action.to_owned(),
            target_type: "instance".to_owned(),
            target_id: None,
            outcome: if authorized { "succeeded" } else { "denied" }.to_owned(),
            previous_event_hash: None,
            event_hash,
            safe_metadata: serde_json::json!({}),
        })
        .await
}

struct ManagementAuthorizationPorts {
    repository: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

#[async_trait]
impl AuthorizationBoundaryPorts for ManagementAuthorizationPorts {
    async fn is_authorized(
        &self,
        request: &ProtectedAccountRequest,
    ) -> Result<bool, pvlog_application::PortError> {
        let now = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| pvlog_application::PortError::Unavailable)?;
        self.repository
            .principal_is_authorized(
                request.principal,
                request.account_id,
                request.system_id,
                request.permission,
                now,
            )
            .await
            .map_err(|_| pvlog_application::PortError::Unavailable)
    }

    async fn account_route(
        &self,
        account_id: pvlog_domain::AccountId,
    ) -> Result<Option<AuthorizedAccountRoute>, pvlog_application::PortError> {
        let route = self
            .repository
            .routing(account_id)
            .await
            .map_err(|_| pvlog_application::PortError::Unavailable)?;
        Ok(route.and_then(authorized_route))
    }

    async fn system_account(
        &self,
        system_id: SystemId,
    ) -> Result<Option<pvlog_domain::AccountId>, pvlog_application::PortError> {
        self.repository
            .system_registry(system_id)
            .await
            .map(|record| record.map(|record| record.account_id))
            .map_err(|_| pvlog_application::PortError::Unavailable)
    }

    async fn append_audit(
        &self,
        request: &ProtectedAccountRequest,
        outcome: &'static str,
    ) -> Result<(), pvlog_application::PortError> {
        let id = AuditEventId::new();
        let mut event_hash = [0_u8; 32];
        event_hash[..16].copy_from_slice(id.as_uuid().as_bytes());
        event_hash[16..].copy_from_slice(id.as_uuid().as_bytes());
        let (actor_type, actor_id) = match request.principal {
            PrincipalId::User(id) => ("user", Some(id.as_uuid())),
            PrincipalId::ApiCredential(id) => ("api_credential", Some(id.as_uuid())),
        };
        let occurred_at = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| pvlog_application::PortError::Unavailable)?;
        self.repository
            .append_audit(&AuditRecord {
                id,
                occurred_at,
                request_id: Some(request.request_id.as_uuid()),
                actor_type: actor_type.to_owned(),
                actor_id,
                account_id: Some(request.account_id),
                action: request.action.to_owned(),
                target_type: request.system_id.map_or("account", |_| "system").to_owned(),
                target_id: request
                    .system_id
                    .map_or(Some(request.account_id.as_uuid()), |id| Some(id.as_uuid())),
                outcome: outcome.to_owned(),
                previous_event_hash: None,
                event_hash,
                safe_metadata: serde_json::json!({}),
            })
            .await
            .map_err(|_| pvlog_application::PortError::Unavailable)
    }
}

fn authorized_route(record: RoutingRecord) -> Option<AuthorizedAccountRoute> {
    let ready = matches!(record.state.as_str(), "active" | "ready");
    ready.then(|| AuthorizedAccountRoute {
        account_id: record.account_id,
        opaque_route: record
            .opaque_locator
            .unwrap_or_else(|| format!("postgres:{}", record.account_id)),
    })
}

fn map_authorization(error: &AuthorizationBoundaryError) -> pvlog_api::RequestAuthorizationError {
    match error {
        AuthorizationBoundaryError::Forbidden => pvlog_api::RequestAuthorizationError::Forbidden,
        AuthorizationBoundaryError::SystemNotFound => {
            pvlog_api::RequestAuthorizationError::NotFound
        }
        AuthorizationBoundaryError::AccountUnavailable | AuthorizationBoundaryError::Port(_) => {
            pvlog_api::RequestAuthorizationError::Unavailable
        }
    }
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
            digest_key: session_digest_key(session_secret),
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

/// Management-backed session bootstrap used by the browser application shell.
pub struct ManagementSessionBootstrap {
    repository: Arc<dyn ManagementRepository>,
}

impl ManagementSessionBootstrap {
    #[must_use]
    pub fn new(repository: Arc<dyn ManagementRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl pvlog_api::SessionBootstrapUseCases for ManagementSessionBootstrap {
    async fn bootstrap(
        &self,
        user_id: UserId,
    ) -> Result<pvlog_api::SessionBootstrap, pvlog_api::SessionApiError> {
        let user = self
            .repository
            .user(user_id)
            .await
            .map_err(|_| pvlog_api::SessionApiError::Bootstrap)?
            .ok_or(pvlog_api::SessionApiError::Bootstrap)?;
        let account = self
            .repository
            .active_accounts_for_user(user_id)
            .await
            .map_err(|_| pvlog_api::SessionApiError::Bootstrap)?
            .into_iter()
            .next();
        let system_ids = match account.as_ref() {
            Some(account) => self
                .repository
                .systems_for_account(account.id)
                .await
                .map_err(|_| pvlog_api::SessionApiError::Bootstrap)?,
            None => Vec::new(),
        };
        Ok(pvlog_api::SessionBootstrap {
            authenticated: true,
            user: Some(pvlog_api::SessionUser {
                id: user.id,
                display_name: user.display_name,
            }),
            account_id: account.map(|account| account.id),
            system_ids,
            permissions: Vec::new(),
            connectors: Vec::new(),
        })
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
