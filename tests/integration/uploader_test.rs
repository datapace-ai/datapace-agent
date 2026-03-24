//! Uploader integration tests using wiremock.
//!
//! These tests verify the uploader against a mock HTTP server,
//! including retry logic, HMAC signing, and error handling.

use datapace_agent::payload::{DatabaseInfo, Payload};
use datapace_agent::uploader::{Upload, Uploader, UploaderConfig, UploaderError};
use std::collections::HashMap;
use std::time::Duration;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a minimal test payload for uploader tests.
fn test_payload() -> Payload {
    Payload::new(DatabaseInfo {
        database_type: "postgres".to_string(),
        version: Some("16.1".to_string()),
        provider: "generic".to_string(),
        provider_metadata: HashMap::new(),
    })
    .with_instance_id("test://localhost/testdb")
    .with_settings(HashMap::new())
}

/// Create an uploader pointing at the given mock server URI.
fn test_uploader(base_uri: &str) -> Uploader {
    let config = UploaderConfig {
        endpoint: format!("{}/v1/ingest", base_uri),
        api_key: "test-api-key-secret".to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 3,
        compress: false, // disable compression so wiremock receives plain JSON
        initial_retry_delay: Duration::from_millis(10),
    };
    Uploader::new(config).expect("Failed to create uploader")
}

#[tokio::test]
async fn test_upload_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let uploader = test_uploader(&mock_server.uri());
    let payload = test_payload();

    let result = uploader.upload(&payload).await;
    assert!(result.is_ok(), "Upload should succeed: {:?}", result.err());
}

#[tokio::test]
async fn test_upload_hmac_headers_present() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .and(header_exists("X-Signature"))
        .and(header_exists("X-Signature-Timestamp"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let uploader = test_uploader(&mock_server.uri());
    let payload = test_payload();

    let result = uploader.upload(&payload).await;
    assert!(
        result.is_ok(),
        "Upload should succeed with HMAC headers: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_upload_signature_valid() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let api_key = "test-api-key-secret";

    let config = UploaderConfig {
        endpoint: format!("{}/v1/ingest", mock_server.uri()),
        api_key: api_key.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 3,
        compress: false,
        initial_retry_delay: Duration::from_millis(10),
    };
    let uploader = Uploader::new(config).expect("Failed to create uploader");

    let payload = test_payload();
    let result = uploader.upload(&payload).await;
    assert!(result.is_ok(), "Upload should succeed: {:?}", result.err());

    // Retrieve the captured request
    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);

    let request = &requests[0];

    let signature_header = request
        .headers
        .get("X-Signature")
        .expect("X-Signature header missing")
        .to_str()
        .expect("Invalid header value");

    let timestamp_header = request
        .headers
        .get("X-Signature-Timestamp")
        .expect("X-Signature-Timestamp header missing")
        .to_str()
        .expect("Invalid header value");

    let body = &request.body;

    // Independently compute the expected HMAC-SHA256 signature
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(api_key.as_bytes()).expect("HMAC can take key of any size");
    mac.update(timestamp_header.as_bytes());
    mac.update(b".");
    mac.update(body);
    let expected_signature = hex::encode(mac.finalize().into_bytes());

    assert_eq!(
        signature_header, expected_signature,
        "HMAC signature mismatch"
    );
}

#[tokio::test]
async fn test_upload_retry_on_500() {
    let mock_server = MockServer::start().await;

    // Mount a mock that returns 500 for the first 2 requests, then 200.
    // wiremock matches mocks by priority (lower = higher), then insertion order.
    // Give the 500 mock higher priority so it matches first, with up_to_n_times(2)
    // to limit it to 2 responses. The 200 mock with lower priority is the fallback.
    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .up_to_n_times(2)
        .expect(2)
        .with_priority(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .with_priority(2)
        .mount(&mock_server)
        .await;

    // Use a short timeout, no compression, and short retry delay for speed
    let config = UploaderConfig {
        endpoint: format!("{}/v1/ingest", mock_server.uri()),
        api_key: "test-api-key-secret".to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 3,
        compress: false,
        initial_retry_delay: Duration::from_millis(10),
    };
    let uploader = Uploader::new(config).expect("Failed to create uploader");

    let payload = test_payload();
    let result = uploader.upload(&payload).await;
    assert!(
        result.is_ok(),
        "Upload should succeed after retries: {:?}",
        result.err()
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        3,
        "Expected 3 requests (2 failures + 1 success)"
    );
}

#[tokio::test]
async fn test_upload_no_retry_on_401() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let uploader = test_uploader(&mock_server.uri());
    let payload = test_payload();

    let result = uploader.upload(&payload).await;
    assert!(result.is_err(), "Upload should fail with 401");

    match result.unwrap_err() {
        UploaderError::AuthError(_) => {} // expected
        other => panic!("Expected AuthError, got: {:?}", other),
    }

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "Should not retry on 401");
}

#[tokio::test]
async fn test_upload_max_retries_exceeded() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .expect(4) // 1 initial + 3 retries
        .mount(&mock_server)
        .await;

    let uploader = test_uploader(&mock_server.uri());
    let payload = test_payload();

    let result = uploader.upload(&payload).await;
    assert!(result.is_err(), "Upload should fail after max retries");

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        4,
        "Expected 4 requests (1 initial + 3 retries)"
    );
}

#[tokio::test]
async fn test_upload_rate_limit_429() {
    let mock_server = MockServer::start().await;

    // First request returns 429 with Retry-After header
    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_string("Too Many Requests"),
        )
        .up_to_n_times(1)
        .expect(1)
        .with_priority(1)
        .mount(&mock_server)
        .await;

    // Subsequent requests return 200
    Mock::given(method("POST"))
        .and(path("/v1/ingest"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .with_priority(2)
        .mount(&mock_server)
        .await;

    let config = UploaderConfig {
        endpoint: format!("{}/v1/ingest", mock_server.uri()),
        api_key: "test-api-key-secret".to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 3,
        compress: false,
        initial_retry_delay: Duration::from_millis(10),
    };
    let uploader = Uploader::new(config).expect("Failed to create uploader");

    let payload = test_payload();
    let result = uploader.upload(&payload).await;
    assert!(
        result.is_ok(),
        "Upload should succeed after 429 rate limit: {:?}",
        result.err()
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        2,
        "Expected 2 requests (1 rate-limited + 1 success)"
    );
}
