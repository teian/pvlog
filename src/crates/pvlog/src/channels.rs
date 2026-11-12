//! Storage-backed measurement-channel bindings.
//!
//! Channels are discovered by ingestion and bound by operators. Binding never
//! rewrites history: the stored readings keep their channel, and changing a
//! target only changes which equipment those readings are attributed to from
//! then on, for every query over the whole range.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{ChannelApiError, ChannelApiUseCases, ChannelBindingInput, ChannelResponse};
use pvlog_application::Clock;
use pvlog_domain::{AccountId, SystemId, UserId};
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(feature = "sqlite")]
use sqlx::Row;
use uuid::Uuid;

/// Opens the measurement channel repository of the account that owns a system.
///
/// Returns `None` when the system is unknown or its storage cannot be reached.
pub async fn channel_repository(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
    system_id: SystemId,
) -> Option<Box<dyn pvlog_storage::ChannelRepository>> {
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
                let account = pvlog_storage::SqliteAccountPoolRouter::new(
                    management_path.clone(),
                    accounts_dir.clone(),
                    pvlog_storage::SqliteAccountPoolConfig::default(),
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

    /// Looks up the account owning a system; an unregistered system is `NotFound`.
    async fn account_for(&self, system_id: SystemId) -> Result<AccountId, ChannelApiError> {
        self.management
            .system_registry(system_id)
            .await
            .map_err(|_| ChannelApiError::Unavailable)?
            .map(|record| record.account_id)
            .ok_or(ChannelApiError::NotFound)
    }

    /// Routes to the `SQLite` account database that holds the system, failing as `Unavailable` for other targets.
    #[cfg(feature = "sqlite")]
    async fn account(
        &self,
        system_id: SystemId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, ChannelApiError> {
        let DatabaseTarget::Sqlite {
            management_path,
            accounts_dir,
        } = &self.target
        else {
            return Err(ChannelApiError::Unavailable);
        };
        let account_id = self.account_for(system_id).await?;
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.clone(),
            accounts_dir.clone(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| ChannelApiError::Unavailable)?
        .route(account_id)
        .await
        .map_err(|_| ChannelApiError::Unavailable)
    }
}

#[async_trait]
impl ChannelApiUseCases for StorageChannelApi {
    /// Lists the system's active channels, unassigned ones first.
    async fn list(&self, system_id: SystemId) -> Result<Vec<ChannelResponse>, ChannelApiError> {
        #[cfg(feature = "sqlite")]
        {
            if matches!(self.target, DatabaseTarget::Sqlite { .. }) {
                let account = self.account(system_id).await?;
                let mut connection = account
                    .acquire()
                    .await
                    .map_err(|_| ChannelApiError::Unavailable)?;
                let rows = sqlx::query(
                    "SELECT c.id,c.channel_key,c.display_name,c.unit,c.target_kind,c.target_id,\
                     c.tracks_power,c.tracks_energy,c.discovered_at,c.last_value_at,\
                     (SELECT v.integer_value FROM telemetry_hot_extended_values v \
                      JOIN telemetry_hot h ON h.observation_id=v.observation_id \
                      WHERE v.channel_id=c.id ORDER BY h.measured_at DESC LIMIT 1) AS last_value \
                     FROM channel_definitions c WHERE c.system_id=? AND c.lifecycle='active' \
                     ORDER BY c.target_kind='unassigned' DESC, c.channel_key",
                )
                .bind(id_blob(system_id))
                .fetch_all(&mut *connection)
                .await
                .map_err(|_| ChannelApiError::Unavailable)?;
                return rows.iter().map(channel_response).collect();
            }
        }
        Err(ChannelApiError::Unavailable)
    }

    /// Updates a channel's binding, drops dependent string and inverter rollups so they are rebuilt, and returns the refreshed channel.
    async fn bind(
        &self,
        _actor: UserId,
        system_id: SystemId,
        channel_key: &str,
        input: ChannelBindingInput,
    ) -> Result<ChannelResponse, ChannelApiError> {
        #[cfg(feature = "sqlite")]
        {
            if matches!(self.target, DatabaseTarget::Sqlite { .. }) {
                let account = self.account(system_id).await?;
                let mut connection = account
                    .acquire()
                    .await
                    .map_err(|_| ChannelApiError::Unavailable)?;
                verify_target(&mut connection, system_id, &input).await?;
                let updated = sqlx::query(
                    "UPDATE channel_definitions SET target_kind=?,target_id=?,tracks_power=?,\
                     tracks_energy=?,display_name=COALESCE(?,display_name),updated_at=?,\
                     version=version+1 WHERE system_id=? AND channel_key=?",
                )
                .bind(&input.target_kind)
                .bind(input.target_id.map(|value| value.as_bytes().to_vec()))
                .bind(i64::from(input.tracks_power))
                .bind(i64::from(input.tracks_energy))
                .bind(input.display_name.as_deref())
                .bind(self.now())
                .bind(id_blob(system_id))
                .bind(channel_key)
                .execute(&mut *connection)
                .await
                .map_err(|_| ChannelApiError::Unavailable)?;
                if updated.rows_affected() == 0 {
                    return Err(ChannelApiError::NotFound);
                }
                // Per-string and per-inverter rollups follow the binding they were
                // built under. Drop them so the aggregation cycle, which notices the
                // newer binding, rebuilds them against the current one.
                sqlx::query(
                    "DELETE FROM telemetry_rollups \
                     WHERE system_id=? AND scope_kind IN ('string','inverter')",
                )
                .bind(id_blob(system_id))
                .execute(&mut *connection)
                .await
                .map_err(|_| ChannelApiError::Unavailable)?;
                drop(connection);
                return self
                    .list(system_id)
                    .await?
                    .into_iter()
                    .find(|channel| channel.channel_key == channel_key)
                    .ok_or(ChannelApiError::NotFound);
            }
        }
        let _ = (channel_key, input);
        Err(ChannelApiError::Unavailable)
    }

    /// Retires a channel while keeping its stored readings; an unknown channel is `NotFound`.
    async fn remove(&self, system_id: SystemId, channel_key: &str) -> Result<(), ChannelApiError> {
        #[cfg(feature = "sqlite")]
        {
            if matches!(self.target, DatabaseTarget::Sqlite { .. }) {
                let account = self.account(system_id).await?;
                let mut connection = account
                    .acquire()
                    .await
                    .map_err(|_| ChannelApiError::Unavailable)?;
                // Retiring keeps the stored readings and their channel row intact;
                // deleting the definition would orphan every extended value.
                let updated = sqlx::query(
                    "UPDATE channel_definitions SET lifecycle='retired',updated_at=?,\
                     version=version+1 WHERE system_id=? AND channel_key=?",
                )
                .bind(self.now())
                .bind(id_blob(system_id))
                .bind(channel_key)
                .execute(&mut *connection)
                .await
                .map_err(|_| ChannelApiError::Unavailable)?;
                return if updated.rows_affected() == 0 {
                    Err(ChannelApiError::NotFound)
                } else {
                    Ok(())
                };
            }
        }
        let _ = channel_key;
        Err(ChannelApiError::Unavailable)
    }
}

impl StorageChannelApi {
    /// Returns the clock's current time in epoch milliseconds.
    fn now(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }
}

/// Confirms the requested equipment exists and belongs to the same system.
#[cfg(feature = "sqlite")]
async fn verify_target(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    input: &ChannelBindingInput,
) -> Result<(), ChannelApiError> {
    let Some(target_id) = input.target_id else {
        return Ok(());
    };
    let target = target_id.as_bytes().to_vec();
    let exists: Option<i64> = match input.target_kind.as_str() {
        "inverter" => {
            sqlx::query_scalar("SELECT 1 FROM inverters WHERE id=? AND system_id=?")
                .bind(target)
                .bind(id_blob(system_id))
                .fetch_optional(&mut *connection)
                .await
        }
        "string" => {
            sqlx::query_scalar(
                "SELECT 1 FROM pv_strings p JOIN inverters i ON i.id=p.inverter_id \
                 WHERE p.id=? AND i.system_id=?",
            )
            .bind(target)
            .bind(id_blob(system_id))
            .fetch_optional(&mut *connection)
            .await
        }
        _ => return Ok(()),
    }
    .map_err(|_| ChannelApiError::Unavailable)?;
    if exists.is_none() {
        return Err(ChannelApiError::InvalidInput("unknown_target_equipment"));
    }
    Ok(())
}

/// Converts a `channel_definitions` row into a channel response, tolerating missing optional columns.
#[cfg(feature = "sqlite")]
fn channel_response(row: &sqlx::sqlite::SqliteRow) -> Result<ChannelResponse, ChannelApiError> {
    Ok(ChannelResponse {
        id: row_uuid(row, "id")?,
        channel_key: row.get("channel_key"),
        display_name: row.get("display_name"),
        unit: row.get("unit"),
        target_kind: row.get("target_kind"),
        target_id: row
            .try_get::<Option<Vec<u8>>, _>("target_id")
            .ok()
            .flatten()
            .and_then(|bytes| Uuid::from_slice(&bytes).ok()),
        tracks_power: row.try_get::<i64, _>("tracks_power").unwrap_or_default() != 0,
        tracks_energy: row.try_get::<i64, _>("tracks_energy").unwrap_or_default() != 0,
        discovered_at_epoch_millis: row
            .try_get::<Option<i64>, _>("discovered_at")
            .ok()
            .flatten(),
        last_value_at_epoch_millis: row
            .try_get::<Option<i64>, _>("last_value_at")
            .ok()
            .flatten(),
        last_value: row.try_get::<Option<i64>, _>("last_value").ok().flatten(),
    })
}

/// Reads a UUID blob column, failing as `Unavailable` when it is malformed.
#[cfg(feature = "sqlite")]
fn row_uuid(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Uuid, ChannelApiError> {
    Uuid::from_slice(&row.get::<Vec<u8>, _>(column)).map_err(|_| ChannelApiError::Unavailable)
}

/// Encodes a system identifier as the blob bytes used in SQL bindings.
#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}
