use pvlog_storage::{
    DailyAggregate, ModeledYieldAggregate, SummaryDay, SummaryPeriod, SummaryProjection,
};
use uuid::Uuid;

#[test]
fn late_and_corrected_days_rebuild_daily_and_lifetime_summaries_idempotently() {
    let system = Uuid::from_u128(1);
    let mut projection = SummaryProjection::default();
    projection.invalidate(system, SummaryDay(2));
    assert_eq!(projection.invalidated_days().count(), 1);

    assert!(projection.reconcile(&day(system, 1, 10, 4, 1)));
    let corrected = day(system, 2, 25, 8, 2);
    assert!(projection.reconcile(&corrected));
    assert!(!projection.reconcile(&corrected));
    assert_eq!(projection.invalidated_days().count(), 0);
    let lifetime = projection.lifetime[&system];
    assert_eq!(lifetime.generation_wh, 35);
    assert_eq!(lifetime.consumption_wh, 12);
    assert_eq!(lifetime.through_day, Some(SummaryDay(2)));

    projection.invalidate(system, SummaryDay(1));
    assert!(!projection.lifetime.contains_key(&system));
    assert!(projection.reconcile(&day(system, 1, 12, 3, 3)));
    assert_eq!(projection.lifetime[&system].generation_wh, 37);

    let month = projection.modeled_summary(
        system,
        SummaryPeriod::Month {
            year: 2026,
            month: 7,
        },
    );
    assert_eq!(month.expected_energy_wh, Some(40));
    assert_eq!(month.forecast_energy_wh, Some(42));
    assert_eq!(month.generation_performance_basis_points, Some(9_250));
    assert_eq!(month.forecast_realization_basis_points, Some(8_809));
    assert_eq!(month.actual_coverage_basis_points, 9_000);
    assert_eq!(month.expected_coverage_basis_points, 8_500);
}

fn day(
    system_id: Uuid,
    day: i32,
    generation_wh: i64,
    consumption_wh: i64,
    source_revision: u64,
) -> DailyAggregate {
    DailyAggregate {
        system_id,
        day: SummaryDay(day),
        generation_wh,
        consumption_wh,
        quality_flags: 1,
        source_revision,
        calendar_year: 2026,
        calendar_month: 7,
        modeled: ModeledYieldAggregate {
            expected_energy_wh: Some(if day == 1 { 14 } else { 26 }),
            expected_lower_wh: Some(if day == 1 { 12 } else { 24 }),
            expected_upper_wh: Some(if day == 1 { 16 } else { 28 }),
            forecast_energy_wh: Some(if day == 1 { 15 } else { 27 }),
            forecast_lower_wh: Some(if day == 1 { 13 } else { 25 }),
            forecast_upper_wh: Some(if day == 1 { 17 } else { 29 }),
            actual_coverage_basis_points: if day == 1 { 9_500 } else { 9_000 },
            expected_coverage_basis_points: if day == 1 { 9_000 } else { 8_500 },
            forecast_coverage_basis_points: if day == 1 { 9_200 } else { 8_800 },
        },
    }
}
