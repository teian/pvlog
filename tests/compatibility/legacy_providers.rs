use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_compatibility::{
    LegacyAuth, LegacyInsolationPoint, LegacyInsolationQuery, LegacyProviderError,
    LegacyProviderUseCases, LegacySupplyQuery, LegacySupplyStatus, legacy_provider_router,
};
use serde::Deserialize;
use std::{error::Error, sync::Arc};
use time::macros::time;
use tower::ServiceExt as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    insolation: Case,
    supply_current: Case,
    supply_history: Case,
    unavailable: Case,
}
#[derive(Deserialize)]
struct Case {
    path: String,
    status: u16,
    body: String,
}

#[tokio::test]
async fn provider_routes_preserve_parameters_history_and_field_order() -> Result<(), Box<dyn Error>>
{
    let golden: Golden =
        serde_json::from_str(include_str!("../fixtures/pvoutput/providers-golden.json"))?;
    let app = legacy_provider_router(Arc::new(FakeProviders));
    for case in [
        golden.insolation,
        golden.supply_current,
        golden.supply_history,
        golden.unavailable,
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(&case.path).body(Body::empty())?)
            .await?;
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await?;
        assert_eq!(status, StatusCode::from_u16(case.status)?);
        assert_eq!(std::str::from_utf8(&body)?, case.body);
    }
    Ok(())
}

struct FakeProviders;
#[async_trait]
impl LegacyProviderUseCases for FakeProviders {
    async fn insolation(
        &self,
        auth: &LegacyAuth,
        query: &LegacyInsolationQuery,
    ) -> Result<Vec<LegacyInsolationPoint>, LegacyProviderError> {
        authorize(auth)?;
        assert_eq!(query.timezone, "Australia/Sydney");
        assert_eq!(
            query.coordinates,
            Some(("-33.907725".to_owned(), "151.026108".to_owned()))
        );
        assert_eq!(query.target_system_id, Some(35));
        Ok(vec![
            LegacyInsolationPoint {
                time: time!(13:40),
                power_watts: 1000,
                energy_wh: 3900,
            },
            LegacyInsolationPoint {
                time: time!(13:45),
                power_watts: 1050,
                energy_wh: 4028,
            },
        ])
    }
    async fn supply(
        &self,
        auth: &LegacyAuth,
        query: &LegacySupplyQuery,
    ) -> Result<Vec<LegacySupplyStatus>, LegacyProviderError> {
        authorize(auth)?;
        if query.region_key.as_deref() == Some("unavailable") {
            return Err(LegacyProviderError::Unavailable);
        }
        assert_eq!(query.include_history, query.region_key.is_some());
        let timestamp = if query.include_history {
            "2021-03-06T21:10:00+1100"
        } else {
            "2021-03-06T10:10:00Z"
        };
        Ok(vec![LegacySupplyStatus {
            timestamp: timestamp.to_owned(),
            region_name: "New South Wales".to_owned(),
            utilisation_milli_percent: 297,
            total_output_watts: 8799,
            total_input_watts: 244_722,
            average_output_watts: 35,
            average_input_watts: 1046,
            average_net_watts: -1011,
            systems_out: 250,
            systems_in: 234,
            total_size_watts: 2_960_410,
            average_size_watts: 11842,
        }])
    }
}
fn authorize(auth: &LegacyAuth) -> Result<(), LegacyProviderError> {
    if auth.api_key == "write-key" && auth.system_id == 42 {
        Ok(())
    } else {
        Err(LegacyProviderError::Unauthorized)
    }
}
