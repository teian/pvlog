//! Storage-backed device events.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    DEFAULT_DEVICE_EVENT_LIMIT, DeviceEventApiError, DeviceEventApiUseCases, DeviceEventInput,
    DeviceEventQuery, DeviceEventResponse, DeviceEventsAccepted,
};
use pvlog_application::Clock;
use pvlog_domain::{AccountId, ApiCredentialId, DataSourceId, SystemId};
use pvlog_storage::{
    DataSourceRepository, DatabaseTarget, DeviceEventFilter, DeviceEventRecord,
    DeviceEventRepository, ManagementRepository,
};
use uuid::Uuid;

/// Prefix of the event key of an event that carries the source's identifier.
const SOURCE_IDENTIFIER: &str = "id:";

/// The account-scoped repositories device events work with.
struct Repositories {
    events: Box<dyn DeviceEventRepository>,
    data_sources: Box<dyn DataSourceRepository>,
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

    /// Opens the device event and data source repositories of an account.
    async fn account_repositories(&self, account_id: AccountId) -> Option<Repositories> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = pvlog_storage::SqliteAccountPoolRouter::new(
                        management_path.clone(),
                        accounts_dir.clone(),
                        pvlog_storage::SqliteAccountPoolConfig::default(),
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
                            account,
                        )),
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
fn event_key(event: &DeviceEventInput) -> String {
    event.event_id.as_ref().map_or_else(
        || {
            format!(
                "at:{}:{}",
                event.occurred_at_epoch_millis,
                event.code.map(|code| code.to_string()).unwrap_or_default()
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
                event_key: event_key(event),
                data_source_id,
                occurred_at: event.occurred_at_epoch_millis,
                code: event.code,
                message: event.message.clone(),
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
}
