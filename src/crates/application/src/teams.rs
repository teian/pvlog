//! Team lifecycle, membership, projected aggregates, and ladder orchestration.

use crate::{
    ComparisonCandidate, ComparisonEntry, ComparisonError, ComparisonMetric, ComparisonPolicy,
    build_ladder,
};
use async_trait::async_trait;
use pvlog_domain::{AccountId, SystemId, TeamId, TeamMembershipId, UserId};
use serde::Serialize;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamAccess {
    Private,
    Unlisted,
    Public,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamResource {
    pub id: TeamId,
    pub account_id: AccountId,
    pub name: String,
    pub description: Option<String>,
    pub access: TeamAccess,
    pub owner_user_id: UserId,
    pub version: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMembershipResource {
    pub id: TeamMembershipId,
    pub team_id: TeamId,
    pub system_id: SystemId,
    pub joined_by: UserId,
    pub effective_from_epoch_millis: i64,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamAggregateResource {
    pub team_id: TeamId,
    pub period_start_epoch_millis: i64,
    pub period_end_epoch_millis: i64,
    pub generation_energy_wh: u64,
    pub normalized_generation_wh_per_kw: Option<u64>,
    pub coverage_basis_points: u16,
    pub source_sequence: u64,
    pub source_checkpoint: u64,
    pub projected_at_epoch_millis: i64,
    pub projection_lag_events: u64,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeamEligibility {
    pub eligible: bool,
    pub coverage_basis_points: u16,
    pub visible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateTeam {
    pub account_id: AccountId,
    pub actor: UserId,
    pub name: String,
    pub description: Option<String>,
    pub access: TeamAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JoinTeam {
    pub team_id: TeamId,
    pub system_id: SystemId,
    pub actor: UserId,
    pub now_epoch_millis: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeamPolicy {
    pub maximum_memberships_per_system: u16,
    pub minimum_ranking_coverage_basis_points: u16,
    pub maximum_projection_age_millis: u64,
    pub exclude_stale_projections: bool,
}

#[async_trait]
pub trait TeamRepository: Send + Sync {
    async fn save_team(&self, team: TeamResource) -> Result<(), TeamServiceError>;
    async fn team(&self, id: TeamId) -> Result<Option<TeamResource>, TeamServiceError>;
    async fn save_membership(
        &self,
        membership: TeamMembershipResource,
    ) -> Result<(), TeamServiceError>;
    async fn membership(
        &self,
        team_id: TeamId,
        system_id: SystemId,
    ) -> Result<Option<TeamMembershipResource>, TeamServiceError>;
    async fn active_membership_count(&self, system_id: SystemId) -> Result<u16, TeamServiceError>;
    async fn system_eligibility(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<TeamEligibility, TeamServiceError>;
    async fn team_aggregate(
        &self,
        team_id: TeamId,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
    ) -> Result<Option<TeamAggregateResource>, TeamServiceError>;
    async fn ladder_candidates(
        &self,
        team_id: TeamId,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
    ) -> Result<Vec<ComparisonCandidate>, TeamServiceError>;
}

#[async_trait]
pub trait TeamUseCases: Send + Sync {
    async fn create_team(&self, command: CreateTeam) -> Result<TeamResource, TeamServiceError>;
    async fn transfer_ownership(
        &self,
        team_id: TeamId,
        actor: UserId,
        new_owner: UserId,
        expected_version: u64,
    ) -> Result<TeamResource, TeamServiceError>;
    async fn join_team(
        &self,
        command: JoinTeam,
    ) -> Result<TeamMembershipResource, TeamServiceError>;
    async fn leave_team(
        &self,
        team_id: TeamId,
        system_id: SystemId,
        actor: UserId,
        now_epoch_millis: i64,
    ) -> Result<(), TeamServiceError>;
    async fn aggregate(
        &self,
        team_id: TeamId,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
        now_epoch_millis: i64,
    ) -> Result<TeamAggregateResource, TeamServiceError>;
    async fn ladder(
        &self,
        team_id: TeamId,
        metric: ComparisonMetric,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
        now_epoch_millis: i64,
    ) -> Result<Vec<ComparisonEntry>, TeamServiceError>;
}

#[derive(Clone)]
pub struct TeamService<R> {
    repository: Arc<R>,
    policy: TeamPolicy,
}

impl<R> TeamService<R>
where
    R: TeamRepository,
{
    #[must_use]
    pub const fn new(repository: Arc<R>, policy: TeamPolicy) -> Self {
        Self { repository, policy }
    }
}

#[async_trait]
impl<R> TeamUseCases for TeamService<R>
where
    R: TeamRepository,
{
    async fn create_team(&self, command: CreateTeam) -> Result<TeamResource, TeamServiceError> {
        let name = command.name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(TeamServiceError::InvalidInput);
        }
        let team = TeamResource {
            id: TeamId::new(),
            account_id: command.account_id,
            name: name.to_owned(),
            description: command.description,
            access: command.access,
            owner_user_id: command.actor,
            version: 1,
        };
        self.repository.save_team(team.clone()).await?;
        Ok(team)
    }

    async fn transfer_ownership(
        &self,
        team_id: TeamId,
        actor: UserId,
        new_owner: UserId,
        expected_version: u64,
    ) -> Result<TeamResource, TeamServiceError> {
        let mut team = self
            .repository
            .team(team_id)
            .await?
            .ok_or(TeamServiceError::NotFound)?;
        if team.owner_user_id != actor {
            return Err(TeamServiceError::Forbidden);
        }
        if team.version != expected_version || new_owner == actor {
            return Err(TeamServiceError::Conflict);
        }
        team.owner_user_id = new_owner;
        team.version = team
            .version
            .checked_add(1)
            .ok_or(TeamServiceError::Overflow)?;
        self.repository.save_team(team.clone()).await?;
        Ok(team)
    }

    async fn join_team(
        &self,
        command: JoinTeam,
    ) -> Result<TeamMembershipResource, TeamServiceError> {
        let team = self
            .repository
            .team(command.team_id)
            .await?
            .ok_or(TeamServiceError::NotFound)?;
        if team.access == TeamAccess::Private && team.owner_user_id != command.actor {
            return Err(TeamServiceError::Forbidden);
        }
        if self
            .repository
            .membership(command.team_id, command.system_id)
            .await?
            .is_some_and(|membership| membership.active)
        {
            return Err(TeamServiceError::AlreadyMember);
        }
        if self
            .repository
            .active_membership_count(command.system_id)
            .await?
            >= self.policy.maximum_memberships_per_system
        {
            return Err(TeamServiceError::MembershipLimit);
        }
        let eligibility = self
            .repository
            .system_eligibility(command.actor, command.system_id)
            .await?;
        if !eligibility.visible
            || !eligibility.eligible
            || eligibility.coverage_basis_points < self.policy.minimum_ranking_coverage_basis_points
        {
            return Err(TeamServiceError::Ineligible);
        }
        let membership = TeamMembershipResource {
            id: TeamMembershipId::new(),
            team_id: command.team_id,
            system_id: command.system_id,
            joined_by: command.actor,
            effective_from_epoch_millis: command.now_epoch_millis,
            active: true,
        };
        self.repository.save_membership(membership.clone()).await?;
        Ok(membership)
    }

    async fn leave_team(
        &self,
        team_id: TeamId,
        system_id: SystemId,
        actor: UserId,
        now_epoch_millis: i64,
    ) -> Result<(), TeamServiceError> {
        let team = self
            .repository
            .team(team_id)
            .await?
            .ok_or(TeamServiceError::NotFound)?;
        if team.owner_user_id == actor {
            return Err(TeamServiceError::OwnerMustTransfer);
        }
        let mut membership = self
            .repository
            .membership(team_id, system_id)
            .await?
            .filter(|membership| membership.active)
            .ok_or(TeamServiceError::NotFound)?;
        if membership.joined_by != actor {
            return Err(TeamServiceError::Forbidden);
        }
        membership.active = false;
        membership.effective_from_epoch_millis = now_epoch_millis;
        self.repository.save_membership(membership).await
    }

    async fn aggregate(
        &self,
        team_id: TeamId,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
        now_epoch_millis: i64,
    ) -> Result<TeamAggregateResource, TeamServiceError> {
        if period_end_epoch_millis <= period_start_epoch_millis {
            return Err(TeamServiceError::InvalidInput);
        }
        let aggregate = self
            .repository
            .team_aggregate(team_id, period_start_epoch_millis, period_end_epoch_millis)
            .await?
            .ok_or(TeamServiceError::NotFound)?;
        validate_aggregate(aggregate, now_epoch_millis, self.policy)
    }

    async fn ladder(
        &self,
        team_id: TeamId,
        metric: ComparisonMetric,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
        now_epoch_millis: i64,
    ) -> Result<Vec<ComparisonEntry>, TeamServiceError> {
        if period_end_epoch_millis <= period_start_epoch_millis {
            return Err(TeamServiceError::InvalidInput);
        }
        build_ladder(
            &self
                .repository
                .ladder_candidates(team_id, period_start_epoch_millis, period_end_epoch_millis)
                .await?,
            Some(team_id),
            metric,
            now_epoch_millis,
            ComparisonPolicy {
                minimum_coverage_basis_points: self.policy.minimum_ranking_coverage_basis_points,
                maximum_projection_age_millis: self.policy.maximum_projection_age_millis,
            },
        )
        .map_err(TeamServiceError::Comparison)
    }
}

fn validate_aggregate(
    mut aggregate: TeamAggregateResource,
    now: i64,
    policy: TeamPolicy,
) -> Result<TeamAggregateResource, TeamServiceError> {
    if aggregate.period_end_epoch_millis <= aggregate.period_start_epoch_millis
        || aggregate.coverage_basis_points > 10_000
        || aggregate.source_sequence > aggregate.source_checkpoint
        || aggregate.projected_at_epoch_millis > now
    {
        return Err(TeamServiceError::InvalidProjection);
    }
    let age = u64::try_from(now - aggregate.projected_at_epoch_millis)
        .map_err(|_| TeamServiceError::InvalidProjection)?;
    aggregate.projection_lag_events = aggregate
        .source_checkpoint
        .saturating_sub(aggregate.source_sequence);
    aggregate.stale =
        age > policy.maximum_projection_age_millis || aggregate.projection_lag_events > 0;
    if aggregate.stale && policy.exclude_stale_projections {
        return Err(TeamServiceError::ProjectionStale);
    }
    Ok(aggregate)
}

#[derive(Debug, Error)]
pub enum TeamServiceError {
    #[error("team input is invalid")]
    InvalidInput,
    #[error("team resource was not found")]
    NotFound,
    #[error("team action is forbidden")]
    Forbidden,
    #[error("team version conflicts with current state")]
    Conflict,
    #[error("system is already an active team member")]
    AlreadyMember,
    #[error("system reached the configured membership limit")]
    MembershipLimit,
    #[error("system is not eligible for team ranking")]
    Ineligible,
    #[error("team owner must transfer ownership before leaving")]
    OwnerMustTransfer,
    #[error("team projection is invalid")]
    InvalidProjection,
    #[error("team projection is too stale")]
    ProjectionStale,
    #[error("team ladder failed: {0}")]
    Comparison(ComparisonError),
    #[error("team arithmetic overflowed")]
    Overflow,
    #[error("team repository is unavailable")]
    Unavailable,
}
