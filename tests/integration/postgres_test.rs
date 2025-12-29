//! PostgreSQL integration tests
//!
//! These tests verify the PostgreSQL collector against a real database.
//!
//! # Requirements
//!
//! - Docker installed and running
//! - Or a PostgreSQL instance with pg_stat_statements enabled
//!
//! # Running
//!
//! ```bash
//! # Start PostgreSQL in Docker
//! docker run --rm -d \
//!     --name datapace-test-pg \
//!     -e POSTGRES_PASSWORD=testpass \
//!     -e POSTGRES_DB=testdb \
//!     -p 5432:5432 \
//!     postgres:16-alpine \
//!     -c shared_preload_libraries=pg_stat_statements
//!
//! # Run tests
//! DATABASE_URL=postgres://postgres:testpass@localhost:5432/testdb \
//!     cargo test --test integration postgres
//!
//! # Cleanup
//! docker stop datapace-test-pg
//! ```

use std::env;

/// Get the test database URL from environment or use default
fn get_test_database_url() -> Option<String> {
    env::var("DATABASE_URL").ok()
}

/// Skip test if no database URL is configured
macro_rules! require_database {
    () => {
        match get_test_database_url() {
            Some(url) => url,
            None => {
                eprintln!("Skipping test: DATABASE_URL not set");
                return;
            }
        }
    };
}

#[tokio::test]
async fn test_postgres_connection() {
    let database_url = require_database!();

    let collector = datapace_agent::collector::postgres::PostgresCollector::new(
        &database_url,
        datapace_agent::config::Provider::Auto,
    )
    .await;

    assert!(collector.is_ok(), "Failed to connect: {:?}", collector.err());

    let collector = collector.unwrap();
    let result = collector.test_connection().await;
    assert!(result.is_ok(), "Connection test failed: {:?}", result.err());
}

#[tokio::test]
async fn test_postgres_collect() {
    let database_url = require_database!();

    let collector = datapace_agent::collector::postgres::PostgresCollector::new(
        &database_url,
        datapace_agent::config::Provider::Auto,
    )
    .await
    .expect("Failed to create collector");

    let payload = collector.collect().await;
    assert!(payload.is_ok(), "Collection failed: {:?}", payload.err());

    let payload = payload.unwrap();

    // Verify payload structure
    assert!(!payload.agent_version.is_empty());
    assert!(!payload.instance_id.is_empty());
    assert_eq!(payload.database.database_type, "postgres");
    assert!(payload.database.version.is_some());

    // Table stats should be present (even if empty)
    assert!(payload.table_stats.is_some());

    // Index stats should be present (even if empty)
    assert!(payload.index_stats.is_some());

    // Settings should be present
    assert!(payload.settings.is_some());

    // Schema should be present
    assert!(payload.schema.is_some());
}

#[tokio::test]
async fn test_postgres_provider_detection() {
    let database_url = require_database!();

    let collector = datapace_agent::collector::postgres::PostgresCollector::new(
        &database_url,
        datapace_agent::config::Provider::Auto,
    )
    .await
    .expect("Failed to create collector");

    let provider = collector.provider();
    assert!(!provider.is_empty(), "Provider should be detected");

    // For local testing, provider should be "generic"
    // For cloud testing, it could be "rds", "neon", "supabase", etc.
    println!("Detected provider: {}", provider);
}

#[tokio::test]
async fn test_postgres_version_detection() {
    let database_url = require_database!();

    let collector = datapace_agent::collector::postgres::PostgresCollector::new(
        &database_url,
        datapace_agent::config::Provider::Auto,
    )
    .await
    .expect("Failed to create collector");

    let version = collector.version();
    assert!(version.is_some(), "Version should be detected");
    assert!(version.unwrap().contains("PostgreSQL"), "Version should contain PostgreSQL");

    println!("Detected version: {}", version.unwrap());
}

#[tokio::test]
async fn test_postgres_database_type() {
    let database_url = require_database!();

    use datapace_agent::collector::Collector;

    let collector = datapace_agent::collector::postgres::PostgresCollector::new(
        &database_url,
        datapace_agent::config::Provider::Auto,
    )
    .await
    .expect("Failed to create collector");

    let db_type = collector.database_type();
    assert_eq!(db_type, datapace_agent::config::DatabaseType::Postgres);
}

// ============================================================================
// Test Utilities
// ============================================================================

#[cfg(test)]
mod test_utils {
    /// Create a test table for integration tests
    #[allow(dead_code)]
    pub async fn setup_test_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS datapace_test_table (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Insert some test data
        sqlx::query(
            r#"
            INSERT INTO datapace_test_table (name)
            SELECT 'test_' || i
            FROM generate_series(1, 100) AS i
            ON CONFLICT DO NOTHING
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Clean up test table
    #[allow(dead_code)]
    pub async fn cleanup_test_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        sqlx::query("DROP TABLE IF EXISTS datapace_test_table")
            .execute(pool)
            .await?;
        Ok(())
    }
}
