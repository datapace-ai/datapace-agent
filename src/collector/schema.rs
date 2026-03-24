use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

/// Collects schema metadata: table sizes, index definitions, bloat estimates, unused indexes.
/// Runs on the slow interval.
pub struct SchemaCollector;

#[derive(Debug, Serialize)]
struct SchemaSnapshot {
    tables: Vec<TableInfo>,
    indexes: Vec<IndexInfo>,
    unused_indexes: Vec<UnusedIndex>,
}

#[derive(Debug, FromRow, Serialize)]
struct TableInfo {
    schema_name: String,
    table_name: String,
    row_estimate: Option<i64>,
    total_bytes: Option<i64>,
    table_bytes: Option<i64>,
    index_bytes: Option<i64>,
    toast_bytes: Option<i64>,
}

#[derive(Debug, FromRow, Serialize)]
struct IndexInfo {
    schema_name: String,
    table_name: String,
    index_name: String,
    index_def: Option<String>,
    index_size: Option<i64>,
    is_unique: Option<bool>,
    is_primary: Option<bool>,
}

#[derive(Debug, FromRow, Serialize)]
struct UnusedIndex {
    schema_name: String,
    table_name: String,
    index_name: String,
    index_size: Option<i64>,
    idx_scan: Option<i64>,
}

const TABLE_QUERY: &str = r#"
SELECT
    n.nspname                                       AS schema_name,
    c.relname                                       AS table_name,
    c.reltuples::bigint                             AS row_estimate,
    pg_total_relation_size(c.oid)::bigint           AS total_bytes,
    pg_relation_size(c.oid)::bigint                 AS table_bytes,
    pg_indexes_size(c.oid)::bigint                  AS index_bytes,
    COALESCE(pg_total_relation_size(c.reltoastrelid), 0)::bigint AS toast_bytes
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind = 'r'
  AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
ORDER BY pg_total_relation_size(c.oid) DESC
"#;

const INDEX_QUERY: &str = r#"
SELECT
    schemaname                                      AS schema_name,
    tablename                                       AS table_name,
    indexname                                       AS index_name,
    indexdef                                        AS index_def,
    pg_relation_size(quote_ident(schemaname) || '.' || quote_ident(indexname))::bigint AS index_size,
    idx.indisunique                                 AS is_unique,
    idx.indisprimary                                AS is_primary
FROM pg_indexes
JOIN pg_class c ON c.relname = indexname
JOIN pg_index idx ON idx.indexrelid = c.oid
WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
ORDER BY pg_relation_size(quote_ident(schemaname) || '.' || quote_ident(indexname)) DESC
"#;

const UNUSED_INDEX_QUERY: &str = r#"
SELECT
    s.schemaname                                    AS schema_name,
    s.relname                                       AS table_name,
    s.indexrelname                                  AS index_name,
    pg_relation_size(i.indexrelid)::bigint          AS index_size,
    s.idx_scan
FROM pg_stat_user_indexes s
JOIN pg_index i ON i.indexrelid = s.indexrelid
WHERE s.idx_scan = 0
  AND NOT i.indisunique
  AND NOT i.indisprimary
ORDER BY pg_relation_size(i.indexrelid) DESC
LIMIT 50
"#;

#[async_trait]
impl Collector for SchemaCollector {
    fn name(&self) -> &'static str {
        "schema"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Slow
    }

    fn requires(&self) -> &[&'static str] {
        &["schema_catalog"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let (tables, indexes, unused) = tokio::try_join!(
            async {
                sqlx::query_as::<_, TableInfo>(TABLE_QUERY)
                    .fetch_all(pg)
                    .await
                    .map_err(CollectorError::from)
            },
            async {
                sqlx::query_as::<_, IndexInfo>(INDEX_QUERY)
                    .fetch_all(pg)
                    .await
                    .map_err(CollectorError::from)
            },
            async {
                sqlx::query_as::<_, UnusedIndex>(UNUSED_INDEX_QUERY)
                    .fetch_all(pg)
                    .await
                    .map_err(CollectorError::from)
            },
        )?;

        let snap = SchemaSnapshot {
            tables,
            indexes,
            unused_indexes: unused,
        };

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&snap).unwrap_or_default(),
            collected_at: Utc::now(),
            idempotency_key: String::new(),
        })
    }
}
