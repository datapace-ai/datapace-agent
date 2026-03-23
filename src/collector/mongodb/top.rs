use super::bson_to_json;
use crate::collector::pool::{require_mongodb, DatabasePool};
use crate::collector::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use bson::doc;
use chrono::Utc;

/// Collects per-namespace read/write time and count via the `top` command.
/// Only available on self-hosted MongoDB (not on Atlas shared-tier M0/M2/M5).
pub struct MongoTopCollector;

#[async_trait]
impl Collector for MongoTopCollector {
    fn name(&self) -> &'static str {
        "mongo_top"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["top"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let mongo = require_mongodb(pool)?;
        let admin = mongo.client.database("admin");

        let doc = admin.run_command(doc! { "top": 1 }, None).await?;
        let full = bson_to_json(&doc);

        let totals = full
            .get("totals")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut rows = Vec::new();
        for (ns, stats) in &totals {
            // Skip the "note" field that top sometimes includes
            if ns == "note" {
                continue;
            }
            rows.push(serde_json::json!({
                "ns": ns,
                "total": stats.get("total"),
                "readLock": stats.get("readLock"),
                "writeLock": stats.get("writeLock"),
                "queries": stats.get("queries"),
                "getmore": stats.get("getmore"),
                "insert": stats.get("insert"),
                "update": stats.get("update"),
                "remove": stats.get("remove"),
                "commands": stats.get("commands"),
            }));
        }

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::Value::Array(rows),
            collected_at: Utc::now(),
            idempotency_key: String::new(),
        })
    }
}
