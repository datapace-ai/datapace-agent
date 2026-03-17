use super::bson_to_json;
use crate::collector::pool::{require_mongodb, DatabasePool};
use crate::collector::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use bson::doc;
use chrono::Utc;

/// Collects replica set status: per-member name, state, health, optimeDate, pingMs.
pub struct MongoReplStatusCollector;

#[async_trait]
impl Collector for MongoReplStatusCollector {
    fn name(&self) -> &'static str {
        "mongo_repl_status"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Slow
    }

    fn requires(&self) -> &[&'static str] {
        &["repl_set"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let mongo = require_mongodb(pool)?;
        let admin = mongo.client.database("admin");

        let doc = admin
            .run_command(doc! { "replSetGetStatus": 1 }, None)
            .await?;
        let full = bson_to_json(&doc);

        let members = full
            .get("members")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let rows: Vec<serde_json::Value> = members
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "name": m.get("name"),
                    "stateStr": m.get("stateStr"),
                    "health": m.get("health"),
                    "state": m.get("state"),
                    "optimeDate": m.get("optimeDate"),
                    "pingMs": m.get("pingMs"),
                    "syncSourceHost": m.get("syncSourceHost"),
                    "uptime": m.get("uptime"),
                })
            })
            .collect();

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::Value::Array(rows),
            collected_at: Utc::now(),
        })
    }
}
