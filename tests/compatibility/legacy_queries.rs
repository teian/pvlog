use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_compatibility::{
    LegacyAuth, LegacyDayStatistics, LegacyHistoryStatus, LegacyQueryError, LegacyQueryUseCases,
    LegacyRangeStatistics, LegacyStatisticQuery, LegacyStatusQuery, LegacyStatusRecord,
    legacy_query_router,
};
use serde::Deserialize;
use std::{collections::BTreeMap, error::Error, sync::Arc};
use time::macros::{date, time};
use tower::ServiceExt as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    latest_extended: HttpCase,
    history_ascending: HttpCase,
    day_statistics: HttpCase,
    range_statistics: HttpCase,
}

#[derive(Deserialize)]
struct HttpCase {
    path: String,
    status: u16,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!("../fixtures/pvoutput/query-golden.json"))
}

#[tokio::test]
async fn getstatus_formats_latest_history_and_day_statistics_in_fixed_order()
-> Result<(), Box<dyn Error>> {
    let app = legacy_query_router(Arc::new(FakeQueries));
    let cases = golden()?;
    for case in [
        cases.latest_extended,
        cases.history_ascending,
        cases.day_statistics,
    ] {
        assert_case(&app, &case).await?;
    }
    Ok(())
}

#[tokio::test]
async fn getstatistic_formats_range_owner_consumption_and_finance_fields()
-> Result<(), Box<dyn Error>> {
    assert_case(
        &legacy_query_router(Arc::new(FakeQueries)),
        &golden()?.range_statistics,
    )
    .await
}

#[tokio::test]
async fn query_adapter_validates_ranges_and_caps_history_limit() -> Result<(), Box<dyn Error>> {
    let app = legacy_query_router(Arc::new(FakeQueries));
    let invalid = HttpCase {
        path: "/service/r2/getstatistic.jsp?key=read-key&sid=42&df=20241220&dt=20241201".to_owned(),
        status: 400,
        body: "Bad request 400: Date range is invalid".to_owned(),
    };
    assert_case(&app, &invalid).await?;
    Ok(())
}

async fn assert_case(app: &axum::Router, case: &HttpCase) -> Result<(), Box<dyn Error>> {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(&case.path).body(Body::empty())?)
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
    let body = std::str::from_utf8(&bytes)?;
    assert_eq!(status, StatusCode::from_u16(case.status)?, "{body}");
    assert_eq!(body, case.body);
    Ok(())
}

struct FakeQueries;

#[async_trait]
impl LegacyQueryUseCases for FakeQueries {
    async fn latest_status(
        &self,
        auth: &LegacyAuth,
        _query: &LegacyStatusQuery,
    ) -> Result<LegacyStatusRecord, LegacyQueryError> {
        authorize(auth)?;
        Ok(status_record(0))
    }

    async fn status_history(
        &self,
        auth: &LegacyAuth,
        query: &LegacyStatusQuery,
    ) -> Result<Vec<LegacyHistoryStatus>, LegacyQueryError> {
        authorize(auth)?;
        if let LegacyStatusQuery::History {
            limit, ascending, ..
        } = query
        {
            assert_eq!(*limit, 288);
            assert!(*ascending);
        }
        Ok(vec![
            LegacyHistoryStatus {
                status: status_record(0),
                efficiency_milli_kwh_per_kw: Some(2_500),
                average_power_watts: Some(450),
            },
            LegacyHistoryStatus {
                status: status_record(5),
                efficiency_milli_kwh_per_kw: Some(2_750),
                average_power_watts: Some(550),
            },
        ])
    }

    async fn day_statistics(
        &self,
        auth: &LegacyAuth,
        _query: &LegacyStatusQuery,
    ) -> Result<LegacyDayStatistics, LegacyQueryError> {
        authorize(auth)?;
        Ok(LegacyDayStatistics {
            generation_energy_wh: Some(12_000),
            generation_power_watts: Some(5_000),
            peak_power_watts: Some(6_000),
            peak_power_time: Some(time!(12:30)),
            consumption_energy_wh: Some(8_000),
            consumption_power_watts: Some(3_000),
            standby_power_watts: Some(100),
            standby_power_time: Some(time!(03:00)),
            minimum_temperature_milli_celsius: Some(5_500),
            maximum_temperature_milli_celsius: Some(24_500),
            average_temperature_milli_celsius: Some(15_000),
            include_consumption: true,
        })
    }

    async fn range_statistics(
        &self,
        auth: &LegacyAuth,
        query: &LegacyStatisticQuery,
    ) -> Result<LegacyRangeStatistics, LegacyQueryError> {
        authorize(auth)?;
        assert!(query.include_consumption && query.include_credit_debit);
        Ok(LegacyRangeStatistics {
            generated_wh: Some(24_600),
            exported_wh: Some(14_220),
            average_generation_wh: Some(2_220),
            minimum_generation_wh: Some(800),
            maximum_generation_wh: Some(3_400),
            average_efficiency_milli_kwh_per_kw: Some(3_358),
            outputs: 20,
            actual_date_from: date!(2024 - 12 - 01),
            actual_date_to: date!(2024 - 12 - 20),
            record_efficiency_milli_kwh_per_kw: Some(4_653),
            record_date: Some(date!(2024 - 12 - 05)),
            consumed_wh: Some(10_800),
            import_peak_wh: Some(5_000),
            import_off_peak_wh: Some(1_000),
            import_shoulder_wh: Some(4_000),
            import_high_shoulder_wh: Some(800),
            average_consumption_wh: Some(1_392),
            minimum_consumption_wh: Some(10),
            maximum_consumption_wh: Some(2_890),
            credit_milli_currency: Some(37_290),
            debit_milli_currency: Some(40_810),
        })
    }
}

fn authorize(auth: &LegacyAuth) -> Result<(), LegacyQueryError> {
    if auth.api_key == "read-key" && auth.system_id == 42 {
        Ok(())
    } else {
        Err(LegacyQueryError::Unauthorized)
    }
}

fn status_record(minute: u8) -> LegacyStatusRecord {
    LegacyStatusRecord {
        date: date!(2024 - 12 - 20),
        time: time!(12:00) + time::Duration::minutes(i64::from(minute)),
        generation_energy_wh: Some(1_000 + i64::from(minute) * 20),
        generation_power_watts: Some(500 + i64::from(minute) * 20),
        consumption_energy_wh: Some(400 + i64::from(minute) * 10),
        consumption_power_watts: Some(200 + i64::from(minute) * 10),
        normalized_output_milli_kw_per_kw: Some(83 + i64::from(minute) * 17 / 5),
        temperature_milli_celsius: Some(21_500 + i64::from(minute) * 40),
        voltage_millivolts: Some(239_200 - i64::from(minute) * 20),
        extended_milli: BTreeMap::from([(7, 1_000), (9, 3_000), (12, 6_000)]),
    }
}
