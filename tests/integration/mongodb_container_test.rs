//! MongoDB integration tests using testcontainers.
//!
//! These tests spin up a MongoDB container programmatically and verify the
//! collector against a real database, without requiring an external
//! `DATABASE_URL` environment variable.
//!
//! # Requirements
//!
//! - Docker installed and running
//!
//! # Running
//!
//! ```bash
//! cargo test --test integration mongodb_container
//! ```

use datapace_agent::collector;
use mongodb::bson::{doc, Document};
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, IndexModel};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::mongo::Mongo;

const TEST_DB: &str = "test_migration_profile";
const USERS: &str = "users";

/// Start a MongoDB container and return (container_handle, connection_url).
///
/// If Docker is not available, returns None so callers can skip the test.
async fn start_mongo() -> Option<(testcontainers::ContainerAsync<Mongo>, String)> {
    let container = match Mongo::default().start().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping test (Docker not available): {e}");
            return None;
        }
    };

    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(27017).await.unwrap();
    let url = format!("mongodb://{host}:{host_port}/{TEST_DB}");

    Some((container, url))
}

/// Insert a canonical fixture covering nesting, arrays, polymorphism, and
/// optional fields. Also creates a unique index on `email`.
async fn seed_fixture(url: &str) {
    let opts = ClientOptions::parse(url).await.unwrap();
    let client = Client::with_options(opts).unwrap();
    let db = client.database(TEST_DB);
    let coll = db.collection::<Document>(USERS);

    let docs = vec![
        doc! { "name": "Alice", "age": 30_i32, "email": "a@x", "address": { "street": "Main", "zip": 10001_i32 }, "tags": ["admin", "beta"] },
        doc! { "name": "Bob",   "age": 25_i32, "email": "b@x", "address": { "street": "Oak",  "zip": 20002_i32 }, "tags": ["user"] },
        doc! { "name": "Carol", "age": "unknown", "email": "c@x" }, // polymorphic age, no address
        doc! { "name": "Dave",  "age": 40_i32, "email": null, "address": { "street": "Elm" } }, // null email, no zip
        doc! { "name": "Eve",   "age": 35_i32, "email": "e@x", "photos": [{ "url": "u1", "w": 100_i32 }, { "url": "u2" }] },
    ];
    coll.insert_many(docs).await.unwrap();

    let idx = IndexModel::builder()
        .keys(doc! { "email": 1 })
        .options(IndexOptions::builder().unique(true).build())
        .build();
    coll.create_index(idx).await.unwrap();
}

#[tokio::test]
async fn test_collector_with_container() {
    let Some((_container, url)) = start_mongo().await else {
        return;
    };
    seed_fixture(&url).await;

    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");

    let payload = collector.collect().await.expect("Collection failed");

    assert!(!payload.agent_version.is_empty(), "agent_version must be set");
    assert_eq!(payload.database.database_type, "mongodb");
    assert!(payload.schema.is_some(), "schema metadata expected");
    assert!(payload.table_stats.is_some());
    assert!(payload.index_stats.is_some());
    assert!(payload.settings.is_some());
}

#[tokio::test]
async fn test_collector_connection_test() {
    let Some((_container, url)) = start_mongo().await else {
        return;
    };
    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");
    assert!(collector.test_connection().await.is_ok());
}

#[tokio::test]
async fn test_collector_provider_generic() {
    let Some((_container, url)) = start_mongo().await else {
        return;
    };
    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");
    assert_eq!(
        collector.provider(),
        "generic",
        "Vanilla Mongo container should detect as generic"
    );
}

#[tokio::test]
async fn test_collector_version_detection() {
    let Some((_container, url)) = start_mongo().await else {
        return;
    };
    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");
    let version = collector.version().expect("version should be detected");
    let leading = version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    assert!(leading >= 4, "expected MongoDB v4+; got {version}");
}

#[tokio::test]
async fn test_schema_inference_nested_paths() {
    let Some((_container, url)) = start_mongo().await else {
        return;
    };
    seed_fixture(&url).await;
    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");
    let payload = collector.collect().await.expect("collect");
    let schema = payload.schema.expect("schema present");

    let users = schema
        .tables
        .iter()
        .find(|t| t.name == USERS)
        .expect("users table");
    assert_eq!(users.schema, TEST_DB);
    assert_eq!(users.row_count_estimate, Some(5));

    let path = |p: &str| {
        users
            .columns
            .iter()
            .find(|c| c.name == p)
            .unwrap_or_else(|| panic!("path missing: {p}"))
    };

    // address present in 3/5 docs (Carol & Eve skip it)
    let addr = path("address");
    assert!(
        (addr.presence_rate.unwrap() - 0.6).abs() < 1e-6,
        "address.presence_rate = {:?}",
        addr.presence_rate
    );

    // address.zip present in 2/5 (Alice & Bob only — Dave's address has no zip)
    let zip = path("address.zip");
    assert!(
        (zip.presence_rate.unwrap() - 0.4).abs() < 1e-6,
        "address.zip.presence_rate = {:?}",
        zip.presence_rate
    );

    // age is polymorphic — int + string
    let age = path("age");
    assert_eq!(age.data_type, "mixed");
    let types = age.bson_types.as_ref().unwrap();
    assert!(types.iter().any(|t| t == "string"));
    assert!(types.iter().any(|t| t == "int32"));

    // photos[] / photos[].url path emitted with array_element flag
    let url_path = path("photos[].url");
    assert_eq!(url_path.is_array_element, Some(true));
    assert_eq!(url_path.data_type, "string");
}

#[tokio::test]
async fn test_unique_index_emitted() {
    let Some((_container, url)) = start_mongo().await else {
        return;
    };
    seed_fixture(&url).await;
    let collector = collector::create_collector(&url, datapace_agent::config::Provider::Auto)
        .await
        .expect("Failed to create collector");
    let payload = collector.collect().await.expect("collect");
    let schema = payload.schema.expect("schema present");

    let email_idx = schema
        .indexes
        .iter()
        .find(|i| i.table == USERS && i.columns.first().map(|s| s.as_str()) == Some("email"))
        .expect("unique email index");
    assert!(email_idx.is_unique, "email index should be unique");
    assert!(!email_idx.is_primary, "email index is not primary");
}
