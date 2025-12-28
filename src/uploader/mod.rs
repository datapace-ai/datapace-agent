//! Datapace Cloud uploader.
//!
//! Handles sending metrics payloads to the Datapace Cloud API with:
//! - Retry logic with exponential backoff
//! - Request compression (gzip)
//! - Error handling and rate limiting

use crate::payload::Payload;
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

    /// API key for authentication
    pub api_key: String,

    /// Request timeout
    pub timeout: Duration,

    /// Maximum number of retries
    pub max_retries: u32,

    /// Enable gzip compression
    pub compress: bool,
}

impl UploaderConfig {
    pub fn new(endpoint: String, api_key: String) -> Self {
        Self {
            endpoint,
            api_key,
            timeout: Duration::from_secs(30),
            max_retries: 3,
            compress: true,
        }
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
            .user_agent(format!(
                "datapace-agent/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;

        Ok(Self { client, config })
    }

    /// Upload a payload to Datapace Cloud
    pub async fn upload(&self, payload: &Payload) -> Result<(), UploaderError> {
        let json = serde_json::to_vec(payload)?;

        info!(
            endpoint = %self.config.endpoint,
            payload_size = json.len(),
            "Uploading metrics to Datapace Cloud"
        );

        let mut last_error = None;
        let mut retry_delay = Duration::from_secs(1);

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
                    if let UploaderError::RateLimited { retry_after } = &e {
                        if let Some(duration) = retry_after {
                            retry_delay = *duration;
                        }
                    }

                    error!(error = %e, attempt, "Upload attempt failed");
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(UploaderError::MaxRetriesExceeded))
    }

    async fn send_request(&self, body: &[u8]) -> Result<(), UploaderError> {
        let response = self
            .client
            .post(&self.config.endpoint)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("X-Agent-Version", env!("CARGO_PKG_VERSION"))
            .body(body.to_vec())
            .send()
            .await?;

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

    /// Test the connection to Datapace Cloud
    pub async fn test_connection(&self) -> Result<(), UploaderError> {
        debug!("Testing connection to Datapace Cloud");

        // Try a HEAD request or a lightweight health check
        let response = self
            .client
            .get(format!("{}/health", self.config.endpoint.trim_end_matches("/ingest")))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uploader_config_defaults() {
        let config = UploaderConfig::new(
            "https://api.datapace.ai/v1/ingest".to_string(),
            "test_key".to_string(),
        );

        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert!(config.compress);
    }

    #[test]
    fn test_error_display() {
        let err = UploaderError::ServerError {
            status: 500,
            message: "Internal error".to_string(),
        };
        assert!(err.to_string().contains("500"));
    }
}
