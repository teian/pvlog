//! Privacy-safe system comparisons and deterministic ladders.

use pvlog_domain::{RankingState, SystemId, TeamId, Visibility};
use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityPerformanceSlice {
    pub generation_wh: u64,
    pub effective_capacity_watts: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonCandidate {
    pub system_id: SystemId,
    pub team_id: Option<TeamId>,
    pub display_name: String,
    pub visibility: Visibility,
    /// True when the caller has explicit access independent of public visibility.
    pub authorized: bool,
    pub eligibility: RankingState,
    pub coverage_basis_points: u16,
    pub performance: Vec<CapacityPerformanceSlice>,
    pub projection_updated_at_epoch_millis: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMetric {
    TotalGeneration,
    NormalizedGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComparisonPolicy {
    pub minimum_coverage_basis_points: u16,
    pub maximum_projection_age_millis: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonEntry {
    pub rank: u32,
    pub system_id: SystemId,
    pub display_name: String,
    pub total_generation_wh: u64,
    pub normalized_generation_wh_per_kw: u64,
    pub coverage_basis_points: u16,
    pub tied: bool,
    pub projection_age_millis: u64,
}

/// Compares an explicit set of systems visible to the caller.
/// # Errors
/// Returns an error when no eligible visible systems remain or candidate data is invalid.
pub fn compare_systems(
    candidates: &[ComparisonCandidate],
    metric: ComparisonMetric,
    now_epoch_millis: i64,
    policy: ComparisonPolicy,
) -> Result<Vec<ComparisonEntry>, ComparisonError> {
    rank_candidates(candidates, None, metric, now_epoch_millis, policy, false)
}

/// Builds a public team or global ladder. Ladder candidates must be publicly visible and eligible.
/// # Errors
/// Returns an error when no eligible visible systems remain or candidate data is invalid.
pub fn build_ladder(
    candidates: &[ComparisonCandidate],
    team_id: Option<TeamId>,
    metric: ComparisonMetric,
    now_epoch_millis: i64,
    policy: ComparisonPolicy,
) -> Result<Vec<ComparisonEntry>, ComparisonError> {
    rank_candidates(candidates, team_id, metric, now_epoch_millis, policy, true)
}

fn rank_candidates(
    candidates: &[ComparisonCandidate],
    team_id: Option<TeamId>,
    metric: ComparisonMetric,
    now_epoch_millis: i64,
    policy: ComparisonPolicy,
    public_ladder: bool,
) -> Result<Vec<ComparisonEntry>, ComparisonError> {
    if policy.minimum_coverage_basis_points > 10_000 {
        return Err(ComparisonError::InvalidPolicy);
    }
    let mut scored = candidates
        .iter()
        .filter(|candidate| team_id.is_none_or(|id| candidate.team_id == Some(id)))
        .filter(|candidate| {
            if public_ladder {
                candidate.visibility == Visibility::Public
                    && candidate.eligibility == RankingState::Eligible
            } else {
                candidate.authorized || candidate.visibility == Visibility::Public
            }
        })
        .filter(|candidate| candidate.coverage_basis_points >= policy.minimum_coverage_basis_points)
        .map(|candidate| score_candidate(candidate, now_epoch_millis, policy))
        .collect::<Result<Vec<_>, _>>()?;
    if scored.is_empty() {
        return Err(ComparisonError::NoEligibleSystems);
    }
    scored.sort_unstable_by(|left, right| {
        score(right, metric)
            .cmp(&score(left, metric))
            .then_with(|| left.system_id.as_uuid().cmp(&right.system_id.as_uuid()))
    });
    let metric_values = scored
        .iter()
        .map(|entry| score(entry, metric))
        .collect::<Vec<_>>();
    for index in 0..scored.len() {
        let same_as_previous = index > 0 && metric_values[index] == metric_values[index - 1];
        scored[index].rank = if same_as_previous {
            scored[index - 1].rank
        } else {
            u32::try_from(index + 1).map_err(|_| ComparisonError::Overflow)?
        };
        scored[index].tied = same_as_previous
            || metric_values
                .get(index + 1)
                .is_some_and(|next| *next == metric_values[index]);
    }
    Ok(scored)
}

fn score_candidate(
    candidate: &ComparisonCandidate,
    now_epoch_millis: i64,
    policy: ComparisonPolicy,
) -> Result<ComparisonEntry, ComparisonError> {
    if candidate.display_name.trim().is_empty()
        || candidate.coverage_basis_points > 10_000
        || candidate.performance.is_empty()
        || candidate
            .performance
            .iter()
            .any(|slice| slice.effective_capacity_watts == 0)
        || candidate.projection_updated_at_epoch_millis > now_epoch_millis
    {
        return Err(ComparisonError::InvalidCandidate);
    }
    let total_generation_wh = checked_sum(
        candidate
            .performance
            .iter()
            .map(|slice| slice.generation_wh),
    )?;
    let normalized_generation_wh_per_kw = checked_sum(candidate.performance.iter().map(|slice| {
        slice
            .generation_wh
            .checked_mul(1_000)
            .map(|scaled| scaled / slice.effective_capacity_watts)
            .ok_or(ComparisonError::Overflow)
    }))?;
    let projection_age_millis =
        u64::try_from(now_epoch_millis - candidate.projection_updated_at_epoch_millis)
            .map_err(|_| ComparisonError::InvalidCandidate)?;
    if projection_age_millis > policy.maximum_projection_age_millis {
        return Err(ComparisonError::StaleProjection);
    }
    Ok(ComparisonEntry {
        rank: 0,
        system_id: candidate.system_id,
        display_name: candidate.display_name.clone(),
        total_generation_wh,
        normalized_generation_wh_per_kw,
        coverage_basis_points: candidate.coverage_basis_points,
        tied: false,
        projection_age_millis,
    })
}

const fn score(entry: &ComparisonEntry, metric: ComparisonMetric) -> u64 {
    match metric {
        ComparisonMetric::TotalGeneration => entry.total_generation_wh,
        ComparisonMetric::NormalizedGeneration => entry.normalized_generation_wh_per_kw,
    }
}

fn checked_sum(
    mut values: impl Iterator<Item = impl IntoComparisonValue>,
) -> Result<u64, ComparisonError> {
    values.try_fold(0_u64, |total, value| {
        value
            .into_result()
            .and_then(|value| total.checked_add(value).ok_or(ComparisonError::Overflow))
    })
}

trait IntoComparisonValue {
    fn into_result(self) -> Result<u64, ComparisonError>;
}

impl IntoComparisonValue for u64 {
    fn into_result(self) -> Result<u64, ComparisonError> {
        Ok(self)
    }
}

impl IntoComparisonValue for Result<u64, ComparisonError> {
    fn into_result(self) -> Result<u64, ComparisonError> {
        self
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ComparisonError {
    #[error("comparison policy is invalid")]
    InvalidPolicy,
    #[error("comparison candidate data is invalid")]
    InvalidCandidate,
    #[error("community projection is too stale for ranking")]
    StaleProjection,
    #[error("no visible eligible systems satisfy the comparison policy")]
    NoEligibleSystems,
    #[error("comparison arithmetic overflowed")]
    Overflow,
}
