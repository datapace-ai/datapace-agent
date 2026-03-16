use crate::store::Store;
use std::fmt::Write;
use tracing::debug;

/// Build Prometheus text-format metrics from the latest snapshots in the store.
/// This is called by the HTTP handler for GET /metrics.
pub async fn render_metrics(store: &Store) -> String {
    let mut out = String::with_capacity(4096);

    // Agent metadata
    writeln!(out, "# HELP datapace_agent_info Agent version info").ok();
    writeln!(out, "# TYPE datapace_agent_info gauge").ok();
    writeln!(
        out,
        "datapace_agent_info{{version=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION")
    )
    .ok();

    let snapshots = match store.get_latest_snapshots().await {
        Ok(s) => s,
        Err(e) => {
            debug!("Failed to get snapshots for prometheus: {e}");
            return out;
        }
    };

    for snap in &snapshots {
        match snap.collector.as_str() {
            "statements" => render_statements(&mut out, &snap.data),
            "activity" => render_activity(&mut out, &snap.data),
            "tables" => render_tables(&mut out, &snap.data),
            "locks" => render_locks(&mut out, &snap.data),
            _ => {}
        }
    }

    out
}

fn render_statements(out: &mut String, data: &serde_json::Value) {
    if let Some(items) = data.as_array() {
        writeln!(
            out,
            "# HELP pg_stat_statements_calls Total number of query executions"
        )
        .ok();
        writeln!(out, "# TYPE pg_stat_statements_calls counter").ok();
        writeln!(
            out,
            "# HELP pg_stat_statements_total_exec_time_ms Total execution time in milliseconds"
        )
        .ok();
        writeln!(out, "# TYPE pg_stat_statements_total_exec_time_ms counter").ok();

        for item in items.iter().take(50) {
            let qid = item.get("queryid").and_then(|v| v.as_i64()).unwrap_or(0);
            if let Some(calls) = item.get("calls").and_then(|v| v.as_i64()) {
                writeln!(out, "pg_stat_statements_calls{{queryid=\"{qid}\"}} {calls}").ok();
            }
            if let Some(time) = item.get("total_exec_time").and_then(|v| v.as_f64()) {
                writeln!(
                    out,
                    "pg_stat_statements_total_exec_time_ms{{queryid=\"{qid}\"}} {time:.2}"
                )
                .ok();
            }
        }
    }
}

fn render_activity(out: &mut String, data: &serde_json::Value) {
    if let Some(items) = data.as_array() {
        writeln!(
            out,
            "# HELP pg_stat_activity_count Number of active sessions"
        )
        .ok();
        writeln!(out, "# TYPE pg_stat_activity_count gauge").ok();
        writeln!(out, "pg_stat_activity_count {}", items.len()).ok();
    }
}

fn render_tables(out: &mut String, data: &serde_json::Value) {
    if let Some(items) = data.as_array() {
        writeln!(
            out,
            "# HELP pg_stat_user_tables_n_live_tup Estimated live rows"
        )
        .ok();
        writeln!(out, "# TYPE pg_stat_user_tables_n_live_tup gauge").ok();
        writeln!(out, "# HELP pg_stat_user_tables_n_dead_tup Dead rows").ok();
        writeln!(out, "# TYPE pg_stat_user_tables_n_dead_tup gauge").ok();

        for item in items {
            let schema = item
                .get("schemaname")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let table = item.get("relname").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(live) = item.get("n_live_tup").and_then(|v| v.as_i64()) {
                writeln!(
                    out,
                    "pg_stat_user_tables_n_live_tup{{schema=\"{schema}\",table=\"{table}\"}} {live}"
                )
                .ok();
            }
            if let Some(dead) = item.get("n_dead_tup").and_then(|v| v.as_i64()) {
                writeln!(
                    out,
                    "pg_stat_user_tables_n_dead_tup{{schema=\"{schema}\",table=\"{table}\"}} {dead}"
                )
                .ok();
            }
        }
    }
}

fn render_locks(out: &mut String, data: &serde_json::Value) {
    if let Some(items) = data.as_array() {
        let blocked = items
            .iter()
            .filter(|i| i.get("granted").and_then(|v| v.as_bool()) == Some(false))
            .count();
        writeln!(
            out,
            "# HELP pg_locks_blocked Number of blocked lock requests"
        )
        .ok();
        writeln!(out, "# TYPE pg_locks_blocked gauge").ok();
        writeln!(out, "pg_locks_blocked {blocked}").ok();
    }
}
