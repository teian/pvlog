use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, ForecastApiError, ForecastApiUseCases, ForecastInputCompletenessResponse,
    ForecastLossInput, ForecastResourceScope, ForecastSettingsInput, ForecastSettingsResponse,
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, forecasting_router,
};
use pvlog_domain::{
    AccountId, ForecastCompletenessReason, InverterId, Permission, PrincipalId, StringId, SystemId,
    UserId,
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
        assert_eq!(expected_version, 4);
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
