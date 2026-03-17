pub mod collections;
pub mod current_ops;
pub mod repl_status;
pub mod server_status;
pub mod slow_queries;
pub mod top;

pub use collections::MongoCollectionsCollector;
pub use current_ops::MongoCurrentOpsCollector;
pub use repl_status::MongoReplStatusCollector;
pub use server_status::MongoServerStatusCollector;
pub use slow_queries::MongoSlowQueriesCollector;
pub use top::MongoTopCollector;

/// Convert a BSON document to a serde_json::Value, handling nested documents.
fn bson_to_json(doc: &bson::Document) -> serde_json::Value {
    // bson::Document implements Into<bson::Bson>, which can convert to serde_json::Value
    let bson_val = bson::Bson::Document(doc.clone());
    bson_val.into_relaxed_extjson()
}
