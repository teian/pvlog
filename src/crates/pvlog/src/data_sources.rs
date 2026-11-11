//! Runtime data source lifecycle: storage, upload credentials and auditing.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{
    CreateDataSourceInput, DataSourceApiError, DataSourceApiUseCases, DataSourceConfiguration,
    DataSourceResponse, IssuedDataSource, RenameDataSourceInput, TestReadingResponse,
};
use pvlog_application::{
    ApiTokenRecord, ApiTokenService, Clock, SOURCE_PROFILES, SourceProfile, connection_state,
    render_configuration, source_profile,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, AuditEventId, DataSourceId, MeasurementValues, SystemId,
    UserId, Watts,
};
use pvlog_storage::{
    AccountConfigurationRepository, AuditRecord, DataSourceActivity, DataSourceRecord,
    DataSourceRepository, DatabaseTarget, ManagementRepository,
    PostgresAccountConfigurationRepository, PostgresDataSourceRepository,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteDataSourceRepository, TestReadingRecord,
};
use secrecy::{ExposeSecret as _, SecretString};

/// Window over which a source's recent readings are counted.
const RECENT_WINDOW_MILLIS: i64 = 24 * 60 * 60 * 1_000;
/// Power a user-triggered test reading reports; small enough to be obviously artificial.
const TEST_READING_POWER_WATTS: i64 = 100;
/// Longest data source name accepted.
const MAXIMUM_NAME_LENGTH: usize = 80;

/// Production data source use cases backed by account and management storage.
pub struct StorageDataSourceApi {
    target: DatabaseTarget,
    tokens: ApiTokenService,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
    /// Public address uploaders send to, without a trailing slash.
    server_url: String,
}

/// The two account repositories a data source operation needs.
struct Repositories {
    sources: Box<dyn DataSourceRepository>,
    configuration: Box<dyn AccountConfigurationRepository>,
}

impl StorageDataSourceApi {
    #[must_use]
    pub fn new(
        target: DatabaseTarget,
        tokens: ApiTokenService,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
        server_url: &str,
    ) -> Self {
        Self {
            target,
            tokens,
            management,
            clock,
            server_url: server_url.trim_end_matches('/').to_owned(),
        }
    }

    fn now(&self) -> Result<i64, DataSourceApiError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| DataSourceApiError::Unavailable)
    }

    async fn repositories(
        &self,
        account_id: AccountId,
    ) -> Result<Repositories, DataSourceApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = SqliteAccountPoolRouter::new(
                        management_path.clone(),
                        accounts_dir.clone(),
                        SqliteAccountPoolConfig::default(),
                    )
                    .map_err(|_| DataSourceApiError::Unavailable)?
                    .route(account_id)
                    .await
                    .map_err(|_| DataSourceApiError::Unavailable)?;
                    Ok(Repositories {
                        sources: Box::new(SqliteDataSourceRepository::new(account.clone())),
                        configuration: Box::new(SqliteAccountConfigurationRepository::new(account)),
                    })
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id);
                    Err(DataSourceApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Ok(Repositories {
                        sources: Box::new(PostgresDataSourceRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        configuration: Box::new(PostgresAccountConfigurationRepository::new(
                            url.clone(),
                            account_id,
                        )),
                    })
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id);
                    Err(DataSourceApiError::Unavailable)
                }
            }
        }
    }

    /// Issues an upload-only credential that works for this one system.
    async fn issue_credential(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
        now: i64,
    ) -> Result<(ApiCredentialId, SecretString), DataSourceApiError> {
        let issued = self
            .tokens
            .issue(
                account_id,
                actor,
                Some(system_id),
                // Credential names are unique per owner even after revocation.
                format!("data-source:{id}:{now}"),
                BTreeSet::from([ApiScope::TelemetryWrite]),
                None,
            )
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        Ok((issued.credential.id, issued.plaintext))
    }

    fn configuration(
        &self,
        profile: &SourceProfile,
        system_id: SystemId,
        api_key: SecretString,
    ) -> DataSourceConfiguration {
        DataSourceConfiguration {
            snippets: render_configuration(
                profile,
                &self.server_url,
                &system_id.to_string(),
                api_key.expose_secret(),
            ),
            server_url: self.server_url.clone(),
            system_id,
            api_key,
        }
    }

    /// Builds responses with the connection state each source has right now.
    async fn responses(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        records: Vec<DataSourceRecord>,
        repositories: &Repositories,
    ) -> Result<Vec<DataSourceResponse>, DataSourceApiError> {
        let now = self.now()?;
        let status_interval_seconds = repositories
            .configuration
            .system(system_id)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?
            .ok_or(DataSourceApiError::NotFound)?
            .status_interval_seconds;
        let status_interval_seconds =
            u32::try_from(status_interval_seconds).map_err(|_| DataSourceApiError::Unavailable)?;
        let activity = repositories
            .sources
            .data_source_activity(system_id, now.saturating_sub(RECENT_WINDOW_MILLIS))
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        let credentials = self
            .tokens
            .list(account_id)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        Ok(records
            .into_iter()
            .map(|record| {
                let delivered = activity
                    .iter()
                    .find(|entry| entry.data_source_id == record.id);
                response(
                    record,
                    delivered,
                    &credentials,
                    status_interval_seconds,
                    now,
                    &self.server_url,
                )
            })
            .collect())
    }

    async fn load(
        &self,
        repositories: &Repositories,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<DataSourceRecord, DataSourceApiError> {
        repositories
            .sources
            .data_source(system_id, id)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?
            .filter(|record| record.deleted_at.is_none())
            .ok_or(DataSourceApiError::NotFound)
    }

    async fn name_is_free(
        &self,
        repositories: &Repositories,
        system_id: SystemId,
        name: &str,
        except: Option<DataSourceId>,
    ) -> Result<(), DataSourceApiError> {
        let taken = repositories
            .sources
            .data_sources(system_id)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?
            .iter()
            .any(|record| Some(record.id) != except && record.name == name);
        if taken {
            Err(DataSourceApiError::NameTaken)
        } else {
            Ok(())
        }
    }

    /// Records a lifecycle event; the metadata never contains secret material.
    async fn audit(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
        action: &'static str,
    ) -> Result<(), DataSourceApiError> {
        let event = AuditEventId::new();
        let mut event_hash = [0_u8; 32];
        event_hash[..16].copy_from_slice(event.as_uuid().as_bytes());
        event_hash[16..].copy_from_slice(event.as_uuid().as_bytes());
        self.management
            .append_audit(&AuditRecord {
                id: event,
                occurred_at: self.now()?,
                request_id: None,
                actor_type: "user".to_owned(),
                actor_id: Some(actor.as_uuid()),
                account_id: Some(account_id),
                action: action.to_owned(),
                target_type: "data_source".to_owned(),
                target_id: Some(id.as_uuid()),
                outcome: "succeeded".to_owned(),
                previous_event_hash: None,
                event_hash,
                safe_metadata: serde_json::json!({ "systemId": system_id }),
            })
            .await
            .map_err(|_| DataSourceApiError::Unavailable)
    }
}

/// Trims a name and rejects an empty or overlong one.
fn valid_name(name: &str) -> Result<String, DataSourceApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAXIMUM_NAME_LENGTH {
        return Err(DataSourceApiError::InvalidInput("name"));
    }
    Ok(name.to_owned())
}

fn response(
    record: DataSourceRecord,
    delivered: Option<&DataSourceActivity>,
    credentials: &[ApiTokenRecord],
    status_interval_seconds: u32,
    now: i64,
    server_url: &str,
) -> DataSourceResponse {
    // A profile without a credential cannot be disconnected by revocation.
    let credential_usable = source_profile(&record.profile)
        .is_some_and(|profile| !profile.issues_credential)
        || record.credential_id.is_some_and(|id| {
            credentials.iter().any(|credential| {
                credential.id == id
                    && credential.revoked_at.is_none()
                    && credential.expires_at.is_none_or(|expiry| expiry > now)
            })
        });
    DataSourceResponse {
        state: connection_state(
            credential_usable,
            delivered.map(|entry| entry.last_received_at),
            status_interval_seconds,
            now,
        ),
        id: record.id,
        system_id: record.system_id,
        name: record.name,
        profile: record.profile,
        server_url: server_url.to_owned(),
        credential_id: record.credential_id,
        last_reading_at_epoch_millis: delivered.map(|entry| entry.last_measured_at),
        last_received_at_epoch_millis: delivered.map(|entry| entry.last_received_at),
        readings_last_day: delivered.map_or(0, |entry| entry.recent_readings),
        created_at_epoch_millis: record.created_at,
        updated_at_epoch_millis: record.updated_at,
    }
}

#[async_trait]
impl DataSourceApiUseCases for StorageDataSourceApi {
    fn profiles(&self) -> Vec<SourceProfile> {
        SOURCE_PROFILES.to_vec()
    }

    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<DataSourceResponse>, DataSourceApiError> {
        let repositories = self.repositories(account_id).await?;
        let records = repositories
            .sources
            .data_sources(system_id)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        self.responses(account_id, system_id, records, &repositories)
            .await
    }

    async fn read(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<DataSourceResponse, DataSourceApiError> {
        let repositories = self.repositories(account_id).await?;
        let record = self.load(&repositories, system_id, id).await?;
        self.responses(account_id, system_id, vec![record], &repositories)
            .await?
            .pop()
            .ok_or(DataSourceApiError::NotFound)
    }

    async fn create(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        input: CreateDataSourceInput,
    ) -> Result<IssuedDataSource, DataSourceApiError> {
        let name = valid_name(&input.name)?;
        let profile =
            source_profile(&input.profile).ok_or(DataSourceApiError::InvalidInput("profile"))?;
        let repositories = self.repositories(account_id).await?;
        self.name_is_free(&repositories, system_id, &name, None)
            .await?;
        let now = self.now()?;
        let id = DataSourceId::new();
        let issued = if profile.issues_credential {
            Some(
                self.issue_credential(actor, account_id, system_id, id, now)
                    .await?,
            )
        } else {
            None
        };
        let record = DataSourceRecord {
            id,
            system_id,
            name,
            profile: profile.id.to_owned(),
            credential_id: issued.as_ref().map(|(credential, _)| *credential),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        if repositories
            .sources
            .save_data_source(&record)
            .await
            .is_err()
        {
            // Without its data source the credential would be an orphan that
            // nobody can see or revoke from the system.
            if let Some((credential, _)) = &issued {
                let _ = self.tokens.revoke(account_id, *credential).await;
            }
            return Err(DataSourceApiError::Unavailable);
        }
        self.audit(
            actor,
            account_id,
            system_id,
            id,
            "system.data_source.created",
        )
        .await?;
        let data_source = self
            .responses(account_id, system_id, vec![record], &repositories)
            .await?
            .pop()
            .ok_or(DataSourceApiError::Unavailable)?;
        Ok(IssuedDataSource {
            data_source,
            configuration: issued
                .map(|(_, api_key)| self.configuration(profile, system_id, api_key)),
        })
    }

    async fn rename(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
        input: RenameDataSourceInput,
    ) -> Result<DataSourceResponse, DataSourceApiError> {
        let name = valid_name(&input.name)?;
        let repositories = self.repositories(account_id).await?;
        let mut record = self.load(&repositories, system_id, id).await?;
        self.name_is_free(&repositories, system_id, &name, Some(id))
            .await?;
        record.name = name;
        record.updated_at = self.now()?;
        repositories
            .sources
            .save_data_source(&record)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        self.audit(
            actor,
            account_id,
            system_id,
            id,
            "system.data_source.renamed",
        )
        .await?;
        self.responses(account_id, system_id, vec![record], &repositories)
            .await?
            .pop()
            .ok_or(DataSourceApiError::Unavailable)
    }

    async fn regenerate(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<IssuedDataSource, DataSourceApiError> {
        let repositories = self.repositories(account_id).await?;
        let mut record = self.load(&repositories, system_id, id).await?;
        let profile = source_profile(&record.profile)
            .filter(|profile| profile.issues_credential)
            .ok_or(DataSourceApiError::InvalidInput("profile"))?;
        let now = self.now()?;
        let previous = record.credential_id;
        let (credential, api_key) = self
            .issue_credential(actor, account_id, system_id, id, now)
            .await?;
        record.credential_id = Some(credential);
        record.updated_at = now;
        if repositories
            .sources
            .save_data_source(&record)
            .await
            .is_err()
        {
            let _ = self.tokens.revoke(account_id, credential).await;
            return Err(DataSourceApiError::Unavailable);
        }
        // The previous secret stops working now; it may already be revoked.
        if let Some(previous) = previous {
            let _ = self.tokens.revoke(account_id, previous).await;
        }
        self.audit(
            actor,
            account_id,
            system_id,
            id,
            "system.data_source.credential.regenerated",
        )
        .await?;
        let data_source = self
            .responses(account_id, system_id, vec![record], &repositories)
            .await?
            .pop()
            .ok_or(DataSourceApiError::Unavailable)?;
        Ok(IssuedDataSource {
            data_source,
            configuration: Some(self.configuration(profile, system_id, api_key)),
        })
    }

    async fn delete(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<(), DataSourceApiError> {
        let repositories = self.repositories(account_id).await?;
        let mut record = self.load(&repositories, system_id, id).await?;
        let credential = record.credential_id.take();
        let now = self.now()?;
        record.deleted_at = Some(now);
        record.updated_at = now;
        // Revoke first: a source that is gone must not keep a working key.
        if let Some(credential) = credential {
            let _ = self.tokens.revoke(account_id, credential).await;
        }
        repositories
            .sources
            .save_data_source(&record)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        self.audit(
            actor,
            account_id,
            system_id,
            id,
            "system.data_source.deleted",
        )
        .await
    }
    async fn send_test_reading(
        &self,
        actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<TestReadingResponse, DataSourceApiError> {
        let repositories = self.repositories(account_id).await?;
        let record = self.load(&repositories, system_id, id).await?;
        let now = self.now()?;
        let values = serde_json::to_value(MeasurementValues {
            generation_power: Some(Watts::new(TEST_READING_POWER_WATTS)),
            ..MeasurementValues::default()
        })
        .map_err(|_| DataSourceApiError::Unavailable)?;
        let reading = TestReadingRecord {
            id: uuid::Uuid::now_v7(),
            system_id,
            data_source_id: Some(record.id),
            measured_at: now,
            received_at: now,
            values,
        };
        repositories
            .sources
            .save_test_reading(&reading)
            .await
            .map_err(|_| DataSourceApiError::Unavailable)?;
        self.audit(
            actor,
            account_id,
            system_id,
            id,
            "system.data_source.test_reading.sent",
        )
        .await?;
        Ok(TestReadingResponse {
            id: reading.id,
            system_id,
            data_source_id: reading.data_source_id,
            measured_at_epoch_millis: reading.measured_at,
            received_at_epoch_millis: reading.received_at,
            values: reading.values,
            test: true,
        })
    }

    async fn delete_test_readings(
        &self,
        _actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<(), DataSourceApiError> {
        self.repositories(account_id)
            .await?
            .sources
            .delete_test_readings(system_id)
            .await
            .map(|_| ())
            .map_err(|_| DataSourceApiError::Unavailable)
    }
}
