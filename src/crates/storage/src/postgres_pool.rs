//! Process-wide connection pools for `PostgreSQL`.
//!
//! Opening a `PostgreSQL` connection costs a TCP handshake, TLS negotiation
//! and authentication. The repositories ask for a connection for every query,
//! so they borrow one from a pool kept per database URL instead.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use sqlx::{
    PgConnection, PgPool, Postgres,
    pool::PoolConnection,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// Most connections a process keeps open to one database.
const MAX_CONNECTIONS: u32 = 16;
/// How long an unused connection stays open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

static POOLS: OnceLock<Mutex<HashMap<String, PgPool>>> = OnceLock::new();

/// A pooled `PostgreSQL` connection. It goes back to the pool when dropped.
pub struct PostgresConnection(PoolConnection<Postgres>);

impl PostgresConnection {
    /// Returns the connection to the pool. It stays open for the next query.
    ///
    /// # Errors
    ///
    /// Never fails; it is awaited like [`sqlx::Connection::close`].
    pub fn close(self) -> std::future::Ready<Result<(), sqlx::Error>> {
        drop(self);
        std::future::ready(Ok(()))
    }
}

impl std::fmt::Debug for PostgresConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresConnection")
            .finish_non_exhaustive()
    }
}

impl Deref for PostgresConnection {
    type Target = PgConnection;

    fn deref(&self) -> &PgConnection {
        &self.0
    }
}

impl DerefMut for PostgresConnection {
    fn deref_mut(&mut self) -> &mut PgConnection {
        &mut self.0
    }
}

/// Borrows a connection to the `PostgreSQL` database at `url`.
///
/// A borrowed connection is checked before it is handed out, so a connection
/// the server or a finished runtime closed is replaced instead of failing.
///
/// # Errors
///
/// Returns the error of parsing `url` or of opening a new connection.
pub async fn postgres_connection(url: &str) -> Result<PostgresConnection, sqlx::Error> {
    let pool = {
        let mut pools = POOLS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(pool) = pools.get(url) {
            pool.clone()
        } else {
            let options: PgConnectOptions = url.parse()?;
            let pool = PgPoolOptions::new()
                .min_connections(0)
                .max_connections(MAX_CONNECTIONS)
                .idle_timeout(Some(IDLE_TIMEOUT))
                .connect_lazy_with(options);
            pools.insert(url.to_owned(), pool.clone());
            pool
        }
    };
    Ok(PostgresConnection(pool.acquire().await?))
}
