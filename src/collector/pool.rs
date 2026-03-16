use super::CollectorError;
use async_trait::async_trait;
use std::any::Any;

/// Database-agnostic pool abstraction.
///
/// Implement this trait to add support for a new database technology.
/// Each implementation wraps its native pool type and exposes it via
/// [`as_any()`](DatabasePool::as_any) for downcasting.
///
/// # Example: adding MySQL support
///
/// ```ignore
/// pub struct MySqlPool(pub sqlx::MySqlPool);
///
/// #[async_trait]
/// impl DatabasePool for MySqlPool {
///     fn as_any(&self) -> &dyn Any { self }
///     fn db_type(&self) -> &'static str { "mysql" }
///     async fn close(&self) { self.0.close().await; }
/// }
/// ```
#[async_trait]
pub trait DatabasePool: Send + Sync + 'static {
    /// Downcast to the concrete pool type.
    fn as_any(&self) -> &dyn Any;

    /// Short identifier for the database type (e.g. "postgres", "mysql").
    fn db_type(&self) -> &'static str;

    /// Gracefully close the pool. Default is a no-op.
    async fn close(&self) {}
}

/// Wraps a [`sqlx::PgPool`] as a [`DatabasePool`].
pub struct PostgresPool(pub sqlx::PgPool);

#[async_trait]
impl DatabasePool for PostgresPool {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn db_type(&self) -> &'static str {
        "postgres"
    }

    async fn close(&self) {
        self.0.close().await;
    }
}

/// Downcast a [`DatabasePool`] to a [`sqlx::PgPool`] reference, or return
/// [`CollectorError::NotAvailable`] if the pool is not PostgreSQL.
pub fn require_postgres(pool: &dyn DatabasePool) -> Result<&sqlx::PgPool, CollectorError> {
    pool.as_any()
        .downcast_ref::<PostgresPool>()
        .map(|pg| &pg.0)
        .ok_or_else(|| {
            CollectorError::NotAvailable(format!("requires PostgreSQL, got {}", pool.db_type()))
        })
}
