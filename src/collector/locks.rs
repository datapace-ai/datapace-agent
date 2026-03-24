use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

/// Collects lock information from pg_locks, including blocking chains.
/// Runs on the fast interval.
pub struct LocksCollector;

#[derive(Debug, FromRow, Serialize)]
struct LockRow {
    pid: Option<i32>,
    locktype: Option<String>,
    mode: Option<String>,
    granted: Option<bool>,
    datname: Option<String>,
    relname: Option<String>,
    usename: Option<String>,
    query: Option<String>,
    wait_event_type: Option<String>,
    wait_event: Option<String>,
    blocking_pid: Option<i32>,
}

const QUERY: &str = r#"
SELECT
    l.pid,
    l.locktype,
    l.mode,
    l.granted,
    d.datname,
    c.relname,
    a.usename,
    a.query,
    a.wait_event_type,
    a.wait_event,
    bl.pid AS blocking_pid
FROM pg_locks l
JOIN pg_stat_activity a ON a.pid = l.pid
LEFT JOIN pg_database d ON d.oid = l.database
LEFT JOIN pg_class c ON c.oid = l.relation
LEFT JOIN pg_locks bl_lock ON
    bl_lock.relation = l.relation
    AND bl_lock.granted
    AND bl_lock.pid != l.pid
LEFT JOIN pg_stat_activity bl ON bl.pid = bl_lock.pid
WHERE NOT l.granted
   OR EXISTS (
       SELECT 1 FROM pg_locks wl
       WHERE wl.relation = l.relation
         AND NOT wl.granted
         AND wl.pid != l.pid
   )
ORDER BY l.pid
"#;

#[async_trait]
impl Collector for LocksCollector {
    fn name(&self) -> &'static str {
        "locks"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["pg_locks"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let rows = sqlx::query_as::<_, LockRow>(QUERY).fetch_all(pg).await?;

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&rows).unwrap_or_default(),
            collected_at: Utc::now(),
            idempotency_key: String::new(),
        })
    }
}
