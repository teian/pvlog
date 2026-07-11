use pvlog_application::{
    CapacityPerformanceSlice, ComparisonCandidate, ComparisonError, ComparisonMetric,
    ComparisonPolicy, build_ladder, compare_systems,
};
use pvlog_domain::{RankingState, SystemId, TeamId, Visibility};
use std::error::Error;

fn candidate(
    name: &str,
    generation_wh: u64,
    capacity_watts: u64,
    visibility: Visibility,
) -> ComparisonCandidate {
    ComparisonCandidate {
        system_id: SystemId::new(),
        team_id: None,
        display_name: name.to_owned(),
        visibility,
        authorized: false,
        eligibility: RankingState::Eligible,
        coverage_basis_points: 9_500,
        performance: vec![CapacityPerformanceSlice {
            generation_wh,
            effective_capacity_watts: capacity_watts,
        }],
        projection_updated_at_epoch_millis: 900,
    }
}

fn policy() -> ComparisonPolicy {
    ComparisonPolicy {
        minimum_coverage_basis_points: 9_000,
        maximum_projection_age_millis: 200,
    }
}

#[test]
fn normalized_ladder_uses_effective_capacity_and_competition_ties() -> Result<(), Box<dyn Error>> {
    let team = TeamId::new();
    let mut large = candidate("Large", 10_000, 10_000, Visibility::Public);
    large.team_id = Some(team);
    let mut small = candidate("Small", 5_000, 5_000, Visibility::Public);
    small.team_id = Some(team);
    let mut lower = candidate("Lower", 3_000, 5_000, Visibility::Public);
    lower.team_id = Some(team);

    let result = build_ladder(
        &[large, small, lower],
        Some(team),
        ComparisonMetric::NormalizedGeneration,
        1_000,
        policy(),
    )?;

    assert_eq!(
        result.iter().map(|entry| entry.rank).collect::<Vec<_>>(),
        [1, 1, 3]
    );
    assert!(result[0].tied && result[1].tied && !result[2].tied);
    assert_eq!(result[0].normalized_generation_wh_per_kw, 1_000);
    Ok(())
}

#[test]
fn public_ladder_filters_privacy_eligibility_and_coverage() -> Result<(), Box<dyn Error>> {
    let visible = candidate("Visible", 1_000, 1_000, Visibility::Public);
    let private = candidate("Private", 2_000, 1_000, Visibility::Private);
    let mut ineligible = candidate("Ineligible", 3_000, 1_000, Visibility::Public);
    ineligible.eligibility = RankingState::Ineligible;
    let mut incomplete = candidate("Incomplete", 4_000, 1_000, Visibility::Public);
    incomplete.coverage_basis_points = 8_999;

    let result = build_ladder(
        &[visible, private, ineligible, incomplete],
        None,
        ComparisonMetric::TotalGeneration,
        1_000,
        policy(),
    )?;

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].display_name, "Visible");
    Ok(())
}

#[test]
fn explicit_authorization_allows_private_comparison_but_stale_projection_fails() {
    let mut private = candidate("Private", 1_000, 1_000, Visibility::Private);
    private.authorized = true;
    let result = compare_systems(
        &[private.clone()],
        ComparisonMetric::TotalGeneration,
        1_000,
        policy(),
    );
    assert!(result.is_ok());

    private.projection_updated_at_epoch_millis = 700;
    assert_eq!(
        compare_systems(
            &[private],
            ComparisonMetric::TotalGeneration,
            1_000,
            policy()
        ),
        Err(ComparisonError::StaleProjection)
    );
}
