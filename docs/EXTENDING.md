# Extending Datapace Agent

This guide explains how to extend the Datapace Agent to support new databases, metrics, and cloud providers.

## Architecture Overview

The agent uses a trait-based architecture that makes it easy to add support for new databases:

```
┌─────────────────────────────────────────────────────────────┐
│                        main.rs                               │
│                    (CLI & Scheduler)                         │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                   Collector Factory                          │
│              create_collector(db_type, url)                  │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
   ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
   │  PostgreSQL │ │    MySQL    │ │   MongoDB   │
   │  Collector  │ │  Collector  │ │  Collector  │
   └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
          │               │               │
          └───────────────┼───────────────┘
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    Payload (Normalized)                      │
│            Database-agnostic data structure                  │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                       Uploader                               │
│               Send to Datapace Cloud API                     │
└─────────────────────────────────────────────────────────────┘
```

## Core Abstractions

### 1. DatabaseType

Defines which database engine to connect to:

```rust
pub enum DatabaseType {
    Postgres,
    MySQL,
    MongoDB,
    // Add new databases here
}
```

### 2. Collector Trait

The main interface that all database collectors must implement:

```rust
#[async_trait]
pub trait Collector: Send + Sync {
    /// Collect all metrics and return a normalized payload
    async fn collect(&self) -> Result<Payload, CollectorError>;

    /// Test the database connection
    async fn test_connection(&self) -> Result<(), CollectorError>;

    /// Return the detected cloud provider (e.g., "rds", "neon", "generic")
    fn provider(&self) -> &str;

    /// Return the database version if known
    fn version(&self) -> Option<&str>;

    /// Return the database type
    fn database_type(&self) -> DatabaseType;
}
```

### 3. MetricType

Database-agnostic metric categories:

```rust
pub enum MetricType {
    /// Query performance statistics
    QueryStats,
    /// Table-level statistics (size, row counts, operations)
    TableStats,
    /// Index usage statistics
    IndexStats,
    /// Database configuration settings
    Settings,
    /// Schema metadata (tables, columns, indexes, keys)
    SchemaMetadata,
}
```

### 4. Payload

The normalized data structure sent to Datapace Cloud:

```rust
pub struct Payload {
    pub agent_version: String,
    pub timestamp: DateTime<Utc>,
    pub instance_id: String,
    pub database: DatabaseInfo,
    pub query_stats: Option<Vec<QueryStatistics>>,
    pub table_stats: Option<Vec<TableStatistics>>,
    pub index_stats: Option<Vec<IndexStatistics>>,
    pub settings: Option<HashMap<String, SettingValue>>,
    pub schema: Option<SchemaMetadata>,
}
```

## Adding a New Database Collector

### Step 1: Create the Module Structure

```
src/collector/mysql/
├── mod.rs           # Main collector implementation
├── queries.rs       # SQL queries for metrics
└── providers.rs     # Cloud provider detection
```

### Step 2: Implement the Collector

**`src/collector/mysql/mod.rs`:**

```rust
use async_trait::async_trait;
use sqlx::{MySql, Pool};

use crate::collector::{Collector, CollectorError, DatabaseType};
use crate::payload::Payload;

mod providers;
mod queries;

pub struct MySQLCollector {
    pool: Pool<MySql>,
    detected_provider: String,
    version: Option<String>,
}

impl MySQLCollector {
    pub async fn new(database_url: &str) -> Result<Self, CollectorError> {
        // Create connection pool
        let pool = Pool::<MySql>::connect(database_url)
            .await
            .map_err(|e| CollectorError::ConnectionFailed(e.to_string()))?;

        // Detect version
        let version = Self::detect_version(&pool).await.ok();

        // Detect cloud provider
        let detected_provider = providers::detect_provider(&pool, database_url).await?;

        Ok(Self {
            pool,
            detected_provider,
            version,
        })
    }

    async fn detect_version(pool: &Pool<MySql>) -> Result<String, CollectorError> {
        let row: (String,) = sqlx::query_as("SELECT VERSION()")
            .fetch_one(pool)
            .await?;
        Ok(row.0)
    }

    async fn collect_query_stats(&self) -> Result<Vec<QueryStatistics>, CollectorError> {
        // Implement using performance_schema
        todo!()
    }

    async fn collect_table_stats(&self) -> Result<Vec<TableStatistics>, CollectorError> {
        // Implement using information_schema
        todo!()
    }

    async fn collect_index_stats(&self) -> Result<Vec<IndexStatistics>, CollectorError> {
        // Implement using information_schema
        todo!()
    }

    async fn collect_settings(&self) -> Result<HashMap<String, SettingValue>, CollectorError> {
        // Implement using SHOW VARIABLES
        todo!()
    }

    async fn collect_schema(&self) -> Result<SchemaMetadata, CollectorError> {
        // Implement using information_schema
        todo!()
    }
}

#[async_trait]
impl Collector for MySQLCollector {
    async fn collect(&self) -> Result<Payload, CollectorError> {
        // Collect all metrics concurrently
        let (query_stats, table_stats, index_stats, settings, schema) = tokio::try_join!(
            self.collect_query_stats(),
            self.collect_table_stats(),
            self.collect_index_stats(),
            self.collect_settings(),
            self.collect_schema(),
        )?;

        Ok(Payload {
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: chrono::Utc::now(),
            instance_id: self.generate_instance_id(),
            database: DatabaseInfo {
                db_type: DatabaseType::MySQL,
                version: self.version.clone(),
                provider: self.detected_provider.clone(),
                provider_metadata: HashMap::new(),
            },
            query_stats: Some(query_stats),
            table_stats: Some(table_stats),
            index_stats: Some(index_stats),
            settings: Some(settings),
            schema: Some(schema),
        })
    }

    async fn test_connection(&self) -> Result<(), CollectorError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| CollectorError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }

    fn provider(&self) -> &str {
        &self.detected_provider
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }
}
```

### Step 3: Implement Provider Detection

**`src/collector/mysql/providers.rs`:**

```rust
use sqlx::{MySql, Pool};
use crate::collector::CollectorError;

pub async fn detect_provider(
    pool: &Pool<MySql>,
    connection_url: &str,
) -> Result<String, CollectorError> {
    // 1. Check URL patterns first
    if let Some(provider) = detect_from_url(connection_url) {
        return Ok(provider);
    }

    // 2. Check database variables
    if let Some(provider) = detect_from_variables(pool).await? {
        return Ok(provider);
    }

    // 3. Default to generic
    Ok("generic".to_string())
}

fn detect_from_url(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();

    if url_lower.contains(".rds.amazonaws.com") {
        // Could be RDS MySQL or Aurora MySQL
        return Some("rds".to_string());
    }

    if url_lower.contains("cloudsql") || url_lower.contains(".google.com") {
        return Some("cloudsql".to_string());
    }

    if url_lower.contains(".mysql.database.azure.com") {
        return Some("azure".to_string());
    }

    if url_lower.contains("planetscale") {
        return Some("planetscale".to_string());
    }

    None
}

async fn detect_from_variables(pool: &Pool<MySql>) -> Result<Option<String>, CollectorError> {
    // Check for Aurora-specific variable
    let result: Option<(String,)> = sqlx::query_as(
        "SHOW VARIABLES LIKE 'aurora_version'"
    )
    .fetch_optional(pool)
    .await?;

    if result.is_some() {
        return Ok(Some("aurora".to_string()));
    }

    // Check for RDS-specific variable
    let result: Option<(String,)> = sqlx::query_as(
        "SHOW VARIABLES LIKE 'rds_%'"
    )
    .fetch_optional(pool)
    .await?;

    if result.is_some() {
        return Ok(Some("rds".to_string()));
    }

    Ok(None)
}
```

### Step 4: Add SQL Queries

**`src/collector/mysql/queries.rs`:**

```rust
/// Query statistics from performance_schema
pub const QUERY_STATS: &str = r#"
SELECT
    DIGEST AS query_id,
    DIGEST_TEXT AS query,
    COUNT_STAR AS calls,
    SUM_TIMER_WAIT / 1000000000 AS total_time_ms,
    AVG_TIMER_WAIT / 1000000000 AS mean_time_ms,
    SUM_ROWS_SENT AS rows_returned,
    SUM_ROWS_EXAMINED AS rows_examined
FROM performance_schema.events_statements_summary_by_digest
WHERE DIGEST IS NOT NULL
ORDER BY SUM_TIMER_WAIT DESC
LIMIT 100
"#;

/// Table statistics from information_schema
pub const TABLE_STATS: &str = r#"
SELECT
    TABLE_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    TABLE_ROWS AS row_count,
    DATA_LENGTH AS data_size_bytes,
    INDEX_LENGTH AS index_size_bytes,
    AUTO_INCREMENT AS auto_increment
FROM information_schema.TABLES
WHERE TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
"#;

/// Index statistics
pub const INDEX_STATS: &str = r#"
SELECT
    TABLE_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    INDEX_NAME AS index_name,
    NON_UNIQUE AS non_unique,
    CARDINALITY AS cardinality
FROM information_schema.STATISTICS
WHERE TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
"#;

/// Key settings to collect
pub const SETTINGS_TO_COLLECT: &[&str] = &[
    "version",
    "max_connections",
    "innodb_buffer_pool_size",
    "innodb_log_file_size",
    "query_cache_size",
    "tmp_table_size",
    "max_heap_table_size",
    "innodb_flush_log_at_trx_commit",
    "sync_binlog",
    "character_set_server",
    "collation_server",
];
```

### Step 5: Register in Collector Factory

**Update `src/collector/mod.rs`:**

```rust
pub mod mysql;
pub mod postgres;

pub async fn create_collector(
    db_type: DatabaseType,
    database_url: &str,
) -> Result<Box<dyn Collector>, CollectorError> {
    match db_type {
        DatabaseType::Postgres => {
            let collector = postgres::PostgresCollector::new(database_url).await?;
            Ok(Box::new(collector))
        }
        DatabaseType::MySQL => {
            let collector = mysql::MySQLCollector::new(database_url).await?;
            Ok(Box::new(collector))
        }
        _ => Err(CollectorError::UnsupportedDatabase(db_type.to_string())),
    }
}
```

### Step 6: Update Configuration

**Update `src/config/mod.rs`:**

```rust
impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingField("DATABASE_URL".to_string()))?;

        // Auto-detect database type from URL
        let db_type = DatabaseType::from_url(&database_url)?;

        // ... rest of config
    }
}

impl DatabaseType {
    pub fn from_url(url: &str) -> Result<Self, ConfigError> {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(DatabaseType::Postgres)
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            Ok(DatabaseType::MySQL)
        } else if url.starts_with("mongodb://") || url.starts_with("mongodb+srv://") {
            Ok(DatabaseType::MongoDB)
        } else {
            Err(ConfigError::ValidationError(
                "Unsupported database URL scheme".to_string()
            ))
        }
    }
}
```

### Step 7: Add Dependencies

**Update `Cargo.toml`:**

```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "mysql"] }
```

### Step 8: Write Tests

**`src/collector/mysql/mod.rs` (add at bottom):**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_detection() {
        assert!(providers::detect_from_url("mysql://rds.amazonaws.com/db")
            .map(|p| p == "rds")
            .unwrap_or(false));
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL
    async fn test_mysql_connection() {
        let collector = MySQLCollector::new("mysql://root:password@localhost/test")
            .await
            .unwrap();
        collector.test_connection().await.unwrap();
    }
}
```

## Adding a New Metric Type

### Step 1: Define the Metric Structure

**Update `src/payload/mod.rs`:**

```rust
/// New metric: Slow queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryInfo {
    pub query: String,
    pub execution_time_ms: f64,
    pub lock_time_ms: f64,
    pub rows_examined: i64,
    pub timestamp: DateTime<Utc>,
}
```

### Step 2: Add to Payload

```rust
pub struct Payload {
    // ... existing fields ...

    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow_queries: Option<Vec<SlowQueryInfo>>,
}
```

### Step 3: Add to MetricType Enum

```rust
pub enum MetricType {
    QueryStats,
    TableStats,
    IndexStats,
    Settings,
    SchemaMetadata,
    SlowQueries,  // NEW
}
```

### Step 4: Implement Collection

Add to your collector:

```rust
async fn collect_slow_queries(&self) -> Result<Vec<SlowQueryInfo>, CollectorError> {
    // PostgreSQL: pg_stat_statements with min_time filter
    // MySQL: slow_log table or performance_schema
    todo!()
}
```

## Adding a New Cloud Provider

### Step 1: Update Provider Detection

**In `src/collector/{db}/providers.rs`:**

```rust
fn detect_from_url(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();

    // Add new provider detection
    if url_lower.contains(".newcloud.com") {
        return Some("newcloud".to_string());
    }

    // ... existing checks ...
}
```

### Step 2: Add Provider Metadata (Optional)

```rust
async fn get_newcloud_metadata(pool: &PgPool) -> Result<HashMap<String, String>, CollectorError> {
    let mut metadata = HashMap::new();

    // Collect provider-specific info
    // e.g., region, instance type, tier

    Ok(metadata)
}
```

### Step 3: Update Provider Enum

**In `src/config/mod.rs`:**

```rust
pub enum Provider {
    Auto,
    Generic,
    Rds,
    Aurora,
    Supabase,
    Neon,
    NewCloud,  // NEW
}
```

## Testing Your Implementation

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_parsing() {
        // Test SQL query result parsing
    }

    #[test]
    fn test_provider_detection() {
        // Test URL pattern matching
    }
}
```

### Integration Tests

Create `tests/integration/mysql_test.rs`:

```rust
use testcontainers::{clients, images::mysql::Mysql};

#[tokio::test]
async fn test_mysql_collector_integration() {
    let docker = clients::Cli::default();
    let mysql = docker.run(Mysql::default());

    let port = mysql.get_host_port_ipv4(3306);
    let url = format!("mysql://root:password@localhost:{}/test", port);

    let collector = MySQLCollector::new(&url).await.unwrap();
    let payload = collector.collect().await.unwrap();

    assert!(payload.table_stats.is_some());
}
```

### Run Tests

```bash
# Unit tests
cargo test

# Integration tests (requires Docker)
cargo test --features integration
```

## Checklist for New Database Support

- [ ] Create module structure (`src/collector/{db}/`)
- [ ] Implement `Collector` trait
- [ ] Implement provider detection
- [ ] Add SQL/query definitions
- [ ] Update collector factory
- [ ] Add `DatabaseType` variant
- [ ] Update URL validation
- [ ] Add to `Cargo.toml` dependencies
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Update documentation
- [ ] Add example configuration
- [ ] Test with real database
- [ ] Test with cloud providers

## Need Help?

- Open an issue on GitHub
- Check existing collector implementations for reference
- Join our Discord community

## Databases We'd Love Support For

- MySQL / MariaDB
- MongoDB
- Redis
- Microsoft SQL Server
- ClickHouse
- CockroachDB
- TimescaleDB
- Cassandra
- Elasticsearch

Contributions welcome!
