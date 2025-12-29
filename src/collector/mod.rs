//! Database metrics collectors.
//!
//! This module defines the `Collector` trait and implementations for
//! various database types and cloud providers.
//!
//! # Supported Databases
//!
//! - **PostgreSQL** - Full support (pg_stat_statements, pg_stat_user_tables, etc.)
//! - **MySQL** - Planned (performance_schema, information_schema)
//! - **MongoDB** - Planned (serverStatus, dbStats)
//!
//! # Adding a New Database
//!
//! See `docs/EXTENDING.md` for a complete guide on adding support for new databases.

pub mod postgres;
// pub mod mysql;    // Coming soon
// pub mod mongodb;  // Coming soon

use crate::config::DatabaseType;
use crate::payload::Payload;
use async_trait::async_trait;
use thiserror::Error;

/// Errors that can occur during metric collection
#[derive(Error, Debug)]
pub enum CollectorError {
    #[error("Database connection failed: {0}")]
    ConnectionError(String),

    #[error("Query execution failed: {0}")]
    QueryError(String),

    #[error("Permission denied: {0}")]
    PermissionError(String),

    #[error("Unsupported database version: {0}")]
    UnsupportedVersion(String),

    #[error("Unsupported database type: {0}")]
    UnsupportedDatabase(String),

    #[error("Provider detection failed: {0}")]
    DetectionError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<sqlx::Error> for CollectorError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::Database(db_err) => {
                let msg = db_err.message();
                if msg.contains("permission denied") {
                    CollectorError::PermissionError(msg.to_string())
                } else {
                    CollectorError::QueryError(msg.to_string())
                }
            }
            sqlx::Error::Io(io_err) => {
                CollectorError::ConnectionError(io_err.to_string())
            }
            _ => CollectorError::InternalError(err.to_string()),
        }
    }
}

/// Trait for database metrics collectors
///
/// Implementations of this trait collect metrics from specific database types
/// and providers, returning a normalized `Payload` that can be sent to Datapace Cloud.
///
/// # Implementing a New Collector
///
/// ```ignore
/// use async_trait::async_trait;
/// use crate::collector::{Collector, CollectorError};
/// use crate::config::DatabaseType;
/// use crate::payload::Payload;
///
/// pub struct MyDatabaseCollector {
///     // ... connection pool and metadata
/// }
///
/// #[async_trait]
/// impl Collector for MyDatabaseCollector {
///     async fn collect(&self) -> Result<Payload, CollectorError> {
///         // Collect metrics and return normalized payload
///     }
///
///     async fn test_connection(&self) -> Result<(), CollectorError> {
///         // Test database connectivity
///     }
///
///     fn provider(&self) -> &str {
///         // Return detected cloud provider (e.g., "rds", "neon", "generic")
///     }
///
///     fn version(&self) -> Option<&str> {
///         // Return database version
///     }
///
///     fn database_type(&self) -> DatabaseType {
///         // Return the database type
///     }
/// }
/// ```
#[async_trait]
pub trait Collector: Send + Sync {
    /// Collect metrics from the database
    ///
    /// Returns a `Payload` containing all collected metrics and metadata.
    async fn collect(&self) -> Result<Payload, CollectorError>;

    /// Test the database connection
    ///
    /// Returns `Ok(())` if the connection is successful, or an error describing
    /// what went wrong.
    async fn test_connection(&self) -> Result<(), CollectorError>;

    /// Get the detected cloud provider (e.g., "rds", "aurora", "neon", "generic")
    fn provider(&self) -> &str;

    /// Get the database version
    fn version(&self) -> Option<&str>;

    /// Get the database type
    fn database_type(&self) -> DatabaseType;
}

/// Factory function to create a collector based on database URL
///
/// Automatically detects the database type from the URL and creates
/// the appropriate collector implementation.
///
/// # Supported Database Types
///
/// - `postgres://` or `postgresql://` - PostgreSQL
/// - `mysql://` or `mariadb://` - MySQL (coming soon)
/// - `mongodb://` or `mongodb+srv://` - MongoDB (coming soon)
///
/// # Example
///
/// ```ignore
/// let collector = create_collector(
///     "postgres://user:pass@localhost/mydb",
///     Provider::Auto,
/// ).await?;
///
/// let payload = collector.collect().await?;
/// ```
pub async fn create_collector(
    database_url: &str,
    provider: crate::config::Provider,
) -> Result<Box<dyn Collector>, CollectorError> {
    // Detect database type from URL
    let db_type = DatabaseType::from_url(database_url)
        .map_err(|e| CollectorError::UnsupportedDatabase(e.to_string()))?;

    match db_type {
        // Implemented
        DatabaseType::Postgres => {
            let collector = postgres::PostgresCollector::new(database_url, provider).await?;
            Ok(Box::new(collector))
        }

        // PostgreSQL-compatible (can use PostgreSQL collector)
        DatabaseType::Timescaledb | DatabaseType::Cockroachdb | DatabaseType::Yugabytedb => {
            // These are PostgreSQL-compatible, use the Postgres collector
            let collector = postgres::PostgresCollector::new(database_url, provider).await?;
            Ok(Box::new(collector))
        }

        DatabaseType::Redshift => {
            // Redshift is PostgreSQL-compatible with some limitations
            let collector = postgres::PostgresCollector::new(database_url, provider).await?;
            Ok(Box::new(collector))
        }

        // Coming soon - Relational
        DatabaseType::Mysql | DatabaseType::Tidb => {
            Err(CollectorError::UnsupportedDatabase(
                "MySQL/TiDB support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Sqlserver => {
            Err(CollectorError::UnsupportedDatabase(
                "SQL Server support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Oracle => {
            Err(CollectorError::UnsupportedDatabase(
                "Oracle support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Db2 => {
            Err(CollectorError::UnsupportedDatabase(
                "IBM DB2 support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        // Coming soon - Document
        DatabaseType::Mongodb => {
            Err(CollectorError::UnsupportedDatabase(
                "MongoDB support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Couchbase => {
            Err(CollectorError::UnsupportedDatabase(
                "Couchbase support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Cosmosdb => {
            Err(CollectorError::UnsupportedDatabase(
                "Azure Cosmos DB support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        // Coming soon - Analytics
        DatabaseType::Elasticsearch => {
            Err(CollectorError::UnsupportedDatabase(
                "Elasticsearch support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Clickhouse => {
            Err(CollectorError::UnsupportedDatabase(
                "ClickHouse support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Snowflake => {
            Err(CollectorError::UnsupportedDatabase(
                "Snowflake support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Bigquery => {
            Err(CollectorError::UnsupportedDatabase(
                "BigQuery support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        // Coming soon - Key-Value
        DatabaseType::Redis => {
            Err(CollectorError::UnsupportedDatabase(
                "Redis support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Dynamodb => {
            Err(CollectorError::UnsupportedDatabase(
                "DynamoDB support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        // Coming soon - Time-series
        DatabaseType::Influxdb => {
            Err(CollectorError::UnsupportedDatabase(
                "InfluxDB support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        // Coming soon - Vector databases
        DatabaseType::Pinecone => {
            Err(CollectorError::UnsupportedDatabase(
                "Pinecone support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Milvus => {
            Err(CollectorError::UnsupportedDatabase(
                "Milvus support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Weaviate => {
            Err(CollectorError::UnsupportedDatabase(
                "Weaviate support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Qdrant => {
            Err(CollectorError::UnsupportedDatabase(
                "Qdrant support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Chroma => {
            Err(CollectorError::UnsupportedDatabase(
                "Chroma support is coming soon. See docs/EXTENDING.md for contribution guide.".to_string()
            ))
        }

        DatabaseType::Pgvector => {
            // pgvector is a PostgreSQL extension, use PostgreSQL collector
            let collector = postgres::PostgresCollector::new(database_url, provider).await?;
            Ok(Box::new(collector))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_error_display() {
        let err = CollectorError::ConnectionError("timeout".to_string());
        assert!(err.to_string().contains("timeout"));
    }
}
