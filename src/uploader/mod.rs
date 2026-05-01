//! Datapace Cloud uploader.
//!
//! Handles sending metrics payloads to the Datapace Cloud API with:
//! - Retry logic with exponential backoff
//! - Request compression (gzip)
//! - HMAC payload signing
//! - Error handling and rate limiting

use crate::payload::Payload;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Errors that can occur during upload
#[derive(Error, Debug)]
pub enum UploaderError {
    #[error("Request failed: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("Server returned error {status}: {message}")]
    ServerError { status: u16, message: String },

    #[error("Rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Option<Duration> },

    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Payload serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,
}

/// Configuration for the uploader
#[derive(Debug, Clone)]
pub struct UploaderConfig {
    /// API endpoint URL
    pub endpoint: String,

    /// API key for authentication (sent as `Authorization: Bearer`).
    pub api_key: String,

    /// Per-connection HMAC-SHA256 secret used to sign payloads. Required and
    /// distinct from `api_key` by design: the API key authenticates the
    /// request (and is sent in every header), the signing secret never
    /// travels and proves body integrity on the network path.
    pub signing_secret: String,

    /// Request timeout
    pub timeout: Duration,

    /// Maximum number of retries
    pub max_retries: u32,

    /// Enable gzip compression
    pub compress: bool,

    /// Initial retry delay (doubles on each retry via exponential backoff)
    pub initial_retry_delay: Duration,
}

impl UploaderConfig {
    /// Build a new uploader config. `signing_secret` must be a distinct value
    /// from `api_key` — see the [`signing_secret`](Self::signing_secret) field
    /// docs.
    pub fn new(endpoint: String, api_key: String, signing_secret: String) -> Self {
        Self {
            endpoint,
            api_key,
            signing_secret,
            timeout: Duration::from_secs(30),
            max_retries: 3,
            compress: true,
            initial_retry_delay: Duration::from_secs(1),
        }
    }
}

/// Trait for uploading payloads to Datapace Cloud
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Upload: Send + Sync {
    /// Upload a payload to Datapace Cloud
    async fn upload(&self, payload: &Payload) -> Result<(), UploaderError>;

    /// Test the connection to Datapace Cloud
    async fn test_connection(&self) -> Result<(), UploaderError>;

    /// Send a lightweight heartbeat to Datapace Cloud.
    ///
    /// The default implementation is a no-op that returns `Ok(())`.
    async fn send_heartbeat(
        &self,
        _status: &str,
        _agent_version: &str,
    ) -> Result<(), UploaderError> {
        Ok(())
    }
}

/// Uploader for sending payloads to Datapace Cloud
pub struct Uploader {
    client: Client,
    config: UploaderConfig,
}

impl Uploader {
    /// Create a new uploader with the given configuration
    pub fn new(config: UploaderConfig) -> Result<Self, UploaderError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .gzip(config.compress)
            .user_agent(format!("datapace-agent/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self { client, config })
    }

    /// Build a signed POST request with HMAC headers, `Authorization: Bearer`,
    /// and the raw `body` bytes as the request body.
    ///
    /// Shared by `send_request` (ingest) and `send_heartbeat` so both
    /// endpoints carry the same signature headers.
    fn signed_post(&self, url: &str, body: &[u8]) -> reqwest::RequestBuilder {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = self.compute_signature(body, &timestamp);

        self.client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("X-Agent-Version", env!("CARGO_PKG_VERSION"))
            .header("X-Signature", signature)
            .header("X-Signature-Timestamp", timestamp)
            .body(body.to_vec())
    }

    async fn send_request(&self, body: &[u8]) -> Result<(), UploaderError> {
        let response = self.signed_post(&self.config.endpoint, body).send().await?;

        let status = response.status();

        debug!(status = %status, "Received response from server");

        match status {
            StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED => Ok(()),

            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                let message = response.text().await.unwrap_or_default();
                Err(UploaderError::AuthError(message))
            }

            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(Duration::from_secs);

                Err(UploaderError::RateLimited { retry_after })
            }

            _ => {
                let message = response.text().await.unwrap_or_default();
                Err(UploaderError::ServerError {
                    status: status.as_u16(),
                    message,
                })
            }
        }
    }

    fn compute_signature(&self, body: &[u8], timestamp: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(self.config.signing_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }
}

#[async_trait]
impl Upload for Uploader {
    /// Upload a payload to Datapace Cloud
    async fn upload(&self, payload: &Payload) -> Result<(), UploaderError> {
        let json = serde_json::to_vec(payload)?;

        info!(
            endpoint = %self.config.endpoint,
            payload_size = json.len(),
            "Uploading metrics to Datapace Cloud"
        );

        let mut last_error = None;
        let mut retry_delay = self.config.initial_retry_delay;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                warn!(attempt, "Retrying upload after failure");
                tokio::time::sleep(retry_delay).await;
                retry_delay *= 2; // Exponential backoff
            }

            match self.send_request(&json).await {
                Ok(()) => {
                    info!("Metrics uploaded successfully");
                    return Ok(());
                }
                Err(e) => {
                    // Don't retry on auth errors
                    if matches!(e, UploaderError::AuthError(_)) {
                        return Err(e);
                    }

                    // Handle rate limiting
                    if let UploaderError::RateLimited {
                        retry_after: Some(duration),
                    } = &e
                    {
                        retry_delay = *duration;
                    }

                    error!(error = %e, attempt, "Upload attempt failed");
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(UploaderError::MaxRetriesExceeded))
    }

    /// Test the connection to Datapace Cloud
    async fn test_connection(&self) -> Result<(), UploaderError> {
        debug!("Testing connection to Datapace Cloud");

        // Try a HEAD request or a lightweight health check
        let response = self
            .client
            .get(format!(
                "{}/health",
                self.config.endpoint.trim_end_matches("/ingest")
            ))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if response.status().is_success() {
            info!("Connection to Datapace Cloud verified");
            Ok(())
        } else if response.status() == StatusCode::UNAUTHORIZED {
            Err(UploaderError::AuthError("Invalid API key".to_string()))
        } else {
            Err(UploaderError::ServerError {
                status: response.status().as_u16(),
                message: "Health check failed".to_string(),
            })
        }
    }

    /// Send a lightweight heartbeat to Datapace Cloud.
    ///
    /// Signed with the same HMAC scheme as `/ingest` so the platform can
    /// reject spoofed heartbeats from anyone who obtained only the API key.
    async fn send_heartbeat(&self, status: &str, agent_version: &str) -> Result<(), UploaderError> {
        // Derive the heartbeat URL from the ingest endpoint.
        // e.g. "https://api.datapace.ai/v1/ingest" → "https://api.datapace.ai/v1/heartbeat"
        let base = self.config.endpoint.trim_end_matches("/ingest");
        let url = format!("{}/heartbeat", base);

        let body = serde_json::json!({
            "status": status,
            "agent_version": agent_version,
            "timestamp": chrono::Utc::now(),
        });
        let body_bytes = serde_json::to_vec(&body)?;

        debug!(url = %url, "Sending heartbeat");

        let resp = self.signed_post(&url, &body_bytes).send().await?;

        if resp.status().is_success() {
            debug!("Heartbeat sent successfully");
            Ok(())
        } else {
            Err(UploaderError::ServerError {
                status: resp.status().as_u16(),
                message: format!("Heartbeat returned non-success status: {}", resp.status()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uploader_config_defaults() {
        let config = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "test_key".to_string(),
            "test-signing-secret".to_string(),
        );

        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert!(config.compress);
    }

    #[test]
    fn test_uploader_config_keeps_keys_distinct() {
        let config = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "api-key".to_string(),
            "distinct-signing-secret".to_string(),
        );
        assert_eq!(config.api_key, "api-key");
        assert_eq!(config.signing_secret, "distinct-signing-secret");
        assert_ne!(config.api_key, config.signing_secret);
    }

    #[test]
    fn test_error_display() {
        let err = UploaderError::ServerError {
            status: 500,
            message: "Internal error".to_string(),
        };
        assert!(err.to_string().contains("500"));
    }

    #[test]
    fn test_compute_signature_uses_signing_secret_not_api_key() {
        let config = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "api-key-xxx".to_string(),
            "separate-signing-secret".to_string(),
        );
        let uploader = Uploader::new(config).expect("Failed to create uploader");

        let body = b"payload";
        let timestamp = "1700000000";
        let signature = uploader.compute_signature(body, timestamp);

        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(b"separate-signing-secret")
            .expect("HMAC can take key of any size");
        mac.update(b"1700000000");
        mac.update(b".");
        mac.update(b"payload");
        let expected = hex::encode(mac.finalize().into_bytes());
        assert_eq!(signature, expected);

        let mut mac_bad =
            HmacSha256::new_from_slice(b"api-key-xxx").expect("HMAC can take key of any size");
        mac_bad.update(b"1700000000");
        mac_bad.update(b".");
        mac_bad.update(b"payload");
        let derived_from_api_key = hex::encode(mac_bad.finalize().into_bytes());
        assert_ne!(signature, derived_from_api_key);
    }

    #[test]
    fn test_compute_signature_deterministic() {
        let config = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "test-api-key-123".to_string(),
            "test-signing-secret".to_string(),
        );
        let uploader = Uploader::new(config).expect("Failed to create uploader");

        let body = b"test payload";
        let timestamp = "1700000000";

        let sig1 = uploader.compute_signature(body, timestamp);
        let sig2 = uploader.compute_signature(body, timestamp);

        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_compute_signature_different_signing_secrets() {
        let config1 = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "shared-key".to_string(),
            "secret-alpha".to_string(),
        );
        let uploader1 = Uploader::new(config1).expect("Failed to create uploader");

        let config2 = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "shared-key".to_string(),
            "secret-beta".to_string(),
        );
        let uploader2 = Uploader::new(config2).expect("Failed to create uploader");

        let body = b"same payload";
        let timestamp = "1700000000";

        let sig1 = uploader1.compute_signature(body, timestamp);
        let sig2 = uploader2.compute_signature(body, timestamp);

        assert_ne!(sig1, sig2);
    }
}
