//! PostgreSQL integration tests using testcontainers.
//!
//! These tests spin up a PostgreSQL container programmatically and verify
//! the collector against a real database, without requiring an external
//! `DATABASE_URL` environment variable.
//!
//! # Requirements
//!
//! - Docker installed and running
//!
//! # Running
//!
//! ```bash
//! cargo test --test integration postgres_container
//! ```

use datapace_agent::collector;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

/// Start a PG container and return (container_handle, connection_url).
///
/// If Docker is not available, returns None so callers can skip the test.
async fn start_postgres() -> Option<(testcontainers::ContainerAsync<Postgres>, String)> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping test (Docker not available): {e}");
            return None;
        }
    };

    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{host_port}/postgres");

    Some((container, url))
}

#[tokio::test]
async fn test_collector_with_container() {
    let Some((_container, url)) = start_postgres().await else {
        return;
    };

    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    let payload = collector.collect().await.expect("Collection failed");

    // Verify payload structure
    assert!(
        !payload.agent_version.is_empty(),
        "agent_version must be set"
    );
    assert_eq!(
        payload.database.database_type, "postgres",
        "database_type should be postgres"
    );

    // Table stats should be present (may be empty on a fresh DB)
    assert!(
        payload.table_stats.is_some(),
        "table_stats should be present"
    );

    // Index stats should be present
    assert!(
        payload.index_stats.is_some(),
        "index_stats should be present"
    );

    // Settings should be present
    assert!(payload.settings.is_some(), "settings should be present");
}

#[tokio::test]
async fn test_collector_connection_test() {
    let Some((_container, url)) = start_postgres().await else {
        return;
    };

    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    let result = collector.test_connection().await;
    assert!(
        result.is_ok(),
        "test_connection should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_collector_provider_generic() {
    let Some((_container, url)) = start_postgres().await else {
        return;
    };

    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    let provider = collector.provider();
    assert_eq!(
        provider, "generic",
        "Vanilla PG container should detect as generic provider"
    );
}

#[tokio::test]
async fn test_collector_version_detection() {
    let Some((_container, url)) = start_postgres().await else {
        return;
    };

    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    let version = collector.version();
    assert!(version.is_some(), "Version should be detected");

    let version_str = version.unwrap();
    assert!(
        version_str.contains("PostgreSQL"),
        "Version should contain 'PostgreSQL', got: {}",
        version_str
    );
}
