use super::bson_to_json;
use crate::collector::pool::{require_mongodb, DatabasePool};
use crate::collector::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use bson::doc;
use chrono::Utc;

/// Collects per-collection statistics: count, size, storageSize, indexSize, etc.
pub struct MongoCollectionsCollector;

#[async_trait]
impl Collector for MongoCollectionsCollector {
    fn name(&self) -> &'static str {
        "mongo_collections"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Slow
    }

    fn requires(&self) -> &[&'static str] {
        &["list_collections"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let mongo = require_mongodb(pool)?;
        let db = mongo.database();

        // List all collection names
        let names = db.list_collection_names(None).await?;

        let mut rows = Vec::new();
        for name in &names {
            // Skip system collections
            if name.starts_with("system.") {
                continue;
            }

            match db.run_command(doc! { "collStats": name }, None).await {
                Ok(stats_doc) => {
                    let stats = bson_to_json(&stats_doc);
                    rows.push(serde_json::json!({
                        "ns": stats.get("ns"),
                        "collection": name,
                        "count": stats.get("count"),
                        "size": stats.get("size"),
                        "storageSize": stats.get("storageSize"),
                        "totalIndexSize": stats.get("totalIndexSize"),
                        "nindexes": stats.get("nindexes"),
                        "avgObjSize": stats.get("avgObjSize"),
                        "capped": stats.get("capped"),
                    }));
                }
                Err(e) => {
                    tracing::debug!(collection = %name, error = %e, "Skipping collection stats");
                }
            }
        }

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::Value::Array(rows),
            collected_at: Utc::now(),
        })
    }
}
