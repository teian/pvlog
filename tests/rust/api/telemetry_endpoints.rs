use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal,
    telemetry_router,
};
use pvlog_application::{
    BatchIngestionMode, BatchIngestionResult, ChannelReading, CorrectObservation, DecimalValue,
    IngestedObservation, ModernTelemetryError, ModernTelemetryUseCases, NormalizeObservation,
    RejectedChannel, VersionedObservation, normalize_observation,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, ObservationId, Permission, PrincipalId, SystemId, UserId,
};
use std::collections::BTreeSet;
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use tower::ServiceExt as _;

#[tokio::test]
async fn telemetry_routes_cover_single_batch_correction_and_deletion_contracts()
-> Result<(), Box<dyn Error>> {
    let system = pvlog_domain::SystemId::new();
    let observation = ObservationId::new();
    let user = UserId::new();
    let authorizer = Arc::new(Authorizer::new(system, user));
    let app =
        telemetry_router(Arc::new(Stub), authorizer).layer(Extension(RequestPrincipal::User(user)));
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

#[tokio::test]
async fn telemetry_write_api_key_is_account_authorized_and_supplies_safe_idempotency_identity()
-> Result<(), Box<dyn Error>> {
    let system = pvlog_domain::SystemId::new();
    let key_id = ApiCredentialId::new();
    let owner = UserId::new();
    let account = AccountId::new();
    let service = Arc::new(RecordingStub::default());
    let app = telemetry_router(service.clone(), Arc::new(Authorizer::new(system, owner))).layer(
        Extension(RequestPrincipal::ApiCredential {
            id: key_id,
            owner_user_id: owner,
            account_id: account,
            system_id: None,
            scopes: BTreeSet::from([ApiScope::TelemetryWrite]),
        }),
    );
    let accepted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/systems/{system}/observations"))
                .header("content-type", "application/json")
                .header("idempotency-key", "same")
                .body(Body::from(r#"{"observedAtEpochMillis":0}"#))?,
        )
        .await?;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    assert_eq!(
        service.namespaces()?.as_slice(),
        [format!("api_credential:{key_id}")]
    );

    let cross_system = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/systems/{}/observations",
                    pvlog_domain::SystemId::new()
                ))
                .header("content-type", "application/json")
                .header("idempotency-key", "same")
                .body(Body::from(r#"{"observedAtEpochMillis":0}"#))?,
        )
        .await?;
    assert_eq!(cross_system.status(), StatusCode::FORBIDDEN);

    let correction = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!(
                    "/api/v1/systems/{system}/observations/{}",
                    ObservationId::new()
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"expectedVersion":1,"reason":"no","delete":false}"#,
                ))?,
        )
        .await?;
    assert_eq!(correction.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

#[tokio::test]
async fn channel_values_are_typed_and_contradictions_are_returned() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let user = UserId::new();
    let stub = Arc::new(ChannelStub::default());
    let app = telemetry_router(stub.clone(), Arc::new(Authorizer::new(system, user)))
        .layer(Extension(RequestPrincipal::User(user)));
    let post = |body: &'static str| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(format!("/api/v1/systems/{system}/observations"))
                        .header("content-type", "application/json")
                        .header("idempotency-key", "one")
                        .body(Body::from(body))?,
                )
                .await?;
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 65_536).await?;
            Ok::<_, Box<dyn Error>>((status, serde_json::from_slice::<serde_json::Value>(&bytes)?))
        }
    };

    let (status, body) = post(
        r#"{"observedAtEpochMillis":0,"channels":{
            "dc.voltage":{"value":384.12,"unit":"V"},
            "status":{"value":307},
            "meter":{"powerWatts":1500,"quality":"estimated"}}}"#,
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    // The decimal reaches the service as written, not as a float.
    let readings = stub.0.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(
        readings.get("dc.voltage"),
        Some(&ChannelReading {
            value: DecimalValue::parse("384.12"),
            unit: Some("V".to_owned()),
            ..ChannelReading::default()
        })
    );
    assert_eq!(
        readings.get("status").and_then(|reading| reading.value),
        Some(DecimalValue::integer(307))
    );
    assert_eq!(
        readings.get("meter"),
        Some(&ChannelReading {
            power_watts: Some(1_500),
            estimated: true,
            ..ChannelReading::default()
        })
    );
    // The reading is stored; the value that did not fit is named.
    assert_eq!(
        body["rejectedChannels"],
        serde_json::json!([{"channelKey": "status", "reason": "unit_mismatch"}])
    );
    assert!(body["id"].is_string());

    // Without a contradiction the member is absent.
    let (status, body) = post(r#"{"observedAtEpochMillis":0}"#).await?;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body.get("rejectedChannels").is_none());

    // A number that is not plain decimal notation and a blank unit are refused.
    for malformed in [
        r#"{"observedAtEpochMillis":0,"channels":{"a":{"value":1e-12}}}"#,
        r#"{"observedAtEpochMillis":0,"channels":{"a":{"value":1,"unit":""}}}"#,
        r#"{"observedAtEpochMillis":0,"channels":{"a":{"value":1,"unit":"seventeen-letters"}}}"#,
    ] {
        let (status, body) = post(malformed).await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{malformed}");
        assert_eq!(body["detail"], "channels", "{malformed}");
    }
    Ok(())
}

/// Records the channel readings it is handed and refuses the one named `status`.
#[derive(Default)]
struct ChannelStub(Mutex<std::collections::BTreeMap<String, ChannelReading>>);

#[async_trait]
impl ModernTelemetryUseCases for ChannelStub {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<IngestedObservation, ModernTelemetryError> {
        *self.0.lock().map_err(|_| ModernTelemetryError::Invalid)? =
            command.channel_readings.clone();
        let rejected_channels = command
            .channel_readings
            .keys()
            .filter(|key| key.as_str() == "status")
            .map(|key| RejectedChannel {
                channel_key: key.clone(),
                reason: "unit_mismatch",
            })
            .collect();
        Ok(IngestedObservation {
            observation: normalize_observation(command)
                .map_err(|_| ModernTelemetryError::Invalid)?,
            rejected_channels,
        })
    }
    async fn ingest_batch(
        &self,
        _commands: Vec<NormalizeObservation>,
        _mode: BatchIngestionMode,
    ) -> Result<BatchIngestionResult, ModernTelemetryError> {
        Err(ModernTelemetryError::Invalid)
    }
    async fn correct(
        &self,
        _command: CorrectObservation,
    ) -> Result<VersionedObservation, ModernTelemetryError> {
        Err(ModernTelemetryError::Invalid)
    }
    async fn delete(
        &self,
        _command: CorrectObservation,
    ) -> Result<ObservationId, ModernTelemetryError> {
        Err(ModernTelemetryError::Invalid)
    }
}

struct Authorizer {
    account: AccountId,
    system: SystemId,
    user: UserId,
}

impl Authorizer {
    fn new(system: SystemId, user: UserId) -> Self {
        Self {
            account: AccountId::new(),
            system,
            user,
        }
    }
}

#[async_trait]
impl ModernRequestAuthorizer for Authorizer {
    async fn authorize_instance(
        &self,
        _principal: PrincipalId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        Ok(self.user)
    }

    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        _account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Ok(AuthorizedRequest {
            actor_user_id: self.user,
            account_id: self.account,
        })
    }

    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        system_id: SystemId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        if system_id != self.system || permission != Permission::TelemetryWrite {
            return Err(RequestAuthorizationError::Forbidden);
        }
        Ok(AuthorizedRequest {
            actor_user_id: self.user,
            account_id: self.account,
        })
    }
}

#[derive(Default)]
struct RecordingStub(Mutex<Vec<String>>);
impl RecordingStub {
    fn namespaces(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self.0.lock().map_err(|_| "poisoned")?.clone())
    }
}

#[async_trait]
impl ModernTelemetryUseCases for RecordingStub {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<IngestedObservation, ModernTelemetryError> {
        self.0
            .lock()
            .map_err(|_| ModernTelemetryError::Invalid)?
            .push(command.idempotency_namespace.clone());
        normalize_observation(command)
            .map(Into::into)
            .map_err(|_| ModernTelemetryError::Invalid)
    }
    async fn ingest_batch(
        &self,
        _commands: Vec<NormalizeObservation>,
        _mode: BatchIngestionMode,
    ) -> Result<BatchIngestionResult, ModernTelemetryError> {
        Ok(BatchIngestionResult { outcomes: vec![] })
    }
    async fn correct(
        &self,
        _command: CorrectObservation,
    ) -> Result<VersionedObservation, ModernTelemetryError> {
        Err(ModernTelemetryError::Invalid)
    }
    async fn delete(
        &self,
        _command: CorrectObservation,
    ) -> Result<ObservationId, ModernTelemetryError> {
        Err(ModernTelemetryError::Invalid)
    }
}
struct Stub;
#[async_trait]
impl ModernTelemetryUseCases for Stub {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<IngestedObservation, ModernTelemetryError> {
        normalize_observation(command)
            .map(Into::into)
            .map_err(|_| ModernTelemetryError::Invalid)
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
