//! Runtime adapter that verifies HTTP credentials against management persistence.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{RequestAuthenticationError, RequestAuthenticator, RequestPrincipal};
use pvlog_application::{
    AssignRole, AuthorizationBoundary, AuthorizationBoundaryError, AuthorizationBoundaryPorts,
    AuthorizedAccountRoute, Clock, CreateCustomRole, ExternalIdentityLinkingUseCases,
    ProtectedAccountRequest, ProtectedSystemRequest, RbacManagementError, RbacRepository,
    RoleManagementService, UpdateCustomRole,
};
use pvlog_domain::{
    ApiScope, AuditEventId, Permission, PrincipalId, RequestId, RoleKind, SystemId, UserId,
};
use pvlog_storage::{
    AuditRecord, DatabaseTarget, ManagementRepository, RoutingRecord, probe_database,
};
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

/// Read-only management adapter for account audit HTTP resources.
pub struct ManagementAuditApi {
    repository: Arc<dyn ManagementRepository>,
}

/// Management-backed role catalog for the RBAC HTTP adapter.
pub struct ManagementRbacApi {
    repository: Arc<dyn RbacRepository>,
    service: RoleManagementService,
}

/// Browser-session view over provider-neutral external identity links.
pub struct ManagementIdentityApi {
    service: Arc<dyn ExternalIdentityLinkingUseCases>,
}

/// Runtime-configured connector catalog that deliberately omits client credentials and secret references.
pub struct ManagementConnectorApi {
    connectors: Vec<pvlog_api::ConnectorAdminResponse>,
}

/// Readiness adapter that verifies the configured management/database topology.
pub struct ManagementReadiness {
    target: DatabaseTarget,
}

impl ManagementReadiness {
    #[must_use]
    pub const fn new(target: DatabaseTarget) -> Self {
        Self { target }
    }
}

#[async_trait]
impl pvlog_api::ReadinessUseCases for ManagementReadiness {
    async fn ready(&self) -> Result<(), pvlog_api::ReadinessError> {
        probe_database(&self.target)
            .await
            .map_err(|_| pvlog_api::ReadinessError::Unavailable)
    }
}

impl ManagementConnectorApi {
    #[must_use]
    pub fn new(connectors: &[crate::config::AuthConnectorConfig]) -> Self {
        Self {
            connectors: connectors
                .iter()
                .map(|connector| pvlog_api::ConnectorAdminResponse {
                    id: connector.id.clone(),
                    display_name: connector.display_name.clone(),
                    protocol: match connector.protocol {
                        crate::config::AuthProtocol::Oidc => "oidc".to_owned(),
                        crate::config::AuthProtocol::Oauth2 => "oauth2".to_owned(),
                    },
                    enabled: connector.enabled,
                    authorization_endpoint: connector
                        .authorization_endpoint
                        .as_ref()
                        .map(url::Url::to_string),
                    scopes: connector.scopes.clone(),
                })
                .collect(),
        }
    }
}

#[async_trait]
impl pvlog_api::ConnectorAdminUseCases for ManagementConnectorApi {
    async fn connectors(
        &self,
    ) -> Result<Vec<pvlog_api::ConnectorAdminResponse>, pvlog_api::ConnectorAdminError> {
        Ok(self.connectors.clone())
    }
}

impl ManagementIdentityApi {
    #[must_use]
    pub fn new(service: Arc<dyn ExternalIdentityLinkingUseCases>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl pvlog_api::IdentityApiUseCases for ManagementIdentityApi {
    async fn list_identities(
        &self,
        user_id: UserId,
    ) -> Result<Vec<pvlog_api::LinkedIdentityResponse>, pvlog_api::IdentityApiError> {
        self.service
            .list_external_identities(user_id)
            .await
            .map(|identities| {
                identities
                    .into_iter()
                    .map(|identity| pvlog_api::LinkedIdentityResponse {
                        id: identity.id,
                        connector_id: identity.connector_id,
                        subject: identity.subject,
                        linked_at_epoch_millis: identity.linked_at_epoch_millis,
                        last_login_at_epoch_millis: identity.last_login_at_epoch_millis,
                    })
                    .collect()
            })
            .map_err(|_| pvlog_api::IdentityApiError::Unavailable)
    }
}

impl ManagementRbacApi {
    #[must_use]
    pub fn new(repository: Arc<dyn RbacRepository>, clock: Arc<dyn Clock>) -> Self {
        Self {
            service: RoleManagementService::new(repository.clone(), clock),
            repository,
        }
    }
}

#[async_trait]
impl pvlog_api::RbacApiUseCases for ManagementRbacApi {
    async fn roles(
        &self,
        account_id: pvlog_domain::AccountId,
    ) -> Result<Vec<pvlog_api::RoleResponse>, pvlog_api::RbacApiError> {
        self.repository
            .roles(Some(account_id))
            .await
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| pvlog_api::RoleResponse {
                        id: record.role.id,
                        name: record.role.name,
                        kind: match record.role.kind {
                            RoleKind::BuiltIn(kind) => format!("built_in:{kind:?}"),
                            RoleKind::Custom => "custom".to_owned(),
                        },
                        permissions: record.role.permissions,
                        parent_role_ids: record.role.parent_role_ids,
                        version: record.version,
                        created_at: record.created_at,
                        updated_at: record.updated_at,
                    })
                    .collect()
            })
            .map_err(|_| pvlog_api::RbacApiError::Unavailable)
    }

    async fn create_role(
        &self,
        actor: UserId,
        account_id: pvlog_domain::AccountId,
        input: pvlog_api::RoleInput,
    ) -> Result<pvlog_api::RoleResponse, pvlog_api::RbacApiError> {
        let record = self
            .service
            .create_custom_role(
                actor,
                CreateCustomRole {
                    account_id,
                    name: input.name,
                    permissions: input.permissions,
                    parent_role_ids: input.parent_role_ids,
                },
            )
            .await
            .map_err(|error| map_rbac_error(&error))?;
        Ok(role_response(record))
    }

    async fn update_role(
        &self,
        actor: UserId,
        account_id: pvlog_domain::AccountId,
        role_id: pvlog_domain::RoleId,
        input: pvlog_api::RoleInput,
    ) -> Result<pvlog_api::RoleResponse, pvlog_api::RbacApiError> {
        let existing = self
            .repository
            .role(role_id)
            .await
            .map_err(|_| pvlog_api::RbacApiError::Unavailable)?
            .ok_or(pvlog_api::RbacApiError::NotFound)?;
        if existing.role.account_id != Some(account_id) {
            return Err(pvlog_api::RbacApiError::NotFound);
        }
        let record = self
            .service
            .update_custom_role(
                actor,
                UpdateCustomRole {
                    id: role_id,
                    name: input.name,
                    permissions: input.permissions,
                    parent_role_ids: input.parent_role_ids,
                },
            )
            .await
            .map_err(|error| map_rbac_error(&error))?;
        Ok(role_response(record))
    }

    async fn delete_role(
        &self,
        actor: UserId,
        account_id: pvlog_domain::AccountId,
        role_id: pvlog_domain::RoleId,
    ) -> Result<(), pvlog_api::RbacApiError> {
        let existing = self
            .repository
            .role(role_id)
            .await
            .map_err(|_| pvlog_api::RbacApiError::Unavailable)?
            .ok_or(pvlog_api::RbacApiError::NotFound)?;
        if existing.role.account_id != Some(account_id) {
            return Err(pvlog_api::RbacApiError::NotFound);
        }
        self.service
            .delete_custom_role(actor, role_id)
            .await
            .map_err(|error| map_rbac_error(&error))
    }

    async fn assign_role(
        &self,
        actor: UserId,
        account_id: pvlog_domain::AccountId,
        input: pvlog_api::RoleAssignmentInput,
    ) -> Result<pvlog_api::RoleAssignmentResponse, pvlog_api::RbacApiError> {
        let assignment = self
            .service
            .assign_role(
                actor,
                AssignRole {
                    principal: input.principal()?,
                    role_id: input.role_id,
                    scope: input.scope(account_id),
                    expires_at: input.expires_at,
                },
            )
            .await
            .map_err(|error| map_rbac_error(&error))?;
        assignment_response(&assignment)
    }

    async fn revoke_assignment(
        &self,
        actor: UserId,
        account_id: pvlog_domain::AccountId,
        assignment_id: pvlog_domain::RoleAssignmentId,
        scope: pvlog_domain::RoleScope,
    ) -> Result<(), pvlog_api::RbacApiError> {
        let scope_account = match scope {
            pvlog_domain::RoleScope::Account(scope_account)
            | pvlog_domain::RoleScope::System {
                account_id: scope_account,
                ..
            } => scope_account,
            pvlog_domain::RoleScope::Instance => return Err(pvlog_api::RbacApiError::Invalid),
        };
        if scope_account != account_id {
            return Err(pvlog_api::RbacApiError::NotFound);
        }
        self.service
            .revoke_assignment(actor, assignment_id, scope)
            .await
            .map_err(|error| map_rbac_error(&error))
    }
}

fn role_response(record: pvlog_application::RbacRoleRecord) -> pvlog_api::RoleResponse {
    pvlog_api::RoleResponse {
        id: record.role.id,
        name: record.role.name,
        kind: match record.role.kind {
            RoleKind::BuiltIn(kind) => format!("built_in:{kind:?}"),
            RoleKind::Custom => "custom".to_owned(),
        },
        permissions: record.role.permissions,
        parent_role_ids: record.role.parent_role_ids,
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn map_rbac_error(error: &RbacManagementError) -> pvlog_api::RbacApiError {
    match error {
        RbacManagementError::Forbidden | RbacManagementError::PrivilegeEscalation => {
            pvlog_api::RbacApiError::Forbidden
        }
        RbacManagementError::NotFound | RbacManagementError::BuiltInImmutable => {
            pvlog_api::RbacApiError::NotFound
        }
        RbacManagementError::InvalidName
        | RbacManagementError::InvalidParent
        | RbacManagementError::InvalidScope
        | RbacManagementError::InvalidExpiry => pvlog_api::RbacApiError::Invalid,
        RbacManagementError::Persistence | RbacManagementError::Internal(_) => {
            pvlog_api::RbacApiError::Unavailable
        }
    }
}

fn assignment_response(
    assignment: &pvlog_domain::RoleAssignment,
) -> Result<pvlog_api::RoleAssignmentResponse, pvlog_api::RbacApiError> {
    let (account_id, system_id) = match assignment.scope {
        pvlog_domain::RoleScope::Account(account_id) => (account_id, None),
        pvlog_domain::RoleScope::System {
            account_id,
            system_id,
        } => (account_id, Some(system_id)),
        pvlog_domain::RoleScope::Instance => return Err(pvlog_api::RbacApiError::Invalid),
    };
    let (principal_type, principal_id) = match assignment.principal {
        PrincipalId::User(id) => ("user".to_owned(), id.as_uuid()),
        PrincipalId::ApiCredential(id) => ("api_credential".to_owned(), id.as_uuid()),
    };
    Ok(pvlog_api::RoleAssignmentResponse {
        id: assignment.id,
        role_id: assignment.role_id,
        principal_type,
        principal_id,
        account_id,
        system_id,
        expires_at: assignment
            .expires_at
            .map(|timestamp| i64::try_from(timestamp.epoch_millis()))
            .transpose()
            .map_err(|_| pvlog_api::RbacApiError::Invalid)?,
    })
}

impl ManagementAuditApi {
    #[must_use]
    pub fn new(repository: Arc<dyn ManagementRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl pvlog_api::AuditApiUseCases for ManagementAuditApi {
    async fn account_audit(
        &self,
        account_id: pvlog_domain::AccountId,
        limit: u32,
    ) -> Result<Vec<pvlog_api::AuditEventResponse>, pvlog_api::AuditApiError> {
        self.repository
            .account_audit(account_id, limit)
            .await
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| pvlog_api::AuditEventResponse {
                        id: record.id,
                        occurred_at: record.occurred_at,
                        actor_type: record.actor_type,
                        actor_id: record.actor_id,
                        action: record.action,
                        target_type: record.target_type,
                        target_id: record.target_id,
                        outcome: record.outcome,
                        safe_metadata: record.safe_metadata,
                    })
                    .collect()
            })
            .map_err(|_| pvlog_api::AuditApiError::Unavailable)
    }
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

#[cfg(test)]
mod tests {
    use super::ManagementConnectorApi;
    use crate::config::{AuthConnectorConfig, AuthProtocol, ClaimMappings};
    use pvlog_api::ConnectorAdminUseCases as _;
    use url::Url;

    #[tokio::test]
    async fn connector_catalog_never_serializes_client_credentials_or_secret_references()
    -> Result<(), Box<dyn std::error::Error>> {
        let connector = AuthConnectorConfig {
            id: "company-sso".to_owned(),
            display_name: "Company SSO".to_owned(),
            protocol: AuthProtocol::Oidc,
            enabled: true,
            client_id: "client-id-must-not-leak".to_owned(),
            client_secret_ref: "secret-ref-must-not-leak".to_owned(),
            discovery_url: Some(Url::parse(
                "https://identity.example/.well-known/openid-configuration",
            )?),
            issuer: Some(Url::parse("https://identity.example")?),
            authorization_endpoint: Some(Url::parse("https://identity.example/authorize")?),
            token_endpoint: Some(Url::parse("https://identity.example/token")?),
            userinfo_endpoint: Some(Url::parse("https://identity.example/userinfo")?),
            scopes: vec!["openid".to_owned()],
            claims: ClaimMappings {
                subject: "sub".to_owned(),
                name: None,
                email: None,
                email_verified: None,
                avatar: None,
            },
        };
        let response = ManagementConnectorApi::new(&[connector]).connectors().await;
        assert!(response.is_ok());
        let response = response.unwrap_or_default();
        let serialized = serde_json::to_string(&response)?;
        assert!(!serialized.contains("client-id-must-not-leak"));
        assert!(!serialized.contains("secret-ref-must-not-leak"));
        assert!(!serialized.contains("https://identity.example/token"));
        assert!(!serialized.contains("https://identity.example/userinfo"));
        assert!(serialized.contains("https://identity.example/authorize"));
        Ok(())
    }
}
