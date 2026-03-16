pub mod activity;
pub mod capability;
pub mod explain;
pub mod io;
pub mod locks;
pub mod pool;
pub mod registry;
pub mod schema;
pub mod statements;
pub mod tables;

use crate::store::Snapshot;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CollectorError {
    #[error("query failed: {0}")]
    Query(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("not available: {0}")]
    NotAvailable(String),
}

impl From<sqlx::Error> for CollectorError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => {
                let msg = db_err.message();
                if msg.contains("permission denied") {
                    CollectorError::Permission(msg.to_string())
                } else {
                    CollectorError::Query(msg.to_string())
                }
            }
            _ => CollectorError::Query(err.to_string()),
        }
    }
}

/// Collection frequency category.
///
/// Fast collectors capture volatile, rapidly-changing metrics (active queries,
/// locks). Slow collectors capture structural data that changes infrequently
/// (table sizes, schema metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorInterval {
    /// ~30s — volatile metrics (statements, activity, locks, explain).
    Fast,
    /// ~300s — structural metrics (tables, schema, io).
    Slow,
}

/// Every collector implements this trait.
///
/// # Adding a new database type
///
/// Implement [`pool::DatabasePool`] for your database's pool type, then
/// write collectors that downcast via a `require_*` helper. See
/// [`pool::PostgresPool`] and [`pool::require_postgres`] for the pattern.
#[async_trait]
pub trait Collector: Send + Sync {
    /// Human-readable name (e.g. "statements", "locks").
    fn name(&self) -> &'static str;

    /// Whether this collector runs on the fast or slow interval.
    fn interval(&self) -> CollectorInterval;

    /// Which capability flags this collector requires.
    fn requires(&self) -> &[&'static str];

    /// Run the collection, returning a snapshot to store.
    async fn collect(&self, pool: &dyn pool::DatabasePool) -> Result<Snapshot, CollectorError>;
}
