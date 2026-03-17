use super::bson_to_json;
use crate::collector::pool::{require_mongodb, DatabasePool};
use crate::collector::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use bson::doc;
use chrono::Utc;

/// Collects currently running operations from MongoDB.
pub struct MongoCurrentOpsCollector;

#[async_trait]
impl Collector for MongoCurrentOpsCollector {
    fn name(&self) -> &'static str {
        "mongo_current_ops"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["current_op"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let mongo = require_mongodb(pool)?;
        let db = mongo.database();

        let doc = db
            .run_command(doc! { "currentOp": 1, "$all": true }, None)
            .await?;

        let full = bson_to_json(&doc);
        let ops = full
            .get("inprog")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let rows: Vec<serde_json::Value> = ops
            .into_iter()
            .map(|op| {
                serde_json::json!({
                    "opid": op.get("opid"),
                    "active": op.get("active"),
                    "op": op.get("op"),
                    "ns": op.get("ns"),
                    "secs_running": op.get("secs_running"),
                    "microsecs_running": op.get("microsecs_running"),
                    "client": op.get("client"),
                    "command": op.get("command"),
                    "desc": op.get("desc"),
                    "waitingForLock": op.get("waitingForLock"),
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
