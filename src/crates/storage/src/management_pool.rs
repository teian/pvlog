//! Process-wide connection pools for the `SQLite` management database.
//!
//! Opening an `SQLite` connection starts a worker thread, opens the file and
//! applies its settings. The management repositories ask for a connection for
//! every query, so they borrow one from a pool kept per database path instead.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use sqlx::{
    Sqlite, SqliteConnection, SqlitePool,
    pool::PoolConnection,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

/// Most connections a process keeps open to one management database.
const MAX_CONNECTIONS: u32 = 8;
/// How long an unused connection stays open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

static POOLS: OnceLock<Mutex<HashMap<PathBuf, SqlitePool>>> = OnceLock::new();

/// A pooled management connection. It goes back to the pool when dropped.
pub(crate) struct ManagementConnection(PoolConnection<Sqlite>);

impl ManagementConnection {
    /// Returns the connection to the pool. It stays open for the next query.
    #[allow(clippy::unused_async)]
    pub(crate) async fn close(self) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

impl Deref for ManagementConnection {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.0
    }
}

impl DerefMut for ManagementConnection {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

/// Borrows a connection to the management database at `path`.
pub(crate) async fn management_connection(
    path: &Path,
) -> Result<ManagementConnection, sqlx::Error> {
    let pool = {
        let mut pools = POOLS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pools
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                SqlitePoolOptions::new()
                    .min_connections(0)
                    .max_connections(MAX_CONNECTIONS)
                    .idle_timeout(Some(IDLE_TIMEOUT))
                    .test_before_acquire(false)
                    .connect_lazy_with(
                        SqliteConnectOptions::new()
                            .filename(path)
                            .create_if_missing(false)
                            .foreign_keys(true),
                    )
            })
            .clone()
    };
    Ok(ManagementConnection(pool.acquire().await?))
}
