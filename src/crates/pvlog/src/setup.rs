//! Runtime setup status and first-data verification backed by account storage.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    DataState, FirstDataResponse, ForecastApiUseCases, ForecastResourceScope, SetupApiError,
    SetupApiUseCases, SetupStatusResponse, VerifiedReading,
};
use pvlog_application::{Clock, SetupFacts, setup_status};
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::{
    DataSourceRepository, DatabaseTarget, PostgresDataSourceRepository, PostgresSetupRepository,
    SetupFactsRecord, SetupRepository, SqliteAccountPoolRouter, SqliteDataSourceRepository,
    SqliteSetupRepository,
};

/// Production setup use cases.
pub struct StorageSetupApi {
    target: DatabaseTarget,
    /// Judges forecast readiness the same way the forecast view does.
    forecasts: Arc<dyn ForecastApiUseCases>,
    clock: Arc<dyn Clock>,
}

struct Repositories {
    setup: Box<dyn SetupRepository>,
    sources: Box<dyn DataSourceRepository>,
}

impl StorageSetupApi {
    #[must_use]
    pub fn new(
        target: DatabaseTarget,
        forecasts: Arc<dyn ForecastApiUseCases>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target,
            forecasts,
            clock,
        }
    }

    fn now(&self) -> Result<i64, SetupApiError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| SetupApiError::Unavailable)
    }

    async fn repositories(&self, account_id: AccountId) -> Result<Repositories, SetupApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = SqliteAccountPoolRouter::shared(
                        management_path.clone(),
                        accounts_dir.clone(),
                    )
                    .map_err(|_| SetupApiError::Unavailable)?
                    .route(account_id)
                    .await
                    .map_err(|_| SetupApiError::Unavailable)?;
                    Ok(Repositories {
                        setup: Box::new(SqliteSetupRepository::new(account.clone())),
                        sources: Box::new(SqliteDataSourceRepository::new(account)),
                    })
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id);
                    Err(SetupApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Ok(Repositories {
                        setup: Box::new(PostgresSetupRepository::new(url.clone(), account_id)),
                        sources: Box::new(PostgresDataSourceRepository::new(
                            url.clone(),
                            account_id,
                        )),
                    })
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id);
                    Err(SetupApiError::Unavailable)
                }
            }
        }
    }

    async fn facts(
        &self,
        repositories: &Repositories,
        system_id: SystemId,
        now: i64,
    ) -> Result<SetupFactsRecord, SetupApiError> {
        repositories
            .setup
            .setup_facts(system_id, now)
            .await
            .map_err(|_| SetupApiError::Unavailable)?
            .ok_or(SetupApiError::NotFound)
    }
}

#[async_trait]
impl SetupApiUseCases for StorageSetupApi {
    async fn status(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<SetupStatusResponse, SetupApiError> {
        let now = self.now()?;
        let repositories = self.repositories(account_id).await?;
        let facts = self.facts(&repositories, system_id, now).await?;
        // Readiness that cannot be determined is reported as unknown, never as done.
        let forecast_ready = self
            .forecasts
            .input_completeness(ForecastResourceScope::System {
                account_id,
                system_id,
            })
            .await
            .ok()
            .map(|completeness| completeness.complete);
        Ok(SetupStatusResponse {
            system_id,
            items: setup_status(&SetupFacts {
                has_location: facts.has_location,
                has_declared_capacity: facts.has_declared_capacity,
                strings: facts.strings,
                data_sources: facts.data_sources,
                has_reading: facts.has_reading,
                unassigned_channels: facts.unassigned_channels,
                has_effective_tariff: facts.has_effective_tariff,
                forecast_ready,
            }),
            data_state: DataState::at(facts.last_received_at, facts.status_interval_seconds, now),
            last_received_at_epoch_millis: facts.last_received_at,
        })
    }

    async fn first_data(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<FirstDataResponse, SetupApiError> {
        let repositories = self.repositories(account_id).await?;
        // Fails with not found for a system that does not exist.
        self.facts(&repositories, system_id, self.now()?).await?;
        let real = repositories
            .setup
            .latest_reading(system_id)
            .await
            .map_err(|_| SetupApiError::Unavailable)?;
        if let Some(reading) = real {
            return Ok(FirstDataResponse {
                system_id,
                received: true,
                reading: Some(VerifiedReading {
                    id: reading.observation_id.as_uuid(),
                    measured_at_epoch_millis: reading.measured_at,
                    received_at_epoch_millis: reading.received_at,
                    generation_power_watts: reading.generation_power_watts,
                    generation_energy_wh: reading.generation_energy_wh,
                    consumption_power_watts: reading.consumption_power_watts,
                    consumption_energy_wh: reading.consumption_energy_wh,
                    message: reading.message,
                    data_source_id: reading.data_source_id,
                    data_source_name: reading.data_source_name,
                    test: false,
                }),
            });
        }
        let test = repositories
            .sources
            .latest_test_reading(system_id)
            .await
            .map_err(|_| SetupApiError::Unavailable)?;
        let reading = match test {
            None => None,
            Some(test) => {
                let name = match test.data_source_id {
                    Some(id) => repositories
                        .sources
                        .data_source(system_id, id)
                        .await
                        .map_err(|_| SetupApiError::Unavailable)?
                        .map(|source| source.name),
                    None => None,
                };
                let value = |key: &str| test.values.get(key).and_then(serde_json::Value::as_i64);
                Some(VerifiedReading {
                    id: test.id,
                    measured_at_epoch_millis: test.measured_at,
                    received_at_epoch_millis: test.received_at,
                    generation_power_watts: value("generation_power"),
                    generation_energy_wh: None,
                    consumption_power_watts: value("consumption_power"),
                    consumption_energy_wh: None,
                    message: test
                        .values
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    data_source_id: test.data_source_id,
                    data_source_name: name,
                    test: true,
                })
            }
        };
        Ok(FirstDataResponse {
            system_id,
            received: false,
            reading,
        })
    }
}
