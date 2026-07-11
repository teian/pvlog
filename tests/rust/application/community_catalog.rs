use async_trait::async_trait;
use pvlog_application::{
    CommunityActivity, CommunityCatalogError, CommunityCatalogPolicy, CommunityCatalogRepository,
    CommunityCatalogService, CommunityCatalogUseCases, CommunityLocationPrecision,
    CommunityProjection, CommunityProjectionEvent, CommunitySearchFilter,
};
use pvlog_domain::{AccountId, SystemId, UserId, Visibility};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn search_exposes_only_fresh_public_projection_fields() -> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::new(vec![projection()]));
    let service = service(repository);
    let resources = service
        .search(
            CommunitySearchFilter {
                query: Some("roof".to_owned()),
                country_code: Some("DE".to_owned()),
                active_only: true,
                ..CommunitySearchFilter::default()
            },
            1_000,
        )
        .await?;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].country_code.as_deref(), Some("DE"));
    assert_eq!(resources[0].location_label.as_deref(), Some("Berlin"));
    assert_eq!(resources[0].projection_lag_events, 0);
    assert!(!resources[0].stale);
    Ok(())
}

#[tokio::test]
async fn privacy_reduction_becomes_invalidation_before_discovery() -> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::new(Vec::new()));
    let service = service(repository.clone());
    let mut private = projection();
    private.visibility = Visibility::Private;
    private.country_code = None;
    private.location_label = None;
    private.location_precision = CommunityLocationPrecision::Hidden;
    service
        .apply_projection_event(CommunityProjectionEvent::Upsert(private))
        .await?;
    let events = repository.events.lock().map_err(|_| "event lock")?;
    assert!(matches!(
        events[0],
        CommunityProjectionEvent::Invalidate { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn favourites_do_not_retain_access_after_projection_disappears() -> Result<(), Box<dyn Error>>
{
    let candidate = projection();
    let system_id = candidate.system_id;
    let repository = Arc::new(FakeRepository::new(vec![candidate]));
    let service = service(repository.clone());
    let actor = UserId::new();
    service.add_favourite(actor, system_id).await?;
    assert_eq!(service.favourites(actor, 1_000).await?.len(), 1);

    repository
        .projections
        .lock()
        .map_err(|_| "projection lock")?
        .clear();
    assert!(service.favourites(actor, 1_000).await?.is_empty());
    Ok(())
}

fn service(repository: Arc<FakeRepository>) -> CommunityCatalogService<FakeRepository> {
    CommunityCatalogService::new(
        repository,
        CommunityCatalogPolicy {
            public_discovery_enabled: true,
            maximum_projection_age_millis: 500,
            exclude_stale_results: true,
        },
        1_000,
    )
}

fn projection() -> CommunityProjection {
    CommunityProjection {
        account_id: AccountId::new(),
        system_id: SystemId::new(),
        display_name: "Roof PV".to_owned(),
        country_code: Some("DE".to_owned()),
        location_label: Some("Berlin".to_owned()),
        location_precision: CommunityLocationPrecision::Locality,
        capacity_watts: 6_000,
        visibility: Visibility::Public,
        activity: CommunityActivity::Active,
        source_sequence: 10,
        source_checkpoint: 10,
        projected_at_epoch_millis: 900,
    }
}

struct FakeRepository {
    projections: Mutex<Vec<CommunityProjection>>,
    events: Mutex<Vec<CommunityProjectionEvent>>,
    favourites: Mutex<Vec<(UserId, SystemId)>>,
}

impl FakeRepository {
    fn new(projections: Vec<CommunityProjection>) -> Self {
        Self {
            projections: Mutex::new(projections),
            events: Mutex::new(Vec::new()),
            favourites: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl CommunityCatalogRepository for FakeRepository {
    async fn apply_projection_event(
        &self,
        event: CommunityProjectionEvent,
    ) -> Result<(), CommunityCatalogError> {
        self.events
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?
            .push(event);
        Ok(())
    }

    async fn search_projections(
        &self,
        filter: &CommunitySearchFilter,
    ) -> Result<Vec<CommunityProjection>, CommunityCatalogError> {
        Ok(self
            .projections
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?
            .iter()
            .filter(|projection| {
                filter.query.as_ref().is_none_or(|query| {
                    projection
                        .display_name
                        .to_lowercase()
                        .contains(&query.to_lowercase())
                }) && filter
                    .country_code
                    .as_ref()
                    .is_none_or(|country| projection.country_code.as_ref() == Some(country))
            })
            .cloned()
            .collect())
    }

    async fn visible_projection(
        &self,
        _actor: UserId,
        system_id: SystemId,
    ) -> Result<Option<CommunityProjection>, CommunityCatalogError> {
        Ok(self
            .projections
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?
            .iter()
            .find(|projection| projection.system_id == system_id)
            .cloned())
    }

    async fn add_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<(), CommunityCatalogError> {
        let mut favourites = self
            .favourites
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?;
        if !favourites.contains(&(actor, system_id)) {
            favourites.push((actor, system_id));
        }
        Ok(())
    }

    async fn remove_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<bool, CommunityCatalogError> {
        let mut favourites = self
            .favourites
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?;
        let before = favourites.len();
        favourites.retain(|item| *item != (actor, system_id));
        Ok(before != favourites.len())
    }

    async fn favourite_projections(
        &self,
        actor: UserId,
    ) -> Result<Vec<CommunityProjection>, CommunityCatalogError> {
        let favourites = self
            .favourites
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?;
        let projections = self
            .projections
            .lock()
            .map_err(|_| CommunityCatalogError::Unavailable)?;
        Ok(projections
            .iter()
            .filter(|projection| favourites.contains(&(actor, projection.system_id)))
            .cloned()
            .collect())
    }
}
