//! The ingestion API of a logger.
//!
//! One request carries what a logger has: its devices, their measurements,
//! archive samples, daily summaries and events. Items are identified by what
//! they describe, so sending them again is always safe, and the answer says
//! how far each kind of data has arrived per device.

use std::{io::Read as _, sync::Arc};

use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{
    INGEST_CONTENT_ENCODINGS, IngestCapabilities, IngestRejection, IngestRequest,
    IngestRequestProblem, MAXIMUM_INGEST_DECODED_BYTES, check_ingest_request, ingest_capabilities,
    unknown_ingest_member, validate_ingest_request,
};
use pvlog_domain::{ApiCredentialId, ApiScope, Permission, SystemId};
use serde::Serialize;

use crate::{
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    authorization::principal_identity,
};

/// Who sends an ingestion request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngestWriter {
    /// API credential used, when the request was not made by a signed-in user.
    pub credential_id: Option<ApiCredentialId>,
    /// Non-secret identity of the principal, used when it has no data source.
    pub identity: String,
}

/// What became of the items of one list of a request.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestCounts {
    /// Items that were new, or replaced a stored item with other values.
    pub stored: u64,
    /// Items that were already stored as sent.
    pub unchanged: u64,
    /// Values that could not be kept while the rest of their item was.
    pub rejected: Vec<IngestRejection>,
}

/// How far a source has delivered for one device or for the system. Times are
/// epoch seconds; an absent member means that nothing of that kind is held.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestMark {
    /// The device as the source names it; absent in the marks of the system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_at: Option<i64>,
    /// Local date of the latest daily summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_at: Option<i64>,
}

/// The marks up to which a source need not send again.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestMarks {
    /// Marks of values that belong to the whole system.
    pub system: IngestMark,
    /// Marks per device, by device identifier.
    pub devices: Vec<IngestMark>,
}

/// The answer to an accepted ingestion request.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResponse {
    pub measurements: IngestCounts,
    pub energy_samples: IngestCounts,
    pub daily_summaries: IngestCounts,
    pub events: IngestCounts,
    pub marks: IngestMarks,
}

#[async_trait::async_trait]
pub trait IngestApiUseCases: Send + Sync {
    /// Stores a request that has passed every check of its form and content.
    async fn ingest(
        &self,
        system_id: SystemId,
        writer: IngestWriter,
        request: IngestRequest,
    ) -> Result<IngestResponse, IngestApiError>;
    /// Reads the marks of the data the writer has delivered for the system.
    async fn state(
        &self,
        system_id: SystemId,
        writer: IngestWriter,
    ) -> Result<IngestMarks, IngestApiError>;
}

#[derive(Clone)]
struct IngestState {
    service: Arc<dyn IngestApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
}

/// Creates the ingestion routes.
pub fn ingest_router(
    service: Arc<dyn IngestApiUseCases>,
    authorizer: Arc<dyn ModernRequestAuthorizer>,
) -> Router {
    Router::new()
        .route("/api/v1/ingest/capabilities", get(capabilities))
        .route("/api/v1/systems/{system_id}/ingest", post(ingest))
        .route("/api/v1/systems/{system_id}/ingest/state", get(state))
        .with_state(IngestState {
            service,
            authorizer,
        })
}

/// Tells any authenticated client what the ingestion API accepts.
async fn capabilities(
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<Json<IngestCapabilities>, IngestApiError> {
    if principal.is_none() {
        return Err(IngestApiError::Forbidden);
    }
    Ok(Json(ingest_capabilities()))
}

/// Checks that the principal may write telemetry of the system. A credential
/// needs the telemetry write scope and must not be restricted to another system.
async fn authorize(
    state: &IngestState,
    principal: Option<Extension<RequestPrincipal>>,
    system_id: SystemId,
    action: &'static str,
) -> Result<IngestWriter, IngestApiError> {
    let Extension(principal) = principal.ok_or(IngestApiError::Forbidden)?;
    let credential_id = match &principal {
        RequestPrincipal::ApiCredential {
            id,
            system_id: allowed_system,
            scopes,
            ..
        } => {
            if !scopes.contains(&ApiScope::TelemetryWrite)
                || allowed_system.is_some_and(|allowed| allowed != system_id)
            {
                return Err(IngestApiError::Forbidden);
            }
            Some(*id)
        }
        RequestPrincipal::User(_) => None,
    };
    state
        .authorizer
        .authorize_system(
            principal_identity(&principal)?,
            system_id,
            Permission::TelemetryWrite,
            action,
        )
        .await?;
    Ok(IngestWriter {
        credential_id,
        identity: principal.safe_ingestion_identity(),
    })
}

async fn ingest(
    State(state): State<IngestState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<IngestResponse>, IngestApiError> {
    let writer = authorize(&state, principal, system_id, "telemetry.ingest_bulk").await?;
    let request = parse(&decoded(&headers, &body)?)?;
    Ok(Json(
        state.service.ingest(system_id, writer, request).await?,
    ))
}

async fn state(
    State(state): State<IngestState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(system_id): Path<SystemId>,
) -> Result<Json<IngestMarks>, IngestApiError> {
    let writer = authorize(&state, principal, system_id, "telemetry.ingest_state").await?;
    Ok(Json(state.service.state(system_id, writer).await?))
}

/// Undoes the content encoding of a request body.
///
/// Decompression stops at the published limit, so that a small body cannot
/// expand into more than the server is prepared to hold.
fn decoded(headers: &HeaderMap, body: &Bytes) -> Result<Vec<u8>, IngestApiError> {
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .map_or("identity", |value| value.to_str().map_or("?", str::trim))
        .to_ascii_lowercase();
    let limit = u64::try_from(MAXIMUM_INGEST_DECODED_BYTES).unwrap_or(u64::MAX);
    let mut decoded = Vec::new();
    let malformed = |_| IngestApiError::Malformed("the body cannot be decompressed".to_owned());
    match encoding.as_str() {
        "identity" | "" => return Ok(body.to_vec()),
        "gzip" => flate2::read::GzDecoder::new(body.as_ref())
            .take(limit + 1)
            .read_to_end(&mut decoded)
            .map_err(malformed)?,
        "zstd" => zstd::stream::read::Decoder::new(body.as_ref())
            .map_err(malformed)?
            .take(limit + 1)
            .read_to_end(&mut decoded)
            .map_err(malformed)?,
        _ => return Err(IngestApiError::UnsupportedEncoding),
    };
    if decoded.len() > MAXIMUM_INGEST_DECODED_BYTES {
        return Err(IngestApiError::BodyTooLarge);
    }
    Ok(decoded)
}

/// Reads a request body and checks it completely, before anything is stored.
fn parse(body: &[u8]) -> Result<IngestRequest, IngestApiError> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| IngestApiError::Malformed(error.to_string()))?;
    if let Some(member) = unknown_ingest_member(&value) {
        return Err(IngestApiError::UnknownMember(member));
    }
    let request: IngestRequest = serde_json::from_value(value)
        .map_err(|error| IngestApiError::Malformed(error.to_string()))?;
    check_ingest_request(&request).map_err(IngestApiError::Request)?;
    let rejected = validate_ingest_request(&request);
    if !rejected.is_empty() {
        return Err(IngestApiError::Rejected(rejected));
    }
    Ok(request)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IngestApiError {
    Forbidden,
    NotFound,
    Unavailable,
    /// The body is not JSON of the expected form; carries what the parser said.
    Malformed(String),
    /// The body has a member the schema version does not define; carries its path.
    UnknownMember(String),
    /// The body uses a content encoding the server does not accept.
    UnsupportedEncoding,
    /// The body is larger than the published limit after decompression.
    BodyTooLarge,
    Request(IngestRequestProblem),
    /// Items cannot be stored; nothing of the request was.
    Rejected(Vec<IngestRejection>),
}

impl From<RequestAuthorizationError> for IngestApiError {
    fn from(value: RequestAuthorizationError) -> Self {
        match value {
            RequestAuthorizationError::Forbidden => Self::Forbidden,
            RequestAuthorizationError::NotFound => Self::NotFound,
            RequestAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for IngestApiError {
    /// Renders the error as an RFC 9457 `application/problem+json` response.
    /// `detail` names the member or limit, and `rejected` lists every item
    /// that cannot be stored with its list, position and reason.
    fn into_response(self) -> Response {
        let unprocessable = StatusCode::UNPROCESSABLE_ENTITY;
        let mut extension = serde_json::Map::new();
        let (status, title, detail) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "forbidden".to_owned()),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "system_not_found".to_owned(),
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "ingestion_service_unavailable".to_owned(),
            ),
            Self::Malformed(reason) => (StatusCode::BAD_REQUEST, "malformed_request", reason),
            Self::UnknownMember(member) => (unprocessable, "unknown_member", member),
            Self::UnsupportedEncoding => {
                extension.insert(
                    "contentEncodings".to_owned(),
                    serde_json::json!(INGEST_CONTENT_ENCODINGS),
                );
                (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "unsupported_content_encoding",
                    "content-encoding".to_owned(),
                )
            }
            Self::BodyTooLarge => {
                extension.insert(
                    "limit".to_owned(),
                    serde_json::json!(MAXIMUM_INGEST_DECODED_BYTES),
                );
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "limit_exceeded",
                    format!(
                        "body: at most {MAXIMUM_INGEST_DECODED_BYTES} bytes after decompression"
                    ),
                )
            }
            Self::Request(IngestRequestProblem::UnsupportedSchemaVersion(version)) => {
                extension.insert(
                    "schemaVersions".to_owned(),
                    serde_json::json!(ingest_capabilities().schema_versions),
                );
                (
                    unprocessable,
                    "unsupported_schema_version",
                    format!("schemaVersion {version} is not supported"),
                )
            }
            Self::Request(IngestRequestProblem::LimitExceeded { member, limit }) => {
                extension.insert("limit".to_owned(), serde_json::json!(limit));
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "limit_exceeded",
                    format!("{member}: at most {limit} items"),
                )
            }
            Self::Rejected(rejected) => {
                extension.insert("rejected".to_owned(), serde_json::json!(rejected));
                (unprocessable, "items_rejected", "items_rejected".to_owned())
            }
        };
        let mut problem = serde_json::json!({
            "type": "https://pvlog.example/problems/ingest",
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
