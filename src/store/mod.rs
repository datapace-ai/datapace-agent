mod sqlite;

pub use sqlite::Store;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single metric snapshot written by a collector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Which collector produced this ("statements", "activity", "locks", etc.)
    pub collector: String,
    /// JSON-encoded payload
    pub data: serde_json::Value,
    /// When the snapshot was taken
    pub collected_at: DateTime<Utc>,
}

/// A query fingerprint record for the `query_fingerprints` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFingerprint {
    pub fingerprint: String,
    pub sanitized_query: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// An agent event (startup, error, capability change, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

/// A shipper destination for a database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipperEntry {
    pub id: String,
    pub name: String,
    pub shipper_type: String, // "datapace", "webhook", "custom"
    pub endpoint: String,
    pub token: Option<String>,
    pub enabled: bool,
}

/// A database entry managed via the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Database technology: "postgres", "mysql", etc.
    pub db_type: String,
    /// Deployment environment: "production", "staging", "development", "local".
    pub environment: String,
    pub pool_size: u32,
    pub fast_interval: u64,
    pub slow_interval: u64,
    /// JSON array of collector names, e.g. ["statements","activity","locks"]
    pub collectors: Vec<String>,
    /// Whether to anonymize sensitive data (emails, IPs, tokens) in collected queries.
    pub anonymize: bool,
    /// Shipper destinations (stored as JSON).
    pub shippers: Vec<ShipperEntry>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// A pipeline event recording per-collector timing for a single tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEvent {
    pub source_id: String,
    pub tick_type: String,
    /// JSON array: [{name, rows, duration_ms, error}]
    pub collectors_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A shipping log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingEntry {
    pub source_id: String,
    pub shipper_id: String,
    pub status: String,
    pub bytes: u64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}
