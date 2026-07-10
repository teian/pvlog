use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, ApiCredentialId, AuditEventId, ConnectorId, ExternalIdentityId, MembershipId,
    RequestId, RoleAssignmentId, RoleId, SessionId, SystemId, UserId, UtcTimestamp,
    ValidationError,
};

/// Local user account independent of any login mechanism.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalUser {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub status: UserStatus,
    pub password: PasswordState,
    pub recovery: RecoveryState,
    pub created_at: UtcTimestamp,
    pub updated_at: UtcTimestamp,
}

/// Administrative lifecycle of a local user.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Invited,
    Active,
    Disabled,
    Deleted,
}

/// Encoded password verifier with redacted diagnostics and no serialization support.
#[derive(Clone, Eq, PartialEq)]
pub struct PasswordHash(String);

impl PasswordHash {
    /// Wraps a non-empty encoded verifier produced by the password service.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty verifier.
    pub fn new(encoded: impl Into<String>) -> Result<Self, ValidationError> {
        let encoded = encoded.into();
        if encoded.is_empty() {
            Err(ValidationError::new(
                "empty_password_hash",
                "password_hash",
                "encoded password verifier must not be empty",
            ))
        } else {
            Ok(Self(encoded))
        }
    }

    /// Exposes the encoded verifier only to credential persistence and verification boundaries.
    #[must_use]
    pub fn expose_encoded(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PasswordHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordHash([REDACTED])")
    }
}

/// Password lifecycle state for local authentication.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PasswordState {
    NotConfigured,
    Active {
        #[serde(skip)]
        hash: PasswordHash,
        changed_at: UtcTimestamp,
        must_change: bool,
    },
    Locked {
        #[serde(skip)]
        hash: PasswordHash,
        locked_at: UtcTimestamp,
    },
}

/// Password recovery request state; only keyed digests are retained.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RecoveryState {
    #[serde(skip)]
    pub token_digest: Option<CredentialDigest>,
    pub requested_at: Option<UtcTimestamp>,
    pub expires_at: Option<UtcTimestamp>,
    pub consumed_at: Option<UtcTimestamp>,
}

/// Fixed-length keyed digest used instead of retaining bearer credentials.
#[derive(Clone, Eq, PartialEq)]
pub struct CredentialDigest([u8; 32]);

impl CredentialDigest {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for CredentialDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialDigest([REDACTED])")
    }
}

/// Provider-neutral external identity linked to a local user.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExternalIdentity {
    pub id: ExternalIdentityId,
    pub connector_id: ConnectorId,
    pub subject: String,
    pub user_id: UserId,
    pub profile: ExternalProfile,
    pub linked_at: UtcTimestamp,
    pub last_login_at: Option<UtcTimestamp>,
}

/// Normalized non-authoritative identity profile claims.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ExternalProfile {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub avatar_url: Option<String>,
}

/// Interactive browser session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Session {
    pub id: SessionId,
    pub user_id: UserId,
    #[serde(skip)]
    pub credential_digest: CredentialDigest,
    #[serde(skip)]
    pub csrf_digest: CredentialDigest,
    pub state: SessionState,
    pub created_at: UtcTimestamp,
    pub last_seen_at: UtcTimestamp,
    pub idle_expires_at: UtcTimestamp,
    pub absolute_expires_at: UtcTimestamp,
}

/// Session lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Active,
    Revoked,
    Expired,
}

/// One tenant account owning systems and account-local data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub status: AccountStatus,
    pub quota: QuotaPolicy,
    pub storage: StorageRoutingState,
    pub created_at: UtcTimestamp,
}

/// Account lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Provisioning,
    Active,
    Suspended,
    Deleting,
    Deleted,
}

/// Opaque routing state known to the management catalog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageRoutingState {
    Pending,
    Active { opaque_locator: String, schema: u32 },
    Quarantined { reason_code: String },
    Deprovisioned,
}

/// Account membership independent from assigned authorization roles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Membership {
    pub id: MembershipId,
    pub account_id: AccountId,
    pub user_id: UserId,
    pub status: MembershipStatus,
    pub joined_at: Option<UtcTimestamp>,
}

/// Invitation and membership lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    Invited,
    Active,
    Suspended,
    Removed,
}

/// Authorization action evaluated by deny-by-default policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    InstanceRead,
    InstanceManage,
    AccountRead,
    AccountManage,
    MembershipManage,
    RoleManage,
    SystemRead,
    SystemManage,
    TelemetryRead,
    TelemetryWrite,
    CredentialManage,
    IntegrationManage,
    AuditRead,
}

/// Built-in least-privilege role templates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInRole {
    InstanceAdministrator,
    AccountOwner,
    AccountAdministrator,
    Manager,
    Contributor,
    Operator,
    Analyst,
    Viewer,
    Auditor,
    Uploader,
}

/// Origin and mutability of an authorization role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    BuiltIn(BuiltInRole),
    Custom,
}

/// Hierarchical role definition. Parent roles contribute permissions transitively.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Role {
    pub id: RoleId,
    pub account_id: Option<AccountId>,
    pub name: String,
    pub kind: RoleKind,
    pub parent_role_ids: BTreeSet<RoleId>,
    pub permissions: BTreeSet<Permission>,
}

/// Principal receiving a role assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalId {
    User(UserId),
    ApiCredential(ApiCredentialId),
}

/// Scope constraining an authorization assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleScope {
    Instance,
    Account(AccountId),
    System {
        account_id: AccountId,
        system_id: SystemId,
    },
}

/// Assignment of one role to one principal at exactly one scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoleAssignment {
    pub id: RoleAssignmentId,
    pub principal: PrincipalId,
    pub role_id: RoleId,
    pub scope: RoleScope,
    pub granted_by: UserId,
    pub granted_at: UtcTimestamp,
    pub expires_at: Option<UtcTimestamp>,
}

/// Coarse action scopes embedded in modern API credentials.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiScope {
    SystemsRead,
    SystemsWrite,
    TelemetryRead,
    TelemetryWrite,
    IntegrationsManage,
}

/// Modern bearer credential metadata and keyed digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApiCredential {
    pub id: ApiCredentialId,
    pub owner: UserId,
    pub account_id: AccountId,
    pub system_id: Option<SystemId>,
    pub name: String,
    #[serde(skip)]
    pub digest: CredentialDigest,
    pub scopes: BTreeSet<ApiScope>,
    pub created_at: UtcTimestamp,
    pub expires_at: Option<UtcTimestamp>,
    pub revoked_at: Option<UtcTimestamp>,
}

/// Resource ceilings applied before work is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct QuotaPolicy {
    pub systems: u32,
    pub members: u32,
    pub api_credentials: u32,
    pub ingestion_requests_per_minute: u32,
    pub ingestion_items_per_request: u32,
    pub retained_hot_days: u32,
}

impl Default for QuotaPolicy {
    fn default() -> Self {
        Self {
            systems: 100,
            members: 25,
            api_credentials: 25,
            ingestion_requests_per_minute: 600,
            ingestion_items_per_request: 1_000,
            retained_hot_days: 90,
        }
    }
}

/// Append-only security and administrative audit event.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub occurred_at: UtcTimestamp,
    pub actor: Option<PrincipalId>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub account_id: Option<AccountId>,
    pub request_id: RequestId,
    pub outcome: AuditOutcome,
    pub safe_metadata: serde_json::Value,
}

/// Result recorded for an attempted audited operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Succeeded,
    Denied,
    Failed,
}
