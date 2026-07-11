use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{
    ComparisonMetric, CreateTeam, JoinTeam, TeamAccess, TeamServiceError, TeamUseCases,
};
use pvlog_domain::{AccountId, SystemId, TeamId, UserId};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
struct TeamState {
    service: Arc<dyn TeamUseCases>,
}

pub fn teams_router(service: Arc<dyn TeamUseCases>) -> Router {
    Router::new()
        .route("/api/v1/teams", post(create))
        .route("/api/v1/teams/{team_id}/ownership", post(transfer))
        .route("/api/v1/teams/{team_id}/memberships", post(join))
        .route(
            "/api/v1/teams/{team_id}/memberships/{system_id}",
            axum::routing::delete(leave),
        )
        .route("/api/v1/teams/{team_id}/aggregate", get(aggregate))
        .route("/api/v1/teams/{team_id}/ladder", get(ladder))
        .with_state(TeamState { service })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBody {
    account_id: AccountId,
    name: String,
    description: Option<String>,
    access: TeamAccessBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TeamAccessBody {
    Private,
    Unlisted,
    Public,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferBody {
    new_owner_user_id: UserId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JoinBody {
    system_id: SystemId,
    effective_from_epoch_millis: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeriodParameters {
    period_start_epoch_millis: i64,
    period_end_epoch_millis: i64,
    now_epoch_millis: i64,
    metric: Option<String>,
}

async fn create(
    State(state): State<TeamState>,
    actor: Option<Extension<UserId>>,
    Json(body): Json<CreateBody>,
) -> Result<Response, TeamApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .service
                .create_team(CreateTeam {
                    account_id: body.account_id,
                    actor: actor_id(actor)?,
                    name: body.name,
                    description: body.description,
                    access: match body.access {
                        TeamAccessBody::Private => TeamAccess::Private,
                        TeamAccessBody::Unlisted => TeamAccess::Unlisted,
                        TeamAccessBody::Public => TeamAccess::Public,
                    },
                })
                .await?,
        ),
    )
        .into_response())
}

async fn transfer(
    State(state): State<TeamState>,
    actor: Option<Extension<UserId>>,
    Path(team_id): Path<TeamId>,
    headers: HeaderMap,
    Json(body): Json<TransferBody>,
) -> Result<Response, TeamApiError> {
    Ok(Json(
        state
            .service
            .transfer_ownership(
                team_id,
                actor_id(actor)?,
                body.new_owner_user_id,
                expected_version(&headers)?,
            )
            .await?,
    )
    .into_response())
}

async fn join(
    State(state): State<TeamState>,
    actor: Option<Extension<UserId>>,
    Path(team_id): Path<TeamId>,
    Json(body): Json<JoinBody>,
) -> Result<Response, TeamApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .service
                .join_team(JoinTeam {
                    team_id,
                    system_id: body.system_id,
                    actor: actor_id(actor)?,
                    now_epoch_millis: body.effective_from_epoch_millis,
                })
                .await?,
        ),
    )
        .into_response())
}

async fn leave(
    State(state): State<TeamState>,
    actor: Option<Extension<UserId>>,
    Path((team_id, system_id)): Path<(TeamId, SystemId)>,
) -> Result<StatusCode, TeamApiError> {
    state
        .service
        .leave_team(team_id, system_id, actor_id(actor)?, unix_now()?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn aggregate(
    State(state): State<TeamState>,
    Path(team_id): Path<TeamId>,
    Query(parameters): Query<PeriodParameters>,
) -> Result<Response, TeamApiError> {
    Ok(Json(
        state
            .service
            .aggregate(
                team_id,
                parameters.period_start_epoch_millis,
                parameters.period_end_epoch_millis,
                parameters.now_epoch_millis,
            )
            .await?,
    )
    .into_response())
}

async fn ladder(
    State(state): State<TeamState>,
    Path(team_id): Path<TeamId>,
    Query(parameters): Query<PeriodParameters>,
) -> Result<Response, TeamApiError> {
    let metric = match parameters
        .metric
        .as_deref()
        .unwrap_or("normalized_generation")
    {
        "total_generation" => ComparisonMetric::TotalGeneration,
        "normalized_generation" => ComparisonMetric::NormalizedGeneration,
        _ => return Err(TeamApiError::Invalid),
    };
    Ok(Json(
        state
            .service
            .ladder(
                team_id,
                metric,
                parameters.period_start_epoch_millis,
                parameters.period_end_epoch_millis,
                parameters.now_epoch_millis,
            )
            .await?,
    )
    .into_response())
}

fn actor_id(actor: Option<Extension<UserId>>) -> Result<UserId, TeamApiError> {
    actor
        .map(|Extension(actor)| actor)
        .ok_or(TeamApiError::Forbidden)
}

fn expected_version(headers: &HeaderMap) -> Result<u64, TeamApiError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse().ok())
        .ok_or(TeamApiError::PreconditionRequired)
}

fn unix_now() -> Result<i64, TeamApiError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .ok_or(TeamApiError::Unavailable)
}

enum TeamApiError {
    Invalid,
    Forbidden,
    PreconditionRequired,
    Unavailable,
    Domain(TeamServiceError),
}

impl From<TeamServiceError> for TeamApiError {
    fn from(value: TeamServiceError) -> Self {
        Self::Domain(value)
    }
}

impl IntoResponse for TeamApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Invalid | Self::Domain(TeamServiceError::InvalidInput) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::Forbidden | Self::Domain(TeamServiceError::Forbidden) => StatusCode::FORBIDDEN,
            Self::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::Domain(TeamServiceError::NotFound) => StatusCode::NOT_FOUND,
            Self::Domain(TeamServiceError::Conflict) => StatusCode::PRECONDITION_FAILED,
            Self::Domain(
                TeamServiceError::AlreadyMember
                | TeamServiceError::MembershipLimit
                | TeamServiceError::Ineligible
                | TeamServiceError::OwnerMustTransfer,
            ) => StatusCode::CONFLICT,
            Self::Unavailable
            | Self::Domain(
                TeamServiceError::InvalidProjection
                | TeamServiceError::ProjectionStale
                | TeamServiceError::Overflow
                | TeamServiceError::Unavailable
                | TeamServiceError::Comparison(_),
            ) => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
