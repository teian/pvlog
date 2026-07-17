//! Production instance-administration service composition.

use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use pvlog_api::{
    AdministrationApiError, AdministrationApiUseCases, BackupResult, ConnectionTestResult,
    EmailNotificationSettings, RetentionBackupSettings, WeatherFeedSettings,
};
use pvlog_application::Clock;
use pvlog_domain::UserId;
use pvlog_storage::{AdministrationRepository, DatabaseTarget, GlobalConfigurationRecord};
use serde::{Serialize, de::DeserializeOwned};
use tokio::{net::TcpStream, time::timeout};
use url::Url;
use uuid::Uuid;

use crate::{
    SystemClock,
    operator_bundle::{export_bundle, verify_bundle},
};

const WEATHER_KEY: &str = "administration.weather_feed";
const EMAIL_KEY: &str = "administration.email_notifications";
const RETENTION_KEY: &str = "administration.retention_backup";

pub struct ManagementAdministrationApi {
    repository: Arc<dyn AdministrationRepository>,
    target: DatabaseTarget,
    clock: Arc<dyn Clock>,
}

impl ManagementAdministrationApi {
    #[must_use]
    pub fn new(repository: Arc<dyn AdministrationRepository>, target: DatabaseTarget) -> Self {
        Self {
            repository,
            target,
            clock: Arc::new(SystemClock),
        }
    }

    fn now(&self) -> Result<i64, AdministrationApiError> {
        i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| AdministrationApiError::Unavailable)
    }

    async fn load<T: DeserializeOwned>(
        &self,
        key: &str,
        default: T,
    ) -> Result<T, AdministrationApiError> {
        self.repository
            .configuration(key)
            .await
            .map_err(|_| AdministrationApiError::Unavailable)?
            .map_or(Ok(default), |record| {
                serde_json::from_value(record.value)
                    .map_err(|_| AdministrationApiError::Unavailable)
            })
    }

    async fn save<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        actor: UserId,
        value: &T,
    ) -> Result<T, AdministrationApiError> {
        let record = self
            .repository
            .save_configuration(&GlobalConfigurationRecord {
                key: key.to_owned(),
                value: serde_json::to_value(value).map_err(|_| AdministrationApiError::Invalid)?,
                value_class: "internal".to_owned(),
                updated_by: Some(actor),
                updated_at: self.now()?,
                version: 1,
            })
            .await
            .map_err(|_| AdministrationApiError::Unavailable)?;
        serde_json::from_value(record.value).map_err(|_| AdministrationApiError::Unavailable)
    }

    fn backup_root(&self) -> Result<PathBuf, AdministrationApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path, ..
            } => Ok(management_path
                .parent()
                .map_or_else(|| PathBuf::from("backups"), |path| path.join("backups"))),
            DatabaseTarget::Postgres { .. } => Err(AdministrationApiError::Unsupported),
        }
    }
}

#[async_trait]
impl AdministrationApiUseCases for ManagementAdministrationApi {
    async fn weather_feed(&self) -> Result<WeatherFeedSettings, AdministrationApiError> {
        self.load(WEATHER_KEY, default_weather()).await
    }

    async fn save_weather_feed(
        &self,
        actor: UserId,
        mut settings: WeatherFeedSettings,
    ) -> Result<WeatherFeedSettings, AdministrationApiError> {
        validate_weather(&settings)?;
        settings.updated_at_epoch_millis = Some(self.now()?);
        self.save(WEATHER_KEY, actor, &settings).await
    }

    async fn test_weather_feed(&self) -> Result<ConnectionTestResult, AdministrationApiError> {
        let settings = self.weather_feed().await?;
        let url = parse_weather_endpoint(&settings.endpoint)?;
        Ok(ConnectionTestResult {
            reachable: settings.enabled && connect_url(&url).await,
        })
    }

    async fn email_notifications(
        &self,
    ) -> Result<EmailNotificationSettings, AdministrationApiError> {
        self.load(EMAIL_KEY, default_email()).await
    }

    async fn save_email_notifications(
        &self,
        actor: UserId,
        mut settings: EmailNotificationSettings,
    ) -> Result<EmailNotificationSettings, AdministrationApiError> {
        validate_email(&settings)?;
        settings.updated_at_epoch_millis = Some(self.now()?);
        self.save(EMAIL_KEY, actor, &settings).await
    }

    async fn test_email_notifications(
        &self,
    ) -> Result<ConnectionTestResult, AdministrationApiError> {
        let settings = self.email_notifications().await?;
        Ok(ConnectionTestResult {
            reachable: settings.enabled
                && timeout(
                    Duration::from_secs(5),
                    TcpStream::connect((settings.host.as_str(), settings.port)),
                )
                .await
                .is_ok_and(|result| result.is_ok()),
        })
    }

    async fn retention_backup(&self) -> Result<RetentionBackupSettings, AdministrationApiError> {
        self.load(RETENTION_KEY, default_retention()).await
    }

    async fn save_retention_backup(
        &self,
        actor: UserId,
        mut settings: RetentionBackupSettings,
    ) -> Result<RetentionBackupSettings, AdministrationApiError> {
        validate_retention(&settings)?;
        settings.updated_at_epoch_millis = Some(self.now()?);
        self.save(RETENTION_KEY, actor, &settings).await
    }

    async fn run_backup(&self, actor: UserId) -> Result<BackupResult, AdministrationApiError> {
        let completed_at = self.now()?;
        let artifact_name = format!("pvlog-{completed_at}-{}", Uuid::new_v4());
        let root = self.backup_root()?;
        tokio::fs::create_dir_all(&root)
            .await
            .map_err(|_| AdministrationApiError::Unavailable)?;
        let artifact_path = root.join(&artifact_name);
        export_bundle(&self.target, &artifact_path)
            .await
            .map_err(|_| AdministrationApiError::Unavailable)?;
        let manifest =
            verify_bundle(&artifact_path).map_err(|_| AdministrationApiError::Unavailable)?;
        let byte_length = manifest.files.iter().map(|file| file.byte_length).sum();
        let mut settings = self.retention_backup().await?;
        settings.last_backup_at_epoch_millis = Some(completed_at);
        settings.last_backup_bytes = Some(byte_length);
        settings.updated_at_epoch_millis = Some(completed_at);
        self.save(RETENTION_KEY, actor, &settings).await?;
        Ok(BackupResult {
            completed_at_epoch_millis: completed_at,
            byte_length,
            artifact_name,
        })
    }
}

fn default_weather() -> WeatherFeedSettings {
    WeatherFeedSettings {
        enabled: false,
        endpoint: String::new(),
        credential_secret_ref: None,
        updated_at_epoch_millis: None,
    }
}

fn default_email() -> EmailNotificationSettings {
    EmailNotificationSettings {
        enabled: false,
        recipient: String::new(),
        host: String::new(),
        port: 587,
        username: String::new(),
        credential_secret_ref: None,
        encryption: pvlog_api::EmailEncryption::Starttls,
        updated_at_epoch_millis: None,
    }
}

fn default_retention() -> RetentionBackupSettings {
    RetentionBackupSettings {
        reading_retention_days: 365,
        automatic_backups_enabled: false,
        backup_schedule: "0 2 * * *".to_owned(),
        last_backup_at_epoch_millis: None,
        last_backup_bytes: None,
        updated_at_epoch_millis: None,
    }
}

fn validate_weather(settings: &WeatherFeedSettings) -> Result<(), AdministrationApiError> {
    if settings.enabled {
        parse_weather_endpoint(&settings.endpoint)?;
    }
    validate_secret_reference(settings.credential_secret_ref.as_deref())
}

fn parse_weather_endpoint(endpoint: &str) -> Result<Url, AdministrationApiError> {
    let url = Url::parse(endpoint).map_err(|_| AdministrationApiError::Invalid)?;
    if !matches!(url.scheme(), "mqtt" | "mqtts" | "http" | "https") || url.host_str().is_none() {
        return Err(AdministrationApiError::Invalid);
    }
    Ok(url)
}

fn validate_email(settings: &EmailNotificationSettings) -> Result<(), AdministrationApiError> {
    if settings.enabled
        && (settings.host.trim().is_empty()
            || settings.port == 0
            || !settings.recipient.contains('@'))
    {
        return Err(AdministrationApiError::Invalid);
    }
    validate_secret_reference(settings.credential_secret_ref.as_deref())
}

fn validate_secret_reference(value: Option<&str>) -> Result<(), AdministrationApiError> {
    if value.is_some_and(|value| value.trim().is_empty() || value.len() > 512) {
        return Err(AdministrationApiError::Invalid);
    }
    Ok(())
}

fn validate_retention(settings: &RetentionBackupSettings) -> Result<(), AdministrationApiError> {
    if !(1..=3_650).contains(&settings.reading_retention_days)
        || settings.backup_schedule.trim().is_empty()
        || settings.backup_schedule.len() > 128
    {
        return Err(AdministrationApiError::Invalid);
    }
    Ok(())
}

async fn connect_url(url: &Url) -> bool {
    let port = url.port_or_known_default().or_else(|| match url.scheme() {
        "mqtt" => Some(1883),
        "mqtts" => Some(8883),
        _ => None,
    });
    let Some((host, port)) = url.host_str().zip(port) else {
        return false;
    };
    timeout(Duration::from_secs(5), TcpStream::connect((host, port)))
        .await
        .is_ok_and(|result| result.is_ok())
}
