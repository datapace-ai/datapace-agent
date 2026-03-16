use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

/// Collects I/O statistics from pg_stat_io (PostgreSQL 16+).
/// Runs on the slow interval.
pub struct IoCollector;

#[derive(Debug, FromRow, Serialize)]
struct IoRow {
    backend_type: Option<String>,
    object: Option<String>,
    context: Option<String>,
    reads: Option<i64>,
    read_time: Option<f64>,
    writes: Option<i64>,
    write_time: Option<f64>,
    writebacks: Option<i64>,
    writeback_time: Option<f64>,
    extends: Option<i64>,
    extend_time: Option<f64>,
    hits: Option<i64>,
    evictions: Option<i64>,
    reuses: Option<i64>,
    fsyncs: Option<i64>,
    fsync_time: Option<f64>,
}

const QUERY: &str = r#"
SELECT
    backend_type,
    object,
    context,
    reads,
    read_time,
    writes,
    write_time,
    writebacks,
    writeback_time,
    extends,
    extend_time,
    hits,
    evictions,
    reuses,
    fsyncs,
    fsync_time
FROM pg_stat_io
WHERE reads > 0 OR writes > 0 OR hits > 0
ORDER BY reads + writes DESC
"#;

#[async_trait]
impl Collector for IoCollector {
    fn name(&self) -> &'static str {
        "io"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Slow
    }

    fn requires(&self) -> &[&'static str] {
        &["pg_stat_io"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let rows = sqlx::query_as::<_, IoRow>(QUERY).fetch_all(pg).await?;

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&rows).unwrap_or_default(),
            collected_at: Utc::now(),
        })
    }
}
