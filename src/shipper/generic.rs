use super::{Shipper, ShipperError};
use crate::store::Store;
use reqwest::Client;
use std::time::Duration;
use tracing::{error, info};

pub struct GenericShipper {
    client: Client,
    url: String,
    token: Option<String>,
}

impl GenericShipper {
    pub fn new(url: String, token: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .gzip(true)
            .user_agent(format!("datapace-agent/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to build HTTP client");

        Self { client, url, token }
    }
}

#[async_trait::async_trait]
impl Shipper for GenericShipper {
    async fn ship(&self, store: &Store) -> Result<(), ShipperError> {
        let snapshots = store.get_latest_snapshots().await?;
        if snapshots.is_empty() {
            return Ok(());
        }

        let body = serde_json::to_vec(&snapshots)
            .map_err(|e| ShipperError::Serialization(e.to_string()))?;

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .body(body);

        if let Some(ref token) = self.token {
            req = req.header("Authorization", token.as_str());
        }

        let resp = req.send().await?;

        if resp.status().is_success() {
            info!("Shipped to generic HTTPS endpoint");
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            error!(status, "Generic shipper upload failed");
            Err(ShipperError::Server {
                status,
                message: msg,
            })
        }
    }
}
