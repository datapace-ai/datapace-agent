use super::bson_to_json;
use crate::collector::pool::{require_mongodb, DatabasePool};
use crate::collector::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use bson::doc;
use chrono::Utc;
use mongodb::options::FindOptions;
use tokio_stream::StreamExt;

/// Reads slow queries from the `system.profile` collection.
/// Requires the database profiler to be enabled (level >= 1).
pub struct MongoSlowQueriesCollector;

#[async_trait]
impl Collector for MongoSlowQueriesCollector {
    fn name(&self) -> &'static str {
        "mongo_slow_queries"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast
    }

    fn requires(&self) -> &[&'static str] {
        &["profiler"]
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let mongo = require_mongodb(pool)?;
        let db = mongo.database();
        let collection = db.collection::<bson::Document>("system.profile");

        let five_min_ago = Utc::now() - chrono::Duration::minutes(5);
        let filter = doc! {
            "ts": { "$gte": bson::DateTime::from_chrono(five_min_ago) }
        };
        let opts = FindOptions::builder()
            .sort(doc! { "millis": -1 })
            .limit(100)
            .build();

        let mut cursor = collection.find(filter, opts).await?;

        let mut rows = Vec::new();
        while let Some(result) = cursor.next().await {
            let doc = result?;
            let full = bson_to_json(&doc);
            rows.push(serde_json::json!({
                "op": full.get("op"),
                "ns": full.get("ns"),
                "millis": full.get("millis"),
                "command": full.get("command"),
                "planSummary": full.get("planSummary"),
                "docsExamined": full.get("docsExamined"),
                "keysExamined": full.get("keysExamined"),
                "nreturned": full.get("nreturned"),
                "ts": full.get("ts"),
            }));
        }

        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::Value::Array(rows),
            collected_at: Utc::now(),
        })
    }
}
