use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

/// Collects table-level statistics from pg_stat_user_tables.
/// Runs on the slow interval. Skips system schemas.
pub struct TablesCollector;

#[derive(Debug, FromRow, Serialize)]
struct TableRow {
    schemaname: String,
    relname: String,
    seq_scan: Option<i64>,
    seq_tup_read: Option<i64>,
    idx_scan: Option<i64>,
    idx_tup_fetch: Option<i64>,
    n_tup_ins: Option<i64>,
    n_tup_upd: Option<i64>,
    n_tup_del: Option<i64>,
    n_live_tup: Option<i64>,
    n_dead_tup: Option<i64>,
    last_vacuum: Option<chrono::DateTime<Utc>>,
    last_autovacuum: Option<chrono::DateTime<Utc>>,
    last_analyze: Option<chrono::DateTime<Utc>>,
    last_autoanalyze: Option<chrono::DateTime<Utc>>,
}

const QUERY: &str = r#"
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
ORDER BY n_live_tup DESC NULLS LAST
"#;

#[async_trait]
impl Collector for TablesCollector {
    fn name(&self) -> &'static str {
        "tables"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Slow
    }

    fn requires(&self) -> &[&'static str] {
        &[] // pg_stat_user_tables is always available
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let rows = sqlx::query_as::<_, TableRow>(QUERY).fetch_all(pg).await?;

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&rows).unwrap_or_default(),
            collected_at: Utc::now(),
            idempotency_key: String::new(),
        })
    }
}
