use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AdministrationApiError, AdministrationApiUseCases, BackupResult, ConnectionTestResult,
    EmailEncryption, EmailNotificationSettings, ModernRequestAuthorizer, RequestAuthorizationError,
    RequestPrincipal, RetentionBackupSettings, WeatherFeedSettings, administration_router,
};
use pvlog_domain::{AccountId, Permission, PrincipalId, SystemId, UserId};
use tower::ServiceExt as _;

#[tokio::test]
async fn administration_settings_require_instance_management() -> Result<(), Box<dyn Error>> {
    let app = administration_router(Arc::new(Settings), Arc::new(Authorizer));
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/weather-feed")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let app = app.layer(Extension(RequestPrincipal::User(UserId::new())));
    for (method, path, expected) in [
        (Method::GET, "/api/v1/admin/weather-feed", StatusCode::OK),
        (
            Method::GET,
            "/api/v1/admin/email-notifications",
            StatusCode::OK,
        ),
        (
            Method::GET,
            "/api/v1/admin/retention-backup",
            StatusCode::OK,
        ),
        (Method::POST, "/api/v1/admin/backups", StatusCode::CREATED),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), expected);
    }
    Ok(())
}

struct Settings;

#[async_trait]
impl AdministrationApiUseCases for Settings {
    async fn weather_feed(&self) -> Result<WeatherFeedSettings, AdministrationApiError> {
        Ok(weather())
    }
    async fn save_weather_feed(
        &self,
        _actor: UserId,
        settings: WeatherFeedSettings,
    ) -> Result<WeatherFeedSettings, AdministrationApiError> {
        Ok(settings)
    }
    async fn test_weather_feed(&self) -> Result<ConnectionTestResult, AdministrationApiError> {
        Ok(ConnectionTestResult { reachable: true })
    }
    async fn email_notifications(
        &self,
    ) -> Result<EmailNotificationSettings, AdministrationApiError> {
        Ok(email())
    }
    async fn save_email_notifications(
        &self,
        _actor: UserId,
        settings: EmailNotificationSettings,
    ) -> Result<EmailNotificationSettings, AdministrationApiError> {
        Ok(settings)
    }
    async fn test_email_notifications(
        &self,
    ) -> Result<ConnectionTestResult, AdministrationApiError> {
        Ok(ConnectionTestResult { reachable: true })
    }
    async fn retention_backup(&self) -> Result<RetentionBackupSettings, AdministrationApiError> {
        Ok(retention())
    }
    async fn save_retention_backup(
        &self,
        _actor: UserId,
        settings: RetentionBackupSettings,
    ) -> Result<RetentionBackupSettings, AdministrationApiError> {
        Ok(settings)
    }
    async fn run_backup(&self, _actor: UserId) -> Result<BackupResult, AdministrationApiError> {
        Ok(BackupResult {
            completed_at_epoch_millis: 1,
            byte_length: 2,
            artifact_name: "backup".to_owned(),
        })
    }
}

fn weather() -> WeatherFeedSettings {
    WeatherFeedSettings {
        enabled: false,
        endpoint: String::new(),
        credential_secret_ref: None,
        updated_at_epoch_millis: None,
    }
}
fn email() -> EmailNotificationSettings {
    EmailNotificationSettings {
        enabled: false,
        recipient: String::new(),
        host: String::new(),
        port: 587,
        username: String::new(),
        credential_secret_ref: None,
        encryption: EmailEncryption::Starttls,
        updated_at_epoch_millis: None,
    }
}
fn retention() -> RetentionBackupSettings {
    RetentionBackupSettings {
        reading_retention_days: 365,
        automatic_backups_enabled: false,
        backup_schedule: "0 2 * * *".to_owned(),
        last_backup_at_epoch_millis: None,
        last_backup_bytes: None,
        updated_at_epoch_millis: None,
    }
}

struct Authorizer;

#[async_trait]
impl ModernRequestAuthorizer for Authorizer {
    async fn authorize_instance(
        &self,
        principal: PrincipalId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        let PrincipalId::User(user) = principal else {
            return Err(RequestAuthorizationError::Forbidden);
        };
        (permission == Permission::InstanceManage)
            .then_some(user)
            .ok_or(RequestAuthorizationError::Forbidden)
    }
    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        _account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<pvlog_api::AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<pvlog_api::AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }
}
