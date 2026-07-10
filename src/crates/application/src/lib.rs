//! `PVLog` application use cases and ports.

#![forbid(unsafe_code)]

mod local_password;
mod oauth2_connector;
mod oidc;
mod ports;
mod rbac_management;
mod user_lifecycle;

pub use local_password::{
    Argon2CredentialConfig, Argon2CredentialService, AuthenticatePassword, AuthenticationOutcome,
    ChangePassword, CommonPasswordHook, DiscardingRecoveryNotifier, LocalCredentialRecord,
    LocalCredentialRepository, LocalPasswordPolicy, LocalPasswordService, LocalPasswordUseCases,
    PasswordPolicyError, PasswordPolicyHook, PasswordRecoveryNotifier, PasswordRecoveryRecord,
    PasswordServiceError, SetInitialPassword,
};
pub use oauth2_connector::{
    EncryptedProviderToken, OAuth2AuthorizationRequest, OAuth2ClaimMappings,
    OAuth2ClientAuthMethod, OAuth2ConnectorSettings, OAuth2ProtocolClient, OAuth2ProtocolError,
    OAuth2UserInfo, ProtectedOAuth2Tokens, ProviderTokenKind, TokenCipher, TokenCipherConfigError,
    XChaCha20Poly1305TokenCipher,
};
pub use oidc::{
    OidcAuthorizationRequest, OidcConnectorSettings, OidcProtocolClient, OidcProtocolError,
};
pub use ports::{
    AuthorizationRequest, Clock, CredentialService, EntityRepository, IdentityClaims,
    IdentityService, InsolationPoint, InsolationProvider, JobQueue, PortError, SecretResolver,
    SupplyPoint, SupplyProvider, Transaction, UnitOfWork, WebhookRequest, WebhookResponse,
    WebhookSender,
};
pub use rbac_management::{
    AssignRole, CreateCustomRole, RbacManagementError, RbacRepository, RbacRoleRecord,
    RoleManagementService, UpdateCustomRole, built_in_account_roles,
};
pub use user_lifecycle::{
    AdminUserActor, CreateLocalUser, InvitationRecord, InvitationResult, InviteLocalUser,
    LifecycleCreateOutcome, LifecycleUserRecord, LocalUserPolicy, PublicLifecycleOutcome,
    RegisterLocalUser, UserLifecycleError, UserLifecycleRepository, UserLifecycleService,
    UserLifecycleUseCases,
};
