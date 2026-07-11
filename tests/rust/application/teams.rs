use async_trait::async_trait;
use pvlog_application::{
    CapacityPerformanceSlice, ComparisonCandidate, ComparisonMetric, CreateTeam, JoinTeam,
    TeamAccess, TeamAggregateResource, TeamEligibility, TeamMembershipResource, TeamPolicy,
    TeamRepository, TeamResource, TeamService, TeamServiceError, TeamUseCases,
};
use pvlog_domain::{AccountId, RankingState, SystemId, TeamId, UserId, Visibility};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn team_lifecycle_supports_create_transfer_join_and_leave() -> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = service(repository.clone());
    let owner = UserId::new();
    let successor = UserId::new();
    let team = service
        .create_team(CreateTeam {
            account_id: AccountId::new(),
            actor: owner,
            name: "  Solar Club  ".to_owned(),
            description: None,
            access: TeamAccess::Public,
        })
        .await?;
    assert_eq!(team.name, "Solar Club");
    let transferred = service
        .transfer_ownership(team.id, owner, successor, 1)
        .await?;
    assert_eq!(transferred.owner_user_id, successor);
    assert_eq!(transferred.version, 2);

    let system = SystemId::new();
    let membership = service
        .join_team(JoinTeam {
            team_id: team.id,
            system_id: system,
            actor: owner,
            now_epoch_millis: 1_000,
        })
        .await?;
    assert!(membership.active);
    service.leave_team(team.id, system, owner, 2_000).await?;
    assert!(
        !repository
            .memberships
            .lock()
            .map_err(|_| "membership lock")?[0]
            .active
    );
    Ok(())
}

#[tokio::test]
async fn owner_cannot_leave_and_ineligible_system_cannot_join() -> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = service(repository.clone());
    let owner = UserId::new();
    let team = service
        .create_team(CreateTeam {
            account_id: AccountId::new(),
            actor: owner,
            name: "Owners".to_owned(),
            description: None,
            access: TeamAccess::Public,
        })
        .await?;
    repository
        .eligibility
        .lock()
        .map_err(|_| "eligibility lock")?
        .eligible = false;
    assert!(matches!(
        service
            .join_team(JoinTeam {
                team_id: team.id,
                system_id: SystemId::new(),
                actor: owner,
                now_epoch_millis: 1,
            })
            .await,
        Err(TeamServiceError::Ineligible)
    ));
    assert!(matches!(
        service.leave_team(team.id, SystemId::new(), owner, 2).await,
        Err(TeamServiceError::OwnerMustTransfer)
    ));
    Ok(())
}

#[tokio::test]
async fn aggregate_and_ladder_expose_projection_lag_and_ties() -> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = service(repository.clone());
    let team_id = TeamId::new();
    repository
        .aggregate
        .lock()
        .map_err(|_| "aggregate lock")?
        .replace(TeamAggregateResource {
            team_id,
            period_start_epoch_millis: 0,
            period_end_epoch_millis: 1_000,
            generation_energy_wh: 10_000,
            normalized_generation_wh_per_kw: Some(2_000),
            coverage_basis_points: 9_500,
            source_sequence: 10,
            source_checkpoint: 10,
            projected_at_epoch_millis: 900,
            projection_lag_events: 0,
            stale: false,
        });
    let aggregate = service.aggregate(team_id, 0, 1_000, 1_000).await?;
    assert!(!aggregate.stale);

    let candidates = [candidate(team_id, "A"), candidate(team_id, "B")];
    repository
        .candidates
        .lock()
        .map_err(|_| "candidate lock")?
        .extend(candidates);
    let ladder = service
        .ladder(
            team_id,
            ComparisonMetric::NormalizedGeneration,
            0,
            1_000,
            1_000,
        )
        .await?;
    assert_eq!(
        ladder.iter().map(|entry| entry.rank).collect::<Vec<_>>(),
        [1, 1]
    );
    Ok(())
}

fn service(repository: Arc<FakeRepository>) -> TeamService<FakeRepository> {
    TeamService::new(
        repository,
        TeamPolicy {
            maximum_memberships_per_system: 10,
            minimum_ranking_coverage_basis_points: 9_000,
            maximum_projection_age_millis: 500,
            exclude_stale_projections: true,
        },
    )
}

fn candidate(team_id: TeamId, name: &str) -> ComparisonCandidate {
    ComparisonCandidate {
        system_id: SystemId::new(),
        team_id: Some(team_id),
        display_name: name.to_owned(),
        visibility: Visibility::Public,
        authorized: false,
        eligibility: RankingState::Eligible,
        coverage_basis_points: 9_500,
        performance: vec![CapacityPerformanceSlice {
            generation_wh: 5_000,
            effective_capacity_watts: 5_000,
        }],
        projection_updated_at_epoch_millis: 900,
    }
}

struct FakeRepository {
    teams: Mutex<Vec<TeamResource>>,
    memberships: Mutex<Vec<TeamMembershipResource>>,
    eligibility: Mutex<TeamEligibility>,
    aggregate: Mutex<Option<TeamAggregateResource>>,
    candidates: Mutex<Vec<ComparisonCandidate>>,
}

impl Default for FakeRepository {
    fn default() -> Self {
        Self {
            teams: Mutex::new(Vec::new()),
            memberships: Mutex::new(Vec::new()),
            eligibility: Mutex::new(TeamEligibility {
                eligible: true,
                coverage_basis_points: 9_500,
                visible: true,
            }),
            aggregate: Mutex::new(None),
            candidates: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl TeamRepository for FakeRepository {
    async fn save_team(&self, team: TeamResource) -> Result<(), TeamServiceError> {
        let mut teams = self
            .teams
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?;
        if let Some(current) = teams.iter_mut().find(|current| current.id == team.id) {
            *current = team;
        } else {
            teams.push(team);
        }
        Ok(())
    }

    async fn team(&self, id: TeamId) -> Result<Option<TeamResource>, TeamServiceError> {
        Ok(self
            .teams
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?
            .iter()
            .find(|team| team.id == id)
            .cloned())
    }

    async fn save_membership(
        &self,
        membership: TeamMembershipResource,
    ) -> Result<(), TeamServiceError> {
        let mut memberships = self
            .memberships
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?;
        if let Some(current) = memberships
            .iter_mut()
            .find(|current| current.id == membership.id)
        {
            *current = membership;
        } else {
            memberships.push(membership);
        }
        Ok(())
    }

    async fn membership(
        &self,
        team_id: TeamId,
        system_id: SystemId,
    ) -> Result<Option<TeamMembershipResource>, TeamServiceError> {
        Ok(self
            .memberships
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?
            .iter()
            .find(|membership| membership.team_id == team_id && membership.system_id == system_id)
            .cloned())
    }

    async fn active_membership_count(&self, system_id: SystemId) -> Result<u16, TeamServiceError> {
        u16::try_from(
            self.memberships
                .lock()
                .map_err(|_| TeamServiceError::Unavailable)?
                .iter()
                .filter(|membership| membership.system_id == system_id && membership.active)
                .count(),
        )
        .map_err(|_| TeamServiceError::Overflow)
    }

    async fn system_eligibility(
        &self,
        _actor: UserId,
        _system_id: SystemId,
    ) -> Result<TeamEligibility, TeamServiceError> {
        Ok(*self
            .eligibility
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?)
    }

    async fn team_aggregate(
        &self,
        _team_id: TeamId,
        _period_start_epoch_millis: i64,
        _period_end_epoch_millis: i64,
    ) -> Result<Option<TeamAggregateResource>, TeamServiceError> {
        Ok(self
            .aggregate
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?
            .clone())
    }

    async fn ladder_candidates(
        &self,
        _team_id: TeamId,
        _period_start_epoch_millis: i64,
        _period_end_epoch_millis: i64,
    ) -> Result<Vec<ComparisonCandidate>, TeamServiceError> {
        Ok(self
            .candidates
            .lock()
            .map_err(|_| TeamServiceError::Unavailable)?
            .clone())
    }
}
