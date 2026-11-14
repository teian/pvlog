//! Storage-backed device events.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    DEFAULT_DEVICE_EVENT_LIMIT, DeviceEventApiError, DeviceEventApiUseCases, DeviceEventDetails,
    DeviceEventInput, DeviceEventQuery, DeviceEventResponse, DeviceEventsAccepted,
    EnergySampleResponse,
};
use pvlog_application::Clock;
use pvlog_domain::{AccountId, ApiCredentialId, DataSourceId, SystemId};
use pvlog_storage::{
    DataSourceRepository, DatabaseTarget, DeviceEventFilter, DeviceEventRecord,
    DeviceEventRepository, EnergySampleFilter, IngestRepository, ManagementRepository,
};
use uuid::Uuid;

/// Prefix of the event key of an event that carries the source's identifier.
const SOURCE_IDENTIFIER: &str = "id:";

/// The account-scoped repositories device events work with.
struct Repositories {
    events: Box<dyn DeviceEventRepository>,
    data_sources: Box<dyn DataSourceRepository>,
    samples: Box<dyn IngestRepository>,
}

/// Serves the device event endpoints from account storage.
pub struct StorageDeviceEventApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageDeviceEventApi {
    /// Creates the device event API over the given database target, management repository, and clock.
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

    /// Opens the repositories of the system's account; an unregistered system is `NotFound`.
    async fn repositories(&self, system_id: SystemId) -> Result<Repositories, DeviceEventApiError> {
        let account_id = self
            .management
            .system_registry(system_id)
            .await
            .map_err(|_| DeviceEventApiError::Unavailable)?
            .ok_or(DeviceEventApiError::NotFound)?
            .account_id;
        self.account_repositories(account_id)
            .await
            .ok_or(DeviceEventApiError::Unavailable)
    }

    /// Opens the device event, data source and energy sample repositories of an account.
    async fn account_repositories(&self, account_id: AccountId) -> Option<Repositories> {
        match &self.target {
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
                    Some(Repositories {
                        events: Box::new(pvlog_storage::SqliteDeviceEventRepository::new(
                            account.clone(),
                        )),
                        data_sources: Box::new(pvlog_storage::SqliteDataSourceRepository::new(
                            account.clone(),
                        )),
                        samples: Box::new(pvlog_storage::SqliteIngestRepository::new(account)),
                    })
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
                    Some(Repositories {
                        events: Box::new(pvlog_storage::PostgresDeviceEventRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        data_sources: Box::new(pvlog_storage::PostgresDataSourceRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        samples: Box::new(pvlog_storage::PostgresIngestRepository::new(
                            url.clone(),
                            account_id,
                        )),
                    })
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id);
                    None
                }
            }
        }
    }

    /// Reads the events that match a query.
    async fn read(
        repository: &dyn DeviceEventRepository,
        system_id: SystemId,
        device_key: Option<&str>,
        query: &DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError> {
        Ok(repository
            .device_events(
                system_id,
                DeviceEventFilter {
                    device_key,
                    from: query.from,
                    to: query.to,
                    limit: query.limit.unwrap_or(DEFAULT_DEVICE_EVENT_LIMIT),
                },
            )
            .await
            .map_err(|_| DeviceEventApiError::Unavailable)?
            .into_iter()
            .map(response)
            .collect())
    }
}

/// The identity of an event within its device: the source's identifier, or
/// the event's time and code when the source has none.
pub(crate) fn event_key(event_id: Option<&str>, occurred_at: i64, code: Option<i64>) -> String {
    event_id.map_or_else(
        || {
            format!(
                "at:{occurred_at}:{}",
                code.map_or_default(|code| code.to_string())
            )
        },
        |id| format!("{SOURCE_IDENTIFIER}{id}"),
    )
}

/// Converts a stored event into its API form.
fn response(event: DeviceEventRecord) -> DeviceEventResponse {
    DeviceEventResponse {
        id: event.id,
        device_id: event.device_key,
        event_id: event
            .event_key
            .strip_prefix(SOURCE_IDENTIFIER)
            .map(ToOwned::to_owned),
        occurred_at_epoch_millis: event.occurred_at,
        code: event.code,
        message: event.message,
        details: DeviceEventDetails {
            event_type: event.details.event_type,
            category: event.details.category,
            group: event.details.group,
            old_value: event.details.old_value,
            new_value: event.details.new_value,
            user_group: event.details.user_group,
        },
        data_source_id: event.data_source_id.map(DataSourceId::as_uuid),
        received_at_epoch_millis: event.received_at,
    }
}

#[async_trait]
impl DeviceEventApiUseCases for StorageDeviceEventApi {
    /// Stores uploaded events once each. An event uploaded through a data
    /// source's credential is attributed to that data source.
    async fn store(
        &self,
        system_id: SystemId,
        credential_id: Option<ApiCredentialId>,
        events: Vec<DeviceEventInput>,
    ) -> Result<DeviceEventsAccepted, DeviceEventApiError> {
        let repositories = self.repositories(system_id).await?;
        let data_source_id = match credential_id {
            Some(credential_id) => repositories
                .data_sources
                .data_source_by_credential(credential_id)
                .await
                .map_err(|_| DeviceEventApiError::Unavailable)?
                // A credential is restricted to its system; a source of
                // another system must never be credited.
                .filter(|record| record.system_id == system_id)
                .map(|record| record.id),
            None => None,
        };
        let received_at = i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default();
        let records: Vec<DeviceEventRecord> = events
            .iter()
            .map(|event| DeviceEventRecord {
                id: Uuid::now_v7(),
                system_id,
                device_key: event.device_id.clone(),
                event_key: event_key(
                    event.event_id.as_deref(),
                    event.occurred_at_epoch_millis,
                    event.code,
                ),
                data_source_id,
                occurred_at: event.occurred_at_epoch_millis,
                code: event.code,
                message: event.message.clone(),
                details: pvlog_storage::DeviceEventDetails {
                    event_type: event.details.event_type.clone(),
                    category: event.details.category.clone(),
                    group: event.details.group.clone(),
                    old_value: event.details.old_value.clone(),
                    new_value: event.details.new_value.clone(),
                    user_group: event.details.user_group.clone(),
                },
                received_at,
            })
            .collect();
        let stored = repositories
            .events
            .save_device_events(&records)
            .await
            .map_err(|error| match error {
                pvlog_storage::AccountRepositoryError::InvalidRecord(_) => {
                    DeviceEventApiError::InvalidInput("events")
                }
                _ => DeviceEventApiError::Unavailable,
            })?;
        let total = u64::try_from(records.len()).unwrap_or(u64::MAX);
        Ok(DeviceEventsAccepted {
            stored,
            unchanged: total.saturating_sub(stored),
        })
    }

    /// Lists the events of a system, newest first.
    async fn list(
        &self,
        system_id: SystemId,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError> {
        let repositories = self.repositories(system_id).await?;
        Self::read(
            repositories.events.as_ref(),
            system_id,
            query.device_id.as_deref(),
            &query,
        )
        .await
    }

    /// Lists the events of the device whose serial the inverter carries. An
    /// inverter without a serial has none.
    async fn list_for_inverter(
        &self,
        system_id: SystemId,
        inverter_id: Uuid,
        query: DeviceEventQuery,
    ) -> Result<Vec<DeviceEventResponse>, DeviceEventApiError> {
        let repositories = self.repositories(system_id).await?;
        let Some(serial) = repositories
            .events
            .inverter_serial(system_id, inverter_id)
            .await
            .map_err(|_| DeviceEventApiError::Unavailable)?
            .ok_or(DeviceEventApiError::NotFound)?
        else {
            return Ok(Vec::new());
        };
        Self::read(
            repositories.events.as_ref(),
            system_id,
            Some(&serial),
            &query,
        )
        .await
    }

    /// Lists the archive energy samples of a system, oldest first.
    async fn list_energy_samples(
        &self,
        system_id: SystemId,
        query: DeviceEventQuery,
    ) -> Result<Vec<EnergySampleResponse>, DeviceEventApiError> {
        let repositories = self.repositories(system_id).await?;
        Ok(repositories
            .samples
            .energy_samples(
                system_id,
                EnergySampleFilter {
                    device_key: query.device_id.as_deref(),
                    from: query.from,
                    to: query.to,
                    limit: query
                        .limit
                        .unwrap_or(pvlog_api::DEFAULT_ENERGY_SAMPLE_LIMIT),
                },
            )
            .await
            .map_err(|_| DeviceEventApiError::Unavailable)?
            .into_iter()
            .map(|sample| EnergySampleResponse {
                device_id: sample.device_key,
                measured_at_epoch_millis: sample.measured_at,
                energy_lifetime_wh: sample.energy_lifetime_wh,
                power_watts: sample.power_watts,
                data_source_id: sample.data_source_id.map(DataSourceId::as_uuid),
                received_at_epoch_millis: sample.received_at,
            })
            .collect())
    }
}
