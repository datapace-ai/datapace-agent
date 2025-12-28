//! SQL queries for PostgreSQL metrics collection.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

/// Query statistics from pg_stat_statements
pub const PG_STAT_STATEMENTS: &str = r#"
SELECT
    queryid,
    query,
    calls,
    total_exec_time,
    mean_exec_time,
    rows,
    shared_blks_hit,
    shared_blks_read
FROM pg_stat_statements
WHERE userid = (SELECT usesysid FROM pg_user WHERE usename = current_user)
ORDER BY total_exec_time DESC
LIMIT 100
"#;

#[derive(Debug, FromRow)]
pub struct PgStatStatementsRow {
    pub queryid: Option<i64>,
    pub query: Option<String>,
    pub calls: Option<i64>,
    pub total_exec_time: Option<f64>,
    pub mean_exec_time: Option<f64>,
    pub rows: Option<i64>,
    pub shared_blks_hit: Option<i64>,
    pub shared_blks_read: Option<i64>,
}

/// Table statistics from pg_stat_user_tables
pub const PG_STAT_USER_TABLES: &str = r#"
SELECT
    schemaname,
    relname,
    seq_scan,
    seq_tup_read,
    idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del,
    n_live_tup,
    n_dead_tup,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
FROM pg_stat_user_tables
ORDER BY n_live_tup DESC
"#;

#[derive(Debug, FromRow)]
pub struct PgStatUserTablesRow {
    pub schemaname: String,
    pub relname: String,
    pub seq_scan: Option<i64>,
    pub seq_tup_read: Option<i64>,
    pub idx_scan: Option<i64>,
    pub idx_tup_fetch: Option<i64>,
    pub n_tup_ins: Option<i64>,
    pub n_tup_upd: Option<i64>,
    pub n_tup_del: Option<i64>,
    pub n_live_tup: Option<i64>,
    pub n_dead_tup: Option<i64>,
    pub last_vacuum: Option<DateTime<Utc>>,
    pub last_autovacuum: Option<DateTime<Utc>>,
    pub last_analyze: Option<DateTime<Utc>>,
    pub last_autoanalyze: Option<DateTime<Utc>>,
}

/// Index statistics from pg_stat_user_indexes
pub const PG_STAT_USER_INDEXES: &str = r#"
SELECT
    schemaname,
    relname,
    indexrelname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
ORDER BY idx_scan DESC
"#;

#[derive(Debug, FromRow)]
pub struct PgStatUserIndexesRow {
    pub schemaname: String,
    pub relname: String,
    pub indexrelname: String,
    pub idx_scan: Option<i64>,
    pub idx_tup_read: Option<i64>,
    pub idx_tup_fetch: Option<i64>,
}

/// Relevant database settings from pg_settings
pub const PG_SETTINGS: &str = r#"
SELECT name, setting
FROM pg_settings
WHERE name IN (
    'max_connections',
    'shared_buffers',
    'effective_cache_size',
    'maintenance_work_mem',
    'checkpoint_completion_target',
    'wal_buffers',
    'default_statistics_target',
    'random_page_cost',
    'effective_io_concurrency',
    'work_mem',
    'min_wal_size',
    'max_wal_size',
    'max_worker_processes',
    'max_parallel_workers_per_gather',
    'max_parallel_workers',
    'max_parallel_maintenance_workers',
    'server_version',
    'server_encoding',
    'timezone'
)
ORDER BY name
"#;

#[derive(Debug, FromRow)]
pub struct PgSettingsRow {
    pub name: String,
    pub setting: String,
}

/// Table information for schema metadata
pub const TABLE_INFO: &str = r#"
SELECT
    t.table_schema,
    t.table_name,
    c.reltuples::bigint as row_estimate,
    pg_total_relation_size(quote_ident(t.table_schema) || '.' || quote_ident(t.table_name))::bigint as total_bytes
FROM information_schema.tables t
JOIN pg_class c ON c.relname = t.table_name
JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = t.table_schema
WHERE t.table_schema NOT IN ('pg_catalog', 'information_schema')
    AND t.table_type = 'BASE TABLE'
ORDER BY total_bytes DESC
"#;

#[derive(Debug, FromRow)]
pub struct TableInfoRow {
    pub table_schema: String,
    pub table_name: String,
    pub row_estimate: Option<i64>,
    pub total_bytes: Option<i64>,
}

/// Column information for schema metadata
pub const COLUMN_INFO: &str = r#"
SELECT
    table_schema,
    table_name,
    column_name,
    ordinal_position,
    is_nullable,
    data_type,
    character_maximum_length,
    numeric_precision,
    column_default
FROM information_schema.columns
WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
ORDER BY table_schema, table_name, ordinal_position
"#;

#[derive(Debug, FromRow)]
pub struct ColumnInfoRow {
    pub table_schema: String,
    pub table_name: String,
    pub column_name: String,
    pub ordinal_position: i32,
    pub is_nullable: String,
    pub data_type: String,
    pub character_maximum_length: Option<i32>,
    pub numeric_precision: Option<i32>,
    pub column_default: Option<String>,
}

/// Index information for schema metadata
pub const INDEX_INFO: &str = r#"
SELECT
    schemaname,
    tablename,
    indexname,
    indexdef,
    pg_relation_size(quote_ident(schemaname) || '.' || quote_ident(indexname))::bigint as index_size,
    idx.indisunique as is_unique,
    idx.indisprimary as is_primary,
    (
        SELECT string_agg(a.attname, ', ' ORDER BY array_position(idx.indkey, a.attnum))
        FROM pg_attribute a
        WHERE a.attrelid = idx.indrelid
        AND a.attnum = ANY(idx.indkey)
    ) as columns
FROM pg_indexes
JOIN pg_class c ON c.relname = indexname
JOIN pg_index idx ON idx.indexrelid = c.oid
WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
ORDER BY index_size DESC
"#;

#[derive(Debug, FromRow)]
pub struct IndexInfoRow {
    pub schemaname: String,
    pub tablename: String,
    pub indexname: String,
    pub indexdef: Option<String>,
    pub index_size: Option<i64>,
    pub is_unique: Option<bool>,
    pub is_primary: Option<bool>,
    pub columns: Option<String>,
}

/// Foreign key information
pub const FOREIGN_KEY_INFO: &str = r#"
SELECT
    tc.table_schema,
    tc.table_name,
    kcu.column_name,
    ccu.table_schema AS foreign_table_schema,
    ccu.table_name AS foreign_table_name,
    ccu.column_name AS foreign_column_name,
    tc.constraint_name
FROM information_schema.table_constraints AS tc
JOIN information_schema.key_column_usage AS kcu
    ON tc.constraint_name = kcu.constraint_name
    AND tc.table_schema = kcu.table_schema
JOIN information_schema.constraint_column_usage AS ccu
    ON ccu.constraint_name = tc.constraint_name
    AND ccu.table_schema = tc.table_schema
WHERE tc.constraint_type = 'FOREIGN KEY'
    AND tc.table_schema NOT IN ('pg_catalog', 'information_schema')
"#;

#[derive(Debug, FromRow)]
pub struct ForeignKeyRow {
    pub table_schema: String,
    pub table_name: String,
    pub column_name: String,
    pub foreign_table_schema: String,
    pub foreign_table_name: String,
    pub foreign_column_name: String,
    pub constraint_name: String,
}
