use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pvlog_application::{CommunityCatalogError, CommunityCatalogUseCases, CommunitySearchFilter};
use pvlog_domain::{SystemId, UserId};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
struct CommunityState {
    service: Arc<dyn CommunityCatalogUseCases>,
    now_epoch_millis: i64,
}

pub fn community_router(
    service: Arc<dyn CommunityCatalogUseCases>,
    now_epoch_millis: i64,
) -> Router {
    Router::new()
        .route("/api/v1/community/systems", get(search))
        .route("/api/v1/users/me/favourites", get(favourites))
        .route(
            "/api/v1/users/me/favourites/{system_id}",
            post(add_favourite).delete(remove_favourite),
        )
        .with_state(CommunityState {
            service,
            now_epoch_millis,
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchParameters {
    query: Option<String>,
    country_code: Option<String>,
    location: Option<String>,
    minimum_capacity_watts: Option<u64>,
    maximum_capacity_watts: Option<u64>,
    active_only: Option<bool>,
}

async fn search(
    State(state): State<CommunityState>,
    Query(parameters): Query<SearchParameters>,
) -> Result<Response, CommunityApiError> {
    Ok(Json(
        state
            .service
            .search(
                CommunitySearchFilter {
                    query: parameters.query,
                    country_code: parameters.country_code,
                    location: parameters.location,
                    minimum_capacity_watts: parameters.minimum_capacity_watts,
                    maximum_capacity_watts: parameters.maximum_capacity_watts,
                    active_only: parameters.active_only.unwrap_or(true),
                },
                state.now_epoch_millis,
            )
            .await?,
    )
    .into_response())
}

async fn favourites(
    State(state): State<CommunityState>,
    actor: Option<Extension<UserId>>,
) -> Result<Response, CommunityApiError> {
    Ok(Json(
        state
            .service
            .favourites(actor_id(actor)?, state.now_epoch_millis)
            .await?,
    )
    .into_response())
}

async fn add_favourite(
    State(state): State<CommunityState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
) -> Result<Response, CommunityApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .service
                .add_favourite(actor_id(actor)?, system_id)
                .await?,
        ),
    )
        .into_response())
}

async fn remove_favourite(
    State(state): State<CommunityState>,
    actor: Option<Extension<UserId>>,
    Path(system_id): Path<SystemId>,
) -> Result<StatusCode, CommunityApiError> {
    state
        .service
        .remove_favourite(actor_id(actor)?, system_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn actor_id(actor: Option<Extension<UserId>>) -> Result<UserId, CommunityApiError> {
    actor
        .map(|Extension(actor)| actor)
        .ok_or(CommunityApiError::Forbidden)
}

enum CommunityApiError {
    Forbidden,
    Domain(CommunityCatalogError),
}

impl From<CommunityCatalogError> for CommunityApiError {
    fn from(value: CommunityCatalogError) -> Self {
        Self::Domain(value)
    }
}

impl IntoResponse for CommunityApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Domain(CommunityCatalogError::InvalidFilter) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Domain(CommunityCatalogError::NotFound) => StatusCode::NOT_FOUND,
            Self::Domain(
                CommunityCatalogError::InvalidProjection | CommunityCatalogError::Unavailable,
            ) => StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response()
    }
}
