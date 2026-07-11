use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use pvlog_api::teams_router;
use pvlog_application::{
    ComparisonEntry, ComparisonMetric, CreateTeam, JoinTeam, TeamAccess, TeamAggregateResource,
    TeamMembershipResource, TeamResource, TeamServiceError, TeamUseCases,
};
use pvlog_domain::{AccountId, SystemId, TeamId, TeamMembershipId, UserId};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn modern_team_routes_cover_lifecycle_membership_aggregate_and_ladder()
-> Result<(), Box<dyn Error>> {
    let owner = UserId::new();
    let team = TeamId::new();
    let system = SystemId::new();
    let app = teams_router(Arc::new(Stub { team, system })).layer(Extension(owner));
    let requests = [
        request(
            Method::POST,
            "/api/v1/teams".to_owned(),
            Some(format!(
                r#"{{"accountId":"{}","name":"Solar Club","access":"public"}}"#,
                AccountId::new()
            )),
            None,
            StatusCode::CREATED,
        ),
        request(
            Method::POST,
            format!("/api/v1/teams/{team}/ownership"),
            Some(format!(r#"{{"newOwnerUserId":"{}"}}"#, UserId::new())),
            Some("\"1\""),
            StatusCode::OK,
        ),
        request(
            Method::POST,
            format!("/api/v1/teams/{team}/memberships"),
            Some(format!(
                r#"{{"systemId":"{system}","effectiveFromEpochMillis":1000}}"#
            )),
            None,
            StatusCode::CREATED,
        ),
        request(
            Method::DELETE,
            format!("/api/v1/teams/{team}/memberships/{system}"),
            None,
            None,
            StatusCode::NO_CONTENT,
        ),
        request(
            Method::GET,
            format!(
                "/api/v1/teams/{team}/aggregate?periodStartEpochMillis=0&periodEndEpochMillis=1000&nowEpochMillis=1000"
            ),
            None,
            None,
            StatusCode::OK,
        ),
        request(
            Method::GET,
            format!(
                "/api/v1/teams/{team}/ladder?periodStartEpochMillis=0&periodEndEpochMillis=1000&nowEpochMillis=1000"
            ),
            None,
            None,
            StatusCode::OK,
        ),
    ];
    for item in requests {
        let (request, expected) = item?;
        let response = app.clone().oneshot(request).await?;
        assert_eq!(response.status(), expected);
    }
    Ok(())
}

fn request(
    method: Method,
    uri: String,
    body: Option<String>,
    if_match: Option<&str>,
    expected: StatusCode,
) -> Result<(Request<Body>, StatusCode), axum::http::Error> {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(value) = if_match {
        builder = builder.header(header::IF_MATCH, value);
    }
    Ok((
        builder.body(body.map_or_else(Body::empty, Body::from))?,
        expected,
    ))
}

struct Stub {
    team: TeamId,
    system: SystemId,
}

#[async_trait]
impl TeamUseCases for Stub {
    async fn create_team(&self, command: CreateTeam) -> Result<TeamResource, TeamServiceError> {
        Ok(team(self.team, command.account_id, command.actor))
    }

    async fn transfer_ownership(
        &self,
        team_id: TeamId,
        _actor: UserId,
        new_owner: UserId,
        _expected_version: u64,
    ) -> Result<TeamResource, TeamServiceError> {
        Ok(team(team_id, AccountId::new(), new_owner))
    }

    async fn join_team(
        &self,
        command: JoinTeam,
    ) -> Result<TeamMembershipResource, TeamServiceError> {
        Ok(TeamMembershipResource {
            id: TeamMembershipId::new(),
            team_id: command.team_id,
            system_id: command.system_id,
            joined_by: command.actor,
            effective_from_epoch_millis: command.now_epoch_millis,
            active: true,
        })
    }

    async fn leave_team(
        &self,
        team_id: TeamId,
        system_id: SystemId,
        _actor: UserId,
        _now_epoch_millis: i64,
    ) -> Result<(), TeamServiceError> {
        assert_eq!((team_id, system_id), (self.team, self.system));
        Ok(())
    }

    async fn aggregate(
        &self,
        team_id: TeamId,
        period_start_epoch_millis: i64,
        period_end_epoch_millis: i64,
        _now_epoch_millis: i64,
    ) -> Result<TeamAggregateResource, TeamServiceError> {
        Ok(TeamAggregateResource {
            team_id,
            period_start_epoch_millis,
            period_end_epoch_millis,
            generation_energy_wh: 1_000,
            normalized_generation_wh_per_kw: Some(200),
            coverage_basis_points: 10_000,
            source_sequence: 1,
            source_checkpoint: 1,
            projected_at_epoch_millis: 900,
            projection_lag_events: 0,
            stale: false,
        })
    }

    async fn ladder(
        &self,
        _team_id: TeamId,
        _metric: ComparisonMetric,
        _period_start_epoch_millis: i64,
        _period_end_epoch_millis: i64,
        _now_epoch_millis: i64,
    ) -> Result<Vec<ComparisonEntry>, TeamServiceError> {
        Ok(Vec::new())
    }
}

fn team(id: TeamId, account_id: AccountId, owner: UserId) -> TeamResource {
    TeamResource {
        id,
        account_id,
        name: "Solar Club".to_owned(),
        description: None,
        access: TeamAccess::Public,
        owner_user_id: owner,
        version: 1,
    }
}
