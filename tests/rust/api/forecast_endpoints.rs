use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, ForecastApiError, ForecastApiUseCases, ForecastInputCompletenessResponse,
    ForecastLossInput, ForecastProvenanceResponse, ForecastResourceScope, ForecastRunQuery,
    ForecastRunResponse, ForecastSettingsInput, ForecastSettingsResponse, ModernRequestAuthorizer,
    PerformanceMetric, PerformancePointResponse, PerformanceQuery, PerformanceSeriesResponse,
    RequestAuthorizationError, RequestPrincipal, YieldSeriesPointResponse, YieldSeriesQuery,
    YieldSeriesResponse, forecasting_router, telemetry_router,
};
use pvlog_application::{
    BatchIngestionMode, BatchIngestionResult, CorrectObservation, ModernTelemetryError,
    ModernTelemetryUseCases, NormalizeObservation, VersionedObservation, normalize_observation,
};
use pvlog_domain::{
    AccountId, CanonicalObservation, ForecastCompleteness, ForecastCompletenessReason, InverterId,
    ObservationId, Permission, PrincipalId, StringId, SystemId, UserId, WeatherDataKind,
    WeatherDataRunId, YieldCalculationRunId,
};
use tower::ServiceExt as _;

#[tokio::test]
async fn forecast_settings_are_authorized_scoped_and_etag_protected() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new();
    let app = fixture.app();
    for path in fixture.settings_paths() {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["etag"], "\"4\"");
    }

    let path = fixture.string_settings_path();
    let missing_precondition = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(&path)
                .header("content-type", "application/json")
                .body(Body::from(valid_settings()))?,
        )
        .await?;
    assert_eq!(
        missing_precondition.status(),
        StatusCode::PRECONDITION_REQUIRED
    );

    let updated = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(path)
                .header("content-type", "application/json")
                .header("if-match", "\"4\"")
                .body(Body::from(valid_settings()))?,
        )
        .await?;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()["etag"], "\"5\"");
    Ok(())
}

#[tokio::test]
async fn completeness_and_field_validation_remain_explicit() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new();
    let app = fixture.app();
    let completeness = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(fixture.string_completeness_path())
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(completeness.status(), StatusCode::OK);
    assert_eq!(completeness.headers()["etag"], "\"8\"");
    let body = to_bytes(completeness.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["complete"], false);
    assert_eq!(json["reasons"][0], "missing_orientation");

    let invalid = fixture
        .app()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(fixture.string_settings_path())
                .header("content-type", "application/json")
                .header("if-match", "\"4\"")
                .body(Body::from(valid_settings().replace(
                    "\"soilingBasisPoints\":100",
                    "\"soilingBasisPoints\":10001",
                )))?,
        )
        .await?;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        invalid.headers()["content-type"],
        "application/problem+json"
    );
    let body = to_bytes(invalid.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["field"], "losses.soilingBasisPoints");
    Ok(())
}

#[tokio::test]
async fn forecast_resources_reject_missing_or_under_scoped_credentials()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new();
    let path = fixture.string_settings_path();
    let no_principal = forecasting_router(
        Arc::new(Stub),
        Arc::new(Authorizer {
            account: fixture.account_id,
            user: fixture.user,
        }),
    )
    .oneshot(Request::builder().uri(&path).body(Body::empty())?)
    .await?;
    assert_eq!(no_principal.status(), StatusCode::FORBIDDEN);

    let credential = RequestPrincipal::ApiCredential {
        id: pvlog_domain::ApiCredentialId::new(),
        owner_user_id: fixture.user,
        account_id: fixture.account_id,
        system_id: Some(fixture.system),
        scopes: BTreeSet::new(),
    };
    let under_scoped = forecasting_router(
        Arc::new(Stub),
        Arc::new(Authorizer {
            account: fixture.account_id,
            user: fixture.user,
        }),
    )
    .layer(Extension(credential))
    .oneshot(Request::builder().uri(path).body(Body::empty())?)
    .await?;
    assert_eq!(under_scoped.status(), StatusCode::FORBIDDEN);
    Ok(())
}

#[tokio::test]
async fn forecast_runs_and_yield_series_are_bounded_and_metadata_complete()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new();
    let app = fixture.app();
    let runs = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/forecast-runs?startEpochMillis=1000&endEpochMillis=3601000&kind=forecast&limit=10",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(runs.status(), StatusCode::OK);
    let body = to_bytes(runs.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json[0]["kind"], "forecast");
    assert_eq!(json[0]["freshness"], "fresh");
    assert_eq!(json[0]["provenance"]["attribution"], "Weather Example");

    let series = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/yield-series?startEpochMillis=1000&endEpochMillis=1801000&basis=forecast&resolution=15m&maximumPoints=4&includePartial=true",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(series.status(), StatusCode::OK);
    let body = to_bytes(series.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["basis"], "forecast");
    assert_eq!(json["resolution"], "fifteen_minutes");
    assert_eq!(json["includedCapacityWatts"], 4_000);
    assert_eq!(json["totalEffectiveCapacityWatts"], 8_000);
    assert_eq!(json["points"][0]["lowerPowerWatts"], 900);

    let excessive = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/yield-series?startEpochMillis=0&endEpochMillis=3600000&resolution=15m&maximumPoints=1",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(excessive.status(), StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

#[tokio::test]
async fn performance_aligns_actual_and_modeled_energy_without_allocating_to_children()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new();
    let app = fixture.app();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/yield-performance?startEpochMillis=1000&endEpochMillis=3601000&metric=generation_performance&resolution=hour&maximumPoints=2",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["metric"], "generation_performance");
    assert_eq!(json["basis"], "expected");
    assert_eq!(json["points"][0]["actualEnergyWattHours"], 800);
    assert_eq!(json["points"][0]["modeledEnergyWattHours"], 1_000);
    assert_eq!(json["points"][0]["ratioBasisPoints"], 8_000);

    let unsupported = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/yield-performance?startEpochMillis=1000&endEpochMillis=3601000&metric=forecast_realization&resolution=hour&maximumPoints=2&inverterId={}",
                    fixture.account_id, fixture.system, fixture.inverter
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(unsupported.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = to_bytes(unsupported.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["field"], "scope");
    assert_eq!(json["detail"], "unsupported_actual_scope");
    Ok(())
}

#[tokio::test]
async fn run_selection_staleness_model_boundaries_and_etag_conflicts_are_explicit()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new();
    let app = fixture.app();
    let stale = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/forecast-runs?startEpochMillis=1000&endEpochMillis=3601000&issuedBeforeEpochMillis=800&limit=10",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    let body = to_bytes(stale.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json[0]["freshness"], "stale");

    let selected_run = WeatherDataRunId::new();
    let selected = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/yield-series?startEpochMillis=1000&endEpochMillis=3601000&basis=expected&resolution=hour&maximumPoints=2&weatherRunId={selected_run}",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(selected.status(), StatusCode::OK);
    let body = to_bytes(selected.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["weatherRunId"], selected_run.to_string());
    assert_eq!(json["modelRevision"], 2);
    assert_eq!(json["basis"], "expected");

    let conflict = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(fixture.string_settings_path())
                .header("content-type", "application/json")
                .header("if-match", "\"3\"")
                .body(Body::from(valid_settings()))?,
        )
        .await?;
    assert_eq!(conflict.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        conflict.headers()["content-type"],
        "application/problem+json"
    );
    Ok(())
}

#[tokio::test]
async fn provider_failure_is_safe_and_does_not_interrupt_telemetry_ingestion()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new();
    let authorizer = Arc::new(Authorizer {
        account: fixture.account_id,
        user: fixture.user,
    });
    let app = forecasting_router(Arc::new(UnavailableStub), authorizer)
        .merge(telemetry_router(Arc::new(TelemetryStub)))
        .layer(Extension(RequestPrincipal::User(fixture.user)));
    let unavailable = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/accounts/{}/systems/{}/forecast-runs?startEpochMillis=1000&endEpochMillis=3601000",
                    fixture.account_id, fixture.system
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        unavailable.headers()["content-type"],
        "application/problem+json"
    );
    let body = to_bytes(unavailable.into_body(), 16_384).await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json["title"], "forecast_unavailable");

    let telemetry = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/systems/{}/observations", fixture.system))
                .header("content-type", "application/json")
                .header("idempotency-key", "forecast-outage")
                .body(Body::from(
                    r#"{"observedAtEpochMillis":1000,"generationPowerWatts":2500}"#,
                ))?,
        )
        .await?;
    assert_eq!(telemetry.status(), StatusCode::CREATED);
    Ok(())
}

struct Fixture {
    account_id: AccountId,
    system: SystemId,
    inverter: InverterId,
    string: StringId,
    user: UserId,
}

impl Fixture {
    fn new() -> Self {
        Self {
            account_id: AccountId::new(),
            system: SystemId::new(),
            inverter: InverterId::new(),
            string: StringId::new(),
            user: UserId::new(),
        }
    }

    fn app(&self) -> axum::Router {
        forecasting_router(
            Arc::new(Stub),
            Arc::new(Authorizer {
                account: self.account_id,
                user: self.user,
            }),
        )
        .layer(Extension(RequestPrincipal::User(self.user)))
    }

    fn settings_paths(&self) -> [String; 4] {
        [
            format!("/api/v1/accounts/{}/forecast-settings", self.account_id),
            format!(
                "/api/v1/accounts/{}/systems/{}/forecast-settings",
                self.account_id, self.system
            ),
            format!(
                "/api/v1/accounts/{}/systems/{}/inverters/{}/forecast-settings",
                self.account_id, self.system, self.inverter
            ),
            self.string_settings_path(),
        ]
    }

    fn string_settings_path(&self) -> String {
        format!(
            "/api/v1/accounts/{}/systems/{}/inverters/{}/strings/{}/forecast-settings",
            self.account_id, self.system, self.inverter, self.string
        )
    }

    fn string_completeness_path(&self) -> String {
        format!(
            "/api/v1/accounts/{}/systems/{}/inverters/{}/strings/{}/forecast-input-completeness",
            self.account_id, self.system, self.inverter, self.string
        )
    }
}

struct Stub;

#[async_trait]
impl ForecastApiUseCases for Stub {
    async fn settings(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastSettingsResponse, ForecastApiError> {
        Ok(settings(scope, 4))
    }

    async fn update_settings(
        &self,
        _actor: UserId,
        scope: ForecastResourceScope,
        expected_version: u64,
        input: ForecastSettingsInput,
    ) -> Result<ForecastSettingsResponse, ForecastApiError> {
        if expected_version != 4 {
            return Err(ForecastApiError::Conflict);
        }
        let mut response = settings(scope, 5);
        response.model_identifier = input.model_identifier;
        Ok(response)
    }

    async fn input_completeness(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastInputCompletenessResponse, ForecastApiError> {
        Ok(ForecastInputCompletenessResponse {
            scope,
            effective_at: 1_000,
            included_capacity_watts: 4_000,
            total_effective_capacity_watts: 8_000,
            complete: false,
            reasons: vec![ForecastCompletenessReason::MissingOrientation],
            version: 8,
        })
    }

    async fn forecast_runs(
        &self,
        scope: ForecastResourceScope,
        query: ForecastRunQuery,
    ) -> Result<Vec<ForecastRunResponse>, ForecastApiError> {
        let ForecastResourceScope::System { system_id, .. } = scope else {
            return Err(ForecastApiError::InvalidPath);
        };
        assert_eq!(query.kind, WeatherDataKind::Forecast);
        assert_eq!(query.limit, 10);
        Ok(vec![ForecastRunResponse {
            id: WeatherDataRunId::new(),
            system_id,
            kind: WeatherDataKind::Forecast,
            issued_at: Some(900),
            valid_from: 1_000,
            valid_to: 3_601_000,
            resolution_seconds: 3_600,
            freshness: if query.issued_before_epoch_millis.is_some() {
                pvlog_api::ForecastFreshness::Stale
            } else {
                pvlog_api::ForecastFreshness::Fresh
            },
            provenance: provenance(),
        }])
    }

    async fn yield_series(
        &self,
        scope: ForecastResourceScope,
        query: YieldSeriesQuery,
    ) -> Result<YieldSeriesResponse, ForecastApiError> {
        let selected_run = query.weather_run_id;
        Ok(YieldSeriesResponse {
            scope,
            basis: query.basis,
            resolution: query.resolution,
            issue_time: Some(900),
            weather_run_id: selected_run.unwrap_or_default(),
            calculation_run_id: YieldCalculationRunId::new(),
            model_identifier: "pvwatts-compatible".to_owned(),
            model_revision: if selected_run.is_some() { 2 } else { 1 },
            configuration_digest: "07".repeat(32),
            freshness: pvlog_api::ForecastFreshness::Fresh,
            provenance: provenance(),
            included_capacity_watts: 4_000,
            total_effective_capacity_watts: 8_000,
            completeness: ForecastCompleteness::Partial {
                reasons: vec![ForecastCompletenessReason::PartialEffectiveCapacity],
            },
            unavailable_reasons: Vec::new(),
            points: vec![YieldSeriesPointResponse {
                interval_start: query.start_epoch_millis,
                interval_end: query.start_epoch_millis + 900_000,
                central_power_watts: Some(1_000),
                lower_power_watts: Some(900),
                upper_power_watts: Some(1_100),
                central_energy_watt_hours: Some(250),
                lower_energy_watt_hours: Some(225),
                upper_energy_watt_hours: Some(275),
                coverage_basis_points: 10_000,
                completeness: ForecastCompleteness::Complete,
            }],
        })
    }

    async fn performance_series(
        &self,
        scope: ForecastResourceScope,
        query: PerformanceQuery,
    ) -> Result<PerformanceSeriesResponse, ForecastApiError> {
        if !matches!(scope, ForecastResourceScope::System { .. }) {
            return Err(ForecastApiError::UnsupportedScope);
        }
        assert_eq!(query.metric, PerformanceMetric::GenerationPerformance);
        Ok(PerformanceSeriesResponse {
            scope,
            metric: query.metric,
            basis: query.metric.basis(),
            resolution: query.resolution,
            issue_time: None,
            weather_run_id: WeatherDataRunId::new(),
            calculation_run_id: YieldCalculationRunId::new(),
            model_identifier: "pvwatts-compatible".to_owned(),
            model_revision: 1,
            configuration_digest: "08".repeat(32),
            freshness: pvlog_api::ForecastFreshness::Fresh,
            provenance: provenance(),
            points: vec![PerformancePointResponse {
                interval_start: query.start_epoch_millis,
                interval_end: query.end_epoch_millis,
                actual_energy_watt_hours: Some(800),
                modeled_energy_watt_hours: Some(1_000),
                ratio_basis_points: Some(8_000),
                actual_coverage_basis_points: 10_000,
                modeled_coverage_basis_points: 10_000,
                unavailable_reason: None,
            }],
        })
    }
}

struct UnavailableStub;

#[async_trait]
impl ForecastApiUseCases for UnavailableStub {
    async fn settings(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastSettingsResponse, ForecastApiError> {
        Stub.settings(scope).await
    }

    async fn update_settings(
        &self,
        actor: UserId,
        scope: ForecastResourceScope,
        expected_version: u64,
        input: ForecastSettingsInput,
    ) -> Result<ForecastSettingsResponse, ForecastApiError> {
        Stub.update_settings(actor, scope, expected_version, input)
            .await
    }

    async fn input_completeness(
        &self,
        scope: ForecastResourceScope,
    ) -> Result<ForecastInputCompletenessResponse, ForecastApiError> {
        Stub.input_completeness(scope).await
    }

    async fn forecast_runs(
        &self,
        _scope: ForecastResourceScope,
        _query: ForecastRunQuery,
    ) -> Result<Vec<ForecastRunResponse>, ForecastApiError> {
        Err(ForecastApiError::Unavailable)
    }

    async fn yield_series(
        &self,
        _scope: ForecastResourceScope,
        _query: YieldSeriesQuery,
    ) -> Result<YieldSeriesResponse, ForecastApiError> {
        Err(ForecastApiError::Unavailable)
    }

    async fn performance_series(
        &self,
        _scope: ForecastResourceScope,
        _query: PerformanceQuery,
    ) -> Result<PerformanceSeriesResponse, ForecastApiError> {
        Err(ForecastApiError::Unavailable)
    }
}

struct TelemetryStub;

#[async_trait]
impl ModernTelemetryUseCases for TelemetryStub {
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

fn provenance() -> ForecastProvenanceResponse {
    ForecastProvenanceResponse {
        provider_id: "weather-example".to_owned(),
        adapter: "normalized-json".to_owned(),
        source_url: "https://weather.example/forecast".to_owned(),
        license_identifier: "example-license".to_owned(),
        attribution: "Weather Example".to_owned(),
        fetched_at: 950,
    }
}

fn settings(scope: ForecastResourceScope, version: u64) -> ForecastSettingsResponse {
    ForecastSettingsResponse {
        scope,
        effective_from: 1,
        effective_to: None,
        model_identifier: "pvwatts-compatible".to_owned(),
        model_revision: 1,
        losses: ForecastLossInput {
            soiling_basis_points: 100,
            shading_basis_points: 200,
            mismatch_basis_points: 100,
            wiring_basis_points: 100,
            unavailability_basis_points: 50,
        },
        calibration_basis_points: 0,
        version,
    }
}

struct Authorizer {
    account: AccountId,
    user: UserId,
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
        _system_id: SystemId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Ok(AuthorizedRequest {
            actor_user_id: self.user,
            account_id: self.account,
        })
    }
}

fn valid_settings() -> String {
    serde_json::json!({
        "effectiveFrom": 1,
        "effectiveTo": null,
        "modelIdentifier": "pvwatts-compatible",
        "modelRevision": 1,
        "losses": {
            "soilingBasisPoints": 100,
            "shadingBasisPoints": 200,
            "mismatchBasisPoints": 100,
            "wiringBasisPoints": 100,
            "unavailabilityBasisPoints": 50
        },
        "calibrationBasisPoints": 0
    })
    .to_string()
}
