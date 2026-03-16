use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

/// Collects query statistics from pg_stat_statements with delta tracking.
/// Top 200 by total_exec_time. Requires pg_stat_statements extension.
pub struct StatementsCollector;

#[derive(Debug, FromRow, Serialize)]
struct StatRow {
    queryid: Option<i64>,
    query: Option<String>,
    calls: Option<i64>,
    total_exec_time: Option<f64>,
    mean_exec_time: Option<f64>,
    min_exec_time: Option<f64>,
    max_exec_time: Option<f64>,
    rows: Option<i64>,
    shared_blks_hit: Option<i64>,
    shared_blks_read: Option<i64>,
    shared_blks_written: Option<i64>,
    local_blks_hit: Option<i64>,
    local_blks_read: Option<i64>,
    temp_blks_read: Option<i64>,
    temp_blks_written: Option<i64>,
}

const QUERY: &str = r#"
SELECT
    queryid,
    query,
    calls,
    total_exec_time,
    mean_exec_time,
    min_exec_time,
    max_exec_time,
    rows,
    shared_blks_hit,
    shared_blks_read,
    shared_blks_written,
    local_blks_hit,
    local_blks_read,
    temp_blks_read,
    temp_blks_written
FROM pg_stat_statements
ORDER BY total_exec_time DESC NULLS LAST
LIMIT 200
"#;

#[async_trait]
impl Collector for StatementsCollector {
    fn name(&self) -> &'static str {
        "statements"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["pg_stat_statements"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let rows = sqlx::query_as::<_, StatRow>(QUERY).fetch_all(pg).await?;

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&rows).unwrap_or_default(),
            collected_at: Utc::now(),
        })
    }
}
