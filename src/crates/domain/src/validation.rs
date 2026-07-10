use std::fmt;

use serde::Serialize;
use thiserror::Error;

/// Stable field-level domain validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize)]
#[error("{field}: {message}")]
pub struct ValidationError {
    /// Machine-readable validation code.
    pub code: &'static str,
    /// Domain field or conceptual input that failed validation.
    pub field: &'static str,
    /// Safe human-readable explanation.
    pub message: String,
}

impl ValidationError {
    /// Creates a stable validation error without retaining rejected secret values.
    #[must_use]
    pub fn new(code: &'static str, field: &'static str, message: impl fmt::Display) -> Self {
        Self {
            code,
            field,
            message: message.to_string(),
        }
    }
}
