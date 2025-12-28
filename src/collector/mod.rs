//! Database metrics collectors.
//!
//! This module defines the `Collector` trait and implementations for
//! various database providers.

pub mod postgres;

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

    /// Get the detected database provider
    fn provider(&self) -> &str;

    /// Get the database version
    fn version(&self) -> Option<&str>;
}

/// Factory function to create a collector based on configuration
pub async fn create_collector(
    database_url: &str,
    provider: crate::config::Provider,
) -> Result<Box<dyn Collector>, CollectorError> {
    // For now, we only support PostgreSQL
    let collector = postgres::PostgresCollector::new(database_url, provider).await?;
    Ok(Box::new(collector))
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
