use super::{Shipper, ShipperError};
use crate::store::Store;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub struct DatapaceShipper {
    client: Client,
    endpoint: String,
    api_key: String,
}

impl DatapaceShipper {
    pub fn new(endpoint: String, api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .gzip(true)
            .user_agent(format!("datapace-agent/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            endpoint,
            api_key,
        }
    }
}

#[async_trait::async_trait]
impl Shipper for DatapaceShipper {
    async fn ship(&self, store: &Store) -> Result<(), ShipperError> {
        let snapshots = store.get_latest_snapshots().await?;
        if snapshots.is_empty() {
            debug!("No snapshots to ship");
            return Ok(());
        }

        let body = serde_json::to_vec(&snapshots)
            .map_err(|e| ShipperError::Serialization(e.to_string()))?;

        info!(
            endpoint = %self.endpoint,
            payload_bytes = body.len(),
            snapshots = snapshots.len(),
            "Shipping to Datapace Cloud"
        );

        let mut last_error = None;
        let mut delay = Duration::from_secs(1);

        for attempt in 0..=3u32 {
            if attempt > 0 {
                warn!(attempt, "Retrying Datapace upload");
                tokio::time::sleep(delay).await;
                delay *= 2;
            }

            let resp = self
                .client
                .post(&self.endpoint)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("X-Agent-Version", env!("CARGO_PKG_VERSION"))
                .body(body.clone())
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    info!("Shipped to Datapace Cloud successfully");
                    return Ok(());
                }
                Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => {
                    let msg = r.text().await.unwrap_or_default();
                    return Err(ShipperError::Server {
                        status: 401,
                        message: msg,
                    });
                }
                Ok(r) => {
                    let status = r.status().as_u16();
                    let msg = r.text().await.unwrap_or_default();
                    error!(status, "Datapace upload failed");
                    last_error = Some(ShipperError::Server {
                        status,
                        message: msg,
                    });
                }
                Err(e) => {
                    error!(error = %e, "Datapace request error");
                    last_error = Some(ShipperError::Request(e));
                }
            }
        }

        Err(last_error.unwrap_or(ShipperError::Server {
            status: 0,
            message: "max retries exceeded".into(),
        }))
    }
}
