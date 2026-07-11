//! `addbatchstatus.jsp` compatibility adapter with stable per-item outcomes.

use crate::add_status::AddStatusApiError;
use crate::{
    AddStatusPolicy, AddStatusServiceError, AddStatusUseCases, LegacyAuth, LegacyError,
    LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError, LegacyStatus, csv_record,
    parse_csv_record, parse_legacy_auth,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use time::PrimitiveDateTime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchStatusOutcome {
    Added,
    Unchanged,
    Retryable,
}

#[async_trait]
pub trait AddBatchStatusUseCases: AddStatusUseCases {
    async fn accept_batch_status(
        &self,
        auth: &LegacyAuth,
        status: LegacyStatus,
    ) -> Result<BatchStatusOutcome, AddStatusServiceError>;
    async fn complete_daily_output(
        &self,
        auth: &LegacyAuth,
        last_successful: &LegacyStatus,
    ) -> Result<(), AddStatusServiceError>;
}

#[derive(Clone)]
struct BatchState {
    service: Arc<dyn AddBatchStatusUseCases>,
}

pub fn add_batch_status_router(service: Arc<dyn AddBatchStatusUseCases>) -> Router {
    Router::new()
        .route(
            "/service/r2/addbatchstatus.jsp",
            get(batch_get).post(batch_post),
        )
        .with_state(BatchState { service })
}

async fn batch_get(
    State(state): State<BatchState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, BatchApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    execute(state, LegacyMethod::Get, headers, parameters).await
}

async fn batch_post(
    State(state): State<BatchState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, BatchApiError> {
    if query.is_some_and(|query| !query.is_empty()) {
        return Err(BatchApiError::bad("POST parameters must use form data"));
    }
    execute(
        state,
        LegacyMethod::Post,
        headers,
        LegacyParameters::parse(&body)?,
    )
    .await
}

async fn execute(
    state: BatchState,
    method: LegacyMethod,
    headers: HeaderMap,
    parameters: LegacyParameters,
) -> Result<Response, BatchApiError> {
    let auth = parse_legacy_auth(method, &headers, &parameters)?;
    let policy = state.service.policy(&auth).await?;
    let net = parameters.get("n").is_some_and(|value| value == "1");
    let cumulative = parameters.get("c1").unwrap_or("0");
    let data = parameters
        .get("data")
        .ok_or_else(|| BatchApiError::bad("Batch data is required"))?;
    let records = split_records(data)?;
    if records.is_empty() || records.len() > 30 {
        return Err(BatchApiError::bad(
            "A maximum of 30 statuses can be sent in a batch",
        ));
    }
    let mut statuses = records
        .iter()
        .map(|record| parse_batch_status(record, cumulative, net, policy))
        .collect::<Result<Vec<_>, _>>()?;
    if net
        && statuses
            .iter()
            .any(|status| status.date != statuses[0].date)
    {
        return Err(BatchApiError::bad(
            "All Net statuses in the batch must have the same date",
        ));
    }
    statuses.sort_by_key(|status| (status.date, status.time));
    if statuses
        .windows(2)
        .any(|pair| (pair[0].date, pair[0].time) == (pair[1].date, pair[1].time))
    {
        return Err(BatchApiError::bad("Batch statuses must have unique times"));
    }
    let first_timestamp = PrimitiveDateTime::new(statuses[0].date, statuses[0].time);
    let mut previous = state
        .service
        .previous_status(&auth, first_timestamp)
        .await?;
    let mut response_rows = Vec::with_capacity(statuses.len());
    let mut last_successful = None;
    let mut retryable = false;
    for mut status in statuses {
        crate::add_status::derive_energy_and_power(&mut status, previous.as_ref())?;
        crate::add_status::validate_status(&status, policy)?;
        let outcome = state
            .service
            .accept_batch_status(&auth, status.clone())
            .await?;
        let code = match outcome {
            BatchStatusOutcome::Added => {
                previous = Some(status.clone());
                last_successful = Some(status.clone());
                "1"
            }
            BatchStatusOutcome::Unchanged => "0",
            BatchStatusOutcome::Retryable => {
                retryable = true;
                "0"
            }
        };
        let formatted_date = crate::format_legacy_date(status.date);
        let formatted_time = crate::format_legacy_time(status.time);
        response_rows.push(csv_record([
            Some(formatted_date.as_str()),
            Some(formatted_time.as_str()),
            Some(code),
        ]));
    }
    if let Some(status) = last_successful.as_ref() {
        state.service.complete_daily_output(&auth, status).await?;
    }
    let mut response = text_response(StatusCode::OK, &response_rows.join(";"));
    if retryable {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    }
    Ok(response)
}

fn parse_batch_status(
    record: &str,
    cumulative: &str,
    net: bool,
    policy: AddStatusPolicy,
) -> Result<LegacyStatus, BatchApiError> {
    let fields = parse_csv_record(record)?;
    if fields.len() > 14 || fields.len() < 2 {
        return Err(BatchApiError::bad("Batch status data is invalid"));
    }
    if net
        && (fields.get(3).is_none_or(String::is_empty)
            || fields.get(5).is_none_or(String::is_empty))
    {
        return Err(BatchApiError::bad(
            "A Net status must have export and import data",
        ));
    }
    let names = [
        "d", "t", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12",
    ];
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in names.iter().zip(fields.iter()) {
        if !value.is_empty() {
            serializer.append_pair(name, value);
        }
    }
    if cumulative != "0" {
        serializer.append_pair("c1", cumulative);
    }
    if net {
        serializer.append_pair("n", "1");
    }
    let parameters = LegacyParameters::parse(serializer.finish().as_bytes())?;
    crate::add_status::parse_status(&parameters, policy).map_err(BatchApiError::from)
}

fn split_records(data: &str) -> Result<Vec<String>, BatchApiError> {
    let mut records = Vec::new();
    let mut record = String::new();
    let mut quoted = false;
    let mut characters = data.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '"' {
            record.push(character);
            if quoted && characters.peek() == Some(&'"') {
                if let Some(escaped) = characters.next() {
                    record.push(escaped);
                }
            } else {
                quoted = !quoted;
            }
        } else if character == ';' && !quoted {
            records.push(std::mem::take(&mut record));
        } else {
            record.push(character);
        }
    }
    if quoted {
        return Err(BatchApiError::bad("Batch status CSV is invalid"));
    }
    records.push(record);
    Ok(records)
}

enum BatchApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Status(AddStatusApiError),
    Service(AddStatusServiceError),
}

impl BatchApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}

impl From<LegacyProtocolError> for BatchApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<AddStatusApiError> for BatchApiError {
    fn from(value: AddStatusApiError) -> Self {
        Self::Status(value)
    }
}

impl From<AddStatusServiceError> for BatchApiError {
    fn from(value: AddStatusServiceError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for BatchApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Legacy(error) => text_response(
                StatusCode::from_u16(error.kind.status()).unwrap_or(StatusCode::BAD_REQUEST),
                &error.body(),
            ),
            Self::Protocol(error) => text_response(
                StatusCode::BAD_REQUEST,
                &LegacyError {
                    kind: LegacyErrorKind::BadRequest,
                    detail: error.to_string(),
                }
                .body(),
            ),
            Self::Status(error) => error.into_response(),
            Self::Service(AddStatusServiceError::Unauthorized) => text_response(
                StatusCode::UNAUTHORIZED,
                "Unauthorized 401: Invalid API Key",
            ),
            Self::Service(AddStatusServiceError::Forbidden) => {
                text_response(StatusCode::FORBIDDEN, "Forbidden 403: Read only key")
            }
            Self::Service(AddStatusServiceError::Unavailable) => {
                text_response(StatusCode::SERVICE_UNAVAILABLE, "Service unavailable")
            }
        }
    }
}

fn text_response(status: StatusCode, body: &str) -> Response {
    let mut response = Response::new(Body::from(body.to_owned()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
