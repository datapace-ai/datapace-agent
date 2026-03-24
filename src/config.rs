use regex::Regex;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::warn;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Validation(String),
}

/// Top-level agent configuration loaded from `agent.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub agent: AgentConfig,
    /// Optional — if absent, agent starts in UI-only mode (multi-DB via UI).
    pub postgres: Option<PostgresConfig>,
    /// Optional MongoDB configuration.
    pub mongodb: Option<MongoDbConfig>,
    #[serde(default)]
    pub shipper: ShipperConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    pub name: Option<String>,
    pub data_dir: PathBuf,
    pub log_level: LogLevel,
    pub retention_days: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: None,
            data_dir: PathBuf::from("/var/lib/datapace"),
            log_level: LogLevel::Info,
            retention_days: 90,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(l: LogLevel) -> Self {
        match l {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresConfig {
    pub url: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    #[serde(default = "default_fast_interval")]
    pub fast_interval: u64,
    #[serde(default = "default_slow_interval")]
    pub slow_interval: u64,
}

fn default_pool_size() -> u32 {
    3
}
fn default_fast_interval() -> u64 {
    30
}
fn default_slow_interval() -> u64 {
    300
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MongoDbConfig {
    pub url: String,
    #[serde(default = "default_fast_interval")]
    pub fast_interval: u64,
    #[serde(default = "default_slow_interval")]
    pub slow_interval: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShipperTarget {
    None,
    Datapace,
    Prometheus,
    GenericHttps,
}

impl Default for ShipperTarget {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShipperConfig {
    pub target: ShipperTarget,
    // Datapace
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
    // Prometheus
    pub prometheus_port: u16,
    // Generic HTTPS
    pub generic_url: Option<String>,
    pub generic_token: Option<String>,
}

impl Default for ShipperConfig {
    fn default() -> Self {
        Self {
            target: ShipperTarget::None,
            api_key: None,
            endpoint: None,
            prometheus_port: 9187,
            generic_url: None,
            generic_token: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub enabled: bool,
    pub listen: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            listen: "127.0.0.1:7080".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file, expanding `${ENV_VAR}` references.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        let expanded = expand_env_vars(&raw);
        let config: Config = toml::from_str(&expanded)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate the loaded configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if let Some(ref pg) = self.postgres {
            if pg.url.is_empty() {
                return Err(ConfigError::Validation("postgres.url is required".into()));
            }
            if !pg.url.starts_with("postgres://") && !pg.url.starts_with("postgresql://") {
                return Err(ConfigError::Validation(
                    "postgres.url must start with postgres:// or postgresql://".into(),
                ));
            }
            if pg.fast_interval < 5 {
                return Err(ConfigError::Validation(
                    "postgres.fast_interval must be >= 5 seconds".into(),
                ));
            }
            if pg.slow_interval < pg.fast_interval {
                return Err(ConfigError::Validation(
                    "postgres.slow_interval must be >= fast_interval".into(),
                ));
            }
        }

        if let Some(ref mongo) = self.mongodb {
            if mongo.url.is_empty() {
                return Err(ConfigError::Validation("mongodb.url is required".into()));
            }
            if !mongo.url.starts_with("mongodb://") && !mongo.url.starts_with("mongodb+srv://") {
                return Err(ConfigError::Validation(
                    "mongodb.url must start with mongodb:// or mongodb+srv://".into(),
                ));
            }
            if mongo.fast_interval < 5 {
                return Err(ConfigError::Validation(
                    "mongodb.fast_interval must be >= 5 seconds".into(),
                ));
            }
            if mongo.slow_interval < mongo.fast_interval {
                return Err(ConfigError::Validation(
                    "mongodb.slow_interval must be >= fast_interval".into(),
                ));
            }
        }

        if self.agent.retention_days == 0 {
            return Err(ConfigError::Validation(
                "agent.retention_days must be > 0".into(),
            ));
        }

        // Validate shipper-specific fields
        match &self.shipper.target {
            ShipperTarget::Datapace => {
                if self.shipper.api_key.as_ref().map_or(true, |k| k.is_empty()) {
                    return Err(ConfigError::Validation(
                        "shipper.api_key is required when target = datapace".into(),
                    ));
                }
            }
            ShipperTarget::GenericHttps => {
                if self
                    .shipper
                    .generic_url
                    .as_ref()
                    .map_or(true, |u| u.is_empty())
                {
                    return Err(ConfigError::Validation(
                        "shipper.generic_url is required when target = generic_https".into(),
                    ));
                }
            }
            _ => {}
        }

        // Warn on common issues (non-fatal)
        if self.ui.enabled && self.ui.listen.starts_with("0.0.0.0") {
            warn!("UI is bound to 0.0.0.0 — accessible from all interfaces");
        }

        Ok(())
    }

    /// SQLite database path derived from data_dir.
    pub fn sqlite_path(&self) -> PathBuf {
        self.agent.data_dir.join("datapace.db")
    }
}

/// Expand `${VAR}` references in a string using environment variables.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
[postgres]
url = "postgres://localhost/test"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let pg = config.postgres.unwrap();
        assert_eq!(pg.fast_interval, 30);
        assert_eq!(pg.slow_interval, 300);
        assert_eq!(config.agent.retention_days, 90);
        assert!(config.ui.enabled);
        assert_eq!(config.shipper.target, ShipperTarget::None);
    }

    #[test]
    fn parse_no_postgres() {
        let toml = r#"
[agent]
data_dir = "/tmp/datapace"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.postgres.is_none());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
[agent]
name = "prod-primary"
data_dir = "/tmp/datapace"
log_level = "debug"
retention_days = 30

[postgres]
url = "postgres://user:pass@host:5432/db"
pool_size = 5
fast_interval = 15
slow_interval = 600

[shipper]
target = "datapace"
api_key = "dp_test_key"
endpoint = "https://api.datapace.ai/v1/ingest"

[ui]
enabled = false
listen = "0.0.0.0:9090"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.agent.name.as_deref(), Some("prod-primary"));
        assert_eq!(config.postgres.unwrap().pool_size, 5);
        assert_eq!(config.shipper.target, ShipperTarget::Datapace);
        assert!(!config.ui.enabled);
    }

    #[test]
    fn reject_invalid_url() {
        let toml = r#"
[postgres]
url = "mysql://localhost/test"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn reject_unknown_fields() {
        let toml = r#"
[postgres]
url = "postgres://localhost/test"
bogus_field = true
"#;
        assert!(toml::from_str::<Config>(toml).is_err());
    }

    #[test]
    fn expand_env() {
        std::env::set_var("TEST_DB_URL", "postgres://expanded/db");
        let result = expand_env_vars("url = \"${TEST_DB_URL}\"");
        assert!(result.contains("postgres://expanded/db"));
    }

    #[test]
    fn validate_fast_gt_slow() {
        let toml = r#"
[postgres]
url = "postgres://localhost/test"
fast_interval = 100
slow_interval = 50
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_datapace_needs_key() {
        let toml = r#"
[postgres]
url = "postgres://localhost/test"

[shipper]
target = "datapace"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }
}
