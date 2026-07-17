//! Instance administration settings and operator actions.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_domain::{Permission, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, principal_identity,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherFeedSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub credential_secret_ref: Option<String>,
    pub updated_at_epoch_millis: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailNotificationSettings {
    pub enabled: bool,
    pub recipient: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credential_secret_ref: Option<String>,
    pub encryption: EmailEncryption,
    pub updated_at_epoch_millis: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailEncryption {
    None,
    Starttls,
    Tls,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionBackupSettings {
    pub reading_retention_days: u32,
    pub automatic_backups_enabled: bool,
    pub backup_schedule: String,
    pub last_backup_at_epoch_millis: Option<i64>,
    pub last_backup_bytes: Option<u64>,
    pub updated_at_epoch_millis: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub reachable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub completed_at_epoch_millis: i64,
    pub byte_length: u64,
    pub artifact_name: String,
}

#[async_trait]
pub trait AdministrationApiUseCases: Send + Sync {
    async fn weather_feed(&self) -> Result<WeatherFeedSettings, AdministrationApiError>;
    async fn save_weather_feed(
        &self,
        actor: UserId,
        settings: WeatherFeedSettings,
    ) -> Result<WeatherFeedSettings, AdministrationApiError>;
    async fn test_weather_feed(&self) -> Result<ConnectionTestResult, AdministrationApiError>;
    async fn email_notifications(
        &self,
    ) -> Result<EmailNotificationSettings, AdministrationApiError>;
    async fn save_email_notifications(
        &self,
        actor: UserId,
        settings: EmailNotificationSettings,
    ) -> Result<EmailNotificationSettings, AdministrationApiError>;
    async fn test_email_notifications(
        &self,
    ) -> Result<ConnectionTestResult, AdministrationApiError>;
    async fn retention_backup(&self) -> Result<RetentionBackupSettings, AdministrationApiError>;
    async fn save_retention_backup(
        &self,
        actor: UserId,
        settings: RetentionBackupSettings,
    ) -> Result<RetentionBackupSettings, AdministrationApiError>;
    async fn run_backup(&self, actor: UserId) -> Result<BackupResult, AdministrationApiError>;
}

#[derive(Clone)]
struct AdministrationState {
    service: Arc<dyn AdministrationApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

pub fn administration_router(
    service: Arc<dyn AdministrationApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/admin/weather-feed",
            get(weather_feed).put(save_weather_feed),
        )
        .route("/api/v1/admin/weather-feed/test", post(test_weather_feed))
        .route(
            "/api/v1/admin/email-notifications",
            get(email_notifications).put(save_email_notifications),
        )
        .route(
            "/api/v1/admin/email-notifications/test",
            post(test_email_notifications),
        )
        .route(
            "/api/v1/admin/retention-backup",
            get(retention_backup).put(save_retention_backup),
        )
        .route("/api/v1/admin/backups", post(run_backup))
        .with_state(AdministrationState {
            service,
            authorizer,
        })
}

async fn weather_feed(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<WeatherFeedSettings>, AdministrationApiError> {
    authorize(&state, principal, "administration.weather.read").await?;
    Ok(Json(state.service.weather_feed().await?))
}

async fn save_weather_feed(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(settings): Json<WeatherFeedSettings>,
) -> Result<Json<WeatherFeedSettings>, AdministrationApiError> {
    let actor = authorize(&state, principal, "administration.weather.update").await?;
    Ok(Json(
        state.service.save_weather_feed(actor, settings).await?,
    ))
}

async fn test_weather_feed(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<ConnectionTestResult>, AdministrationApiError> {
    authorize(&state, principal, "administration.weather.test").await?;
    Ok(Json(state.service.test_weather_feed().await?))
}

async fn email_notifications(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<EmailNotificationSettings>, AdministrationApiError> {
    authorize(&state, principal, "administration.email.read").await?;
    Ok(Json(state.service.email_notifications().await?))
}

async fn save_email_notifications(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(settings): Json<EmailNotificationSettings>,
) -> Result<Json<EmailNotificationSettings>, AdministrationApiError> {
    let actor = authorize(&state, principal, "administration.email.update").await?;
    Ok(Json(
        state
            .service
            .save_email_notifications(actor, settings)
            .await?,
    ))
}

async fn test_email_notifications(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<ConnectionTestResult>, AdministrationApiError> {
    authorize(&state, principal, "administration.email.test").await?;
    Ok(Json(state.service.test_email_notifications().await?))
}

async fn retention_backup(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<RetentionBackupSettings>, AdministrationApiError> {
    authorize(&state, principal, "administration.retention.read").await?;
    Ok(Json(state.service.retention_backup().await?))
}

async fn save_retention_backup(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
    Json(settings): Json<RetentionBackupSettings>,
) -> Result<Json<RetentionBackupSettings>, AdministrationApiError> {
    let actor = authorize(&state, principal, "administration.retention.update").await?;
    Ok(Json(
        state.service.save_retention_backup(actor, settings).await?,
    ))
}

async fn run_backup(
    State(state): State<AdministrationState>,
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Response, AdministrationApiError> {
    let actor = authorize(&state, principal, "administration.backup.run").await?;
    Ok((
        StatusCode::CREATED,
        Json(state.service.run_backup(actor).await?),
    )
        .into_response())
}

async fn authorize(
    state: &AdministrationState,
    principal: Option<Extension<RequestPrincipal>>,
    action: &'static str,
) -> Result<UserId, AdministrationApiError> {
    let Extension(principal) = principal.ok_or(AdministrationApiError::Forbidden)?;
    Ok(state
        .authorizer
        .authorize_instance(
            principal_identity(&principal).map_err(|_| AdministrationApiError::Forbidden)?,
            Permission::InstanceManage,
            action,
        )
        .await?)
}

impl From<RequestAuthorizationError> for AdministrationApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdministrationApiError {
    #[error("administration input is invalid")]
    Invalid,
    #[error("administration operation is forbidden")]
    Forbidden,
    #[error("administration resource is unavailable")]
    Unavailable,
    #[error("administration operation is unsupported by this deployment")]
    Unsupported,
}

impl IntoResponse for AdministrationApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unsupported => StatusCode::NOT_IMPLEMENTED,
        }
        .into_response()
    }
}
