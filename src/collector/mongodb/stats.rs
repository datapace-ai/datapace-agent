//! MongoDB-side stats collection.
//!
//! Maps Mongo admin commands onto the agent's existing [`Payload`] slots so
//! consumers don't have to grow new top-level fields:
//!
//! - `collStats` per collection → [`TableStats`] (lossy: only `n_live_tup`
//!   is populated; SQL-only fields like `seq_scan` stay `None`)
//! - `$indexStats` per collection → [`IndexStats`]
//! - Curated `serverStatus` slice → [`Payload::settings`] (allowlisted; we
//!   don't dump the full blob)
//! - `query_stats` is intentionally empty for v1 — `serverStatus.opcounters`
//!   is server-wide rather than per-query, so it doesn't fit the shape.

use crate::collector::CollectorError;
use crate::payload::{IndexStats, QueryStats, TableStats};
use futures::StreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::Database;
use std::collections::HashMap;

/// Collect per-collection document counts and sizes.
pub async fn collect_table_stats(
    db: &Database,
    collections: &[String],
) -> Result<Vec<TableStats>, CollectorError> {
    let db_name = db.name().to_string();
    let mut out = Vec::with_capacity(collections.len());
    for name in collections {
        match db.run_command(doc! { "collStats": name.as_str() }).await {
            Ok(stats) => {
                out.push(TableStats {
                    schema: db_name.clone(),
                    table: name.clone(),
                    seq_scan: None,
                    seq_tup_read: None,
                    idx_scan: None,
                    idx_tup_fetch: None,
                    n_tup_ins: None,
                    n_tup_upd: None,
                    n_tup_del: None,
                    n_live_tup: stats
                        .get_i64("count")
                        .ok()
                        .or_else(|| stats.get_i32("count").ok().map(i64::from)),
                    n_dead_tup: None,
                    last_vacuum: None,
                    last_autovacuum: None,
                    last_analyze: None,
                    last_autoanalyze: None,
                });
            }
            Err(err) => {
                tracing::debug!(collection=%name, error=%err, "collStats failed (likely view); skipping");
            }
        }
    }
    Ok(out)
}

/// Collect per-index access counters via `$indexStats` aggregation.
pub async fn collect_index_stats(
    db: &Database,
    collections: &[String],
) -> Result<Vec<IndexStats>, CollectorError> {
    let db_name = db.name().to_string();
    let mut out = Vec::new();
    for coll_name in collections {
        let coll = db.collection::<Document>(coll_name);
        let pipeline = vec![doc! { "$indexStats": {} }];
        let mut cursor = match coll.aggregate(pipeline).await {
            Ok(c) => c,
            Err(err) => {
                tracing::debug!(collection=%coll_name, error=%err, "$indexStats unavailable");
                continue;
            }
        };
        while let Some(doc_res) = cursor.next().await {
            let doc = match doc_res {
                Ok(d) => d,
                Err(err) => {
                    tracing::debug!(collection=%coll_name, error=%err, "$indexStats stream error");
                    continue;
                }
            };
            let name = doc.get_str("name").unwrap_or("").to_string();
            let ops = doc.get_document("accesses").ok().and_then(|a| {
                a.get_i64("ops")
                    .ok()
                    .or_else(|| a.get_i32("ops").ok().map(i64::from))
            });
            out.push(IndexStats {
                schema: db_name.clone(),
                table: coll_name.clone(),
                index: name,
                idx_scan: ops,
                idx_tup_read: None,
                idx_tup_fetch: None,
            });
        }
    }
    Ok(out)
}

/// Curated subset of `serverStatus` exposed as the platform-agnostic
/// `settings` map. Avoids dumping the entire (large) serverStatus blob.
pub async fn collect_settings(db: &Database) -> Result<HashMap<String, String>, CollectorError> {
    let mut out = HashMap::new();
    let admin = db.client().database("admin");
    if let Ok(status) = admin.run_command(doc! { "serverStatus": 1 }).await {
        for key in [
            "version",
            "process",
            "host",
            "uptime",
            "uptimeMillis",
            "localTime",
        ] {
            if let Some(v) = status.get(key) {
                out.insert(key.to_string(), bson_to_string(v));
            }
        }
        if let Ok(storage) = status.get_document("storageEngine") {
            if let Some(v) = storage.get("name") {
                out.insert("storageEngine.name".into(), bson_to_string(v));
            }
        }
        if let Ok(wt) = status.get_document("wiredTiger") {
            if let Ok(cache) = wt.get_document("cache") {
                if let Some(v) = cache.get("maximum bytes configured") {
                    out.insert("wiredTiger.cache.maxBytes".into(), bson_to_string(v));
                }
            }
        }
        if let Ok(repl) = status.get_document("repl") {
            for k in ["setName", "ismaster", "secondary"] {
                if let Some(v) = repl.get(k) {
                    out.insert(format!("repl.{}", k), bson_to_string(v));
                }
            }
        }
    }
    Ok(out)
}

/// Per-query stats — explicitly empty for v1.
///
/// MongoDB exposes `serverStatus.opcounters` (server-wide) and
/// `$collStats.latencyStats` (per-collection), but neither matches the
/// per-query shape the existing payload expects. Real per-query telemetry
/// requires either the database profiler (high overhead) or an APM hook —
/// both are v2 territory.
pub async fn collect_query_stats(_db: &Database) -> Result<Vec<QueryStats>, CollectorError> {
    tracing::debug!("MongoDB query_stats not implemented (v1) — emitting empty Vec");
    Ok(Vec::new())
}

fn bson_to_string(v: &Bson) -> String {
    match v {
        Bson::String(s) => s.clone(),
        Bson::Int32(i) => i.to_string(),
        Bson::Int64(i) => i.to_string(),
        Bson::Double(d) => d.to_string(),
        Bson::Boolean(b) => b.to_string(),
        other => format!("{:?}", other),
    }
}
