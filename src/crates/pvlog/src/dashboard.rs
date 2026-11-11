//! Storage-backed operational dashboard projection.
//!
//! Summarizes the newest observation across every system the signed-in user's
//! accounts own, together with today's ingestion counters and the open alerts
//! those systems raised.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    DashboardAlertResponse, DashboardApiError, DashboardApiUseCases, DashboardIngestionResponse,
    DashboardResponse, DashboardRestriction,
};
use pvlog_application::Clock;
use pvlog_domain::{AccountId, SystemId, UserId};
use pvlog_storage::{DatabaseTarget, ManagementRepository};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::Row;

const FRESHNESS_THRESHOLD_SECONDS: u64 = 900;
const MILLIS_PER_DAY: i64 = 86_400_000;
const RECENT_ALERT_LIMIT: usize = 5;

/// Newest observation and counters for one system.
#[derive(Clone, Debug, Default)]
struct SystemSnapshot {
    observed_at_epoch_millis: i64,
    generation_watts: Option<i64>,
    consumption_watts: Option<i64>,
    grid_watts: Option<i64>,
    battery_basis_points: Option<i64>,
    coverage_basis_points: u16,
    accepted_today: u64,
}

/// Serves `/api/v1/dashboard` from account telemetry storage.
pub struct StorageDashboardApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

impl StorageDashboardApi {
    /// Creates the dashboard service over the given storage target, management repository and clock.
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

    /// Lists the account and system pairs the actor may see, narrowed by the optional account and system restriction.
    async fn visible_systems(
        &self,
        actor: UserId,
        restriction: DashboardRestriction,
    ) -> Result<Vec<(AccountId, SystemId)>, DashboardApiError> {
        let accounts = self
            .management
            .active_accounts_for_user(actor)
            .await
            .map_err(|_| DashboardApiError::Unavailable)?;
        let mut visible = Vec::new();
        for account in accounts {
            if restriction
                .account_id
                .is_some_and(|allowed| allowed != account.id)
            {
                continue;
            }
            let systems = self
                .management
                .systems_for_account(account.id)
                .await
                .map_err(|_| DashboardApiError::Unavailable)?;
            visible.extend(
                systems
                    .into_iter()
                    .filter(|system| {
                        restriction
                            .system_id
                            .is_none_or(|allowed| allowed == *system)
                    })
                    .map(|system| (account.id, system)),
            );
        }
        Ok(visible)
    }
}

#[async_trait]
impl DashboardApiUseCases for StorageDashboardApi {
    /// Aggregates the newest snapshot, today's accepted count and the most recent open alerts across the visible systems.
    async fn dashboard(
        &self,
        actor: UserId,
        restriction: DashboardRestriction,
    ) -> Result<DashboardResponse, DashboardApiError> {
        let now = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| DashboardApiError::Unavailable)?;
        let visible = self.visible_systems(actor, restriction).await?;

        let mut newest = SystemSnapshot::default();
        let mut accepted_today = 0_u64;
        let mut alerts = Vec::new();
        for (account_id, system_id) in visible {
            let snapshot = self.snapshot(account_id, system_id, now).await?;
            accepted_today = accepted_today.saturating_add(snapshot.accepted_today);
            if snapshot.observed_at_epoch_millis > newest.observed_at_epoch_millis {
                newest = snapshot;
            }
            alerts.extend(self.open_alerts(account_id, system_id).await?);
        }
        alerts.sort_by(|left, right| {
            right
                .opened_at_epoch_millis
                .cmp(&left.opened_at_epoch_millis)
        });
        alerts.truncate(RECENT_ALERT_LIMIT);

        let age_millis = now.saturating_sub(newest.observed_at_epoch_millis).max(0);
        Ok(DashboardResponse {
            observed_at_epoch_millis: newest.observed_at_epoch_millis,
            age_seconds: if newest.observed_at_epoch_millis == 0 {
                0
            } else {
                u64::try_from(age_millis / 1_000).unwrap_or(u64::MAX)
            },
            freshness_threshold_seconds: FRESHNESS_THRESHOLD_SECONDS,
            #[allow(clippy::cast_precision_loss)]
            generation_watts: newest.generation_watts.unwrap_or_default() as f64,
            #[allow(clippy::cast_precision_loss)]
            consumption_watts: newest.consumption_watts.map(|watts| watts as f64),
            #[allow(clippy::cast_precision_loss)]
            grid_watts: newest.grid_watts.map(|watts| watts as f64),
            battery_basis_points: newest.battery_basis_points,
            coverage_basis_points: newest.coverage_basis_points,
            recent_alerts: alerts,
            ingestion: DashboardIngestionResponse {
                accepted_today,
                rejected_today: 0,
                lag_seconds: 0,
            },
        })
    }
}

impl StorageDashboardApi {
    /// Routes to the `SQLite` database of the given account, mapping any routing failure to unavailable.
    #[cfg(feature = "sqlite")]
    async fn sqlite_account(
        &self,
        management_path: &std::path::Path,
        accounts_dir: &std::path::Path,
        account_id: AccountId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, DashboardApiError> {
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.to_path_buf(),
            accounts_dir.to_path_buf(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| DashboardApiError::Unavailable)?
        .route(account_id)
        .await
        .map_err(|_| DashboardApiError::Unavailable)
    }

    /// Reads the latest observation, accepted count over the last day and coverage for a system; empty for `PostgreSQL` or no data.
    async fn snapshot(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        now: i64,
    ) -> Result<SystemSnapshot, DashboardApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| DashboardApiError::Unavailable)?;
                    let latest = sqlx::query(
                        "SELECT measured_at,generation_power_watts,consumption_power_watts,\
                         net_grid_power_watts,battery_state_basis_points FROM telemetry_hot \
                         WHERE system_id=? ORDER BY measured_at DESC LIMIT 1",
                    )
                    .bind(id_blob(system_id))
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(|_| DashboardApiError::Unavailable)?;
                    let accepted: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM telemetry_hot WHERE system_id=? AND received_at>=?",
                    )
                    .bind(id_blob(system_id))
                    .bind(now - MILLIS_PER_DAY)
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(|_| DashboardApiError::Unavailable)?;
                    let coverage: Option<i64> = sqlx::query_scalar(
                        "SELECT coverage_basis_points FROM system_lifetime_summaries WHERE system_id=?",
                    )
                    .bind(id_blob(system_id))
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(|_| DashboardApiError::Unavailable)?
                    .flatten();
                    Ok(
                        latest.map_or_else(SystemSnapshot::default, |row| SystemSnapshot {
                            observed_at_epoch_millis: row.get("measured_at"),
                            generation_watts: row
                                .try_get::<Option<i64>, _>("generation_power_watts")
                                .ok()
                                .flatten(),
                            consumption_watts: row
                                .try_get::<Option<i64>, _>("consumption_power_watts")
                                .ok()
                                .flatten(),
                            grid_watts: row
                                .try_get::<Option<i64>, _>("net_grid_power_watts")
                                .ok()
                                .flatten(),
                            battery_basis_points: row
                                .try_get::<Option<i64>, _>("battery_state_basis_points")
                                .ok()
                                .flatten(),
                            coverage_basis_points: coverage
                                .and_then(|value| u16::try_from(value).ok())
                                .unwrap_or_default()
                                .min(10_000),
                            accepted_today: u64::try_from(accepted).unwrap_or_default(),
                        }),
                    )
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id, now);
                    Ok(SystemSnapshot::default())
                }
            }
            DatabaseTarget::Postgres { .. } => Ok(SystemSnapshot::default()),
        }
    }

    /// Loads the system's most recent active alerts; yields none for `PostgreSQL`.
    async fn open_alerts(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<DashboardAlertResponse>, DashboardApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = self
                        .sqlite_account(management_path, accounts_dir, account_id)
                        .await?;
                    let mut connection = account
                        .acquire()
                        .await
                        .map_err(|_| DashboardApiError::Unavailable)?;
                    let rows = sqlx::query(
                        "SELECT e.id,e.state,e.triggered_at,r.name FROM alert_events e \
                         JOIN alert_rules r ON r.id=e.rule_id WHERE e.system_id=? \
                         AND e.state='active' ORDER BY e.triggered_at DESC LIMIT ?",
                    )
                    .bind(id_blob(system_id))
                    .bind(i64::try_from(RECENT_ALERT_LIMIT).unwrap_or(5))
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(|_| DashboardApiError::Unavailable)?;
                    Ok(rows
                        .iter()
                        .map(|row| DashboardAlertResponse {
                            id: uuid_text(row.try_get::<Vec<u8>, _>("id").ok().as_deref()),
                            title: row.try_get("name").unwrap_or_default(),
                            state: row.try_get("state").unwrap_or_default(),
                            opened_at_epoch_millis: row.try_get("triggered_at").unwrap_or_default(),
                        })
                        .collect())
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id, system_id);
                    Ok(Vec::new())
                }
            }
            DatabaseTarget::Postgres { .. } => Ok(Vec::new()),
        }
    }
}

/// Formats a 16-byte blob as a UUID string, or an empty string when absent or malformed.
#[cfg(feature = "sqlite")]
fn uuid_text(bytes: Option<&[u8]>) -> String {
    bytes
        .and_then(|value| <[u8; 16]>::try_from(value).ok())
        .map(uuid::Uuid::from_bytes)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Encodes a system identifier as the 16-byte blob used by `SQLite` keys.
#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}
