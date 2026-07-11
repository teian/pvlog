//! Legacy notification registration and callback payload adapter.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    parse_legacy_auth,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use thiserror::Error;
use url::Url;

const ALERT_TYPES: [u8; 16] = [0, 1, 3, 4, 5, 6, 8, 11, 14, 15, 16, 17, 18, 19, 20, 23];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyNotificationRegistration {
    pub application_id: String,
    pub callback_url: Url,
    pub alert_type: u8,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyNotificationCallback {
    pub application_id: String,
    pub message: String,
    pub alert_type: u8,
}

#[async_trait]
pub trait LegacyNotificationUseCases: Send + Sync {
    async fn register(
        &self,
        auth: &LegacyAuth,
        registration: LegacyNotificationRegistration,
    ) -> Result<(), LegacyNotificationError>;
    async fn deregister(
        &self,
        auth: &LegacyAuth,
        application_id: &str,
        alert_type: u8,
    ) -> Result<(), LegacyNotificationError>;
}
#[derive(Clone)]
struct NotificationState {
    service: Arc<dyn LegacyNotificationUseCases>,
}
pub fn legacy_notification_router(service: Arc<dyn LegacyNotificationUseCases>) -> Router {
    Router::new()
        .route("/service/r2/registernotification.jsp", get(register))
        .route("/service/r2/deregisternotification.jsp", get(deregister))
        .with_state(NotificationState { service })
}
async fn register(
    State(state): State<NotificationState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, NotificationApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    let application_id = required(&parameters, "appid")?;
    let callback = required(&parameters, "url")?;
    if application_id.chars().count() > 100 || callback.chars().count() > 150 {
        return Err(NotificationApiError::bad(
            "Notification registration exceeds field limit",
        ));
    }
    let callback_url =
        Url::parse(callback).map_err(|_| NotificationApiError::bad("Callback URL invalid"))?;
    let alert_type = alert_type(&parameters)?;
    state
        .service
        .register(
            &auth,
            LegacyNotificationRegistration {
                application_id: application_id.to_owned(),
                callback_url,
                alert_type,
            },
        )
        .await?;
    Ok(text_response(StatusCode::OK, "Registered Notification"))
}
async fn deregister(
    State(state): State<NotificationState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, NotificationApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &headers, &parameters)?;
    state
        .service
        .deregister(
            &auth,
            required(&parameters, "appid")?,
            alert_type(&parameters)?,
        )
        .await?;
    Ok(text_response(StatusCode::OK, "Deregistered Notification"))
}
fn required<'a>(
    parameters: &'a LegacyParameters,
    field: &str,
) -> Result<&'a str, NotificationApiError> {
    parameters
        .get(field)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| NotificationApiError::bad(format!("{field} is required")))
}
fn alert_type(parameters: &LegacyParameters) -> Result<u8, NotificationApiError> {
    let value = required(parameters, "type")?
        .parse::<u8>()
        .map_err(|_| NotificationApiError::bad("Alert Type invalid"))?;
    if ALERT_TYPES.contains(&value) || value == 24 {
        Ok(value)
    } else {
        Err(NotificationApiError::bad("Alert Type invalid"))
    }
}
#[must_use]
pub fn legacy_notification_callback_body(callback: &LegacyNotificationCallback) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("appid", &callback.application_id)
        .append_pair("msg", &callback.message)
        .append_pair("type", &callback.alert_type.to_string())
        .finish()
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LegacyNotificationError {
    #[error("notification credentials are invalid")]
    Unauthorized,
    #[error("notification registration limit reached")]
    RegistrationLimit,
    #[error("notification alert type is not enabled")]
    AlertDisabled,
    #[error("notification registration was not found")]
    NotFound,
    #[error("notification storage is unavailable")]
    Unavailable,
}
enum NotificationApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(LegacyNotificationError),
}
impl NotificationApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}
impl From<LegacyProtocolError> for NotificationApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}
impl From<LegacyNotificationError> for NotificationApiError {
    fn from(value: LegacyNotificationError) -> Self {
        Self::Service(value)
    }
}
impl IntoResponse for NotificationApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Legacy(error) => text_response(
                StatusCode::from_u16(error.kind.status()).unwrap_or(StatusCode::BAD_REQUEST),
                &error.body(),
            ),
            Self::Protocol(error) => text_response(
                StatusCode::BAD_REQUEST,
                &format!("Bad request 400: {error}"),
            ),
            Self::Service(LegacyNotificationError::Unauthorized) => {
                text_response(StatusCode::FORBIDDEN, "Forbidden 403: Invalid API Key")
            }
            Self::Service(LegacyNotificationError::RegistrationLimit) => text_response(
                StatusCode::BAD_REQUEST,
                "Bad request 400: Maximum application registrations reached",
            ),
            Self::Service(LegacyNotificationError::AlertDisabled) => text_response(
                StatusCode::BAD_REQUEST,
                "Bad request 400: Alert type is not enabled",
            ),
            Self::Service(LegacyNotificationError::NotFound) => text_response(
                StatusCode::BAD_REQUEST,
                "Bad request 400: Notification registration not found",
            ),
            Self::Service(LegacyNotificationError::Unavailable) => {
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
