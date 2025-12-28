//! Normalized payload schema for Datapace Cloud.
//!
//! This module defines the data structures that are sent to the Datapace Cloud API.
//! The schema is designed to be database-agnostic while capturing all relevant metrics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// The main payload sent to Datapace Cloud
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    /// Agent version
    pub agent_version: String,

    /// Timestamp when the payload was created
    pub timestamp: DateTime<Utc>,

    /// Unique identifier for this database instance
    pub instance_id: String,

    /// Database information
    pub database: DatabaseInfo,

    /// Query statistics (from pg_stat_statements or equivalent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_stats: Option<Vec<QueryStats>>,

    /// Table statistics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_stats: Option<Vec<TableStats>>,

    /// Index statistics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_stats: Option<Vec<IndexStats>>,

    /// Database configuration settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, String>>,

    /// Schema metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaMetadata>,
}

impl Payload {
    /// Create a new payload with database info
    pub fn new(database: DatabaseInfo) -> Self {
        Self {
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: Utc::now(),
            instance_id: String::new(), // Set later
            database,
            query_stats: None,
            table_stats: None,
            index_stats: None,
            settings: None,
            schema: None,
        }
    }

    /// Set the instance ID based on connection info
    pub fn with_instance_id(mut self, connection_info: &str) -> Self {
        self.instance_id = generate_instance_id(connection_info);
        self
    }

    /// Add query statistics
    pub fn with_query_stats(mut self, stats: Vec<QueryStats>) -> Self {
        self.query_stats = Some(stats);
        self
    }

    /// Add table statistics
    pub fn with_table_stats(mut self, stats: Vec<TableStats>) -> Self {
        self.table_stats = Some(stats);
        self
    }

    /// Add index statistics
    pub fn with_index_stats(mut self, stats: Vec<IndexStats>) -> Self {
        self.index_stats = Some(stats);
        self
    }

    /// Add database settings
    pub fn with_settings(mut self, settings: HashMap<String, String>) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Add schema metadata
    pub fn with_schema(mut self, schema: SchemaMetadata) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Serialize the payload to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize the payload to pretty JSON (for debugging)
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Database information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database type (postgres, mysql, etc.)
    #[serde(rename = "type")]
    pub database_type: String,

    /// Database version string
    pub version: Option<String>,

    /// Detected provider (generic, rds, aurora, supabase, neon)
    pub provider: String,

    /// Provider-specific metadata
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub provider_metadata: HashMap<String, String>,
}

/// Query statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    /// Query hash/ID
    pub query_hash: Option<String>,

    /// Normalized query text
    pub query: Option<String>,

    /// Number of times executed
    pub calls: Option<i64>,

    /// Total execution time in milliseconds
    pub total_time_ms: Option<f64>,

    /// Mean execution time in milliseconds
    pub mean_time_ms: Option<f64>,

    /// Total rows returned
    pub rows: Option<i64>,

    /// Shared buffer hits
    pub shared_blks_hit: Option<i64>,

    /// Shared blocks read from disk
    pub shared_blks_read: Option<i64>,
}

/// Table statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStats {
    /// Schema name
    pub schema: String,

    /// Table name
    pub table: String,

    /// Number of sequential scans
    pub seq_scan: Option<i64>,

    /// Rows fetched by sequential scans
    pub seq_tup_read: Option<i64>,

    /// Number of index scans
    pub idx_scan: Option<i64>,

    /// Rows fetched by index scans
    pub idx_tup_fetch: Option<i64>,

    /// Rows inserted
    pub n_tup_ins: Option<i64>,

    /// Rows updated
    pub n_tup_upd: Option<i64>,

    /// Rows deleted
    pub n_tup_del: Option<i64>,

    /// Live row count
    pub n_live_tup: Option<i64>,

    /// Dead row count
    pub n_dead_tup: Option<i64>,

    /// Last manual vacuum
    pub last_vacuum: Option<DateTime<Utc>>,

    /// Last auto vacuum
    pub last_autovacuum: Option<DateTime<Utc>>,

    /// Last manual analyze
    pub last_analyze: Option<DateTime<Utc>>,

    /// Last auto analyze
    pub last_autoanalyze: Option<DateTime<Utc>>,
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Schema name
    pub schema: String,

    /// Table name
    pub table: String,

    /// Index name
    pub index: String,

    /// Number of index scans
    pub idx_scan: Option<i64>,

    /// Index entries read
    pub idx_tup_read: Option<i64>,

    /// Live table rows fetched by index
    pub idx_tup_fetch: Option<i64>,
}

/// Schema metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    /// Tables in the database
    pub tables: Vec<TableMetadata>,

    /// Indexes in the database
    pub indexes: Vec<IndexMetadata>,
}

/// Table metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    /// Schema name
    pub schema: String,

    /// Table name
    pub name: String,

    /// Column definitions
    pub columns: Vec<ColumnMetadata>,

    /// Estimated row count
    pub row_count_estimate: Option<i64>,

    /// Table size in bytes
    pub size_bytes: Option<i64>,
}

/// Column metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMetadata {
    /// Column name
    pub name: String,

    /// Data type
    pub data_type: String,

    /// Whether the column is nullable
    pub nullable: bool,

    /// Default value expression
    pub default: Option<String>,

    /// Ordinal position
    pub position: i32,
}

/// Index metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    /// Schema name
    pub schema: String,

    /// Table name
    pub table: String,

    /// Index name
    pub name: String,

    /// Columns in the index
    pub columns: Vec<String>,

    /// Whether the index enforces uniqueness
    pub is_unique: bool,

    /// Whether this is the primary key
    pub is_primary: bool,

    /// Index size in bytes
    pub size_bytes: Option<i64>,
}

/// Generate a stable instance ID from connection info
fn generate_instance_id(connection_info: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(connection_info.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..16]) // Use first 16 bytes for shorter ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_serialization() {
        let payload = Payload::new(DatabaseInfo {
            database_type: "postgres".to_string(),
            version: Some("15.4".to_string()),
            provider: "generic".to_string(),
            provider_metadata: HashMap::new(),
        });

        let json = payload.to_json().unwrap();
        assert!(json.contains("postgres"));
        assert!(json.contains("agent_version"));
    }

    #[test]
    fn test_instance_id_generation() {
        let id1 = generate_instance_id("postgres://localhost/test");
        let id2 = generate_instance_id("postgres://localhost/test");
        let id3 = generate_instance_id("postgres://localhost/other");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(id1.len(), 32); // 16 bytes = 32 hex chars
    }

    #[test]
    fn test_payload_builder() {
        let payload = Payload::new(DatabaseInfo {
            database_type: "postgres".to_string(),
            version: None,
            provider: "rds".to_string(),
            provider_metadata: HashMap::new(),
        })
        .with_instance_id("postgres://localhost/test")
        .with_table_stats(vec![])
        .with_settings(HashMap::new());

        assert!(!payload.instance_id.is_empty());
        assert!(payload.table_stats.is_some());
        assert!(payload.settings.is_some());
        assert!(payload.query_stats.is_none());
    }
}
