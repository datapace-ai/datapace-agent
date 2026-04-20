//! End-to-end integration tests.
//!
//! Combines testcontainers (PostgreSQL) and wiremock (mock platform API)
//! for full lifecycle tests.

use datapace_agent::collector;
use datapace_agent::health::{HealthState, SharedHealthState};
use datapace_agent::uploader::{Upload, Uploader, UploaderConfig};
use std::sync::Arc;
use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::RwLock;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Start a PG container and return (container_handle, connection_url).
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
async fn test_full_collection_cycle() {
    // Start PG container
    let Some((_container, db_url)) = start_postgres().await else {
        return;
    };

    // Start wiremock server
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .and(header_exists("X-Signature"))
        .and(header_exists("X-Signature-Timestamp"))
        .and(header_exists("Authorization"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create collector from PG container
    let collector = collector::create_collector(&db_url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    // Create uploader pointed at mock server
    let config = UploaderConfig {
        endpoint: format!("{}/v1/ingest", mock_server.uri()),
        api_key: "e2e-test-api-key".to_string(),
        signing_secret: "e2e-test-api-key".to_string(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
        compress: false,
        initial_retry_delay: Duration::from_secs(1),
    };
    let uploader = Uploader::new(config).expect("Failed to create uploader");

    // Run one collection cycle
    let payload = collector.collect().await.expect("Collection failed");
    uploader.upload(&payload).await.expect("Upload failed");

    // Verify the mock received exactly 1 POST request
    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "Expected exactly 1 request to mock server"
    );

    let request = &requests[0];

    // Verify request body is valid JSON with expected Payload structure
    let body: serde_json::Value =
        serde_json::from_slice(&request.body).expect("Request body should be valid JSON");

    assert!(
        body.get("agent_version").is_some(),
        "Payload should contain agent_version"
    );
    assert!(
        body.get("database").is_some(),
        "Payload should contain database"
    );
    assert_eq!(
        body["database"]["type"], "postgres",
        "database.type should be postgres"
    );

    // Verify HMAC headers are present
    assert!(
        request.headers.get("X-Signature").is_some(),
        "X-Signature header must be present"
    );
    assert!(
        request.headers.get("X-Signature-Timestamp").is_some(),
        "X-Signature-Timestamp header must be present"
    );

    // Verify Authorization header contains the API key
    let auth = request
        .headers
        .get("Authorization")
        .expect("Authorization header missing")
        .to_str()
        .unwrap();
    assert!(
        auth.contains("e2e-test-api-key"),
        "Authorization should contain the API key"
    );
}

#[tokio::test]
async fn test_health_endpoint_during_operation() {
    // Start PG container
    let Some((_container, db_url)) = start_postgres().await else {
        return;
    };

    // Create shared health state
    let health_state: SharedHealthState = Arc::new(RwLock::new(HealthState::new()));

    // Start health server on a random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().expect("failed to get local addr");

    let health_state_clone = health_state.clone();
    let app = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(
                |axum::extract::State(state): axum::extract::State<SharedHealthState>| async move {
                    let s = state.read().await;
                    axum::Json(s.clone())
                },
            ),
        )
        .with_state(health_state_clone);

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server failed");
    });

    // Create collector and run collect
    let collector = collector::create_collector(&db_url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    let _payload = collector.collect().await.expect("Collection failed");

    // Update health state to reflect a successful collection
    {
        let mut state = health_state.write().await;
        state.last_collection_time = Some(chrono::Utc::now());
        state.database_connected = true;
        state.status = "ok".to_string();
    }

    // Query the health endpoint
    let url = format!("http://{}/health", addr);
    let resp = reqwest::get(&url).await.expect("request failed");
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("failed to parse JSON");

    assert_eq!(body["status"], "ok");
    assert!(
        !body["last_collection_time"].is_null(),
        "last_collection_time should be set after collection"
    );
    assert_eq!(
        body["database_connected"], true,
        "database_connected should be true"
    );
}
