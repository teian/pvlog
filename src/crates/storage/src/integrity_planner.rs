//! Read-only integrity verification and explicit repair planning.

use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IntegrityIssue {
    HotRowWithoutSystem(Uuid),
    MissingSegment(String),
    SegmentHashMismatch(String),
    OverlayWithoutSegment(String),
    MissingRollup(String),
    StaleSummary(Uuid),
    OrphanedJob(Uuid),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairAction {
    QuarantineHotRow(Uuid),
    RestoreOrRecompactSegment(String),
    RebuildRollups(String),
    RebuildSummary(Uuid),
    DeadLetterJob(Uuid),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntegritySnapshot {
    pub hot_rows_without_system: BTreeSet<Uuid>,
    pub expected_segments: BTreeSet<String>,
    pub present_segments: BTreeSet<String>,
    pub corrupt_segments: BTreeSet<String>,
    pub overlay_segments: BTreeSet<String>,
    pub rollup_segments: BTreeSet<String>,
    pub stale_summaries: BTreeSet<Uuid>,
    pub orphaned_jobs: BTreeSet<Uuid>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntegrityReport {
    pub issues: Vec<IntegrityIssue>,
    pub repair_plan: Vec<RepairAction>,
}

/// Verifies a point-in-time inventory and produces a plan without mutating data.
#[must_use]
pub fn plan_integrity_repairs(snapshot: &IntegritySnapshot) -> IntegrityReport {
    let mut report = IntegrityReport::default();
    for id in &snapshot.hot_rows_without_system {
        report.issues.push(IntegrityIssue::HotRowWithoutSystem(*id));
        report.repair_plan.push(RepairAction::QuarantineHotRow(*id));
    }
    for key in snapshot
        .expected_segments
        .difference(&snapshot.present_segments)
    {
        report
            .issues
            .push(IntegrityIssue::MissingSegment(key.clone()));
        report
            .repair_plan
            .push(RepairAction::RestoreOrRecompactSegment(key.clone()));
    }
    for key in &snapshot.corrupt_segments {
        report
            .issues
            .push(IntegrityIssue::SegmentHashMismatch(key.clone()));
        report
            .repair_plan
            .push(RepairAction::RestoreOrRecompactSegment(key.clone()));
    }
    for key in snapshot
        .overlay_segments
        .difference(&snapshot.present_segments)
    {
        report
            .issues
            .push(IntegrityIssue::OverlayWithoutSegment(key.clone()));
    }
    for key in snapshot
        .present_segments
        .difference(&snapshot.rollup_segments)
    {
        report
            .issues
            .push(IntegrityIssue::MissingRollup(key.clone()));
        report
            .repair_plan
            .push(RepairAction::RebuildRollups(key.clone()));
    }
    for id in &snapshot.stale_summaries {
        report.issues.push(IntegrityIssue::StaleSummary(*id));
        report.repair_plan.push(RepairAction::RebuildSummary(*id));
    }
    for id in &snapshot.orphaned_jobs {
        report.issues.push(IntegrityIssue::OrphanedJob(*id));
        report.repair_plan.push(RepairAction::DeadLetterJob(*id));
    }
    report
}
