//! SQL queries for MySQL/MariaDB metrics collection.
//!
//! These queries are designed to work with MySQL 5.7+ and MariaDB 10.2+.

// ============================================================================
// Query Statistics (performance_schema)
// ============================================================================

/// Query statistics from performance_schema.events_statements_summary_by_digest
///
/// Requires: performance_schema enabled
/// Returns: Top 100 queries by total execution time
pub const QUERY_STATS: &str = r#"
SELECT
    DIGEST AS query_id,
    DIGEST_TEXT AS query,
    COUNT_STAR AS calls,
    SUM_TIMER_WAIT / 1000000000000 AS total_time_ms,
    AVG_TIMER_WAIT / 1000000000000 AS mean_time_ms,
    SUM_ROWS_SENT AS rows_returned,
    SUM_ROWS_EXAMINED AS rows_examined,
    SUM_ROWS_AFFECTED AS rows_affected,
    SUM_NO_INDEX_USED AS no_index_used,
    SUM_NO_GOOD_INDEX_USED AS no_good_index_used
FROM performance_schema.events_statements_summary_by_digest
WHERE DIGEST IS NOT NULL
  AND SCHEMA_NAME NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY SUM_TIMER_WAIT DESC
LIMIT 100
"#;

// ============================================================================
// Table Statistics (information_schema)
// ============================================================================

/// Table statistics from information_schema.TABLES
///
/// Returns: All user tables with size and row count information
pub const TABLE_STATS: &str = r#"
SELECT
    TABLE_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    TABLE_ROWS AS row_count,
    DATA_LENGTH AS data_size_bytes,
    INDEX_LENGTH AS index_size_bytes,
    DATA_LENGTH + INDEX_LENGTH AS total_size_bytes,
    AUTO_INCREMENT AS auto_increment,
    CREATE_TIME AS created_at,
    UPDATE_TIME AS updated_at,
    TABLE_COLLATION AS collation,
    ENGINE AS engine
FROM information_schema.TABLES
WHERE TABLE_TYPE = 'BASE TABLE'
  AND TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY TABLE_SCHEMA, TABLE_NAME
"#;

// ============================================================================
// Index Statistics (information_schema)
// ============================================================================

/// Index statistics from information_schema.STATISTICS
///
/// Returns: All indexes on user tables
pub const INDEX_STATS: &str = r#"
SELECT
    TABLE_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    INDEX_NAME AS index_name,
    NON_UNIQUE AS non_unique,
    SEQ_IN_INDEX AS seq_in_index,
    COLUMN_NAME AS column_name,
    CARDINALITY AS cardinality,
    NULLABLE AS nullable,
    INDEX_TYPE AS index_type
FROM information_schema.STATISTICS
WHERE TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY TABLE_SCHEMA, TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX
"#;

/// Index usage statistics from sys schema (if available)
///
/// Requires: sys schema installed
/// Returns: Index usage information
pub const INDEX_USAGE: &str = r#"
SELECT
    object_schema AS schema_name,
    object_name AS table_name,
    index_name,
    rows_selected,
    rows_inserted,
    rows_updated,
    rows_deleted
FROM sys.schema_index_statistics
WHERE object_schema NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY rows_selected DESC
"#;

// ============================================================================
// Database Settings
// ============================================================================

/// Key settings to collect from SHOW VARIABLES
pub const SETTINGS_TO_COLLECT: &[&str] = &[
    // Version info
    "version",
    "version_comment",
    // Connection settings
    "max_connections",
    "max_user_connections",
    "wait_timeout",
    "interactive_timeout",
    // InnoDB settings
    "innodb_buffer_pool_size",
    "innodb_buffer_pool_instances",
    "innodb_log_file_size",
    "innodb_log_buffer_size",
    "innodb_flush_log_at_trx_commit",
    "innodb_file_per_table",
    // Query cache (MySQL 5.7, removed in 8.0)
    "query_cache_type",
    "query_cache_size",
    // Memory settings
    "tmp_table_size",
    "max_heap_table_size",
    "sort_buffer_size",
    "join_buffer_size",
    "read_buffer_size",
    "read_rnd_buffer_size",
    // Replication
    "server_id",
    "log_bin",
    "binlog_format",
    "gtid_mode",
    // Character set
    "character_set_server",
    "collation_server",
    // Performance schema
    "performance_schema",
    // Slow query log
    "slow_query_log",
    "long_query_time",
];

/// Query to get settings values
pub const SETTINGS_QUERY: &str = r#"
SHOW VARIABLES WHERE Variable_name IN (
    'version', 'version_comment',
    'max_connections', 'max_user_connections', 'wait_timeout', 'interactive_timeout',
    'innodb_buffer_pool_size', 'innodb_buffer_pool_instances', 'innodb_log_file_size',
    'innodb_log_buffer_size', 'innodb_flush_log_at_trx_commit', 'innodb_file_per_table',
    'tmp_table_size', 'max_heap_table_size', 'sort_buffer_size', 'join_buffer_size',
    'read_buffer_size', 'read_rnd_buffer_size',
    'server_id', 'log_bin', 'binlog_format', 'gtid_mode',
    'character_set_server', 'collation_server',
    'performance_schema', 'slow_query_log', 'long_query_time'
)
"#;

// ============================================================================
// Schema Metadata
// ============================================================================

/// Table metadata from information_schema
pub const TABLE_METADATA: &str = r#"
SELECT
    TABLE_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    TABLE_ROWS AS row_estimate,
    DATA_LENGTH + INDEX_LENGTH AS total_bytes,
    ENGINE AS engine
FROM information_schema.TABLES
WHERE TABLE_TYPE = 'BASE TABLE'
  AND TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY TABLE_SCHEMA, TABLE_NAME
"#;

/// Column metadata from information_schema
pub const COLUMN_METADATA: &str = r#"
SELECT
    TABLE_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    COLUMN_NAME AS column_name,
    ORDINAL_POSITION AS ordinal_position,
    DATA_TYPE AS data_type,
    COLUMN_TYPE AS column_type,
    IS_NULLABLE AS is_nullable,
    COLUMN_KEY AS column_key,
    COLUMN_DEFAULT AS column_default,
    EXTRA AS extra
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION
"#;

/// Index metadata from information_schema
pub const INDEX_METADATA: &str = r#"
SELECT
    s.TABLE_SCHEMA AS schema_name,
    s.TABLE_NAME AS table_name,
    s.INDEX_NAME AS index_name,
    GROUP_CONCAT(s.COLUMN_NAME ORDER BY s.SEQ_IN_INDEX) AS columns,
    IF(s.NON_UNIQUE = 0, 1, 0) AS is_unique,
    IF(s.INDEX_NAME = 'PRIMARY', 1, 0) AS is_primary,
    s.INDEX_TYPE AS index_type,
    COALESCE(SUM(t.INDEX_LENGTH), 0) AS index_size_bytes
FROM information_schema.STATISTICS s
LEFT JOIN information_schema.TABLES t
    ON s.TABLE_SCHEMA = t.TABLE_SCHEMA AND s.TABLE_NAME = t.TABLE_NAME
WHERE s.TABLE_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
GROUP BY s.TABLE_SCHEMA, s.TABLE_NAME, s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE
ORDER BY s.TABLE_SCHEMA, s.TABLE_NAME, s.INDEX_NAME
"#;

/// Foreign key metadata from information_schema
pub const FOREIGN_KEY_METADATA: &str = r#"
SELECT
    CONSTRAINT_SCHEMA AS schema_name,
    TABLE_NAME AS table_name,
    CONSTRAINT_NAME AS constraint_name,
    COLUMN_NAME AS column_name,
    REFERENCED_TABLE_SCHEMA AS referenced_schema,
    REFERENCED_TABLE_NAME AS referenced_table,
    REFERENCED_COLUMN_NAME AS referenced_column
FROM information_schema.KEY_COLUMN_USAGE
WHERE REFERENCED_TABLE_NAME IS NOT NULL
  AND CONSTRAINT_SCHEMA NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
ORDER BY CONSTRAINT_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION
"#;

// ============================================================================
// Row Structs (for sqlx)
// ============================================================================

// TODO: Uncomment when adding MySQL support
// use sqlx::FromRow;
//
// #[derive(Debug, FromRow)]
// pub struct QueryStatsRow {
//     pub query_id: Option<String>,
//     pub query: Option<String>,
//     pub calls: i64,
//     pub total_time_ms: f64,
//     pub mean_time_ms: f64,
//     pub rows_returned: i64,
//     pub rows_examined: i64,
//     pub rows_affected: i64,
//     pub no_index_used: i64,
//     pub no_good_index_used: i64,
// }
//
// #[derive(Debug, FromRow)]
// pub struct TableStatsRow {
//     pub schema_name: String,
//     pub table_name: String,
//     pub row_count: Option<i64>,
//     pub data_size_bytes: Option<i64>,
//     pub index_size_bytes: Option<i64>,
//     pub total_size_bytes: Option<i64>,
//     pub engine: Option<String>,
// }
//
// #[derive(Debug, FromRow)]
// pub struct IndexStatsRow {
//     pub schema_name: String,
//     pub table_name: String,
//     pub index_name: String,
//     pub non_unique: i32,
//     pub cardinality: Option<i64>,
//     pub index_type: String,
// }
