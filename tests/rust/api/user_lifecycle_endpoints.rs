//! HTTP contracts for local-user lifecycle and enumeration-resistant public responses.

use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{Extension, body::Body, http::Request};
use pvlog_api::user_lifecycle_router;
use pvlog_application::{
    AdminUserActor, CreateLocalUser, InvitationResult, InviteLocalUser, LifecycleUserRecord,
    PublicLifecycleOutcome, RegisterLocalUser, UserLifecycleError, UserLifecycleUseCases,
};
use pvlog_domain::{UserId, UserInvitationId, UserStatus};
use secrecy::SecretString;
use tower::ServiceExt as _;

#[tokio::test]
async fn public_lifecycle_responses_do_not_disclose_account_existence() -> Result<(), Box<dyn Error>>
{
    let app = user_lifecycle_router(Arc::new(StubLifecycle));
    let first = request(
        &app,
        "/api/v1/auth/register",
        r#"{"email":"known@example.test","displayName":"Known"}"#,
    )
    .await?;
    let second = request(
        &app,
        "/api/v1/auth/register",
        r#"{"email":"unknown@example.test","displayName":"Unknown"}"#,
    )
    .await?;
    assert_eq!(first.0, 202);
    assert_eq!(first, second);

    let invalid_invitation = request(
        &app,
        "/api/v1/auth/invitations/accept",
        r#"{"token":"not-a-real-token","displayName":"Anybody"}"#,
    )
    .await?;
    assert_eq!(invalid_invitation, first);
    Ok(())
}

#[tokio::test]
async fn administration_requires_an_authorized_actor() -> Result<(), Box<dyn Error>> {
    let service = Arc::new(StubLifecycle);
    let without_actor = user_lifecycle_router(service.clone());
    let forbidden = request(
        &without_actor,
        "/api/v1/admin/users",
        r#"{"email":"new@example.test","displayName":"New","emailVerified":true}"#,
    )
    .await?;
    assert_eq!(forbidden.0, 403);

    let with_actor = user_lifecycle_router(service).layer(Extension(AdminUserActor {
        user_id: UserId::new(),
        can_manage_users: true,
    }));
    let created = request(
        &with_actor,
        "/api/v1/admin/users",
        r#"{"email":"new@example.test","displayName":"New","emailVerified":true}"#,
    )
    .await?;
    assert_eq!(created.0, 201);
    Ok(())
}

async fn request(
    app: &axum::Router,
    uri: &str,
    body: &'static str,
) -> Result<(u16, Vec<u8>), Box<dyn Error>> {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header("content-type", "application/json")
                .body(Body::from(body))?,
        )
        .await?;
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), 16_384)
        .await?
        .to_vec();
    Ok((status, body))
}

struct StubLifecycle;

#[async_trait]
impl UserLifecycleUseCases for StubLifecycle {
    async fn create_user(
        &self,
        _actor: AdminUserActor,
        command: CreateLocalUser,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        Ok(user(command.email, command.display_name))
    }

    async fn invite_user(
        &self,
        _actor: AdminUserActor,
        _command: InviteLocalUser,
    ) -> Result<InvitationResult, UserLifecycleError> {
        Ok(InvitationResult {
            invitation_id: UserInvitationId::new(),
            token: SecretString::from("one-time-token".to_owned()),
            expires_at: 2,
        })
    }

    async fn register(
        &self,
        _command: RegisterLocalUser,
    ) -> Result<PublicLifecycleOutcome, UserLifecycleError> {
        Ok(PublicLifecycleOutcome::Accepted)
    }

    async fn accept_invitation(
        &self,
        _token: SecretString,
        _display_name: String,
    ) -> Result<PublicLifecycleOutcome, UserLifecycleError> {
        Ok(PublicLifecycleOutcome::Accepted)
    }

    async fn activate(
        &self,
        _actor: AdminUserActor,
        id: UserId,
        _email_verified: bool,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        Ok(user_with_id(id))
    }

    async fn disable(
        &self,
        _actor: AdminUserActor,
        id: UserId,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        Ok(user_with_id(id))
    }

    async fn unlock(
        &self,
        _actor: AdminUserActor,
        id: UserId,
    ) -> Result<LifecycleUserRecord, UserLifecycleError> {
        Ok(user_with_id(id))
    }

    async fn delete(&self, _actor: AdminUserActor, _id: UserId) -> Result<(), UserLifecycleError> {
        Ok(())
    }
}

fn user(email: String, display_name: String) -> LifecycleUserRecord {
    LifecycleUserRecord {
        id: UserId::new(),
        email,
        display_name,
        status: UserStatus::Active,
        email_verified_at: Some(1),
        disabled_at: None,
        locked_until: None,
        created_at: 1,
        updated_at: 1,
    }
}

fn user_with_id(id: UserId) -> LifecycleUserRecord {
    let mut user = user("user@example.test".to_owned(), "User".to_owned());
    user.id = id;
    user
}
