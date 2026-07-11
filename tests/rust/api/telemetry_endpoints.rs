use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::telemetry_router;
use pvlog_application::{
    BatchIngestionMode, BatchIngestionResult, CorrectObservation, ModernTelemetryError,
    ModernTelemetryUseCases, NormalizeObservation, VersionedObservation, normalize_observation,
};
use pvlog_domain::{CanonicalObservation, ObservationId, UserId};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn telemetry_routes_cover_single_batch_correction_and_deletion_contracts()
-> Result<(), Box<dyn Error>> {
    let system = pvlog_domain::SystemId::new();
    let observation = ObservationId::new();
    let app = telemetry_router(Arc::new(Stub)).layer(Extension(UserId::new()));
    let single = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/systems/{system}/observations"))
                .header("content-type", "application/json")
                .header("idempotency-key", "one")
                .body(Body::from(
                    r#"{"observedAtEpochMillis":0,"generationPowerWatts":10}"#,
                ))?,
        )
        .await?;
    assert_eq!(single.status(), StatusCode::CREATED);
    let batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/systems/{system}/observations/batch"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"partial","items":[]}"#))?,
        )
        .await?;
    assert_eq!(batch.status(), StatusCode::OK);
    let correction = app.clone().oneshot(Request::builder().method(Method::PATCH).uri(format!("/api/v1/systems/{system}/observations/{observation}")).header("content-type", "application/json").body(Body::from(r#"{"expectedVersion":1,"reason":"fix","generationPowerWatts":11,"delete":false}"#))?).await?;
    assert_eq!(correction.status(), StatusCode::OK);
    let deletion = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!(
                    "/api/v1/systems/{system}/observations/{observation}"
                ))
                .header("if-match", "\"1\"")
                .header("x-correction-reason", "remove")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(deletion.status(), StatusCode::NO_CONTENT);
    Ok(())
}
struct Stub;
#[async_trait]
impl ModernTelemetryUseCases for Stub {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<CanonicalObservation, ModernTelemetryError> {
        normalize_observation(command).map_err(|_| ModernTelemetryError::Invalid)
    }
    async fn ingest_batch(
        &self,
        _commands: Vec<NormalizeObservation>,
        _mode: BatchIngestionMode,
    ) -> Result<BatchIngestionResult, ModernTelemetryError> {
        Ok(BatchIngestionResult {
            outcomes: Vec::new(),
        })
    }
    async fn correct(
        &self,
        command: CorrectObservation,
    ) -> Result<VersionedObservation, ModernTelemetryError> {
        Ok(VersionedObservation {
            id: command.observation_id,
            system_id: command.system_id,
            values: command.replacement,
            version: command.expected_version + 1,
            archived: false,
        })
    }
    async fn delete(
        &self,
        command: CorrectObservation,
    ) -> Result<ObservationId, ModernTelemetryError> {
        Ok(command.observation_id)
    }
}
