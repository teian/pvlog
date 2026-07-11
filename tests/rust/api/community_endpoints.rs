use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::community_router;
use pvlog_application::{
    CommunityActivity, CommunityCatalogError, CommunityCatalogUseCases, CommunityLocationPrecision,
    CommunitySearchFilter, CommunitySystemResource,
};
use pvlog_domain::{SystemId, UserId};
use std::{error::Error, sync::Arc};
use tower::ServiceExt as _;

#[tokio::test]
async fn community_endpoints_cover_search_and_favourite_lifecycle() -> Result<(), Box<dyn Error>> {
    let actor = UserId::new();
    let system = SystemId::new();
    let app = community_router(Arc::new(Stub { system }), 1_000).layer(Extension(actor));
    for (method, uri, expected) in [
        (
            Method::GET,
            "/api/v1/community/systems?countryCode=DE&activeOnly=true".to_owned(),
            StatusCode::OK,
        ),
        (
            Method::GET,
            "/api/v1/users/me/favourites".to_owned(),
            StatusCode::OK,
        ),
        (
            Method::POST,
            format!("/api/v1/users/me/favourites/{system}"),
            StatusCode::CREATED,
        ),
        (
            Method::DELETE,
            format!("/api/v1/users/me/favourites/{system}"),
            StatusCode::NO_CONTENT,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), expected);
    }
    Ok(())
}

struct Stub {
    system: SystemId,
}

#[async_trait]
impl CommunityCatalogUseCases for Stub {
    async fn search(
        &self,
        _filter: CommunitySearchFilter,
        _now_epoch_millis: i64,
    ) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError> {
        Ok(vec![resource(self.system)])
    }

    async fn add_favourite(
        &self,
        _actor: UserId,
        system_id: SystemId,
    ) -> Result<CommunitySystemResource, CommunityCatalogError> {
        Ok(resource(system_id))
    }

    async fn remove_favourite(
        &self,
        _actor: UserId,
        _system_id: SystemId,
    ) -> Result<(), CommunityCatalogError> {
        Ok(())
    }

    async fn favourites(
        &self,
        _actor: UserId,
        _now_epoch_millis: i64,
    ) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError> {
        Ok(vec![resource(self.system)])
    }
}

fn resource(system_id: SystemId) -> CommunitySystemResource {
    CommunitySystemResource {
        system_id,
        display_name: "Roof PV".to_owned(),
        country_code: Some("DE".to_owned()),
        location_label: Some("Berlin".to_owned()),
        location_precision: CommunityLocationPrecision::Locality,
        capacity_watts: 6_000,
        activity: CommunityActivity::Active,
        projection_age_millis: 100,
        projection_lag_events: 0,
        stale: false,
    }
}
