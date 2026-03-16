use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;

/// Runs EXPLAIN (not ANALYZE) on the slowest queries found in pg_stat_statements.
/// Triggered on-demand by the fast loop when slow queries are detected.
pub struct ExplainCollector {
    /// Minimum mean_exec_time (ms) to consider a query "slow".
    pub threshold_ms: f64,
    /// Max number of queries to explain per tick.
    pub max_explains: usize,
}

impl Default for ExplainCollector {
    fn default() -> Self {
        Self {
            threshold_ms: 1000.0,
            max_explains: 5,
        }
    }
}

#[derive(Debug, Serialize)]
struct ExplainResult {
    queryid: i64,
    query: String,
    mean_exec_time: f64,
    plan: Vec<String>,
}

#[async_trait]
impl Collector for ExplainCollector {
    fn name(&self) -> &'static str {
        "explain"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["pg_stat_statements"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;

        // Find slow queries
        let slow_queries: Vec<(i64, String, f64)> = sqlx::query_as(
            "SELECT queryid, query, mean_exec_time
             FROM pg_stat_statements
             WHERE mean_exec_time > $1
               AND query NOT LIKE 'EXPLAIN%'
               AND query NOT LIKE 'COPY%'
               AND query NOT LIKE 'SET%'
             ORDER BY mean_exec_time DESC
             LIMIT $2",
        )
        .bind(self.threshold_ms)
        .bind(self.max_explains as i32)
        .fetch_all(pg)
        .await?;

        let mut results = Vec::new();

        for (queryid, query, mean_time) in slow_queries {
            // Only EXPLAIN queries that look safe (no DDL, no writes in the EXPLAIN itself)
            let trimmed = query.trim().to_uppercase();
            if !trimmed.starts_with("SELECT")
                && !trimmed.starts_with("WITH")
                && !trimmed.starts_with("TABLE")
            {
                continue;
            }

            let explain_query = format!("EXPLAIN (FORMAT TEXT) {}", query);
            match sqlx::query_as::<_, (String,)>(&explain_query)
                .fetch_all(pg)
                .await
            {
                Ok(plan_rows) => {
                    results.push(ExplainResult {
                        queryid,
                        query: query.clone(),
                        mean_exec_time: mean_time,
                        plan: plan_rows.into_iter().map(|(line,)| line).collect(),
                    });
                }
                Err(_) => {
                    // Skip queries that can't be explained (e.g. parameterized with $1)
                    continue;
                }
            }
        }

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&results).unwrap_or_default(),
            collected_at: Utc::now(),
        })
    }
}
