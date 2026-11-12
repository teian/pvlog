//! History import endpoints of a data source with the import profile.
//!
//! A `PVOutput` status or output export is uploaded for a dry run, which reads
//! every row and reports the covered range, accepted and rejected rows and the
//! overlap with stored data, and stores nothing else. Committing the dry run
//! writes what it accepted. Rows for times or days another source already
//! delivered are skipped, and committing the same file again stores nothing new.

use std::io::Read as _;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_domain::{ApiScope, DataSourceId, Permission, SystemId, UserId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity,
};

/// Largest file an import accepts, in bytes after decompression.
pub const MAXIMUM_IMPORT_FILE_BYTES: usize = 4 * 1024 * 1024;
/// Largest request body of an upload: the file, escaped as JSON, and its name.
const MAXIMUM_UPLOAD_BYTES: usize = MAXIMUM_IMPORT_FILE_BYTES + MAXIMUM_IMPORT_FILE_BYTES / 4;
/// Most rejected rows a report lists; the count covers all of them.
pub const MAXIMUM_LISTED_REJECTIONS: usize = 500;
const MAXIMUM_FILE_NAME_CHARACTERS: usize = 200;

/// A file uploaded for a dry run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryImportUpload {
    pub file_name: String,
    /// The file as text.
    pub content: String,
}

/// A row of the file that is not imported, with the reason.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedImportRow {
    /// Position of the row in the file, counting from 1 and including a header.
    pub row: u64,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
}

/// Rows whose time or day already holds stored data.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOverlap {
    /// Rows this data source already imported; committing them changes nothing.
    pub already_imported: u64,
    /// Rows for which another data source delivered data; they are skipped.
    pub other_sources: u64,
}

/// What a dry run found in a file.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryImportReport {
    /// Data rows of the file, without a header.
    pub rows: u64,
    /// Rows that can be imported.
    pub accepted: u64,
    pub rejected_count: u64,
    /// The first rejected rows, at most [`MAXIMUM_LISTED_REJECTIONS`].
    pub rejected: Vec<RejectedImportRow>,
    /// First local date the accepted rows cover, as `YYYY-MM-DD`.
    pub first_date: Option<String>,
    /// Last local date the accepted rows cover, as `YYYY-MM-DD`.
    pub last_date: Option<String>,
    pub overlap: ImportOverlap,
}

/// What committing an import stored.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryImportResult {
    /// Readings or daily summaries that were new or changed.
    pub stored: u64,
    /// Rows that were already stored as the file states them.
    pub already_present: u64,
    /// Rows for which another data source delivered data; existing data is kept.
    pub skipped: u64,
}

/// One history import.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryImportResponse {
    pub id: Uuid,
    pub data_source_id: Uuid,
    pub file_name: String,
    /// `status` for statuses through the day, `output` for one row per day.
    pub kind: String,
    /// `ready` after the dry run, `committed` once it is stored.
    pub state: String,
    pub created_at_epoch_millis: i64,
    /// When a dry run that is not committed is removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_epoch_millis: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_at_epoch_millis: Option<i64>,
    pub report: HistoryImportReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<HistoryImportResult>,
}

#[async_trait::async_trait]
pub trait HistoryImportApiUseCases: Send + Sync {
    /// Reads a file in a dry run and keeps it with its report until it is
    /// committed or expires.
    async fn dry_run(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
        requested_by: UserId,
        upload: HistoryImportUpload,
    ) -> Result<HistoryImportResponse, HistoryImportApiError>;
    /// Stores what a dry run accepted. Committing an import again returns it
    /// as it is.
    async fn commit(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
        import_id: Uuid,
    ) -> Result<HistoryImportResponse, HistoryImportApiError>;
    /// Lists the imports of a data source, newest first.
    async fn list(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
    ) -> Result<Vec<HistoryImportResponse>, HistoryImportApiError>;
    /// Reads one import of a data source.
    async fn get(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
        import_id: Uuid,
    ) -> Result<HistoryImportResponse, HistoryImportApiError>;
}

#[derive(Clone)]
struct HistoryImportState {
    service: Arc<dyn HistoryImportApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the history import routes.
pub fn history_imports_router(
    service: Arc<dyn HistoryImportApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/systems/{system_id}/data-sources/{data_source_id}/imports",
            get(list).post(dry_run),
        )
        .route(
            "/api/v1/systems/{system_id}/data-sources/{data_source_id}/imports/{import_id}",
            get(read),
        )
        .route(
            "/api/v1/systems/{system_id}/data-sources/{data_source_id}/imports/{import_id}/commit",
            post(commit),
        )
        .with_state(HistoryImportState {
            service,
            authorizer,
        })
}

/// Checks that the principal may manage the system's data sources, or read
/// them when `write` is false. A credential needs the systems scope and must
/// not be restricted to one system.
async fn authorize(
    state: &HistoryImportState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    write: bool,
    action: &'static str,
) -> Result<UserId, HistoryImportApiError> {
    let Extension(principal) = principal.ok_or(HistoryImportApiError::Forbidden)?;
    if let RequestPrincipal::ApiCredential {
        scopes,
        system_id: restricted,
        ..
    } = &principal
    {
        let scope = if write {
            ApiScope::SystemsWrite
        } else {
            ApiScope::SystemsRead
        };
        if !scopes.contains(&scope) || restricted.is_some() {
            return Err(HistoryImportApiError::Forbidden);
        }
    }
    Ok(state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            if write {
                Permission::SystemManage
            } else {
                Permission::SystemRead
            },
            action,
        )
        .await?
        .actor_user_id)
}

async fn dry_run(
    State(state): State<HistoryImportState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, data_source_id)): Path<(SystemId, DataSourceId)>,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<HistoryImportResponse>), HistoryImportApiError> {
    let actor = authorize(
        &state,
        principal,
        system_id,
        true,
        "history_imports.dry_run",
    )
    .await?;
    let upload = upload(&headers, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .service
                .dry_run(system_id, data_source_id, actor, upload)
                .await?,
        ),
    ))
}

async fn commit(
    State(state): State<HistoryImportState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, data_source_id, import_id)): Path<(SystemId, DataSourceId, Uuid)>,
) -> Result<Json<HistoryImportResponse>, HistoryImportApiError> {
    authorize(&state, principal, system_id, true, "history_imports.commit").await?;
    Ok(Json(
        state
            .service
            .commit(system_id, data_source_id, import_id)
            .await?,
    ))
}

async fn list(
    State(state): State<HistoryImportState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, data_source_id)): Path<(SystemId, DataSourceId)>,
) -> Result<Json<Vec<HistoryImportResponse>>, HistoryImportApiError> {
    authorize(&state, principal, system_id, false, "history_imports.list").await?;
    Ok(Json(state.service.list(system_id, data_source_id).await?))
}

async fn read(
    State(state): State<HistoryImportState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path((system_id, data_source_id, import_id)): Path<(SystemId, DataSourceId, Uuid)>,
) -> Result<Json<HistoryImportResponse>, HistoryImportApiError> {
    authorize(&state, principal, system_id, false, "history_imports.read").await?;
    Ok(Json(
        state
            .service
            .get(system_id, data_source_id, import_id)
            .await?,
    ))
}

/// Reads an upload: JSON, optionally gzip-compressed, whose file is at most
/// [`MAXIMUM_IMPORT_FILE_BYTES`] long. Reading stops at the limit, so that a
/// large or compressed body cannot make the server hold more.
async fn upload(
    headers: &HeaderMap,
    body: Body,
) -> Result<HistoryImportUpload, HistoryImportApiError> {
    let bytes = axum::body::to_bytes(body, MAXIMUM_UPLOAD_BYTES)
        .await
        .map_err(|_| HistoryImportApiError::FileTooLarge)?;
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .map_or("identity", |value| value.to_str().map_or("?", str::trim))
        .to_ascii_lowercase();
    let json = match encoding.as_str() {
        "identity" | "" => bytes.to_vec(),
        "gzip" => {
            let mut decoded = Vec::new();
            flate2::read::GzDecoder::new(bytes.as_ref())
                .take(u64::try_from(MAXIMUM_UPLOAD_BYTES).unwrap_or(u64::MAX) + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| HistoryImportApiError::InvalidInput("body"))?;
            if decoded.len() > MAXIMUM_UPLOAD_BYTES {
                return Err(HistoryImportApiError::FileTooLarge);
            }
            decoded
        }
        _ => return Err(HistoryImportApiError::UnsupportedEncoding),
    };
    let upload: HistoryImportUpload =
        serde_json::from_slice(&json).map_err(|_| HistoryImportApiError::InvalidInput("body"))?;
    if upload.content.len() > MAXIMUM_IMPORT_FILE_BYTES {
        return Err(HistoryImportApiError::FileTooLarge);
    }
    let file_name = upload.file_name.trim();
    if file_name.is_empty()
        || file_name.chars().count() > MAXIMUM_FILE_NAME_CHARACTERS
        || file_name.chars().any(char::is_control)
    {
        return Err(HistoryImportApiError::InvalidInput("fileName"));
    }
    Ok(HistoryImportUpload {
        file_name: file_name.to_owned(),
        content: upload.content,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryImportApiError {
    Forbidden,
    /// The system, data source or import does not exist.
    NotFound,
    /// The data source does not have the import profile.
    NotAnImportSource,
    /// A member of the request is invalid; carries its wire name.
    InvalidInput(&'static str),
    /// The file is no `PVOutput` export; carries the reason.
    UnreadableFile(String),
    FileTooLarge,
    UnsupportedEncoding,
    Unavailable,
}

impl From<RequestAuthorizationError> for HistoryImportApiError {
    /// Maps authorization failures onto the import errors, hiding `NotFound` behind `Forbidden`.
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden | RequestAuthorizationError::NotFound => {
                Self::Forbidden
            }
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for HistoryImportApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response.
    /// A file over the limit names the limit.
    fn into_response(self) -> Response {
        let unprocessable = StatusCode::UNPROCESSABLE_ENTITY;
        let mut extension = serde_json::Map::new();
        let (status, title, detail) = match self {
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "system_access_denied".to_owned(),
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "history_import_not_found".to_owned(),
            ),
            Self::NotAnImportSource => (
                unprocessable,
                "not_an_import_source",
                "the data source does not have the import profile".to_owned(),
            ),
            Self::InvalidInput(member) => (unprocessable, "invalid_import", member.to_owned()),
            Self::UnreadableFile(reason) => (unprocessable, "unreadable_file", reason),
            Self::FileTooLarge => {
                extension.insert(
                    "limit".to_owned(),
                    serde_json::json!(MAXIMUM_IMPORT_FILE_BYTES),
                );
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "file_too_large",
                    format!(
                        "an import file may have at most {MAXIMUM_IMPORT_FILE_BYTES} bytes; \
                         import a longer history file by file"
                    ),
                )
            }
            Self::UnsupportedEncoding => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_content_encoding",
                "content-encoding".to_owned(),
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "history_import_service_unavailable".to_owned(),
            ),
        };
        let mut problem = serde_json::json!({
            "type": "https://pvlog.example/problems/history-imports",
            "title": title,
            "status": status.as_u16(),
            "detail": detail,
        });
        if let Some(object) = problem.as_object_mut() {
            object.extend(extension);
        }
        let mut response = (status, Json(problem)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
