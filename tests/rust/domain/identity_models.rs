use std::{collections::BTreeSet, error::Error};

use pvlog_domain::{
    AccountId, ApiCredential, ApiCredentialId, ApiScope, CredentialDigest, PasswordHash,
    Permission, PrincipalId, QuotaPolicy, Role, RoleId, RoleKind, RoleScope, SystemId, UserId,
};

#[test]
fn credential_material_is_redacted_and_skipped_from_serialized_models() -> Result<(), Box<dyn Error>>
{
    let password_hash = PasswordHash::new("encoded-verifier")?;
    let digest = CredentialDigest::new([42; 32]);

    assert!(!format!("{password_hash:?}").contains("encoded-verifier"));
    assert_eq!(format!("{digest:?}"), "CredentialDigest([REDACTED])");

    let credential = ApiCredential {
        id: ApiCredentialId::new(),
        owner: UserId::new(),
        account_id: AccountId::new(),
        system_id: None,
        name: "uploader".to_owned(),
        digest,
        scopes: BTreeSet::from([ApiScope::TelemetryWrite]),
        created_at: pvlog_domain::UtcTimestamp::from_epoch_millis(0)?,
        expires_at: None,
        revoked_at: None,
    };
    let serialized = serde_json::to_value(&credential)?;
    assert!(serialized.get("digest").is_none());
    Ok(())
}

#[test]
fn role_models_preserve_hierarchy_principal_and_scope_without_framework_types() {
    let account_id = AccountId::new();
    let system_id = SystemId::new();
    let parent_id = RoleId::new();
    let role = Role {
        id: RoleId::new(),
        account_id: Some(account_id),
        name: "system operator".to_owned(),
        kind: RoleKind::Custom,
        parent_role_ids: BTreeSet::from([parent_id]),
        permissions: BTreeSet::from([Permission::SystemRead, Permission::TelemetryWrite]),
    };

    assert!(role.parent_role_ids.contains(&parent_id));
    assert!(role.permissions.contains(&Permission::TelemetryWrite));
    assert!(matches!(
        RoleScope::System {
            account_id,
            system_id
        },
        RoleScope::System { .. }
    ));
    assert!(matches!(
        PrincipalId::User(UserId::new()),
        PrincipalId::User(_)
    ));
}

#[test]
fn quota_defaults_are_bounded_and_nonzero() {
    let quota = QuotaPolicy::default();

    assert!(quota.systems > 0);
    assert!(quota.ingestion_items_per_request > 0);
    assert!(quota.retained_hot_days > 0);
}
