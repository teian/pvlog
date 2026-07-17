//! Persistent instance-administration settings for both management database engines.

#[cfg(feature = "postgres")]
use std::fmt;
#[cfg(feature = "sqlite")]
use std::path::PathBuf;

use async_trait::async_trait;
use pvlog_domain::UserId;
use serde_json::Value;
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
use sqlx::{Connection as _, Row as _};
#[cfg(feature = "sqlite")]
use sqlx::{SqliteConnection, sqlite::SqliteConnectOptions};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalConfigurationRecord {
    pub key: String,
    pub value: Value,
    pub value_class: String,
    pub updated_by: Option<UserId>,
    pub updated_at: i64,
    pub version: i64,
}

#[async_trait]
pub trait AdministrationRepository: Send + Sync {
    async fn configuration(
        &self,
        key: &str,
    ) -> Result<Option<GlobalConfigurationRecord>, AdministrationRepositoryError>;
    async fn save_configuration(
        &self,
        record: &GlobalConfigurationRecord,
    ) -> Result<GlobalConfigurationRecord, AdministrationRepositoryError>;
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteAdministrationRepository {
    path: PathBuf,
}

#[cfg(feature = "sqlite")]
impl SqliteAdministrationRepository {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    async fn connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&self.path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
    }
}

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresAdministrationRepository {
    url: String,
}

#[cfg(feature = "postgres")]
impl PostgresAdministrationRepository {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresAdministrationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresAdministrationRepository")
            .field("url", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl AdministrationRepository for SqliteAdministrationRepository {
    async fn configuration(
        &self,
        key: &str,
    ) -> Result<Option<GlobalConfigurationRecord>, AdministrationRepositoryError> {
        validate_key(key)?;
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT key,value_json,value_class,updated_by,updated_at,version \
             FROM global_configuration WHERE key=?",
        )
        .bind(key)
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| sqlite_record(&row)).transpose()
    }

    async fn save_configuration(
        &self,
        record: &GlobalConfigurationRecord,
    ) -> Result<GlobalConfigurationRecord, AdministrationRepositoryError> {
        validate(record)?;
        let value = serde_json::to_string(&record.value)?;
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "INSERT INTO global_configuration \
             (key,value_json,value_class,updated_by,updated_at,version) VALUES (?,?,?,?,?,1) \
             ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json, \
             value_class=excluded.value_class,updated_by=excluded.updated_by, \
             updated_at=excluded.updated_at,version=global_configuration.version+1 \
             RETURNING key,value_json,value_class,updated_by,updated_at,version",
        )
        .bind(&record.key)
        .bind(value)
        .bind(&record.value_class)
        .bind(record.updated_by.map(|id| id.as_uuid().as_bytes().to_vec()))
        .bind(record.updated_at)
        .fetch_one(&mut connection)
        .await?;
        connection.close().await?;
        sqlite_record(&row)
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl AdministrationRepository for PostgresAdministrationRepository {
    async fn configuration(
        &self,
        key: &str,
    ) -> Result<Option<GlobalConfigurationRecord>, AdministrationRepositoryError> {
        validate_key(key)?;
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "SELECT key,value,value_class,updated_by,updated_at,version \
             FROM management.global_configuration WHERE key=$1",
        )
        .bind(key)
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| postgres_record(&row)).transpose()
    }

    async fn save_configuration(
        &self,
        record: &GlobalConfigurationRecord,
    ) -> Result<GlobalConfigurationRecord, AdministrationRepositoryError> {
        validate(record)?;
        let mut connection = self.connection().await?;
        let row = sqlx::query(
            "INSERT INTO management.global_configuration \
             (key,value,value_class,updated_by,updated_at,version) VALUES ($1,$2,$3,$4,$5,1) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value,value_class=excluded.value_class, \
             updated_by=excluded.updated_by,updated_at=excluded.updated_at, \
             version=management.global_configuration.version+1 \
             RETURNING key,value,value_class,updated_by,updated_at,version",
        )
        .bind(&record.key)
        .bind(&record.value)
        .bind(&record.value_class)
        .bind(record.updated_by.map(|id| id.as_uuid()))
        .bind(record.updated_at)
        .fetch_one(&mut connection)
        .await?;
        connection.close().await?;
        postgres_record(&row)
    }
}

fn validate(record: &GlobalConfigurationRecord) -> Result<(), AdministrationRepositoryError> {
    validate_key(&record.key)?;
    if !matches!(
        record.value_class.as_str(),
        "public" | "internal" | "secret_reference"
    ) || record.updated_at < 0
    {
        return Err(AdministrationRepositoryError::Invalid);
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), AdministrationRepositoryError> {
    if key.is_empty()
        || key.len() > 128
        || !key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(AdministrationRepositoryError::Invalid);
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
fn sqlite_record(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<GlobalConfigurationRecord, AdministrationRepositoryError> {
    let updated_by = row
        .try_get::<Option<Vec<u8>>, _>("updated_by")?
        .map(|value| {
            uuid::Uuid::from_slice(&value)
                .map_err(|_| AdministrationRepositoryError::Invalid)
                .and_then(|id| {
                    UserId::from_uuid(id).map_err(|_| AdministrationRepositoryError::Invalid)
                })
        })
        .transpose()?;
    Ok(GlobalConfigurationRecord {
        key: row.try_get("key")?,
        value: serde_json::from_str(&row.try_get::<String, _>("value_json")?)?,
        value_class: row.try_get("value_class")?,
        updated_by,
        updated_at: row.try_get("updated_at")?,
        version: row.try_get("version")?,
    })
}

#[cfg(feature = "postgres")]
fn postgres_record(
    row: &sqlx::postgres::PgRow,
) -> Result<GlobalConfigurationRecord, AdministrationRepositoryError> {
    Ok(GlobalConfigurationRecord {
        key: row.try_get("key")?,
        value: row.try_get("value")?,
        value_class: row.try_get("value_class")?,
        updated_by: row
            .try_get::<Option<uuid::Uuid>, _>("updated_by")?
            .map(|id| UserId::from_uuid(id).map_err(|_| AdministrationRepositoryError::Invalid))
            .transpose()?,
        updated_at: row.try_get("updated_at")?,
        version: row.try_get("version")?,
    })
}

#[derive(Debug, Error)]
pub enum AdministrationRepositoryError {
    #[error("administration configuration is invalid")]
    Invalid,
    #[error("administration database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("administration configuration JSON is invalid")]
    Json(#[from] serde_json::Error),
}
