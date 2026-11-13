//! Bounded routing to isolated account-owned `SQLite` databases.

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use pvlog_domain::AccountId;
use serde::Serialize;
use sqlx::{
    Row as _, Sqlite, SqliteConnection, SqlitePool,
    pool::PoolConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use thiserror::Error;
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

/// Resource limits and connection behavior for account database routing.
#[derive(Clone, Debug)]
pub struct SqliteAccountPoolConfig {
    /// Maximum number of account pools retained by one process.
    pub max_open_account_pools: usize,
    /// Maximum physical connections in each account pool.
    pub max_connections_per_account: u32,
    /// Time a connection waits for a locked database.
    pub busy_timeout: Duration,
    /// Time a caller waits for a pooled connection.
    pub acquire_timeout: Duration,
    /// Minimum unused time before an unreferenced account pool can be evicted.
    pub idle_pool_timeout: Duration,
    /// Page cache of each account connection in KiB. A larger cache speeds up
    /// rebuilding aggregates; every open connection may fill its own.
    pub cache_size_kib: u32,
}

impl Default for SqliteAccountPoolConfig {
    fn default() -> Self {
        Self {
            max_open_account_pools: 128,
            max_connections_per_account: 4,
            busy_timeout: Duration::from_secs(5),
            acquire_timeout: Duration::from_secs(10),
            idle_pool_timeout: Duration::from_mins(5),
            cache_size_kib: 64 * 1024,
        }
    }
}

/// Limits of the routers shared within the process.
static SHARED_CONFIG: std::sync::OnceLock<SqliteAccountPoolConfig> = std::sync::OnceLock::new();

/// The routers shared within the process, one per pair of database locations.
static SHARED_ROUTERS: std::sync::OnceLock<
    std::sync::Mutex<HashMap<(PathBuf, PathBuf), SqliteAccountPoolRouter>>,
> = std::sync::OnceLock::new();

/// Process-local bounded pool registry for account databases.
#[derive(Clone, Debug)]
pub struct SqliteAccountPoolRouter {
    management_path: PathBuf,
    accounts_dir: PathBuf,
    config: SqliteAccountPoolConfig,
    state: Arc<Mutex<RouterState>>,
    /// Connections to the management database for resolving routes, opened
    /// on first use.
    management: Arc<std::sync::OnceLock<SqlitePool>>,
}

impl SqliteAccountPoolRouter {
    /// Creates a lazy router without opening any account database.
    ///
    /// # Errors
    ///
    /// Returns an error for zero pool or connection limits.
    pub fn new(
        management_path: PathBuf,
        accounts_dir: PathBuf,
        config: SqliteAccountPoolConfig,
    ) -> Result<Self, SqliteRoutingError> {
        if config.max_open_account_pools == 0 || config.max_connections_per_account == 0 {
            return Err(SqliteRoutingError::InvalidPoolLimits);
        }
        Ok(Self {
            management_path,
            accounts_dir,
            config,
            state: Arc::new(Mutex::new(RouterState::default())),
            management: Arc::default(),
        })
    }

    /// Sets the limits of the routers [`Self::shared`] creates. Only the
    /// first call counts, and only before the first shared router exists;
    /// reports whether these limits apply.
    pub fn configure_shared(config: SqliteAccountPoolConfig) -> bool {
        SHARED_CONFIG.set(config).is_ok()
    }

    /// Returns the process's router for these databases, creating it on first
    /// use with the limits set by [`Self::configure_shared`] or the default ones.
    ///
    /// Every caller shares its account pools and writer lanes: a router made
    /// anew for each request would open and close the account database each
    /// time, and closing its last connection checkpoints the write-ahead log.
    ///
    /// # Errors
    ///
    /// Returns an error for zero pool or connection limits.
    pub fn shared(
        management_path: PathBuf,
        accounts_dir: PathBuf,
    ) -> Result<Self, SqliteRoutingError> {
        let mut routers = SHARED_ROUTERS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (management_path, accounts_dir);
        if let Some(router) = routers.get(&key) {
            return Ok(router.clone());
        }
        let config = SHARED_CONFIG
            .get_or_init(SqliteAccountPoolConfig::default)
            .clone();
        let router = Self::new(key.0.clone(), key.1.clone(), config)?;
        routers.insert(key, router.clone());
        Ok(router)
    }

    /// Resolves an active account through management metadata and returns its lazy pool.
    ///
    /// Routing metadata is checked on every call so suspended, unavailable, or quarantined
    /// accounts cannot continue to obtain new handles from a cached pool.
    ///
    /// # Errors
    ///
    /// Returns an error for missing/inactive routing metadata, unsafe paths, missing files,
    /// or exhausted process pool capacity.
    pub async fn route(
        &self,
        account_id: AccountId,
    ) -> Result<RoutedSqliteAccount, SqliteRoutingError> {
        let route = self.resolve_active_route(account_id).await?;
        let path = self.resolve_managed_path(&route.opaque_locator).await?;
        let key = account_id.as_uuid();
        let now = Instant::now();
        let mut state = self.state.lock().await;

        if let Some(cached) = state.pools.get_mut(&key)
            && cached.entry.path == path
        {
            cached.last_used = now;
            return Ok(RoutedSqliteAccount {
                account_id,
                entry: Arc::clone(&cached.entry),
            });
        }

        state.evict_eligible(now, self.config.idle_pool_timeout);
        if state.pools.len() >= self.config.max_open_account_pools {
            state.evict_oldest_unreferenced();
        }
        if state.pools.len() >= self.config.max_open_account_pools {
            return Err(SqliteRoutingError::PoolCapacity {
                limit: self.config.max_open_account_pools,
            });
        }

        let entry = Arc::new(PoolEntry {
            pool: lazy_account_pool(&path, account_id, &self.config),
            writer: Arc::new(Mutex::new(())),
            path,
        });
        state.pools.insert(
            key,
            CachedPool {
                entry: Arc::clone(&entry),
                last_used: now,
            },
        );
        Ok(RoutedSqliteAccount { account_id, entry })
    }

    /// Removes unreferenced account pools whose idle timeout elapsed.
    pub async fn evict_idle(&self) -> usize {
        self.state
            .lock()
            .await
            .evict_eligible(Instant::now(), self.config.idle_pool_timeout)
    }

    /// Returns the number of account pools currently retained by the process.
    pub async fn open_pool_count(&self) -> usize {
        self.state.lock().await.pools.len()
    }

    async fn resolve_active_route(
        &self,
        account_id: AccountId,
    ) -> Result<ResolvedRoute, SqliteRoutingError> {
        let management = self.management.get_or_init(|| {
            SqlitePoolOptions::new()
                .min_connections(0)
                .max_connections(self.config.max_connections_per_account)
                .acquire_timeout(self.config.acquire_timeout)
                .idle_timeout(Some(self.config.idle_pool_timeout))
                .connect_lazy_with(
                    SqliteConnectOptions::new()
                        .filename(&self.management_path)
                        .create_if_missing(false)
                        .foreign_keys(true)
                        .busy_timeout(self.config.busy_timeout),
                )
        });
        let row = sqlx::query(
            "SELECT opaque_locator, lifecycle_state FROM account_database_registry \
             WHERE account_id = ?",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .fetch_optional(management)
        .await?
        .ok_or_else(|| SqliteRoutingError::RouteNotFound(account_id.to_string()))?;
        let lifecycle: String = row.get("lifecycle_state");
        if lifecycle != "active" {
            return Err(SqliteRoutingError::RouteInactive {
                account_id: account_id.to_string(),
                lifecycle,
            });
        }
        Ok(ResolvedRoute {
            opaque_locator: row.get("opaque_locator"),
        })
    }

    async fn resolve_managed_path(&self, locator: &str) -> Result<PathBuf, SqliteRoutingError> {
        let path = Path::new(locator);
        let mut components = path.components();
        if !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
            || path.extension().and_then(|value| value.to_str()) != Some("sqlite3")
        {
            return Err(SqliteRoutingError::UnsafeLocator(locator.to_owned()));
        }

        let root = tokio::fs::canonicalize(&self.accounts_dir).await?;
        let candidate = self.accounts_dir.join(path);
        let metadata = tokio::fs::symlink_metadata(&candidate).await?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(SqliteRoutingError::UnsafeLocator(locator.to_owned()));
        }
        let canonical = tokio::fs::canonicalize(candidate).await?;
        if canonical.parent() != Some(root.as_path()) {
            return Err(SqliteRoutingError::UnsafeLocator(locator.to_owned()));
        }
        Ok(canonical)
    }
}

/// Handle to one lazily connected account database.
#[derive(Clone, Debug)]
pub struct RoutedSqliteAccount {
    account_id: AccountId,
    entry: Arc<PoolEntry>,
}

impl RoutedSqliteAccount {
    /// Account identity bound to this route.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// Number of physical connections currently opened by this lazy pool.
    #[must_use]
    pub fn pooled_connection_count(&self) -> u32 {
        self.entry.pool.size()
    }

    /// Acquires a read-capable pooled connection.
    ///
    /// The first acquisition opens the file and verifies its durable account binding.
    ///
    /// # Errors
    ///
    /// Returns a pool, pragma, or account-binding error.
    pub async fn acquire(&self) -> Result<PoolConnection<Sqlite>, SqliteRoutingError> {
        Ok(self.entry.pool.acquire().await?)
    }

    /// Acquires the account's process-local serialized writer lane and a pooled connection.
    ///
    /// # Errors
    ///
    /// Returns an error when a connection cannot be acquired.
    pub async fn acquire_writer(&self) -> Result<SerializedSqliteWriter, SqliteRoutingError> {
        let guard = Arc::clone(&self.entry.writer).lock_owned().await;
        let connection = self.entry.pool.acquire().await?;
        Ok(SerializedSqliteWriter {
            connection,
            _guard: guard,
        })
    }

    /// Runs a WAL checkpoint while holding the serialized writer lane.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection or checkpoint pragma fails.
    pub async fn checkpoint(
        &self,
        mode: SqliteCheckpointMode,
    ) -> Result<SqliteCheckpointReport, SqliteRoutingError> {
        let mut writer = self.acquire_writer().await?;
        let statement = match mode {
            SqliteCheckpointMode::Passive => "PRAGMA wal_checkpoint(PASSIVE)",
            SqliteCheckpointMode::Truncate => "PRAGMA wal_checkpoint(TRUNCATE)",
        };
        let row = sqlx::query(statement)
            .fetch_one(writer.connection())
            .await?;
        Ok(SqliteCheckpointReport {
            busy: row.get(0),
            log_frames: row.get(1),
            checkpointed_frames: row.get(2),
        })
    }

    /// Runs `SQLite`'s full integrity probe for this account database.
    ///
    /// # Errors
    ///
    /// Returns an error when the probe cannot execute or reports corruption.
    pub async fn integrity_probe(&self) -> Result<(), SqliteRoutingError> {
        let mut connection = self.acquire().await?;
        let findings = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_all(&mut *connection)
            .await?;
        if findings.len() == 1 && findings[0] == "ok" {
            Ok(())
        } else {
            Err(SqliteRoutingError::IntegrityCheckFailed(findings))
        }
    }
}

/// Exclusive writer connection for one account database within this process.
pub struct SerializedSqliteWriter {
    connection: PoolConnection<Sqlite>,
    _guard: OwnedMutexGuard<()>,
}

impl SerializedSqliteWriter {
    /// Mutable connection used for the serialized write transaction.
    pub fn connection(&mut self) -> &mut SqliteConnection {
        &mut self.connection
    }
}

/// WAL checkpoint aggressiveness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqliteCheckpointMode {
    /// Checkpoint without waiting for readers.
    Passive,
    /// Checkpoint and truncate the WAL after readers release it.
    Truncate,
}

/// Raw counters returned by `PRAGMA wal_checkpoint`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteCheckpointReport {
    /// Non-zero when the requested checkpoint could not fully complete.
    pub busy: i64,
    /// Frames present in the WAL.
    pub log_frames: i64,
    /// Frames copied into the main database.
    pub checkpointed_frames: i64,
}

#[derive(Debug)]
struct PoolEntry {
    pool: SqlitePool,
    writer: Arc<Mutex<()>>,
    path: PathBuf,
}

#[derive(Debug)]
struct CachedPool {
    entry: Arc<PoolEntry>,
    last_used: Instant,
}

#[derive(Debug, Default)]
struct RouterState {
    pools: HashMap<Uuid, CachedPool>,
}

impl RouterState {
    fn evict_eligible(&mut self, now: Instant, idle_timeout: Duration) -> usize {
        let before = self.pools.len();
        self.pools.retain(|_, cached| {
            Arc::strong_count(&cached.entry) > 1
                || now.saturating_duration_since(cached.last_used) < idle_timeout
        });
        before - self.pools.len()
    }

    fn evict_oldest_unreferenced(&mut self) -> bool {
        let candidate = self
            .pools
            .iter()
            .filter(|(_, cached)| Arc::strong_count(&cached.entry) == 1)
            .min_by_key(|(_, cached)| cached.last_used)
            .map(|(account_id, _)| *account_id);
        candidate.is_some_and(|account_id| self.pools.remove(&account_id).is_some())
    }
}

struct ResolvedRoute {
    opaque_locator: String,
}

fn lazy_account_pool(
    path: &Path,
    account_id: AccountId,
    config: &SqliteAccountPoolConfig,
) -> SqlitePool {
    let expected_account = account_id.as_uuid().as_bytes().to_vec();
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(config.busy_timeout)
        .pragma("cache_size", format!("-{}", config.cache_size_kib));
    SqlitePoolOptions::new()
        .min_connections(0)
        .max_connections(config.max_connections_per_account)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(Some(config.idle_pool_timeout))
        .after_connect(move |connection, _metadata| {
            let expected_account = expected_account.clone();
            Box::pin(async move {
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("PRAGMA synchronous = NORMAL")
                    .execute(&mut *connection)
                    .await?;
                // A statement that writes many rows keeps a journal for undoing
                // just itself; kept as a file, writing it costs a fifth of an
                // ingest. Every write runs in a transaction that is undone as a
                // whole on failure, so the journal never needs to outlive it.
                sqlx::query("PRAGMA temp_store = MEMORY")
                    .execute(&mut *connection)
                    .await?;
                let bound_account: Vec<u8> = sqlx::query_scalar(
                    "SELECT account_id FROM pvlog_account_identity WHERE singleton = 1",
                )
                .fetch_one(&mut *connection)
                .await?;
                if bound_account != expected_account {
                    return Err(sqlx::Error::Protocol(
                        "account database identity does not match route".to_owned(),
                    ));
                }
                Ok(())
            })
        })
        .connect_lazy_with(options)
}

/// Failure while resolving, pooling, or maintaining an account database.
#[derive(Debug, Error)]
pub enum SqliteRoutingError {
    /// Pool limits must be non-zero.
    #[error("SQLite account pool limits must be greater than zero")]
    InvalidPoolLimits,
    /// Management or account database operation failed.
    #[error("SQLite account routing database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// Filesystem validation failed.
    #[error("SQLite account routing filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    /// No durable route exists for the account.
    #[error("account {0} has no SQLite database route")]
    RouteNotFound(String),
    /// Durable state disallows routing.
    #[error("account {account_id} SQLite route is {lifecycle}, not active")]
    RouteInactive {
        /// Account whose route was rejected.
        account_id: String,
        /// Durable lifecycle returned by management storage.
        lifecycle: String,
    },
    /// The opaque locator is not a direct managed `.sqlite3` file.
    #[error("unsafe SQLite account locator: {0}")]
    UnsafeLocator(String),
    /// All bounded account-pool slots are currently referenced.
    #[error("SQLite account pool capacity of {limit} is exhausted")]
    PoolCapacity {
        /// Configured maximum retained pools.
        limit: usize,
    },
    /// `SQLite` reported one or more integrity findings.
    #[error("SQLite account integrity check failed: {0:?}")]
    IntegrityCheckFailed(Vec<String>),
}
