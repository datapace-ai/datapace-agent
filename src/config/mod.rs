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
    #[default]
    Postgres,
    Mysql,
    Mongodb,
}

impl DatabaseType {
    /// Auto-detect database type from connection URL
    pub fn from_url(url: &str) -> Result<Self, ConfigError> {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(DatabaseType::Postgres)
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            Ok(DatabaseType::Mysql)
        } else if url.starts_with("mongodb://") || url.starts_with("mongodb+srv://") {
            Ok(DatabaseType::Mongodb)
        } else {
            Err(ConfigError::UnsupportedDatabase(
                "Unable to detect database type from URL. Supported schemes: postgres://, postgresql://, mysql://, mariadb://, mongodb://, mongodb+srv://".to_string()
            ))
        }
    }

    /// Get all supported URL schemes for this database type
    pub fn url_schemes(&self) -> &'static [&'static str] {
        match self {
            DatabaseType::Postgres => &["postgres://", "postgresql://"],
            DatabaseType::Mysql => &["mysql://", "mariadb://"],
            DatabaseType::Mongodb => &["mongodb://", "mongodb+srv://"],
        }
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseType::Postgres => write!(f, "postgres"),
            DatabaseType::Mysql => write!(f, "mysql"),
            DatabaseType::Mongodb => write!(f, "mongodb"),
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

        // Unsupported URL
        assert!(DatabaseType::from_url("redis://localhost").is_err());
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(format!("{}", DatabaseType::Postgres), "postgres");
        assert_eq!(format!("{}", DatabaseType::Mysql), "mysql");
        assert_eq!(format!("{}", DatabaseType::Mongodb), "mongodb");
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
