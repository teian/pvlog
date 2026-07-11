use async_trait::async_trait;
use pvlog_application::{
    GapKind, PlannedSeries, QueryPlan, QueryPlanRequest, QueryResolution, RequestedResolution,
    SeriesField, SeriesGap, SeriesPoint, SeriesQueryError, SeriesQueryRepository,
    SeriesQueryRepositoryError, SeriesQueryService, SeriesUnit,
};
use pvlog_domain::SystemId;
use std::{collections::BTreeSet, error::Error, sync::Arc};

struct FakeRepository {
    series: Vec<PlannedSeries>,
}

#[async_trait]
impl SeriesQueryRepository for FakeRepository {
    async fn execute_plan(
        &self,
        _system_id: SystemId,
        _plan: &QueryPlan,
    ) -> Result<Vec<PlannedSeries>, SeriesQueryRepositoryError> {
        Ok(self.series.clone())
    }
}

fn request() -> QueryPlanRequest {
    QueryPlanRequest {
        start_epoch_millis: 0,
        end_epoch_millis: 3_600_000,
        requested_resolution: RequestedResolution::Raw,
        fields: BTreeSet::from([SeriesField::GenerationPower, SeriesField::ConsumptionPower]),
        timezone: "UTC".to_owned(),
        maximum_points: 20,
        expected_raw_interval_millis: 300_000,
        hot_data_start_epoch_millis: 0,
        available_rollups: BTreeSet::new(),
    }
}

fn series(field: SeriesField, unit: SeriesUnit, provenance: Option<&str>) -> PlannedSeries {
    PlannedSeries {
        field,
        unit,
        points: vec![SeriesPoint {
            timestamp_epoch_millis: 300_000,
            value: 1_200,
            coverage_basis_points: 10_000,
            quality_flags: 0,
            provenance: provenance.map(str::to_owned),
        }],
        gaps: vec![SeriesGap {
            start_epoch_millis: 600_000,
            end_epoch_millis: 900_000,
            kind: GapKind::Missing,
        }],
    }
}

#[tokio::test]
async fn returns_multiple_raw_series_with_units_gaps_and_provenance() -> Result<(), Box<dyn Error>>
{
    let repository = Arc::new(FakeRepository {
        series: vec![
            series(
                SeriesField::ConsumptionPower,
                SeriesUnit::Watts,
                Some("meter:west"),
            ),
            series(
                SeriesField::GenerationPower,
                SeriesUnit::Watts,
                Some("inverter:east"),
            ),
        ],
    });

    let result = SeriesQueryService::new(repository)
        .query(SystemId::new(), request())
        .await?;

    assert_eq!(result.actual_resolution, QueryResolution::Raw);
    assert_eq!(result.timezone, "UTC");
    assert_eq!(result.series.len(), 2);
    assert_eq!(result.series[0].field, SeriesField::GenerationPower);
    assert_eq!(result.series[0].gaps[0].kind, GapKind::Missing);
    assert_eq!(
        result.series[0].points[0].provenance.as_deref(),
        Some("inverter:east")
    );
    Ok(())
}

#[tokio::test]
async fn rejects_missing_unrequested_and_invalid_series() {
    let missing = SeriesQueryService::new(Arc::new(FakeRepository {
        series: vec![series(
            SeriesField::GenerationPower,
            SeriesUnit::Watts,
            None,
        )],
    }))
    .query(SystemId::new(), request())
    .await;
    assert!(matches!(missing, Err(SeriesQueryError::MissingSeries)));

    let mut invalid = series(SeriesField::GenerationPower, SeriesUnit::Watts, None);
    invalid.points[0].coverage_basis_points = 10_001;
    let invalid = SeriesQueryService::new(Arc::new(FakeRepository {
        series: vec![
            invalid,
            series(SeriesField::ConsumptionPower, SeriesUnit::Watts, None),
        ],
    }))
    .query(SystemId::new(), request())
    .await;
    assert!(matches!(invalid, Err(SeriesQueryError::InvalidPoint)));
}

#[tokio::test]
async fn aggregate_queries_reject_raw_provenance() {
    let mut input = request();
    input.requested_resolution = RequestedResolution::Hourly;
    input.maximum_points = 10;
    input.available_rollups = BTreeSet::from([QueryResolution::Hourly]);
    let result = SeriesQueryService::new(Arc::new(FakeRepository {
        series: vec![
            series(
                SeriesField::GenerationPower,
                SeriesUnit::Watts,
                Some("raw-source"),
            ),
            series(SeriesField::ConsumptionPower, SeriesUnit::Watts, None),
        ],
    }))
    .query(SystemId::new(), input)
    .await;

    assert!(matches!(
        result,
        Err(SeriesQueryError::AggregateHasRawProvenance)
    ));
}
