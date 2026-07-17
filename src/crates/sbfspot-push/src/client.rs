use crate::Reading;
use reqwest::{StatusCode, header::RETRY_AFTER};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Clone, Debug)]
pub struct PvlogClientConfig {
    pub base_url: Url,
    pub system_id: String,
    pub api_key: String,
    pub request_timeout: Duration,
    pub maximum_attempts: u32,
}

#[derive(Clone, Debug)]
pub struct PvlogClient {
    http: reqwest::Client,
    endpoint: Url,
    api_key: String,
    maximum_attempts: u32,
}

impl PvlogClient {
    /// Builds a client for the canonical system observation batch endpoint.
    ///
    /// # Errors
    /// Returns an error for invalid settings or an unusable base URL.
    pub fn new(config: PvlogClientConfig) -> Result<Self, ApiError> {
        if config.api_key.trim().is_empty() {
            return Err(ApiError::MissingApiKey);
        }
        if config.system_id.trim().is_empty() {
            return Err(ApiError::MissingSystemId);
        }
        if config.maximum_attempts == 0 {
            return Err(ApiError::InvalidMaximumAttempts);
        }
        let endpoint = batch_endpoint(config.base_url, &config.system_id)?;
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .user_agent(concat!("pvlog-sbfspot-push/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(ApiError::BuildClient)?;
        Ok(Self {
            http,
            endpoint,
            api_key: config.api_key,
            maximum_attempts: config.maximum_attempts,
        })
    }

    /// Sends an atomic batch and accepts only complete inserted/duplicate outcomes.
    ///
    /// # Errors
    /// Returns an error on a permanent response or after transient retries are exhausted.
    pub async fn send(&self, readings: &[Reading]) -> Result<(), ApiError> {
        if readings.is_empty() {
            return Err(ApiError::EmptyBatch);
        }
        let request = BatchRequest {
            mode: "atomic",
            items: readings
                .iter()
                .map(|reading| BatchItem {
                    idempotency_key: format!("sbfspot-daydata-{}", reading.timestamp),
                    reading,
                })
                .collect(),
        };

        for attempt in 1..=self.maximum_attempts {
            let response = self
                .http
                .post(self.endpoint.clone())
                .bearer_auth(&self.api_key)
                .json(&request)
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    let result: BatchResponse = response.json().await.map_err(ApiError::Decode)?;
                    return validate_outcomes(result, readings.len());
                }
                Ok(response) => {
                    let status = response.status();
                    let retry_after = retry_after(&response);
                    let body = response.text().await.unwrap_or_default();
                    if retryable_status(status) && attempt < self.maximum_attempts {
                        wait_before_retry(attempt, retry_after).await;
                        continue;
                    }
                    return Err(ApiError::Http {
                        status,
                        body: truncate_body(&body),
                        attempts: attempt,
                    });
                }
                Err(source) if attempt < self.maximum_attempts => {
                    tracing::warn!(attempt, error = %source, "PVLog request failed; retrying");
                    wait_before_retry(attempt, None).await;
                }
                Err(source) => {
                    return Err(ApiError::Request {
                        attempts: attempt,
                        source,
                    });
                }
            }
        }
        Err(ApiError::InvalidMaximumAttempts)
    }
}

fn batch_endpoint(mut base_url: Url, system_id: &str) -> Result<Url, ApiError> {
    if !matches!(base_url.scheme(), "http" | "https") {
        return Err(ApiError::InvalidUrlScheme);
    }
    let has_api_prefix = base_url.path().trim_end_matches('/').ends_with("/api/v1");
    let mut segments = base_url
        .path_segments_mut()
        .map_err(|()| ApiError::InvalidBaseUrl)?;
    segments.pop_if_empty();
    if !has_api_prefix {
        segments.extend(["api", "v1"]);
    }
    segments.extend(["systems", system_id, "observations", "batch"]);
    drop(segments);
    base_url.set_query(None);
    base_url.set_fragment(None);
    Ok(base_url)
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

async fn wait_before_retry(attempt: u32, retry_after: Option<Duration>) {
    let exponent = attempt.saturating_sub(1).min(6);
    let backoff = Duration::from_millis(250_u64.saturating_mul(1_u64 << exponent));
    tokio::time::sleep(retry_after.unwrap_or(backoff)).await;
}

fn validate_outcomes(response: BatchResponse, expected: usize) -> Result<(), ApiError> {
    if response.outcomes.len() != expected {
        return Err(ApiError::IncompleteBatch {
            expected,
            actual: response.outcomes.len(),
        });
    }
    if let Some(outcome) = response
        .outcomes
        .into_iter()
        .find(|outcome| !matches!(outcome.status.as_str(), "inserted" | "duplicate"))
    {
        return Err(ApiError::RejectedItem {
            index: outcome.index,
            status: outcome.status,
            code: outcome.code,
        });
    }
    Ok(())
}

fn truncate_body(body: &str) -> String {
    body.chars().take(2048).collect()
}

#[derive(Serialize)]
struct BatchRequest<'a> {
    mode: &'static str,
    items: Vec<BatchItem<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchItem<'a> {
    idempotency_key: String,
    #[serde(flatten)]
    reading: &'a Reading,
}

#[derive(Deserialize)]
struct BatchResponse {
    outcomes: Vec<BatchOutcome>,
}

#[derive(Deserialize)]
struct BatchOutcome {
    index: usize,
    status: String,
    code: Option<String>,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("PVLog API key is empty")]
    MissingApiKey,
    #[error("PVLog system ID is empty")]
    MissingSystemId,
    #[error("maximum attempts must be at least one")]
    InvalidMaximumAttempts,
    #[error("PVLog URL must use http or https")]
    InvalidUrlScheme,
    #[error("PVLog base URL cannot be used as a hierarchical URL")]
    InvalidBaseUrl,
    #[error("cannot send an empty observation batch")]
    EmptyBatch,
    #[error("failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("PVLog request failed after {attempts} attempts: {source}")]
    Request {
        attempts: u32,
        source: reqwest::Error,
    },
    #[error("PVLog returned HTTP {status} after {attempts} attempts: {body}")]
    Http {
        status: StatusCode,
        body: String,
        attempts: u32,
    },
    #[error("failed to decode the PVLog batch response: {0}")]
    Decode(reqwest::Error),
    #[error("PVLog returned {actual} outcomes for {expected} observations")]
    IncompleteBatch { expected: usize, actual: usize },
    #[error("PVLog rejected batch item {index} with status {status} and code {code:?}")]
    RejectedItem {
        index: usize,
        status: String,
        code: Option<String>,
    },
}
