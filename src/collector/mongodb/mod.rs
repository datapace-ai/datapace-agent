//! MongoDB collector — schema-rich metadata profiling for migration use cases.
//!
//! Unlike the Postgres collector, this one is built around document sampling
//! rather than catalog queries. The MongoDB world is schemaless, so the
//! `SchemaWalker` (in [`schema`]) infers per-field type variance, presence
//! rates, and value distributions from a bounded random sample of each
//! collection.

pub mod bson_type;
pub mod providers;
pub mod schema;
pub mod stats;

use crate::collector::{Collector, CollectorError};
use crate::config::{DatabaseType, Provider};
use crate::payload::{
    DatabaseInfo, IndexMetadata, Payload, SchemaMetadata, TableMetadata,
};
use async_trait::async_trait;
use futures::StreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::ClientOptions;
use mongodb::{Client, Database};
use schema::SchemaWalker;

/// MongoDB metadata collector — connects to a single database extracted from
/// the connection URL, samples each collection, and emits a [`Payload`].
pub struct MongoCollector {
    client: Client,
    database: Database,
    database_url: String,
    detected_provider: String,
    version: Option<String>,
}

impl MongoCollector {
    /// Connect, ping, detect provider, and read the server version.
    pub async fn new(database_url: &str, _provider: Provider) -> Result<Self, CollectorError> {
        let mut opts = ClientOptions::parse(database_url)
            .await
            .map_err(map_mongo_err)?;
        opts.app_name = Some("datapace-agent".to_string());
        let client = Client::with_options(opts.clone()).map_err(map_mongo_err)?;

        // Determine target database: prefer URL-encoded default; else fall
        // back to the first non-system DB on the cluster.
        let db_name = opts
            .default_database
            .clone()
            .unwrap_or_else(|| "admin".to_string());
        let database = client.database(&db_name);

        // Verify connectivity early.
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(map_mongo_err)?;

        let version = Self::fetch_version(&client).await;
        let detected_provider = providers::detect_provider(&client, database_url).await.to_string();

        Ok(Self {
            client,
            database,
            database_url: database_url.to_string(),
            detected_provider,
            version,
        })
    }

    async fn fetch_version(client: &Client) -> Option<String> {
        client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 })
            .await
            .ok()
            .and_then(|d| d.get_str("version").ok().map(|s| s.to_string()))
    }

    async fn list_collection_names(&self) -> Result<Vec<String>, CollectorError> {
        let names = self
            .database
            .list_collection_names()
            .await
            .map_err(map_mongo_err)?;
        Ok(names.into_iter().filter(|n| !n.starts_with("system.")).collect())
    }

    async fn collect_schema(&self) -> Result<SchemaMetadata, CollectorError> {
        let collections = self.list_collection_names().await?;
        let db_name = self.database.name().to_string();

        let mut tables: Vec<TableMetadata> = Vec::with_capacity(collections.len());
        let mut indexes: Vec<IndexMetadata> = Vec::new();

        for coll_name in collections {
            let stats_doc = self
                .database
                .run_command(doc! { "collStats": coll_name.as_str() })
                .await
                .ok();

            let count = stats_doc
                .as_ref()
                .and_then(|s| s.get_i64("count").ok().or_else(|| s.get_i32("count").ok().map(i64::from)));
            let size_bytes = stats_doc
                .as_ref()
                .and_then(|s| s.get_i64("size").ok().or_else(|| s.get_i32("size").ok().map(i64::from)));
            let avg_obj_size = stats_doc
                .as_ref()
                .and_then(|s| s.get_i64("avgObjSize").ok().or_else(|| s.get_i32("avgObjSize").ok().map(i64::from)));
            let storage_size = stats_doc
                .as_ref()
                .and_then(|s| s.get_i64("storageSize").ok().or_else(|| s.get_i32("storageSize").ok().map(i64::from)));
            let is_capped = stats_doc.as_ref().and_then(|s| s.get_bool("capped").ok());

            let sample_size = schema::pick_sample_size(count.unwrap_or(0));
            let coll = self.database.collection::<Document>(&coll_name);

            let mut walker = SchemaWalker::new();
            let pipeline = vec![doc! { "$sample": { "size": sample_size } }];
            match coll.aggregate(pipeline).await {
                Ok(mut cursor) => {
                    while let Some(doc_res) = cursor.next().await {
                        match doc_res {
                            Ok(d) => walker.observe_document(&d),
                            Err(err) => {
                                tracing::warn!(collection=%coll_name, error=%err, "sample doc decode failed");
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(collection=%coll_name, error=%err, "$sample aggregate failed");
                }
            }

            let docs_sampled = walker.docs_sampled();
            let columns = walker.into_columns();

            tables.push(TableMetadata {
                schema: db_name.clone(),
                name: coll_name.clone(),
                columns,
                row_count_estimate: count,
                size_bytes,
                document_count_sampled: Some(docs_sampled),
                avg_document_size_bytes: avg_obj_size,
                storage_size_bytes: storage_size,
                is_capped,
                is_view: None,
                is_timeseries: None,
            });

            // Indexes — pull names + size from stats_doc.indexSizes
            let index_sizes: std::collections::HashMap<String, i64> = stats_doc
                .as_ref()
                .and_then(|s| s.get_document("indexSizes").ok())
                .map(|d| {
                    d.iter()
                        .filter_map(|(k, v)| {
                            v.as_i64()
                                .or_else(|| v.as_i32().map(i64::from))
                                .map(|sz| (k.to_string(), sz))
                        })
                        .collect()
                })
                .unwrap_or_default();

            match coll.list_indexes().await {
                Ok(mut idx_cursor) => {
                    while let Some(idx_res) = idx_cursor.next().await {
                        match idx_res {
                            Ok(model) => {
                                let name = model
                                    .options
                                    .as_ref()
                                    .and_then(|o| o.name.clone())
                                    .unwrap_or_else(|| "_unnamed_".to_string());
                                let columns: Vec<String> = model.keys.keys().cloned().collect();
                                let is_unique =
                                    model.options.as_ref().and_then(|o| o.unique).unwrap_or(false);
                                let is_primary = name == "_id_";
                                let size_bytes = index_sizes.get(&name).copied();
                                indexes.push(IndexMetadata {
                                    schema: db_name.clone(),
                                    table: coll_name.clone(),
                                    name,
                                    columns,
                                    is_unique,
                                    is_primary,
                                    size_bytes,
                                });
                            }
                            Err(err) => {
                                tracing::warn!(collection=%coll_name, error=%err, "list_indexes stream error");
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(collection=%coll_name, error=%err, "list_indexes failed");
                }
            }
        }

        Ok(SchemaMetadata { tables, indexes })
    }
}

#[async_trait]
impl Collector for MongoCollector {
    async fn collect(&self) -> Result<Payload, CollectorError> {
        let collections = self.list_collection_names().await?;

        let (table_stats, index_stats, settings, query_stats, schema) = tokio::try_join!(
            stats::collect_table_stats(&self.database, &collections),
            stats::collect_index_stats(&self.database, &collections),
            stats::collect_settings(&self.database),
            stats::collect_query_stats(&self.database),
            self.collect_schema(),
        )?;

        let database_info = DatabaseInfo {
            database_type: "mongodb".to_string(),
            version: self.version.clone(),
            provider: self.detected_provider.clone(),
            provider_metadata: providers::provider_metadata(
                &self.detected_provider,
                &self.database_url,
            ),
        };

        let payload = Payload::new(database_info)
            .with_instance_id(&self.database_url)
            .with_query_stats(query_stats)
            .with_table_stats(table_stats)
            .with_index_stats(index_stats)
            .with_settings(settings)
            .with_schema(schema);

        Ok(payload)
    }

    async fn test_connection(&self) -> Result<(), CollectorError> {
        self.client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(map_mongo_err)?;
        Ok(())
    }

    fn provider(&self) -> &str {
        &self.detected_provider
    }

    fn version(&self) -> Option<String> {
        self.version.clone()
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::Mongodb
    }
}

/// Map a [`mongodb::error::Error`] to the existing [`CollectorError`] taxonomy.
fn map_mongo_err(err: mongodb::error::Error) -> CollectorError {
    use mongodb::error::ErrorKind;
    match *err.kind {
        ErrorKind::Authentication { .. } => CollectorError::PermissionError(err.to_string()),
        ErrorKind::ServerSelection { .. }
        | ErrorKind::DnsResolve { .. }
        | ErrorKind::Io(_)
        | ErrorKind::ConnectionPoolCleared { .. } => CollectorError::ConnectionError(err.to_string()),
        ErrorKind::InvalidArgument { .. } | ErrorKind::Command(_) => {
            CollectorError::QueryError(err.to_string())
        }
        _ => CollectorError::InternalError(err.to_string()),
    }
}
