//! Account-storage adapter for the history imports of a system.
//!
//! An import is created by a dry run, which keeps the file and its report. It
//! is committed once: the readings and summaries are written elsewhere, the
//! result is kept here and the file is dropped. A dry run that is not
//! committed expires.

use std::fmt;

use async_trait::async_trait;
use pvlog_domain::{AccountId, DataSourceId, SystemId};
use sqlx::Row as _;
#[cfg(feature = "postgres")]
use sqlx::{Connection as _, PgConnection};
use uuid::Uuid;

use crate::AccountRepositoryError;
#[cfg(feature = "sqlite")]
use crate::RoutedSqliteAccount;

/// One history import of a data source.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryImportRecord {
    pub id: Uuid,
    pub system_id: SystemId,
    pub data_source_id: DataSourceId,
    /// User who uploaded the file.
    pub requested_by: Option<Uuid>,
    pub file_name: String,
    /// `status` or `output`.
    pub file_kind: String,
    pub content_hash: [u8; 32],
    /// The uploaded file until the import is committed; not read by listings.
    pub content: Option<Vec<u8>>,
    /// `ready` after the dry run, `committed` afterwards.
    pub state: String,
    /// What the dry run found.
    pub report: serde_json::Value,
    /// What committing stored; absent until then.
    pub result: Option<serde_json::Value>,
    pub created_at: i64,
    pub committed_at: Option<i64>,
    /// When an uncommitted dry run is removed.
    pub expires_at: i64,
}

/// Persistence of history imports within one account.
#[async_trait]
pub trait HistoryImportRepository: Send + Sync {
    /// Stores a new dry run and removes the dry runs of the account that
    /// expired before its creation.
    async fn save_history_import(
        &self,
        record: &HistoryImportRecord,
    ) -> Result<(), AccountRepositoryError>;
    /// Reads one import of a system with its file; an expired dry run is absent.
    async fn history_import(
        &self,
        system_id: SystemId,
        id: Uuid,
        now: i64,
    ) -> Result<Option<HistoryImportRecord>, AccountRepositoryError>;
    /// Lists the imports of a system without their files, newest first; expired
    /// dry runs are left out.
    async fn history_imports(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Vec<HistoryImportRecord>, AccountRepositoryError>;
    /// Marks a dry run as committed with its result and drops its file.
    /// Returns false when the import was already committed or does not exist.
    async fn commit_history_import(
        &self,
        system_id: SystemId,
        id: Uuid,
        result: &serde_json::Value,
        committed_at: i64,
    ) -> Result<bool, AccountRepositoryError>;
}

fn validate(record: &HistoryImportRecord) -> Result<(), AccountRepositoryError> {
    if !(1..=200).contains(&record.file_name.chars().count())
        || !matches!(record.file_kind.as_str(), "status" | "output")
        || !matches!(record.state.as_str(), "ready" | "committed")
    {
        return Err(AccountRepositoryError::InvalidRecord("history import"));
    }
    Ok(())
}

const COLUMNS: &str = "id,system_id,data_source_id,requested_by,file_name,file_kind,\
                       content_hash,state,created_at,committed_at,expires_at";

/// Reads the columns both databases share, given how each decodes its identifiers and JSON.
fn record<R>(
    row: &R,
    uuid: impl Fn(&R, &str) -> Result<Uuid, AccountRepositoryError>,
    report: serde_json::Value,
    result: Option<serde_json::Value>,
    content: Option<Vec<u8>>,
) -> Result<HistoryImportRecord, AccountRepositoryError>
where
    R: sqlx::Row,
    for<'r> &'r str: sqlx::ColumnIndex<R>,
    for<'r> String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'r> Vec<u8>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'r> Option<i64>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    let invalid = || AccountRepositoryError::InvalidRecord("history import");
    Ok(HistoryImportRecord {
        id: uuid(row, "id")?,
        system_id: SystemId::from_uuid(uuid(row, "system_id")?).map_err(|_| invalid())?,
        data_source_id: DataSourceId::from_uuid(uuid(row, "data_source_id")?)
            .map_err(|_| invalid())?,
        requested_by: uuid(row, "requested_by").ok(),
        file_name: row.get("file_name"),
        file_kind: row.get("file_kind"),
        content_hash: row
            .get::<Vec<u8>, _>("content_hash")
            .try_into()
            .map_err(|_| invalid())?,
        content,
        state: row.get("state"),
        report,
        result,
        created_at: row.get("created_at"),
        committed_at: row.get("committed_at"),
        expires_at: row.get("expires_at"),
    })
}

/// `SQLite` history import repository for one routed account database.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
pub struct SqliteHistoryImportRepository {
    account: RoutedSqliteAccount,
}

#[cfg(feature = "sqlite")]
impl SqliteHistoryImportRepository {
    #[must_use]
    pub fn new(account: RoutedSqliteAccount) -> Self {
        Self { account }
    }
}

#[cfg(feature = "sqlite")]
fn blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
fn sqlite_record(
    row: &sqlx::sqlite::SqliteRow,
    with_content: bool,
) -> Result<HistoryImportRecord, AccountRepositoryError> {
    let json = |column: &str| -> Result<Option<serde_json::Value>, AccountRepositoryError> {
        row.get::<Option<String>, _>(column)
            .map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(|_| AccountRepositoryError::InvalidRecord("history import"))
    };
    record(
        row,
        |row, column| {
            row.get::<Option<Vec<u8>>, _>(column)
                .and_then(|bytes| Uuid::from_slice(&bytes).ok())
                .ok_or(AccountRepositoryError::InvalidRecord("history import"))
        },
        json("report_json")?.unwrap_or_default(),
        json("result_json")?,
        if with_content {
            row.get("content")
        } else {
            None
        },
    )
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl HistoryImportRepository for SqliteHistoryImportRepository {
    async fn save_history_import(
        &self,
        record: &HistoryImportRecord,
    ) -> Result<(), AccountRepositoryError> {
        use sqlx::Connection as _;
        validate(record)?;
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        sqlx::query("DELETE FROM history_imports WHERE state='ready' AND expires_at<=?")
            .bind(record.created_at)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO history_imports (id,system_id,data_source_id,requested_by,file_name,\
             file_kind,content_hash,content,state,report_json,result_json,created_at,\
             committed_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(blob(record.id))
        .bind(blob(record.system_id.as_uuid()))
        .bind(blob(record.data_source_id.as_uuid()))
        .bind(record.requested_by.map(blob))
        .bind(&record.file_name)
        .bind(&record.file_kind)
        .bind(record.content_hash.as_slice())
        .bind(record.content.as_deref())
        .bind(&record.state)
        .bind(serde_json::to_string(&record.report)?)
        .bind(
            record
                .result
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
        )
        .bind(record.created_at)
        .bind(record.committed_at)
        .bind(record.expires_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn history_import(
        &self,
        system_id: SystemId,
        id: Uuid,
        now: i64,
    ) -> Result<Option<HistoryImportRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS},content,report_json,result_json FROM history_imports \
             WHERE system_id=? AND id=? AND (state='committed' OR expires_at>?)"
        )))
        .bind(blob(system_id.as_uuid()))
        .bind(blob(id))
        .bind(now)
        .fetch_optional(&mut *connection)
        .await?
        .map(|row| sqlite_record(&row, true))
        .transpose()
    }

    async fn history_imports(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Vec<HistoryImportRecord>, AccountRepositoryError> {
        let mut connection = self.account.acquire().await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS},report_json,result_json FROM history_imports \
             WHERE system_id=? AND (state='committed' OR expires_at>?) \
             ORDER BY created_at DESC,id DESC"
        )))
        .bind(blob(system_id.as_uuid()))
        .bind(now)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| sqlite_record(row, false))
        .collect()
    }

    async fn commit_history_import(
        &self,
        system_id: SystemId,
        id: Uuid,
        result: &serde_json::Value,
        committed_at: i64,
    ) -> Result<bool, AccountRepositoryError> {
        let mut writer = self.account.acquire_writer().await?;
        let updated = sqlx::query(
            "UPDATE history_imports SET state='committed',result_json=?,committed_at=?,\
             content=NULL WHERE system_id=? AND id=? AND state='ready'",
        )
        .bind(serde_json::to_string(result)?)
        .bind(committed_at)
        .bind(blob(system_id.as_uuid()))
        .bind(blob(id))
        .execute(writer.connection())
        .await?
        .rows_affected();
        Ok(updated == 1)
    }
}

/// `PostgreSQL` history import repository for one account.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresHistoryImportRepository {
    url: String,
    account_id: AccountId,
}

#[cfg(feature = "postgres")]
impl PostgresHistoryImportRepository {
    #[must_use]
    pub fn new(url: String, account_id: AccountId) -> Self {
        Self { url, account_id }
    }

    async fn connection(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect(&self.url).await
    }
}

#[cfg(feature = "postgres")]
impl fmt::Debug for PostgresHistoryImportRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresHistoryImportRepository")
            .field("url", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[cfg(feature = "sqlite")]
impl fmt::Debug for SqliteHistoryImportRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteHistoryImportRepository")
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "postgres")]
fn pg_record(
    row: &sqlx::postgres::PgRow,
    with_content: bool,
) -> Result<HistoryImportRecord, AccountRepositoryError> {
    record(
        row,
        |row, column| {
            row.get::<Option<Uuid>, _>(column)
                .ok_or(AccountRepositoryError::InvalidRecord("history import"))
        },
        row.get("report"),
        row.get("result"),
        if with_content {
            row.get("content")
        } else {
            None
        },
    )
}

#[cfg(feature = "postgres")]
#[async_trait]
impl HistoryImportRepository for PostgresHistoryImportRepository {
    async fn save_history_import(
        &self,
        record: &HistoryImportRecord,
    ) -> Result<(), AccountRepositoryError> {
        validate(record)?;
        let mut connection = self.connection().await?;
        let mut transaction = connection.begin().await?;
        sqlx::query(
            "DELETE FROM account_data.history_imports \
             WHERE account_id=$1 AND state='ready' AND expires_at<=$2",
        )
        .bind(self.account_id.as_uuid())
        .bind(record.created_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO account_data.history_imports (account_id,id,system_id,data_source_id,\
             requested_by,file_name,file_kind,content_hash,content,state,report,result,\
             created_at,committed_at,expires_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(self.account_id.as_uuid())
        .bind(record.id)
        .bind(record.system_id.as_uuid())
        .bind(record.data_source_id.as_uuid())
        .bind(record.requested_by)
        .bind(&record.file_name)
        .bind(&record.file_kind)
        .bind(record.content_hash.as_slice())
        .bind(record.content.as_deref())
        .bind(&record.state)
        .bind(&record.report)
        .bind(&record.result)
        .bind(record.created_at)
        .bind(record.committed_at)
        .bind(record.expires_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn history_import(
        &self,
        system_id: SystemId,
        id: Uuid,
        now: i64,
    ) -> Result<Option<HistoryImportRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS},content,report,result FROM account_data.history_imports \
             WHERE account_id=$1 AND system_id=$2 AND id=$3 \
             AND (state='committed' OR expires_at>$4)"
        )))
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(id)
        .bind(now)
        .fetch_optional(&mut connection)
        .await?;
        connection.close().await?;
        row.map(|row| pg_record(&row, true)).transpose()
    }

    async fn history_imports(
        &self,
        system_id: SystemId,
        now: i64,
    ) -> Result<Vec<HistoryImportRecord>, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS},report,result FROM account_data.history_imports \
             WHERE account_id=$1 AND system_id=$2 AND (state='committed' OR expires_at>$3) \
             ORDER BY created_at DESC,id DESC"
        )))
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(now)
        .fetch_all(&mut connection)
        .await?;
        connection.close().await?;
        rows.iter().map(|row| pg_record(row, false)).collect()
    }

    async fn commit_history_import(
        &self,
        system_id: SystemId,
        id: Uuid,
        result: &serde_json::Value,
        committed_at: i64,
    ) -> Result<bool, AccountRepositoryError> {
        let mut connection = self.connection().await?;
        let updated = sqlx::query(
            "UPDATE account_data.history_imports SET state='committed',result=$1,\
             committed_at=$2,content=NULL \
             WHERE account_id=$3 AND system_id=$4 AND id=$5 AND state='ready'",
        )
        .bind(result)
        .bind(committed_at)
        .bind(self.account_id.as_uuid())
        .bind(system_id.as_uuid())
        .bind(id)
        .execute(&mut connection)
        .await?
        .rows_affected();
        connection.close().await?;
        Ok(updated == 1)
    }
}
