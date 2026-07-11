use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_compatibility::{
    LegacyAuth, LegacyTeam, LegacyTeamError, LegacyTeamUseCases, legacy_team_router,
};
use serde::Deserialize;
use std::{error::Error, sync::Arc};
use time::macros::date;
use tower::ServiceExt as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    get: Case,
    join: Case,
    leave: Case,
    membership_limit: Case,
    ineligible: Case,
    owner_leave: Case,
}

#[derive(Deserialize)]
struct Case {
    path: String,
    status: u16,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!("../fixtures/pvoutput/teams-golden.json"))
}

#[tokio::test]
async fn legacy_team_routes_preserve_fields_membership_rules_and_errors()
-> Result<(), Box<dyn Error>> {
    let app = legacy_team_router(Arc::new(FakeTeams));
    let cases = golden()?;
    for case in [
        cases.get,
        cases.join,
        cases.leave,
        cases.membership_limit,
        cases.ineligible,
        cases.owner_leave,
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(&case.path).body(Body::empty())?)
            .await?;
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await?;
        assert_eq!(status, StatusCode::from_u16(case.status)?);
        assert_eq!(std::str::from_utf8(&body)?, case.body);
    }
    Ok(())
}

struct FakeTeams;

#[async_trait]
impl LegacyTeamUseCases for FakeTeams {
    async fn team(
        &self,
        auth: &LegacyAuth,
        team_id: Option<u64>,
    ) -> Result<LegacyTeam, LegacyTeamError> {
        authorize(auth)?;
        assert_eq!(team_id, Some(7));
        Ok(LegacyTeam {
            name: "PV Friends".to_owned(),
            size_watts: 108_131,
            average_size_watts: 2_845,
            systems: 38,
            generated_wh: 402_662_180,
            outputs: 46_887,
            average_energy_wh: 9_014,
            kind: "Community".to_owned(),
            description: "Sunny roofs".to_owned(),
            created_at: date!(2010 - 09 - 17),
        })
    }

    async fn join(
        &self,
        auth: &LegacyAuth,
        team_id: Option<u64>,
    ) -> Result<String, LegacyTeamError> {
        authorize(auth)?;
        match team_id {
            Some(10) => Err(LegacyTeamError::MembershipLimit),
            Some(5) => Err(LegacyTeamError::InsufficientOutputs),
            _ => Ok("PV Friends".to_owned()),
        }
    }

    async fn leave(
        &self,
        auth: &LegacyAuth,
        team_id: Option<u64>,
    ) -> Result<String, LegacyTeamError> {
        authorize(auth)?;
        match team_id {
            Some(42) => Err(LegacyTeamError::OwnerCannotLeave("Owner Team".to_owned())),
            _ => Ok("PV Friends".to_owned()),
        }
    }
}

fn authorize(auth: &LegacyAuth) -> Result<(), LegacyTeamError> {
    if auth.api_key == "write-key" && auth.system_id == 42 {
        Ok(())
    } else {
        Err(LegacyTeamError::Unauthorized)
    }
}
