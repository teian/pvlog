//! Storage-backed measurement-channel bindings.
//!
//! Channels are discovered by ingestion and bound by operators. Binding never
//! rewrites history: the stored readings keep their channel, and changing a
//! target only changes which equipment those readings are attributed to from
//! then on, for every query over the whole range.

use std::sync::Arc;

use crate::channel_aggregation::{aggregate_value, quarter_hour_rollups};
use async_trait::async_trait;
use pvlog_api::{
    ChannelAggregateBucket, ChannelAggregateQuery, ChannelAggregateResponse, ChannelApiError,
    ChannelApiUseCases, ChannelBindingInput, ChannelResponse,
};
use pvlog_application::Clock;
use pvlog_domain::{SystemId, UserId};
use pvlog_storage::{
    AccountRepositoryError, ChannelBindingRecord, ChannelRecord, ChannelRepository, DatabaseTarget,
    ManagementRepository,
};

/// Opens the measurement channel repository of the account that owns a system.
///
/// Returns `None` when the system is unknown or its storage cannot be reached.
pub async fn channel_repository(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
    system_id: SystemId,
) -> Option<Box<dyn ChannelRepository>> {
    let account_id = management
        .system_registry(system_id)
        .await
        .ok()??
        .account_id;
    match target {
        DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } => {
            #[cfg(feature = "sqlite")]
            {
                let account = pvlog_storage::SqliteAccountPoolRouter::shared(
                    management_path.clone(),
                    accounts_dir.clone(),
                )
                .ok()?
                .route(account_id)
                .await
                .ok()?;
                Some(Box::new(pvlog_storage::SqliteChannelRepository::new(
                    account,
                )))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = (management_path, accounts_dir, account_id);
                None
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Some(Box::new(pvlog_storage::PostgresChannelRepository::new(
                    url.clone(),
                    account_id,
                )))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = (url, account_id);
                None
            }
        }
    }
}

/// Serves the channel binding endpoints from account storage.
pub struct StorageChannelApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageChannelApi {
    /// Creates the channel API over the given database target, management repository, and clock.
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target,
            management,
            clock,
        }
    }

    /// Opens the channel repository of the system's account; an unregistered
    /// system is `NotFound`.
    async fn repository(
        &self,
        system_id: SystemId,
    ) -> Result<Box<dyn ChannelRepository>, ChannelApiError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| ChannelApiError::Unavailable)?
            .ok_or(ChannelApiError::NotFound)?;
        channel_repository(&self.target, self.management.as_ref(), system_id)
            .await
            .ok_or(ChannelApiError::Unavailable)
    }

    /// Returns the clock's current time in epoch milliseconds.
    fn now(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }
}

#[async_trait]
impl ChannelApiUseCases for StorageChannelApi {
    /// Reads the aggregates of the active channels bound to one target. A
    /// channel without an aggregate in the range is left out.
    async fn aggregates(
        &self,
        system_id: SystemId,
        query: ChannelAggregateQuery,
    ) -> Result<Vec<ChannelAggregateResponse>, ChannelApiError> {
        let repository = self.repository(system_id).await?;
        let channels = repository
            .channels(system_id)
            .await
            .map_err(|_| ChannelApiError::Unavailable)?;
        let rollups = if query.resolution == "fifteen_minute" {
            // Quarter-hours are not stored; they come from the values.
            let bound: Vec<ChannelRecord> = channels
                .iter()
                .filter(|channel| {
                    channel.target_kind == query.target_kind && channel.target_id == query.target_id
                })
                .cloned()
                .collect();
            quarter_hour_rollups(repository.as_ref(), system_id, &bound, query.from, query.to)
                .await
                .map_err(|_| ChannelApiError::Unavailable)?
        } else {
            repository
                .target_channel_rollups(
                    system_id,
                    &query.target_kind,
                    query.target_id,
                    &query.resolution,
                    query.from,
                    query.to,
                )
                .await
                .map_err(|_| ChannelApiError::Unavailable)?
        };
        Ok(channels
            .into_iter()
            .filter_map(|channel| {
                let buckets: Vec<ChannelAggregateBucket> = rollups
                    .iter()
                    .filter(|rollup| rollup.channel_id == channel.id)
                    .map(|rollup| ChannelAggregateBucket {
                        start_epoch_millis: rollup.bucket_start,
                        end_epoch_millis: rollup.bucket_end,
                        value: aggregate_value(&channel.aggregation, rollup),
                        minimum: rollup.value_min,
                        maximum: rollup.value_max,
                        point_count: rollup.point_count,
                    })
                    .collect();
                (!buckets.is_empty()).then_some(ChannelAggregateResponse {
                    channel_key: channel.channel_key,
                    display_name: channel.display_name,
                    unit: channel.unit,
                    scale: channel.scale,
                    aggregation: channel.aggregation,
                    buckets,
                })
            })
            .collect())
    }

    /// Lists the system's active channels, unassigned ones first.
    async fn list(&self, system_id: SystemId) -> Result<Vec<ChannelResponse>, ChannelApiError> {
        Ok(self
            .repository(system_id)
            .await?
            .channels(system_id)
            .await
            .map_err(|_| ChannelApiError::Unavailable)?
            .into_iter()
            .map(channel_response)
            .collect())
    }

    /// Updates a channel's binding and returns the refreshed channel. The
    /// aggregates built under the previous binding are dropped and rebuilt.
    async fn bind(
        &self,
        _actor: UserId,
        system_id: SystemId,
        channel_key: &str,
        input: ChannelBindingInput,
    ) -> Result<ChannelResponse, ChannelApiError> {
        self.repository(system_id)
            .await?
            .bind_channel(
                system_id,
                channel_key,
                &ChannelBindingRecord {
                    target_kind: input.target_kind,
                    target_id: input.target_id,
                    display_name: input.display_name,
                    aggregation: input.aggregation,
                },
                self.now(),
            )
            .await
            .map_err(|error| match error {
                AccountRepositoryError::InvalidRecord("channel target") => {
                    ChannelApiError::InvalidInput("unknown_target_equipment")
                }
                AccountRepositoryError::InvalidRecord(_) => {
                    ChannelApiError::InvalidInput("invalid_binding")
                }
                _ => ChannelApiError::Unavailable,
            })?
            .map(channel_response)
            .ok_or(ChannelApiError::NotFound)
    }

    /// Retires a channel while keeping its stored readings; an unknown channel is `NotFound`.
    async fn remove(&self, system_id: SystemId, channel_key: &str) -> Result<(), ChannelApiError> {
        if self
            .repository(system_id)
            .await?
            .retire_channel(system_id, channel_key, self.now())
            .await
            .map_err(|_| ChannelApiError::Unavailable)?
        {
            Ok(())
        } else {
            Err(ChannelApiError::NotFound)
        }
    }
}

/// Converts a stored channel into its API form.
fn channel_response(channel: ChannelRecord) -> ChannelResponse {
    ChannelResponse {
        id: channel.id.as_uuid(),
        channel_key: channel.channel_key,
        display_name: channel.display_name,
        data_type: channel.data_type,
        unit: channel.unit,
        scale: channel.scale,
        aggregation: channel.aggregation,
        target_kind: channel.target_kind,
        target_id: channel.target_id,
        discovered_at_epoch_millis: channel.discovered_at,
        last_value_at_epoch_millis: channel.last_value_at,
        last_value: channel.last_value,
    }
}
