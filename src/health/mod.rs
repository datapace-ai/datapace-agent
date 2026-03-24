//! Health check endpoint for the Datapace Agent.
//!
//! Provides an HTTP endpoint that exposes agent status, uptime,
//! and last collection information for monitoring and orchestration.

use crate::config::HealthConfig;
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};
use tracing::info;

/// Shared health state accessible from both the scheduler and the HTTP handler.
pub type SharedHealthState = Arc<RwLock<HealthState>>;

/// Snapshot of agent health information returned by the health endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct HealthState {
    pub status: String,
    pub agent_version: String,
    pub uptime_secs: u64,
    pub last_collection_time: Option<DateTime<Utc>>,
    pub last_collection_duration_ms: Option<u64>,
    pub last_collection_error: Option<String>,
    pub database_connected: bool,
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            status: "ok".to_string(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: 0,
            last_collection_time: None,
            last_collection_duration_ms: None,
            last_collection_error: None,
            database_connected: false,
        }
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the health HTTP server.
///
/// Binds to `0.0.0.0:{config.port}` and serves the health endpoint at `config.path`.
/// Shuts down gracefully when `shutdown_rx` receives `true`.
pub async fn start_health_server(
    config: &HealthConfig,
    state: SharedHealthState,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route(&config.path, get(health_handler))
        .with_state(state);

    let addr: std::net::SocketAddr = ([0, 0, 0, 0], config.port).into();
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!(port = config.port, path = %config.path, "Health server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Wait until shutdown_rx receives `true`
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
                if *shutdown_rx.borrow() {
                    break;
                }
            }
        })
        .await?;

    info!("Health server stopped");
    Ok(())
}

async fn health_handler(State(state): State<SharedHealthState>) -> Json<HealthState> {
    let state = state.read().await;
    Json(state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_state_serialization() {
        let state = HealthState::new();
        let json = serde_json::to_value(&state).expect("serialization should succeed");

        assert_eq!(json["status"], "ok");
        assert_eq!(json["agent_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["uptime_secs"], 0);
        assert!(json["last_collection_time"].is_null());
        assert!(json["last_collection_duration_ms"].is_null());
        assert!(json["last_collection_error"].is_null());
        assert_eq!(json["database_connected"], false);
    }

    #[tokio::test]
    async fn test_health_server_responds() {
        let state: SharedHealthState = Arc::new(RwLock::new(HealthState::new()));

        // Bind to a random OS-assigned port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind");
        let addr = listener.local_addr().expect("failed to get local addr");

        let app = Router::new()
            .route("/health", get(health_handler))
            .with_state(state);

        // Spawn the server
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server failed");
        });

        // Send a GET request to the health endpoint
        let url = format!("http://{}/health", addr);
        let resp = reqwest::get(&url).await.expect("request failed");

        assert_eq!(resp.status(), 200);

        let body: serde_json::Value = resp.json().await.expect("failed to parse JSON");
        assert_eq!(body["status"], "ok");
        assert_eq!(body["agent_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(body["database_connected"], false);
    }
}
