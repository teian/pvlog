//! Signed opaque cursors bound to filters, sorting, stable position, and expiry.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorPosition {
    pub sort_value: String,
    pub id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct Payload {
    sort_value: String,
    id: Uuid,
    query_hash: [u8; 32],
    expires_at: i64,
}

pub struct PageCursorCodec {
    key: [u8; 32],
    lifetime_seconds: u32,
}
impl PageCursorCodec {
    #[must_use]
    pub fn new(key: [u8; 32], lifetime_seconds: u32) -> Self {
        Self {
            key,
            lifetime_seconds,
        }
    }

    /// Encodes a stable position bound to the normalized query shape.
    /// # Errors
    /// Returns an error when expiry arithmetic or serialization fails.
    pub fn encode(
        &self,
        position: CursorPosition,
        query_shape: &str,
        now: i64,
    ) -> Result<String, PaginationError> {
        let expires_at = now
            .checked_add(i64::from(self.lifetime_seconds) * 1_000)
            .ok_or(PaginationError::InvalidCursor)?;
        let payload = serde_json::to_vec(&Payload {
            sort_value: position.sort_value,
            id: position.id,
            query_hash: hash(query_shape.as_bytes()),
            expires_at,
        })
        .map_err(|_| PaginationError::InvalidCursor)?;
        let signature = blake3::keyed_hash(&self.key, &payload);
        Ok(format!("{}.{}", hex(&payload), hex(signature.as_bytes())))
    }

    /// Validates and decodes a cursor for exactly one query shape.
    /// # Errors
    /// Returns an error for malformed, tampered, expired, or query-mismatched cursors.
    pub fn decode(
        &self,
        cursor: &str,
        query_shape: &str,
        now: i64,
    ) -> Result<CursorPosition, PaginationError> {
        let (payload_hex, signature_hex) = cursor
            .split_once('.')
            .ok_or(PaginationError::InvalidCursor)?;
        let payload = unhex(payload_hex)?;
        let signature = unhex(signature_hex)?;
        let expected = blake3::keyed_hash(&self.key, &payload);
        if signature.len() != 32 || !constant_time_eq(&signature, expected.as_bytes()) {
            return Err(PaginationError::InvalidCursor);
        }
        let payload: Payload =
            serde_json::from_slice(&payload).map_err(|_| PaginationError::InvalidCursor)?;
        if payload.expires_at <= now {
            return Err(PaginationError::ExpiredCursor);
        }
        if payload.query_hash != hash(query_shape.as_bytes()) {
            return Err(PaginationError::QueryMismatch);
        }
        Ok(CursorPosition {
            sort_value: payload.sort_value,
            id: payload.id,
        })
    }

    /// Validates a requested bounded page size.
    /// # Errors
    /// Returns an error when the size is zero or exceeds the endpoint maximum.
    pub fn page_size(requested: u32, maximum: u32) -> Result<u32, PaginationError> {
        (requested > 0 && requested <= maximum)
            .then_some(requested)
            .ok_or(PaginationError::InvalidPageSize)
    }
}

fn hash(value: &[u8]) -> [u8; 32] {
    *blake3::hash(value).as_bytes()
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)] as char,
                DIGITS[usize::from(byte & 15)] as char,
            ]
        })
        .collect()
}
fn unhex(value: &str) -> Result<Vec<u8>, PaginationError> {
    if !value.len().is_multiple_of(2) {
        return Err(PaginationError::InvalidCursor);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = digit(pair[0])?;
            let low = digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}
fn digit(value: u8) -> Result<u8, PaginationError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(PaginationError::InvalidCursor),
    }
}
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.iter()
        .zip(right)
        .fold(left.len() ^ right.len(), |difference, (left, right)| {
            difference | usize::from(left ^ right)
        })
        == 0
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaginationError {
    #[error("cursor is invalid")]
    InvalidCursor,
    #[error("cursor has expired")]
    ExpiredCursor,
    #[error("cursor does not match the query")]
    QueryMismatch,
    #[error("page size is outside the allowed range")]
    InvalidPageSize,
}
