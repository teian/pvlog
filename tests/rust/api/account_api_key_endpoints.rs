use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AccountApiKeyError, AccountApiKeyMetadata, AccountApiKeyScope, AccountApiKeyUseCases,
    AuthorizedRequest, IssuedAccountApiKey, ModernRequestAuthorizer, RequestAuthorizationError,
    RequestPrincipal, account_api_keys_router,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, Permission, PrincipalId, SystemId, UserId,
};
use secrecy::SecretString;
use tower::ServiceExt as _;

#[tokio::test]
async fn current_account_keys_are_one_time_scoped_and_session_only() -> Result<(), Box<dyn Error>> {
    let actor = UserId::new();
    let account = AccountId::from_uuid(actor.as_uuid())?;
    let service = Arc::new(Stub {
        id: ApiCredentialId::new(),
    });
    let router = account_api_keys_router(
        service.clone(),
        Arc::new(AllowAuthorizer { actor, account }),
    );
    let app = router
        .clone()
        .layer(Extension(RequestPrincipal::User(actor)));
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/account/api-keys")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"Uploader","scopes":["telemetry:write"]}"#,
                ))?,
        )
        .await?;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: serde_json::Value =
        serde_json::from_slice(&to_bytes(created.into_body(), 1024 * 1024).await?)?;
    assert_eq!(created["apiKey"], "pvlog_once");
    assert_eq!(created["credential"]["scopes"][0], "telemetry:write");

    let listed = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/account/api-keys")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: serde_json::Value =
        serde_json::from_slice(&to_bytes(listed.into_body(), 1024 * 1024).await?)?;
    assert!(listed[0].get("apiKey").is_none());
    assert!(listed[0].get("credentialDigest").is_none());

    let deleted = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/account/api-keys/{}", service.id))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let bearer = router.layer(Extension(RequestPrincipal::ApiCredential {
        id: ApiCredentialId::new(),
        owner_user_id: actor,
        account_id: account,
        system_id: None,
        scopes: BTreeSet::from([ApiScope::SystemsWrite]),
    }));
    let denied = bearer
        .oneshot(
            Request::builder()
                .uri("/api/v1/account/api-keys")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    Ok(())
}

struct Stub {
    id: ApiCredentialId,
}

#[async_trait]
impl AccountApiKeyUseCases for Stub {
    async fn issue(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        name: String,
        scopes: BTreeSet<ApiScope>,
        _expires_at: Option<i64>,
    ) -> Result<IssuedAccountApiKey, AccountApiKeyError> {
        Ok(IssuedAccountApiKey {
            api_key: SecretString::from("pvlog_once"),
            credential: metadata(self.id, name, scopes),
        })
    }

    async fn list(
        &self,
        _actor: UserId,
        _account_id: AccountId,
    ) -> Result<Vec<AccountApiKeyMetadata>, AccountApiKeyError> {
        Ok(vec![metadata(
            self.id,
            "Uploader".to_owned(),
            BTreeSet::from([ApiScope::TelemetryWrite]),
        )])
    }

    async fn revoke(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        id: ApiCredentialId,
    ) -> Result<(), AccountApiKeyError> {
        (id == self.id)
            .then_some(())
            .ok_or(AccountApiKeyError::NotFound)
    }
}

fn metadata(
    id: ApiCredentialId,
    name: String,
    scopes: BTreeSet<ApiScope>,
) -> AccountApiKeyMetadata {
    AccountApiKeyMetadata {
        id,
        name,
        scopes: scopes
            .into_iter()
            .map(AccountApiKeyScope::try_from)
            .collect::<Result<_, _>>()
            .expect("public scopes"),
        created_at_epoch_millis: 1_780_000_000_000,
        expires_at_epoch_millis: None,
        revoked_at_epoch_millis: None,
    }
}

struct AllowAuthorizer {
    actor: UserId,
    account: AccountId,
}

#[async_trait]
impl ModernRequestAuthorizer for AllowAuthorizer {
    async fn authorize_instance(
        &self,
        _principal: PrincipalId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        Ok(self.actor)
    }

    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        account_id: AccountId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        assert_eq!(account_id, self.account);
        assert_eq!(permission, Permission::CredentialManage);
        Ok(AuthorizedRequest {
            actor_user_id: self.actor,
            account_id,
        })
    }

    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
}
