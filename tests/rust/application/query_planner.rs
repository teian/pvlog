use pvlog_application::{
    QueryPlan, QueryPlanError, QueryPlanRequest, QueryResolution, QuerySource, RawSources,
    RequestedResolution, SeriesField, plan_query,
};
use std::collections::BTreeSet;

const DAY: i64 = 86_400_000;

fn request(days: i64) -> QueryPlanRequest {
    QueryPlanRequest {
        start_epoch_millis: 0,
        end_epoch_millis: days * DAY,
        requested_resolution: RequestedResolution::Auto,
        fields: BTreeSet::from([SeriesField::GenerationPower]),
        timezone: "Europe/Berlin".to_owned(),
        maximum_points: 2_000,
        expected_raw_interval_millis: 300_000,
        hot_data_start_epoch_millis: 35 * DAY,
        available_rollups: BTreeSet::from([
            QueryResolution::FifteenMinutes,
            QueryResolution::Hourly,
            QueryResolution::Daily,
            QueryResolution::Monthly,
            QueryResolution::Yearly,
        ]),
    }
}

fn successful_plan(input: QueryPlanRequest) -> QueryPlan {
    match plan_query(input) {
        Ok(plan) => plan,
        Err(error) => panic!("expected a query plan, got {error}"),
    }
}

#[test]
fn twenty_five_year_query_selects_monthly_rollups_within_budget() {
    let plan = successful_plan(request(25 * 365));

    assert_eq!(plan.actual_resolution, QueryResolution::Monthly);
    assert_eq!(plan.source, QuerySource::Rollup(QueryResolution::Monthly));
    assert!(plan.estimated_points <= 2_000);
    assert_eq!(plan.timezone.name(), "Europe/Berlin");
}

#[test]
fn automatic_short_query_uses_only_hot_rows() {
    let mut input = request(1);
    input.start_epoch_millis = 40 * DAY;
    input.end_epoch_millis = 41 * DAY;

    let plan = successful_plan(input);

    assert_eq!(plan.actual_resolution, QueryResolution::Raw);
    assert_eq!(plan.source, QuerySource::Raw(RawSources::Hot));
}

#[test]
fn exact_raw_query_crossing_compaction_boundary_uses_both_raw_stores() {
    let mut input = request(10);
    input.start_epoch_millis = 30 * DAY;
    input.end_epoch_millis = 40 * DAY;
    input.requested_resolution = RequestedResolution::Raw;
    input.maximum_points = 3_000;

    let plan = successful_plan(input);

    assert_eq!(
        plan.source,
        QuerySource::Raw(RawSources::HotAndArchivedSegments)
    );
}

#[test]
fn exact_raw_query_is_rejected_instead_of_silently_downsampled() {
    let mut input = request(30);
    input.requested_resolution = RequestedResolution::Raw;

    assert_eq!(plan_query(input), Err(QueryPlanError::PointBudgetExceeded));
}

#[test]
fn provenance_forces_raw_data_and_remains_bounded() {
    let mut input = request(1);
    input.fields.insert(SeriesField::Provenance);

    assert_eq!(
        successful_plan(input).actual_resolution,
        QueryResolution::Raw
    );

    let mut too_large = request(30);
    too_large.fields.insert(SeriesField::Provenance);
    assert_eq!(
        plan_query(too_large),
        Err(QueryPlanError::PointBudgetExceeded)
    );
}

#[test]
fn explicit_resolution_must_be_available_and_fit_the_budget() {
    let mut unavailable = request(30);
    unavailable.requested_resolution = RequestedResolution::Hourly;
    unavailable
        .available_rollups
        .remove(&QueryResolution::Hourly);
    assert_eq!(
        plan_query(unavailable),
        Err(QueryPlanError::ResolutionUnavailable)
    );

    let mut too_many = request(365);
    too_many.requested_resolution = RequestedResolution::FifteenMinutes;
    assert_eq!(
        plan_query(too_many),
        Err(QueryPlanError::PointBudgetExceeded)
    );
}

#[test]
fn invalid_timezone_and_empty_fields_are_rejected() {
    let mut invalid_timezone = request(1);
    invalid_timezone.timezone = "local-ish".to_owned();
    assert_eq!(
        plan_query(invalid_timezone),
        Err(QueryPlanError::InvalidTimezone)
    );

    let mut empty = request(1);
    empty.fields.clear();
    assert_eq!(plan_query(empty), Err(QueryPlanError::NoFields));
}
