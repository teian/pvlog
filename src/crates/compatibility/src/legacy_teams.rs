//! Legacy team query and membership adapters.

use crate::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    csv_record, format_legacy_date, parse_legacy_auth,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use thiserror::Error;
use time::Date;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyTeam {
    pub name: String,
    pub size_watts: i64,
    pub average_size_watts: i64,
    pub systems: u64,
    pub generated_wh: i64,
    pub outputs: u64,
    pub average_energy_wh: i64,
    pub kind: String,
    pub description: String,
    pub created_at: Date,
}

#[async_trait]
pub trait LegacyTeamUseCases: Send + Sync {
    async fn team(
        &self,
        auth: &LegacyAuth,
        team_id: Option<u64>,
    ) -> Result<LegacyTeam, LegacyTeamError>;
    async fn join(
        &self,
        auth: &LegacyAuth,
        team_id: Option<u64>,
    ) -> Result<String, LegacyTeamError>;
    async fn leave(
        &self,
        auth: &LegacyAuth,
        team_id: Option<u64>,
    ) -> Result<String, LegacyTeamError>;
}

#[derive(Clone)]
struct TeamState {
    service: Arc<dyn LegacyTeamUseCases>,
}

pub fn legacy_team_router(service: Arc<dyn LegacyTeamUseCases>) -> Router {
    Router::new()
        .route("/service/r2/getteam.jsp", get(get_team))
        .route("/service/r2/jointeam.jsp", get(join_team))
        .route("/service/r2/leaveteam.jsp", get(leave_team))
        .with_state(TeamState { service })
}

async fn get_team(
    State(state): State<TeamState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, TeamApiError> {
    let (auth, team_id) = request(&headers, query)?;
    let team = state.service.team(&auth, team_id).await?;
    let fields = [
        team.name,
        team.size_watts.to_string(),
        team.average_size_watts.to_string(),
        team.systems.to_string(),
        team.generated_wh.to_string(),
        team.outputs.to_string(),
        team.average_energy_wh.to_string(),
        team.kind,
        team.description,
        format_legacy_date(team.created_at),
    ];
    Ok(text_response(
        StatusCode::OK,
        &csv_record(fields.iter().map(|field| Some(field.as_str()))),
    ))
}

async fn join_team(
    State(state): State<TeamState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, TeamApiError> {
    let (auth, team_id) = request(&headers, query)?;
    let name = state.service.join(&auth, team_id).await?;
    Ok(text_response(
        StatusCode::OK,
        &format!("You have joined team {name}"),
    ))
}

async fn leave_team(
    State(state): State<TeamState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, TeamApiError> {
    let (auth, team_id) = request(&headers, query)?;
    let name = state.service.leave(&auth, team_id).await?;
    Ok(text_response(
        StatusCode::OK,
        &format!("You have left team {name}"),
    ))
}

fn request(
    headers: &HeaderMap,
    query: Option<String>,
) -> Result<(LegacyAuth, Option<u64>), TeamApiError> {
    let parameters = LegacyParameters::parse(query.unwrap_or_default().as_bytes())?;
    let auth = parse_legacy_auth(LegacyMethod::Get, headers, &parameters)?;
    let team_id = parameters
        .get("tid")
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| TeamApiError::bad("Team Id invalid"))?;
    Ok((auth, team_id))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LegacyTeamError {
    #[error("credentials are invalid")]
    Unauthorized,
    #[error("team was not found")]
    NotFound,
    #[error("team is inaccessible")]
    Inaccessible,
    #[error("system is already a member")]
    AlreadyMember(String),
    #[error("system is not a member")]
    NotMember(String),
    #[error("system owns the team")]
    OwnerCannotLeave(String),
    #[error("system has reached the membership limit")]
    MembershipLimit,
    #[error("system is not eligible")]
    InsufficientOutputs,
    #[error("team storage is unavailable")]
    Unavailable,
}

enum TeamApiError {
    Legacy(LegacyError),
    Protocol(LegacyProtocolError),
    Service(LegacyTeamError),
}

impl TeamApiError {
    fn bad(detail: impl Into<String>) -> Self {
        Self::Legacy(LegacyError {
            kind: LegacyErrorKind::BadRequest,
            detail: detail.into(),
        })
    }
}

impl From<LegacyProtocolError> for TeamApiError {
    fn from(value: LegacyProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LegacyTeamError> for TeamApiError {
    fn from(value: LegacyTeamError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for TeamApiError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            Self::Legacy(error) => (error.kind.status(), error.body()),
            Self::Protocol(error) => (400, format!("Bad request 400: {error}")),
            Self::Service(LegacyTeamError::Unauthorized) => {
                (403, "Forbidden 403: Invalid API Key".to_owned())
            }
            Self::Service(LegacyTeamError::NotFound) => {
                (400, "Bad request 400: No team found".to_owned())
            }
            Self::Service(LegacyTeamError::Inaccessible) => {
                (403, "Forbidden 403: Team is inaccessible".to_owned())
            }
            Self::Service(LegacyTeamError::AlreadyMember(name)) => (
                400,
                format!("Bad request 400: You are already a member of {name}"),
            ),
            Self::Service(LegacyTeamError::NotMember(name)) => (
                400,
                format!("Bad request 400: You are not a member of team {name}"),
            ),
            Self::Service(LegacyTeamError::OwnerCannotLeave(name)) => (
                400,
                format!("Bad request 400: You cannot leave the team {name} which you started"),
            ),
            Self::Service(LegacyTeamError::MembershipLimit) => (
                400,
                "Bad request 400: Your system cannot join more than 10 teams".to_owned(),
            ),
            Self::Service(LegacyTeamError::InsufficientOutputs) => (
                400,
                "Bad request 400: Your system must have at least 5 outputs to join a team"
                    .to_owned(),
            ),
            Self::Service(LegacyTeamError::Unavailable) => (503, "Service unavailable".to_owned()),
        };
        text_response(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
            &body,
        )
    }
}

fn text_response(status: StatusCode, body: &str) -> Response {
    let mut response = Response::new(Body::from(body.to_owned()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
