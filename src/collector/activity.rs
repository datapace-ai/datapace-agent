use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

/// Collects non-idle sessions from pg_stat_activity.
/// Query text is included but will be anonymized downstream.
pub struct ActivityCollector;

#[derive(Debug, FromRow, Serialize)]
struct ActivityRow {
    pid: Option<i32>,
    datname: Option<String>,
    usename: Option<String>,
    application_name: Option<String>,
    state: Option<String>,
    wait_event_type: Option<String>,
    wait_event: Option<String>,
    query: Option<String>,
    query_start: Option<chrono::DateTime<Utc>>,
    state_change: Option<chrono::DateTime<Utc>>,
    backend_type: Option<String>,
}

const QUERY: &str = r#"
SELECT
    pid,
    datname,
    usename,
    application_name,
    state,
    wait_event_type,
    wait_event,
    query,
    query_start,
    state_change,
    backend_type
FROM pg_stat_activity
WHERE state != 'idle'
  AND pid != pg_backend_pid()
ORDER BY query_start ASC NULLS LAST
"#;

#[async_trait]
impl Collector for ActivityCollector {
    fn name(&self) -> &'static str {
        "activity"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["pg_stat_activity"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let rows = sqlx::query_as::<_, ActivityRow>(QUERY)
            .fetch_all(pg)
            .await?;

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&rows).unwrap_or_default(),
            collected_at: Utc::now(),
        })
    }
}
