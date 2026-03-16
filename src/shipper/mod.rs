pub mod datapace;
pub mod generic;
pub mod prometheus;

use crate::config::ShipperConfig;
use crate::store::Store;
use std::sync::Arc;

/// Trait for all shipper implementations.
#[async_trait::async_trait]
pub trait Shipper: Send + Sync {
    /// Ship any pending data. Called after each scheduler tick.
    async fn ship(&self, store: &Store) -> Result<(), ShipperError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ShipperError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("server error {status}: {message}")]
    Server { status: u16, message: String },
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("store error: {0}")]
    Store(#[from] sqlx::Error),
}

/// Create the appropriate shipper based on config, or None if target = none.
pub fn create_shipper(config: &ShipperConfig) -> Option<Arc<dyn Shipper>> {
    match config.target {
        crate::config::ShipperTarget::None => None,
        crate::config::ShipperTarget::Datapace => {
            let endpoint = config
                .endpoint
                .clone()
                .unwrap_or_else(|| "https://api.datapace.ai/v1/ingest".into());
            let api_key = config.api_key.clone().unwrap_or_default();
            Some(Arc::new(datapace::DatapaceShipper::new(endpoint, api_key)))
        }
        crate::config::ShipperTarget::Prometheus => {
            // Prometheus is a pull-based exporter, handled in the UI/HTTP server
            None
        }
        crate::config::ShipperTarget::GenericHttps => {
            let url = config.generic_url.clone().unwrap_or_default();
            let token = config.generic_token.clone();
            Some(Arc::new(generic::GenericShipper::new(url, token)))
        }
    }
}
