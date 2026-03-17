use super::bson_to_json;
use crate::collector::pool::{require_mongodb, DatabasePool};
use crate::collector::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use bson::doc;
use chrono::Utc;

/// Collects MongoDB server status: connections, opcounters, memory,
/// network, wiredTiger cache, and globalLock metrics.
pub struct MongoServerStatusCollector;

#[async_trait]
impl Collector for MongoServerStatusCollector {
    fn name(&self) -> &'static str {
        "mongo_server_status"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["server_status"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let mongo = require_mongodb(pool)?;
        let db = mongo.database();

        let doc = db.run_command(doc! { "serverStatus": 1 }, None).await?;
        let full = bson_to_json(&doc);

        let data = serde_json::json!({
            "connections": full.get("connections"),
            "opcounters": full.get("opcounters"),
            "mem": full.get("mem"),
            "network": full.get("network"),
            "globalLock": full.get("globalLock"),
            "wiredTiger_cache": full.pointer("/wiredTiger/cache"),
            "uptime": full.get("uptime"),
            "version": full.get("version"),
        });

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::Value::Array(vec![data]),
            collected_at: Utc::now(),
        })
    }
}
