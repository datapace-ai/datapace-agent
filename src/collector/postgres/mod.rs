//! PostgreSQL metrics collector.
//!
//! Collects metrics from PostgreSQL databases including:
//! - Query statistics (pg_stat_statements)
//! - Table statistics (pg_stat_user_tables)
//! - Index statistics (pg_stat_user_indexes)
//! - Configuration settings (pg_settings)
//! - Schema metadata

mod providers;
mod queries;

use crate::collector::{Collector, CollectorError};
use crate::config::{DatabaseType, Provider};
use crate::payload::{
    DatabaseInfo, IndexMetadata, IndexStats, Payload, QueryStats, SchemaMetadata,
    TableMetadata, TableStats,
};
use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};

/// PostgreSQL metrics collector
pub struct PostgresCollector {
    pool: PgPool,
    #[allow(dead_code)]
    provider: Provider,
    detected_provider: String,
    version: Option<String>,
}

impl PostgresCollector {
    /// Create a new PostgreSQL collector
    pub async fn new(database_url: &str, provider: Provider) -> Result<Self, CollectorError> {
        info!("Connecting to PostgreSQL database");

        let pool = PgPoolOptions::new()
            .min_connections(1)
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await
            .map_err(|e| CollectorError::ConnectionError(e.to_string()))?;

        // Get database version
        let version = Self::get_version(&pool).await?;
        info!(version = %version, "Connected to PostgreSQL");

        // Detect provider if set to auto
        let detected_provider = if provider == Provider::Auto {
            providers::detect_provider(&pool, database_url).await?
        } else {
            provider.to_string()
        };

        info!(provider = %detected_provider, "Database provider detected");

        Ok(Self {
            pool,
            provider,
            detected_provider,
            version: Some(version),
        })
    }

    async fn get_version(pool: &PgPool) -> Result<String, CollectorError> {
        let row: (String,) = sqlx::query_as("SELECT version()")
            .fetch_one(pool)
            .await?;
        Ok(row.0)
    }

    async fn collect_query_stats(&self) -> Result<Vec<QueryStats>, CollectorError> {
        debug!("Collecting query statistics from pg_stat_statements");

        // Check if pg_stat_statements is available
        let has_extension: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements')",
        )
        .fetch_one(&self.pool)
        .await?;

        if !has_extension.0 {
            warn!("pg_stat_statements extension not installed, skipping query stats");
            return Ok(vec![]);
        }

        let rows = sqlx::query_as::<_, queries::PgStatStatementsRow>(queries::PG_STAT_STATEMENTS)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| QueryStats {
                query_hash: row.queryid.map(|id| format!("{:x}", id)),
                query: row.query,
                calls: row.calls,
                total_time_ms: row.total_exec_time,
                mean_time_ms: row.mean_exec_time,
                rows: row.rows,
                shared_blks_hit: row.shared_blks_hit,
                shared_blks_read: row.shared_blks_read,
            })
            .collect())
    }

    async fn collect_table_stats(&self) -> Result<Vec<TableStats>, CollectorError> {
        debug!("Collecting table statistics from pg_stat_user_tables");

        let rows = sqlx::query_as::<_, queries::PgStatUserTablesRow>(queries::PG_STAT_USER_TABLES)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| TableStats {
                schema: row.schemaname,
                table: row.relname,
                seq_scan: row.seq_scan,
                seq_tup_read: row.seq_tup_read,
                idx_scan: row.idx_scan,
                idx_tup_fetch: row.idx_tup_fetch,
                n_tup_ins: row.n_tup_ins,
                n_tup_upd: row.n_tup_upd,
                n_tup_del: row.n_tup_del,
                n_live_tup: row.n_live_tup,
                n_dead_tup: row.n_dead_tup,
                last_vacuum: row.last_vacuum,
                last_autovacuum: row.last_autovacuum,
                last_analyze: row.last_analyze,
                last_autoanalyze: row.last_autoanalyze,
            })
            .collect())
    }

    async fn collect_index_stats(&self) -> Result<Vec<IndexStats>, CollectorError> {
        debug!("Collecting index statistics from pg_stat_user_indexes");

        let rows =
            sqlx::query_as::<_, queries::PgStatUserIndexesRow>(queries::PG_STAT_USER_INDEXES)
                .fetch_all(&self.pool)
                .await?;

        Ok(rows
            .into_iter()
            .map(|row| IndexStats {
                schema: row.schemaname,
                table: row.relname,
                index: row.indexrelname,
                idx_scan: row.idx_scan,
                idx_tup_read: row.idx_tup_read,
                idx_tup_fetch: row.idx_tup_fetch,
            })
            .collect())
    }

    async fn collect_settings(&self) -> Result<HashMap<String, String>, CollectorError> {
        debug!("Collecting database settings from pg_settings");

        let rows = sqlx::query_as::<_, queries::PgSettingsRow>(queries::PG_SETTINGS)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.name, row.setting))
            .collect())
    }

    async fn collect_schema_metadata(&self) -> Result<SchemaMetadata, CollectorError> {
        debug!("Collecting schema metadata");

        // Collect tables
        let table_rows = sqlx::query_as::<_, queries::TableInfoRow>(queries::TABLE_INFO)
            .fetch_all(&self.pool)
            .await?;

        let tables: Vec<TableMetadata> = table_rows
            .into_iter()
            .map(|row| TableMetadata {
                schema: row.table_schema,
                name: row.table_name,
                columns: vec![], // Columns collected separately
                row_count_estimate: row.row_estimate,
                size_bytes: row.total_bytes,
            })
            .collect();

        // Collect indexes
        let index_rows = sqlx::query_as::<_, queries::IndexInfoRow>(queries::INDEX_INFO)
            .fetch_all(&self.pool)
            .await?;

        let indexes: Vec<IndexMetadata> = index_rows
            .into_iter()
            .map(|row| IndexMetadata {
                schema: row.schemaname,
                table: row.tablename,
                name: row.indexname,
                columns: row
                    .columns
                    .map(|c| c.split(", ").map(String::from).collect())
                    .unwrap_or_default(),
                is_unique: row.is_unique.unwrap_or(false),
                is_primary: row.is_primary.unwrap_or(false),
                size_bytes: row.index_size,
            })
            .collect();

        Ok(SchemaMetadata { tables, indexes })
    }
}

#[async_trait]
impl Collector for PostgresCollector {
    async fn collect(&self) -> Result<Payload, CollectorError> {
        info!("Starting metrics collection");

        // Collect all metrics concurrently
        let (query_stats, table_stats, index_stats, settings, schema) = tokio::try_join!(
            self.collect_query_stats(),
            self.collect_table_stats(),
            self.collect_index_stats(),
            self.collect_settings(),
            self.collect_schema_metadata(),
        )?;

        let database_info = DatabaseInfo {
            database_type: "postgres".to_string(),
            version: self.version.clone(),
            provider: self.detected_provider.clone(),
            provider_metadata: providers::get_provider_metadata(&self.pool, &self.detected_provider)
                .await
                .unwrap_or_default(),
        };

        let payload = Payload::new(database_info)
            .with_query_stats(query_stats)
            .with_table_stats(table_stats)
            .with_index_stats(index_stats)
            .with_settings(settings)
            .with_schema(schema);

        info!(
            tables = payload.schema.as_ref().map(|s| s.tables.len()).unwrap_or(0),
            indexes = payload.schema.as_ref().map(|s| s.indexes.len()).unwrap_or(0),
            queries = payload.query_stats.as_ref().map(|q| q.len()).unwrap_or(0),
            "Metrics collection complete"
        );

        Ok(payload)
    }

    async fn test_connection(&self) -> Result<(), CollectorError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| CollectorError::ConnectionError(e.to_string()))?;
        Ok(())
    }

    fn provider(&self) -> &str {
        &self.detected_provider
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::Postgres
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_to_string() {
        assert_eq!(Provider::Auto.to_string(), "auto");
        assert_eq!(Provider::Rds.to_string(), "rds");
    }
}
