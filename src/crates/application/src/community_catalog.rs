//! Privacy-safe management-catalog discovery and favourites.

use async_trait::async_trait;
use pvlog_domain::{AccountId, SystemId, UserId, Visibility};
use serde::Serialize;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunityActivity {
    Active,
    Archived,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunityLocationPrecision {
    Hidden,
    Country,
    Region,
    Locality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommunityProjection {
    pub account_id: AccountId,
    pub system_id: SystemId,
    pub display_name: String,
    pub country_code: Option<String>,
    pub location_label: Option<String>,
    pub location_precision: CommunityLocationPrecision,
    pub capacity_watts: u64,
    pub visibility: Visibility,
    pub activity: CommunityActivity,
    pub source_sequence: u64,
    pub source_checkpoint: u64,
    pub projected_at_epoch_millis: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommunityProjectionEvent {
    Upsert(CommunityProjection),
    Invalidate {
        account_id: AccountId,
        system_id: SystemId,
        source_sequence: u64,
    },
    Delete {
        account_id: AccountId,
        system_id: SystemId,
        source_sequence: u64,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommunitySearchFilter {
    pub query: Option<String>,
    pub country_code: Option<String>,
    pub location: Option<String>,
    pub minimum_capacity_watts: Option<u64>,
    pub maximum_capacity_watts: Option<u64>,
    pub active_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySystemResource {
    pub system_id: SystemId,
    pub display_name: String,
    pub country_code: Option<String>,
    pub location_label: Option<String>,
    pub location_precision: CommunityLocationPrecision,
    pub capacity_watts: u64,
    pub activity: CommunityActivity,
    pub projection_age_millis: u64,
    pub projection_lag_events: u64,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommunityCatalogPolicy {
    pub public_discovery_enabled: bool,
    pub maximum_projection_age_millis: u64,
    pub exclude_stale_results: bool,
}

#[async_trait]
pub trait CommunityCatalogRepository: Send + Sync {
    async fn apply_projection_event(
        &self,
        event: CommunityProjectionEvent,
    ) -> Result<(), CommunityCatalogError>;
    async fn search_projections(
        &self,
        filter: &CommunitySearchFilter,
    ) -> Result<Vec<CommunityProjection>, CommunityCatalogError>;
    async fn visible_projection(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<Option<CommunityProjection>, CommunityCatalogError>;
    async fn add_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<(), CommunityCatalogError>;
    async fn remove_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<bool, CommunityCatalogError>;
    async fn favourite_projections(
        &self,
        actor: UserId,
    ) -> Result<Vec<CommunityProjection>, CommunityCatalogError>;
}

#[async_trait]
pub trait CommunityCatalogUseCases: Send + Sync {
    async fn search(
        &self,
        filter: CommunitySearchFilter,
        now_epoch_millis: i64,
    ) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError>;
    async fn add_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<CommunitySystemResource, CommunityCatalogError>;
    async fn remove_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<(), CommunityCatalogError>;
    async fn favourites(
        &self,
        actor: UserId,
        now_epoch_millis: i64,
    ) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError>;
}

#[derive(Clone)]
pub struct CommunityCatalogService<R> {
    repository: Arc<R>,
    policy: CommunityCatalogPolicy,
    now_epoch_millis: i64,
}

impl<R> CommunityCatalogService<R>
where
    R: CommunityCatalogRepository,
{
    #[must_use]
    pub const fn new(
        repository: Arc<R>,
        policy: CommunityCatalogPolicy,
        now_epoch_millis: i64,
    ) -> Self {
        Self {
            repository,
            policy,
            now_epoch_millis,
        }
    }

    /// Applies one account event to the management catalog. Non-public upserts become invalidations.
    /// # Errors
    /// Returns an error for unsafe payloads or repository failures.
    pub async fn apply_projection_event(
        &self,
        event: CommunityProjectionEvent,
    ) -> Result<(), CommunityCatalogError> {
        let event = match event {
            CommunityProjectionEvent::Upsert(projection) => {
                validate_projection(&projection)?;
                if projection.visibility == Visibility::Public {
                    CommunityProjectionEvent::Upsert(projection)
                } else {
                    CommunityProjectionEvent::Invalidate {
                        account_id: projection.account_id,
                        system_id: projection.system_id,
                        source_sequence: projection.source_sequence,
                    }
                }
            }
            event => event,
        };
        self.repository.apply_projection_event(event).await
    }
}

#[async_trait]
impl<R> CommunityCatalogUseCases for CommunityCatalogService<R>
where
    R: CommunityCatalogRepository,
{
    async fn search(
        &self,
        filter: CommunitySearchFilter,
        now_epoch_millis: i64,
    ) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError> {
        validate_filter(&filter)?;
        if !self.policy.public_discovery_enabled {
            return Ok(Vec::new());
        }
        let projections = self.repository.search_projections(&filter).await?;
        resources(projections, now_epoch_millis, self.policy)
    }

    async fn add_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<CommunitySystemResource, CommunityCatalogError> {
        let projection = self
            .repository
            .visible_projection(actor, system_id)
            .await?
            .ok_or(CommunityCatalogError::NotFound)?;
        let resource = resource(projection, self.now_epoch_millis, self.policy)?
            .ok_or(CommunityCatalogError::NotFound)?;
        self.repository.add_favourite(actor, system_id).await?;
        Ok(resource)
    }

    async fn remove_favourite(
        &self,
        actor: UserId,
        system_id: SystemId,
    ) -> Result<(), CommunityCatalogError> {
        if self.repository.remove_favourite(actor, system_id).await? {
            Ok(())
        } else {
            Err(CommunityCatalogError::NotFound)
        }
    }

    async fn favourites(
        &self,
        actor: UserId,
        now_epoch_millis: i64,
    ) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError> {
        resources(
            self.repository.favourite_projections(actor).await?,
            now_epoch_millis,
            self.policy,
        )
    }
}

fn resources(
    projections: Vec<CommunityProjection>,
    now: i64,
    policy: CommunityCatalogPolicy,
) -> Result<Vec<CommunitySystemResource>, CommunityCatalogError> {
    let mut result = projections
        .into_iter()
        .filter_map(|projection| resource(projection, now, policy).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_unstable_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.system_id.as_uuid().cmp(&right.system_id.as_uuid()))
    });
    Ok(result)
}

fn resource(
    projection: CommunityProjection,
    now: i64,
    policy: CommunityCatalogPolicy,
) -> Result<Option<CommunitySystemResource>, CommunityCatalogError> {
    validate_projection(&projection)?;
    if projection.visibility != Visibility::Public {
        return Ok(None);
    }
    let age = u64::try_from(now - projection.projected_at_epoch_millis)
        .map_err(|_| CommunityCatalogError::InvalidProjection)?;
    let lag = projection
        .source_checkpoint
        .saturating_sub(projection.source_sequence);
    let stale = age > policy.maximum_projection_age_millis || lag > 0;
    if stale && policy.exclude_stale_results {
        return Ok(None);
    }
    Ok(Some(CommunitySystemResource {
        system_id: projection.system_id,
        display_name: projection.display_name,
        country_code: projection.country_code,
        location_label: projection.location_label,
        location_precision: projection.location_precision,
        capacity_watts: projection.capacity_watts,
        activity: projection.activity,
        projection_age_millis: age,
        projection_lag_events: lag,
        stale,
    }))
}

fn validate_projection(projection: &CommunityProjection) -> Result<(), CommunityCatalogError> {
    if projection.display_name.trim().is_empty()
        || projection.source_sequence > projection.source_checkpoint
        || projection.country_code.as_ref().is_some_and(|country| {
            country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_uppercase())
        })
        || projection.visibility != Visibility::Public
            && (projection.country_code.is_some()
                || projection.location_label.is_some()
                || projection.location_precision != CommunityLocationPrecision::Hidden)
    {
        Err(CommunityCatalogError::InvalidProjection)
    } else {
        Ok(())
    }
}

fn validate_filter(filter: &CommunitySearchFilter) -> Result<(), CommunityCatalogError> {
    if filter.country_code.as_ref().is_some_and(|country| {
        country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_uppercase())
    }) || filter
        .minimum_capacity_watts
        .zip(filter.maximum_capacity_watts)
        .is_some_and(|(minimum, maximum)| minimum > maximum)
    {
        Err(CommunityCatalogError::InvalidFilter)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CommunityCatalogError {
    #[error("community projection payload is unsafe or inconsistent")]
    InvalidProjection,
    #[error("community search filter is invalid")]
    InvalidFilter,
    #[error("community system was not found or is no longer visible")]
    NotFound,
    #[error("community catalog storage is unavailable")]
    Unavailable,
}
