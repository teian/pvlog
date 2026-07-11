//! Reusable `PVOutput` r2 wire parsing and formatting primitives.

use axum::http::HeaderMap;
use std::collections::BTreeMap;
use thiserror::Error;
use time::{Date, Month, Time};
use url::form_urlencoded;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyMethod {
    Get,
    Post,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyParameters(BTreeMap<String, String>);

impl LegacyParameters {
    /// Parses URL query or form encoding and rejects ambiguous duplicate parameters.
    /// # Errors
    /// Returns an error for non-UTF-8 form bytes, empty names, or duplicate names.
    pub fn parse(encoded: &[u8]) -> Result<Self, LegacyProtocolError> {
        let decoded = std::str::from_utf8(encoded).map_err(|_| LegacyProtocolError::Encoding)?;
        let mut values = BTreeMap::new();
        for (name, value) in form_urlencoded::parse(decoded.as_bytes()) {
            if name.is_empty() {
                return Err(LegacyProtocolError::EmptyParameterName);
            }
            if values
                .insert(name.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(LegacyProtocolError::DuplicateParameter);
            }
        }
        Ok(Self(values))
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// Returns a required non-empty parameter.
    /// # Errors
    /// Returns an error when the parameter is absent or empty.
    pub fn required(&self, name: &str) -> Result<&str, LegacyProtocolError> {
        self.get(name)
            .filter(|value| !value.is_empty())
            .ok_or(LegacyProtocolError::MissingParameter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyAuth {
    pub api_key: String,
    pub system_id: u64,
}

/// Parses legacy header authentication, with documented GET-only key/sid query fallback.
/// # Errors
/// Returns an error when credentials are missing, malformed, or query credentials are used on POST.
pub fn parse_legacy_auth(
    method: LegacyMethod,
    headers: &HeaderMap,
    parameters: &LegacyParameters,
) -> Result<LegacyAuth, LegacyProtocolError> {
    let header_key = header(headers, "x-pvoutput-apikey");
    let header_system = header(headers, "x-pvoutput-systemid");
    let query_key = parameters.get("key");
    let query_system = parameters.get("sid");
    if method == LegacyMethod::Post && (query_key.is_some() || query_system.is_some()) {
        return Err(LegacyProtocolError::QueryAuthenticationOnPost);
    }
    let (api_key, system) = match (header_key, header_system) {
        (Some(key), Some(system)) => (key, system),
        (None, None) if method == LegacyMethod::Get => (
            query_key.ok_or(LegacyProtocolError::MissingAuthentication)?,
            query_system.ok_or(LegacyProtocolError::MissingAuthentication)?,
        ),
        _ => return Err(LegacyProtocolError::MissingAuthentication),
    };
    if api_key.trim().is_empty() || system.is_empty() {
        return Err(LegacyProtocolError::InvalidAuthentication);
    }
    Ok(LegacyAuth {
        api_key: api_key.to_owned(),
        system_id: system
            .parse()
            .map_err(|_| LegacyProtocolError::InvalidAuthentication)?,
    })
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Parses `yyyymmdd` exactly.
/// # Errors
/// Returns an error for any other width, non-digit input, or invalid calendar date.
pub fn parse_legacy_date(value: &str) -> Result<Date, LegacyProtocolError> {
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(LegacyProtocolError::InvalidDate);
    }
    let year = value[0..4]
        .parse::<i32>()
        .map_err(|_| LegacyProtocolError::InvalidDate)?;
    let month = value[4..6]
        .parse::<u8>()
        .ok()
        .and_then(|month| Month::try_from(month).ok())
        .ok_or(LegacyProtocolError::InvalidDate)?;
    let day = value[6..8]
        .parse::<u8>()
        .map_err(|_| LegacyProtocolError::InvalidDate)?;
    Date::from_calendar_date(year, month, day).map_err(|_| LegacyProtocolError::InvalidDate)
}

#[must_use]
pub fn format_legacy_date(value: Date) -> String {
    format!(
        "{:04}{:02}{:02}",
        value.year(),
        u8::from(value.month()),
        value.day()
    )
}

/// Parses `hh:mm` using a 24-hour clock.
/// # Errors
/// Returns an error for any other shape or an invalid time.
pub fn parse_legacy_time(value: &str) -> Result<Time, LegacyProtocolError> {
    if value.len() != 5 || value.as_bytes().get(2) != Some(&b':') {
        return Err(LegacyProtocolError::InvalidTime);
    }
    let hour = value[0..2]
        .parse::<u8>()
        .map_err(|_| LegacyProtocolError::InvalidTime)?;
    let minute = value[3..5]
        .parse::<u8>()
        .map_err(|_| LegacyProtocolError::InvalidTime)?;
    Time::from_hms(hour, minute, 0).map_err(|_| LegacyProtocolError::InvalidTime)
}

#[must_use]
pub fn format_legacy_time(value: Time) -> String {
    format!("{:02}:{:02}", value.hour(), value.minute())
}

/// Parses the legacy numeric boolean convention and tolerant textual equivalents.
/// # Errors
/// Returns an error for values other than `1`, `0`, `true`, or `false`.
pub fn parse_legacy_bool(value: &str) -> Result<bool, LegacyProtocolError> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(LegacyProtocolError::InvalidBoolean),
    }
}

#[must_use]
pub fn csv_field(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[must_use]
pub fn csv_record<'a>(fields: impl IntoIterator<Item = Option<&'a str>>) -> String {
    fields
        .into_iter()
        .map(csv_field)
        .collect::<Vec<_>>()
        .join(",")
}

/// Parses one RFC 4180-style record while retaining empty fields.
/// # Errors
/// Returns an error for unterminated quotes or characters following a closing quote.
pub fn parse_csv_record(value: &str) -> Result<Vec<String>, LegacyProtocolError> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut characters = value.chars().peekable();
    let mut quoted = false;
    let mut closed_quote = false;
    while let Some(character) = characters.next() {
        if quoted {
            if character == '"' {
                if characters.peek() == Some(&'"') {
                    field.push('"');
                    let _ = characters.next();
                } else {
                    quoted = false;
                    closed_quote = true;
                }
            } else {
                field.push(character);
            }
        } else {
            match character {
                '"' if field.is_empty() && !closed_quote => quoted = true,
                ',' => {
                    fields.push(std::mem::take(&mut field));
                    closed_quote = false;
                }
                _ if closed_quote => return Err(LegacyProtocolError::InvalidCsv),
                _ => field.push(character),
            }
        }
    }
    if quoted {
        return Err(LegacyProtocolError::InvalidCsv);
    }
    fields.push(field);
    Ok(fields)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacySuccess {
    AddedOutput,
    AddedStatus,
    DeletedStatus,
}

impl LegacySuccess {
    #[must_use]
    pub const fn body(self) -> &'static str {
        match self {
            Self::AddedOutput => "Added Output",
            Self::AddedStatus => "Added Status",
            Self::DeletedStatus => "Deleted Status",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyErrorKind {
    BadRequest,
    Unauthorized,
    Forbidden,
    MethodNotAllowed,
}

impl LegacyErrorKind {
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::MethodNotAllowed => 405,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::BadRequest => "Bad request",
            Self::Unauthorized => "Unauthorized",
            Self::Forbidden => "Forbidden",
            Self::MethodNotAllowed => "Method Not Allowed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyError {
    pub kind: LegacyErrorKind,
    pub detail: String,
}

impl LegacyError {
    #[must_use]
    pub fn body(&self) -> String {
        format!(
            "{} {}: {}",
            self.kind.title(),
            self.kind.status(),
            self.detail
        )
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LegacyProtocolError {
    #[error("legacy input is not UTF-8")]
    Encoding,
    #[error("legacy parameter name is empty")]
    EmptyParameterName,
    #[error("legacy parameter is duplicated")]
    DuplicateParameter,
    #[error("required legacy parameter is missing")]
    MissingParameter,
    #[error("legacy authentication is missing")]
    MissingAuthentication,
    #[error("legacy authentication is invalid")]
    InvalidAuthentication,
    #[error("query authentication is accepted only for GET requests")]
    QueryAuthenticationOnPost,
    #[error("legacy date is invalid")]
    InvalidDate,
    #[error("legacy time is invalid")]
    InvalidTime,
    #[error("legacy boolean is invalid")]
    InvalidBoolean,
    #[error("legacy CSV record is invalid")]
    InvalidCsv,
}
