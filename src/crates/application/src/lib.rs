//! `PVLog` application use cases and ports.

#![forbid(unsafe_code)]

mod local_password;
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
pub use ports::{
    AuthorizationRequest, Clock, CredentialService, EntityRepository, IdentityClaims,
    IdentityService, InsolationPoint, InsolationProvider, JobQueue, PortError, SupplyPoint,
    SupplyProvider, Transaction, UnitOfWork, WebhookRequest, WebhookResponse, WebhookSender,
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
