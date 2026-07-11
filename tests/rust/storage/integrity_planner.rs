use pvlog_storage::{IntegrityIssue, IntegritySnapshot, RepairAction, plan_integrity_repairs};
use std::collections::BTreeSet;
use uuid::Uuid;

#[test]
fn verification_reports_every_layer_and_only_plans_explicit_repairs() {
    let system = Uuid::from_u128(1);
    let job = Uuid::from_u128(2);
    let snapshot = IntegritySnapshot {
        hot_rows_without_system: BTreeSet::from([Uuid::from_u128(3)]),
        expected_segments: BTreeSet::from(["missing".into(), "present".into()]),
        present_segments: BTreeSet::from(["present".into(), "corrupt".into()]),
        corrupt_segments: BTreeSet::from(["corrupt".into()]),
        overlay_segments: BTreeSet::from(["orphan-overlay".into()]),
        rollup_segments: BTreeSet::new(),
        stale_summaries: BTreeSet::from([system]),
        orphaned_jobs: BTreeSet::from([job]),
    };
    let original = snapshot.clone();
    let report = plan_integrity_repairs(&snapshot);
    assert_eq!(snapshot, original, "verification is read-only");
    assert!(
        report
            .issues
            .contains(&IntegrityIssue::MissingSegment("missing".into()))
    );
    assert!(
        report
            .issues
            .contains(&IntegrityIssue::SegmentHashMismatch("corrupt".into()))
    );
    assert!(
        report
            .issues
            .contains(&IntegrityIssue::OverlayWithoutSegment(
                "orphan-overlay".into()
            ))
    );
    assert!(
        report
            .issues
            .contains(&IntegrityIssue::StaleSummary(system))
    );
    assert!(report.issues.contains(&IntegrityIssue::OrphanedJob(job)));
    assert!(
        report
            .repair_plan
            .contains(&RepairAction::RebuildSummary(system))
    );
    assert!(!report.repair_plan.is_empty());
}
