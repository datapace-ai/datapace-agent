//! Configuration management for the Datapace Agent.
//!
//! Supports loading configuration from:
//! - YAML config files
//! - Environment variables
//! - Command-line arguments

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("Invalid configuration: {0}")]
    ValidationError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unsupported database type: {0}")]
    UnsupportedDatabase(String),
}

/// Supported database types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    // Relational databases
    #[default]
    Postgres,
    Mysql,
    Sqlserver,
    Oracle,
    Db2,

    // Document databases
    Mongodb,
    Couchbase,
    Cosmosdb,

    // Search & Analytics
    Elasticsearch,
    Clickhouse,
    Snowflake,
    Bigquery,
    Redshift,

    // Key-Value & Cache
    Redis,
    Dynamodb,

    // Time-series
    Timescaledb,
    Influxdb,

    // NewSQL
    Cockroachdb,
    Yugabytedb,
    Tidb,

    // Vector databases
    Pinecone,
    Milvus,
    Weaviate,
    Qdrant,
    Chroma,
    Pgvector,  // PostgreSQL extension
}

impl DatabaseType {
    /// Auto-detect database type from connection URL
    pub fn from_url(url: &str) -> Result<Self, ConfigError> {
        let url_lower = url.to_lowercase();

        // PostgreSQL variants
        if url_lower.starts_with("postgres://") || url_lower.starts_with("postgresql://") {
            // Check for TimescaleDB or CockroachDB hints in the URL
            if url_lower.contains("timescale") {
                return Ok(DatabaseType::Timescaledb);
            }
            if url_lower.contains("cockroach") {
                return Ok(DatabaseType::Cockroachdb);
            }
            if url_lower.contains("yugabyte") {
                return Ok(DatabaseType::Yugabytedb);
            }
            return Ok(DatabaseType::Postgres);
        }

        // MySQL variants
        if url_lower.starts_with("mysql://") || url_lower.starts_with("mariadb://") {
            if url_lower.contains("tidb") {
                return Ok(DatabaseType::Tidb);
            }
            return Ok(DatabaseType::Mysql);
        }

        // MongoDB
        if url_lower.starts_with("mongodb://") || url_lower.starts_with("mongodb+srv://") {
            return Ok(DatabaseType::Mongodb);
        }

        // SQL Server
        if url_lower.starts_with("sqlserver://")
            || url_lower.starts_with("mssql://")
            || url_lower.contains("database.windows.net")
        {
            return Ok(DatabaseType::Sqlserver);
        }

        // Oracle
        if url_lower.starts_with("oracle://") || url_lower.contains("oraclecloud.com") {
            return Ok(DatabaseType::Oracle);
        }

        // IBM DB2
        if url_lower.starts_with("db2://") || url_lower.starts_with("ibmdb://") {
            return Ok(DatabaseType::Db2);
        }

        // Redis
        if url_lower.starts_with("redis://") || url_lower.starts_with("rediss://") {
            return Ok(DatabaseType::Redis);
        }

        // Elasticsearch
        if url_lower.starts_with("elasticsearch://")
            || url_lower.starts_with("https://") && url_lower.contains("elastic")
        {
            return Ok(DatabaseType::Elasticsearch);
        }

        // ClickHouse
        if url_lower.starts_with("clickhouse://") || url_lower.contains("clickhouse") {
            return Ok(DatabaseType::Clickhouse);
        }

        // Azure Cosmos DB
        if url_lower.contains("cosmos.azure.com") || url_lower.contains("cosmosdb") {
            return Ok(DatabaseType::Cosmosdb);
        }

        // Couchbase
        if url_lower.starts_with("couchbase://") || url_lower.starts_with("couchbases://") {
            return Ok(DatabaseType::Couchbase);
        }

        // Snowflake
        if url_lower.contains("snowflakecomputing.com") {
            return Ok(DatabaseType::Snowflake);
        }

        // BigQuery
        if url_lower.starts_with("bigquery://") || url_lower.contains("bigquery.googleapis.com") {
            return Ok(DatabaseType::Bigquery);
        }

        // Redshift
        if url_lower.contains("redshift.amazonaws.com") {
            return Ok(DatabaseType::Redshift);
        }

        // DynamoDB
        if url_lower.contains("dynamodb") {
            return Ok(DatabaseType::Dynamodb);
        }

        // InfluxDB
        if url_lower.starts_with("influxdb://") || url_lower.contains("influxdata") {
            return Ok(DatabaseType::Influxdb);
        }

        // Vector databases
        if url_lower.contains("pinecone.io") || url_lower.contains("pinecone") {
            return Ok(DatabaseType::Pinecone);
        }

        if url_lower.contains("milvus") || url_lower.starts_with("milvus://") {
            return Ok(DatabaseType::Milvus);
        }

        if url_lower.contains("weaviate") {
            return Ok(DatabaseType::Weaviate);
        }

        if url_lower.contains("qdrant") {
            return Ok(DatabaseType::Qdrant);
        }

        if url_lower.contains("chroma") {
            return Ok(DatabaseType::Chroma);
        }

        // pgvector is detected via PostgreSQL URL + extension check at runtime

        Err(ConfigError::UnsupportedDatabase(format!(
            "Unable to detect database type from URL. Supported databases: PostgreSQL, MySQL, MongoDB, SQL Server, Oracle, DB2, Redis, Elasticsearch, ClickHouse, Cosmos DB, Couchbase, Snowflake, BigQuery, Redshift, DynamoDB, InfluxDB, TimescaleDB, CockroachDB, YugabyteDB, TiDB, Pinecone, Milvus, Weaviate, Qdrant, Chroma"
        )))
    }

    /// Get all supported URL schemes for this database type
    pub fn url_schemes(&self) -> &'static [&'static str] {
        match self {
            DatabaseType::Postgres => &["postgres://", "postgresql://"],
            DatabaseType::Mysql => &["mysql://", "mariadb://"],
            DatabaseType::Mongodb => &["mongodb://", "mongodb+srv://"],
            DatabaseType::Sqlserver => &["sqlserver://", "mssql://"],
            DatabaseType::Oracle => &["oracle://"],
            DatabaseType::Db2 => &["db2://", "ibmdb://"],
            DatabaseType::Redis => &["redis://", "rediss://"],
            DatabaseType::Elasticsearch => &["elasticsearch://", "https://"],
            DatabaseType::Clickhouse => &["clickhouse://"],
            DatabaseType::Cosmosdb => &["https://"],
            DatabaseType::Couchbase => &["couchbase://", "couchbases://"],
            DatabaseType::Snowflake => &["https://"],
            DatabaseType::Bigquery => &["bigquery://"],
            DatabaseType::Redshift => &["postgres://"],  // Redshift uses PostgreSQL protocol
            DatabaseType::Dynamodb => &["https://"],
            DatabaseType::Influxdb => &["influxdb://", "https://"],
            DatabaseType::Timescaledb => &["postgres://", "postgresql://"],
            DatabaseType::Cockroachdb => &["postgres://", "postgresql://"],
            DatabaseType::Yugabytedb => &["postgres://", "postgresql://"],
            DatabaseType::Tidb => &["mysql://"],
            DatabaseType::Pinecone => &["https://"],
            DatabaseType::Milvus => &["milvus://", "https://"],
            DatabaseType::Weaviate => &["https://"],
            DatabaseType::Qdrant => &["https://", "http://"],
            DatabaseType::Chroma => &["https://", "http://"],
            DatabaseType::Pgvector => &["postgres://", "postgresql://"],
        }
    }

    /// Check if this database type is currently supported
    pub fn is_implemented(&self) -> bool {
        matches!(self, DatabaseType::Postgres)
    }

    /// Get the category of this database
    pub fn category(&self) -> &'static str {
        match self {
            DatabaseType::Postgres
            | DatabaseType::Mysql
            | DatabaseType::Sqlserver
            | DatabaseType::Oracle
            | DatabaseType::Db2 => "Relational",

            DatabaseType::Mongodb | DatabaseType::Couchbase | DatabaseType::Cosmosdb => "Document",

            DatabaseType::Elasticsearch
            | DatabaseType::Clickhouse
            | DatabaseType::Snowflake
            | DatabaseType::Bigquery
            | DatabaseType::Redshift => "Analytics",

            DatabaseType::Redis | DatabaseType::Dynamodb => "Key-Value",

            DatabaseType::Timescaledb | DatabaseType::Influxdb => "Time-Series",

            DatabaseType::Cockroachdb | DatabaseType::Yugabytedb | DatabaseType::Tidb => "NewSQL",

            DatabaseType::Pinecone
            | DatabaseType::Milvus
            | DatabaseType::Weaviate
            | DatabaseType::Qdrant
            | DatabaseType::Chroma
            | DatabaseType::Pgvector => "Vector",
        }
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseType::Postgres => write!(f, "postgres"),
            DatabaseType::Mysql => write!(f, "mysql"),
            DatabaseType::Mongodb => write!(f, "mongodb"),
            DatabaseType::Sqlserver => write!(f, "sqlserver"),
            DatabaseType::Oracle => write!(f, "oracle"),
            DatabaseType::Db2 => write!(f, "db2"),
            DatabaseType::Redis => write!(f, "redis"),
            DatabaseType::Elasticsearch => write!(f, "elasticsearch"),
            DatabaseType::Clickhouse => write!(f, "clickhouse"),
            DatabaseType::Cosmosdb => write!(f, "cosmosdb"),
            DatabaseType::Couchbase => write!(f, "couchbase"),
            DatabaseType::Snowflake => write!(f, "snowflake"),
            DatabaseType::Bigquery => write!(f, "bigquery"),
            DatabaseType::Redshift => write!(f, "redshift"),
            DatabaseType::Dynamodb => write!(f, "dynamodb"),
            DatabaseType::Influxdb => write!(f, "influxdb"),
            DatabaseType::Timescaledb => write!(f, "timescaledb"),
            DatabaseType::Cockroachdb => write!(f, "cockroachdb"),
            DatabaseType::Yugabytedb => write!(f, "yugabytedb"),
            DatabaseType::Tidb => write!(f, "tidb"),
            DatabaseType::Pinecone => write!(f, "pinecone"),
            DatabaseType::Milvus => write!(f, "milvus"),
            DatabaseType::Weaviate => write!(f, "weaviate"),
            DatabaseType::Qdrant => write!(f, "qdrant"),
            DatabaseType::Chroma => write!(f, "chroma"),
            DatabaseType::Pgvector => write!(f, "pgvector"),
        }
    }
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub datapace: DatapaceConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub collection: CollectionConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub health: HealthConfig,
}

/// Datapace Cloud connection settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatapaceConfig {
    pub api_key: String,

    #[serde(default = "default_endpoint")]
    pub endpoint: String,

    #[serde(default = "default_timeout")]
    pub timeout: u64,

    #[serde(default = "default_retries")]
    pub retries: u32,
}

/// Database connection settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: String,

    /// Database type (auto-detected from URL if not specified)
    #[serde(default)]
    pub db_type: DatabaseType,

    /// Cloud provider (auto-detected if not specified)
    #[serde(default)]
    pub provider: Provider,

    /// Connection pool settings
    #[serde(default)]
    pub pool: PoolConfig,
}

/// Database provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Auto,
    Generic,
    Rds,
    Aurora,
    Supabase,
    Neon,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Auto => write!(f, "auto"),
            Provider::Generic => write!(f, "generic"),
            Provider::Rds => write!(f, "rds"),
            Provider::Aurora => write!(f, "aurora"),
            Provider::Supabase => write!(f, "supabase"),
            Provider::Neon => write!(f, "neon"),
        }
    }
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,

    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: default_min_connections(),
            max_connections: default_max_connections(),
            acquire_timeout: default_acquire_timeout(),
        }
    }
}

/// Metrics collection settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,

    #[serde(default = "default_metrics")]
    pub metrics: Vec<MetricType>,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_interval_secs(),
            metrics: default_metrics(),
        }
    }
}

impl CollectionConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }
}

/// Types of metrics that can be collected (database-agnostic)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Query performance statistics (pg_stat_statements, performance_schema, etc.)
    #[serde(alias = "pg_stat_statements")]
    QueryStats,

    /// Table-level statistics (sizes, row counts, operations)
    #[serde(alias = "pg_stat_user_tables")]
    TableStats,

    /// Index usage statistics
    #[serde(alias = "pg_stat_user_indexes")]
    IndexStats,

    /// Database configuration settings
    #[serde(alias = "pg_settings")]
    Settings,

    /// Schema metadata (tables, columns, indexes, foreign keys)
    SchemaMetadata,
}

impl MetricType {
    /// Get all available metric types
    pub fn all() -> Vec<MetricType> {
        vec![
            MetricType::QueryStats,
            MetricType::TableStats,
            MetricType::IndexStats,
            MetricType::Settings,
            MetricType::SchemaMetadata,
        ]
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: LogLevel,

    #[serde(default = "default_log_format")]
    pub format: LogFormat,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Json,
    Pretty,
}

/// Health check endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    #[serde(default = "default_health_enabled")]
    pub enabled: bool,

    #[serde(default = "default_health_port")]
    pub port: u16,

    #[serde(default = "default_health_path")]
    pub path: String,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: default_health_enabled(),
            port: default_health_port(),
            path: default_health_path(),
        }
    }
}

// Default value functions
fn default_endpoint() -> String {
    "https://api.datapace.ai/v1/ingest".to_string()
}

fn default_timeout() -> u64 {
    30
}

fn default_retries() -> u32 {
    3
}

fn default_min_connections() -> u32 {
    1
}

fn default_max_connections() -> u32 {
    5
}

fn default_acquire_timeout() -> u64 {
    30
}

fn default_interval_secs() -> u64 {
    60
}

fn default_metrics() -> Vec<MetricType> {
    MetricType::all()
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

fn default_log_format() -> LogFormat {
    LogFormat::Json
}

fn default_health_enabled() -> bool {
    true
}

fn default_health_port() -> u16 {
    8080
}

fn default_health_path() -> String {
    "/health".to_string()
}

impl Config {
    /// Load configuration from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let expanded = expand_env_vars(&content);
        let config: Config = serde_yaml::from_str(&expanded)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from environment variables only
    pub fn from_env() -> Result<Self, ConfigError> {
        let api_key = std::env::var("DATAPACE_API_KEY")
            .map_err(|_| ConfigError::MissingField("DATAPACE_API_KEY".to_string()))?;

        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingField("DATABASE_URL".to_string()))?;

        let endpoint = std::env::var("DATAPACE_ENDPOINT").unwrap_or_else(|_| default_endpoint());

        let interval_secs = std::env::var("COLLECTION_INTERVAL")
            .ok()
            .and_then(|s| parse_duration_secs(&s))
            .unwrap_or_else(default_interval_secs);

        let log_level = std::env::var("LOG_LEVEL")
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "trace" => Some(LogLevel::Trace),
                "debug" => Some(LogLevel::Debug),
                "info" => Some(LogLevel::Info),
                "warn" => Some(LogLevel::Warn),
                "error" => Some(LogLevel::Error),
                _ => None,
            })
            .unwrap_or_else(default_log_level);

        let log_format = std::env::var("LOG_FORMAT")
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "json" => Some(LogFormat::Json),
                "pretty" => Some(LogFormat::Pretty),
                _ => None,
            })
            .unwrap_or_else(default_log_format);

        let config = Config {
            datapace: DatapaceConfig {
                api_key,
                endpoint,
                timeout: default_timeout(),
                retries: default_retries(),
            },
            database: DatabaseConfig {
                url: database_url,
                provider: Provider::Auto,
                pool: PoolConfig::default(),
            },
            collection: CollectionConfig {
                interval_secs,
                metrics: default_metrics(),
            },
            logging: LoggingConfig {
                level: log_level,
                format: log_format,
            },
            health: HealthConfig::default(),
        };

        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.datapace.api_key.is_empty() {
            return Err(ConfigError::ValidationError(
                "API key cannot be empty".to_string(),
            ));
        }

        if self.database.url.is_empty() {
            return Err(ConfigError::ValidationError(
                "Database URL cannot be empty".to_string(),
            ));
        }

        // Validate database URL matches a supported database type
        DatabaseType::from_url(&self.database.url)?;

        if self.collection.interval_secs < 10 {
            return Err(ConfigError::ValidationError(
                "Collection interval must be at least 10 seconds".to_string(),
            ));
        }

        Ok(())
    }

    /// Get the detected database type from the URL
    pub fn database_type(&self) -> Result<DatabaseType, ConfigError> {
        DatabaseType::from_url(&self.database.url)
    }
}

/// Expand environment variables in a string using ${VAR} syntax
fn expand_env_vars(input: &str) -> String {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut result = input.to_string();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        if let Ok(value) = std::env::var(var_name) {
            result = result.replace(&cap[0], &value);
        }
    }

    result
}

/// Parse duration string like "60s", "5m", "1h" into seconds
fn parse_duration_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, suffix) = if s.ends_with('s') {
        (&s[..s.len() - 1], 1u64)
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], 60u64)
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], 3600u64)
    } else {
        (s, 1u64)
    };

    num_str.parse::<u64>().ok().map(|n| n * suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_secs("60s"), Some(60));
        assert_eq!(parse_duration_secs("5m"), Some(300));
        assert_eq!(parse_duration_secs("1h"), Some(3600));
        assert_eq!(parse_duration_secs("30"), Some(30));
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(format!("{}", Provider::Auto), "auto");
        assert_eq!(format!("{}", Provider::Rds), "rds");
        assert_eq!(format!("{}", Provider::Supabase), "supabase");
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "hello");
        let result = expand_env_vars("prefix ${TEST_VAR} suffix");
        assert_eq!(result, "prefix hello suffix");
    }

    #[test]
    fn test_database_type_from_url() {
        // PostgreSQL URLs
        assert_eq!(
            DatabaseType::from_url("postgres://localhost/db").unwrap(),
            DatabaseType::Postgres
        );
        assert_eq!(
            DatabaseType::from_url("postgresql://localhost/db").unwrap(),
            DatabaseType::Postgres
        );

        // MySQL URLs
        assert_eq!(
            DatabaseType::from_url("mysql://localhost/db").unwrap(),
            DatabaseType::Mysql
        );
        assert_eq!(
            DatabaseType::from_url("mariadb://localhost/db").unwrap(),
            DatabaseType::Mysql
        );

        // MongoDB URLs
        assert_eq!(
            DatabaseType::from_url("mongodb://localhost/db").unwrap(),
            DatabaseType::Mongodb
        );
        assert_eq!(
            DatabaseType::from_url("mongodb+srv://cluster.example.com/db").unwrap(),
            DatabaseType::Mongodb
        );

        // SQL Server
        assert_eq!(
            DatabaseType::from_url("sqlserver://localhost/db").unwrap(),
            DatabaseType::Sqlserver
        );
        assert_eq!(
            DatabaseType::from_url("mssql://myserver.database.windows.net/db").unwrap(),
            DatabaseType::Sqlserver
        );

        // Oracle
        assert_eq!(
            DatabaseType::from_url("oracle://localhost:1521/db").unwrap(),
            DatabaseType::Oracle
        );

        // Redis
        assert_eq!(
            DatabaseType::from_url("redis://localhost:6379").unwrap(),
            DatabaseType::Redis
        );
        assert_eq!(
            DatabaseType::from_url("rediss://localhost:6379").unwrap(),
            DatabaseType::Redis
        );

        // Elasticsearch
        assert_eq!(
            DatabaseType::from_url("elasticsearch://localhost:9200").unwrap(),
            DatabaseType::Elasticsearch
        );

        // ClickHouse
        assert_eq!(
            DatabaseType::from_url("clickhouse://localhost:8123/db").unwrap(),
            DatabaseType::Clickhouse
        );

        // Cosmos DB
        assert_eq!(
            DatabaseType::from_url("https://myaccount.documents.azure.cosmos.azure.com").unwrap(),
            DatabaseType::Cosmosdb
        );

        // NewSQL - PostgreSQL compatible
        assert_eq!(
            DatabaseType::from_url("postgres://cockroachdb.example.com/db").unwrap(),
            DatabaseType::Cockroachdb
        );
        assert_eq!(
            DatabaseType::from_url("postgres://timescaledb.example.com/db").unwrap(),
            DatabaseType::Timescaledb
        );

        // Cloud data warehouses
        assert_eq!(
            DatabaseType::from_url("postgres://mydb.redshift.amazonaws.com/db").unwrap(),
            DatabaseType::Redshift
        );
        assert_eq!(
            DatabaseType::from_url("https://myaccount.snowflakecomputing.com").unwrap(),
            DatabaseType::Snowflake
        );
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(format!("{}", DatabaseType::Postgres), "postgres");
        assert_eq!(format!("{}", DatabaseType::Mysql), "mysql");
        assert_eq!(format!("{}", DatabaseType::Mongodb), "mongodb");
        assert_eq!(format!("{}", DatabaseType::Sqlserver), "sqlserver");
        assert_eq!(format!("{}", DatabaseType::Oracle), "oracle");
        assert_eq!(format!("{}", DatabaseType::Elasticsearch), "elasticsearch");
    }

    #[test]
    fn test_database_type_category() {
        assert_eq!(DatabaseType::Postgres.category(), "Relational");
        assert_eq!(DatabaseType::Mysql.category(), "Relational");
        assert_eq!(DatabaseType::Oracle.category(), "Relational");
        assert_eq!(DatabaseType::Mongodb.category(), "Document");
        assert_eq!(DatabaseType::Redis.category(), "Key-Value");
        assert_eq!(DatabaseType::Elasticsearch.category(), "Analytics");
        assert_eq!(DatabaseType::Timescaledb.category(), "Time-Series");
        assert_eq!(DatabaseType::Cockroachdb.category(), "NewSQL");
    }

    #[test]
    fn test_metric_type_aliases() {
        // Test that old PostgreSQL-specific names still work via serde aliases
        let yaml = "pg_stat_statements";
        let metric: MetricType = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(metric, MetricType::QueryStats);

        let yaml = "pg_stat_user_tables";
        let metric: MetricType = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(metric, MetricType::TableStats);

        let yaml = "query_stats";
        let metric: MetricType = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(metric, MetricType::QueryStats);
    }
}
