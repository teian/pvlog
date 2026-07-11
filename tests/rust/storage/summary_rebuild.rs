use pvlog_storage::{DailyAggregate, SummaryDay, SummaryProjection};
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
    }
}
