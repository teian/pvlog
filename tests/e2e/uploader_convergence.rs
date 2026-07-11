use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::telemetry_router;
use pvlog_application::{
    BatchIngestionMode, BatchIngestionResult, CorrectObservation, ModernTelemetryError,
    ModernTelemetryUseCases, NormalizeObservation, StatisticsBucket, StatisticsPeriod,
    VersionedObservation, calculate_statistics, normalize_observation,
};
use pvlog_compatibility::{
    AddStatusPolicy, AddStatusServiceError, AddStatusUseCases, LegacyAuth, LegacyStatus,
    add_status_router,
};
use pvlog_domain::{CanonicalObservation, EnergyReading, ObservationId, UserId};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use time::{
    PrimitiveDateTime,
    macros::{date, time},
};
use tower::ServiceExt as _;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CanonicalPoint {
    generation_energy_wh: i64,
    generation_power_watts: i64,
    consumption_energy_wh: i64,
    consumption_power_watts: i64,
}

#[tokio::test]
async fn modern_and_legacy_uploaders_converge_on_canonical_data_and_statistics()
-> Result<(), Box<dyn Error>> {
    let service = Arc::new(ConvergenceService::default());
    let system = pvlog_domain::SystemId::new();
    let app = Router::new()
        .merge(telemetry_router(service.clone()))
        .merge(add_status_router(service.clone()))
        .layer(Extension(UserId::new()));

    let modern = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/systems/{system}/observations"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "modern-equivalent")
                .body(Body::from(
                    r#"{"observedAtEpochMillis":1734696000000,"generationPowerWatts":1200,"generationEnergyWh":1000,"consumptionPowerWatts":400,"consumptionEnergyWh":300}"#,
                ))?,
        )
        .await?;
    assert_eq!(modern.status(), StatusCode::CREATED);

    let legacy = app
        .oneshot(
            Request::builder()
                .uri("/service/r2/addstatus.jsp?key=write-key&sid=42&d=20241220&t=12%3A00&v1=1000&v2=1200&v3=300&v4=400")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(legacy.status(), StatusCode::OK);

    let points = service.points.lock().map_err(|_| "point lock poisoned")?;
    assert_eq!(points.len(), 2);
    assert_eq!(points[0], points[1]);
    let modern_statistics = statistics(points[0])?;
    let legacy_statistics = statistics(points[1])?;
    assert_eq!(modern_statistics, legacy_statistics);
    assert_eq!(modern_statistics.generation_wh, Some(1_000));
    assert_eq!(modern_statistics.consumption_wh, Some(300));
    Ok(())
}

fn statistics(
    point: CanonicalPoint,
) -> Result<pvlog_application::EnergyStatistics, pvlog_application::StatisticsError> {
    calculate_statistics(
        StatisticsPeriod::Daily,
        &[StatisticsBucket {
            generation_wh: u64::try_from(point.generation_energy_wh).ok(),
            consumption_wh: u64::try_from(point.consumption_energy_wh).ok(),
            peak_generation_watts: Some(point.generation_power_watts),
            covered_millis: 300_000,
            expected_millis: 300_000,
            ..StatisticsBucket::default()
        }],
        Some(6_000),
    )
}

#[derive(Default)]
struct ConvergenceService {
    points: Mutex<Vec<CanonicalPoint>>,
}

#[async_trait]
impl ModernTelemetryUseCases for ConvergenceService {
    async fn ingest(
        &self,
        command: NormalizeObservation,
    ) -> Result<CanonicalObservation, ModernTelemetryError> {
        let observation =
            normalize_observation(command).map_err(|_| ModernTelemetryError::Invalid)?;
        let point = CanonicalPoint {
            generation_energy_wh: energy(observation.values.generation_energy)
                .ok_or(ModernTelemetryError::Invalid)?,
            generation_power_watts: observation
                .values
                .generation_power
                .map(pvlog_domain::Watts::value)
                .ok_or(ModernTelemetryError::Invalid)?,
            consumption_energy_wh: energy(observation.values.consumption_energy)
                .ok_or(ModernTelemetryError::Invalid)?,
            consumption_power_watts: observation
                .values
                .consumption_power
                .map(pvlog_domain::Watts::value)
                .ok_or(ModernTelemetryError::Invalid)?,
        };
        self.points
            .lock()
            .map_err(|_| ModernTelemetryError::Invalid)?
            .push(point);
        Ok(observation)
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

#[async_trait]
impl AddStatusUseCases for ConvergenceService {
    async fn policy(&self, auth: &LegacyAuth) -> Result<AddStatusPolicy, AddStatusServiceError> {
        if auth.api_key != "write-key" || auth.system_id != 42 {
            return Err(AddStatusServiceError::Unauthorized);
        }
        Ok(AddStatusPolicy {
            today: date!(2024 - 12 - 20),
            effective_capacity_watts: 6_000,
            status_interval_minutes: 5,
            daylight_start: time!(06:00),
            daylight_end: time!(20:00),
            extended_enabled: true,
            battery_enabled: true,
        })
    }

    async fn previous_status(
        &self,
        _auth: &LegacyAuth,
        _before: PrimitiveDateTime,
    ) -> Result<Option<LegacyStatus>, AddStatusServiceError> {
        Ok(None)
    }

    async fn accept_status(
        &self,
        _auth: &LegacyAuth,
        status: LegacyStatus,
    ) -> Result<(), AddStatusServiceError> {
        self.points
            .lock()
            .map_err(|_| AddStatusServiceError::Unavailable)?
            .push(CanonicalPoint {
                generation_energy_wh: status
                    .generation_energy
                    .map(|energy| energy.watt_hours)
                    .ok_or(AddStatusServiceError::Unavailable)?,
                generation_power_watts: status
                    .generation_power_watts
                    .ok_or(AddStatusServiceError::Unavailable)?,
                consumption_energy_wh: status
                    .consumption_energy
                    .map(|energy| energy.watt_hours)
                    .ok_or(AddStatusServiceError::Unavailable)?,
                consumption_power_watts: status
                    .consumption_power_watts
                    .ok_or(AddStatusServiceError::Unavailable)?,
            });
        Ok(())
    }
}

const fn energy(reading: Option<EnergyReading>) -> Option<i64> {
    match reading {
        Some(EnergyReading::Interval(value)) => Some(value.value()),
        Some(EnergyReading::Cumulative { total, .. }) => Some(total.value()),
        None => None,
    }
}
