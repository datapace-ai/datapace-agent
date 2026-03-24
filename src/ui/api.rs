use super::AppState;
use crate::store::{DatabaseEntry, ShipperEntry};
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

// ═══ Health ═══

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
    databases: usize,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let dbs = state.store.list_databases().await.unwrap_or_default();
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        databases: dbs.len(),
    })
}

// ═══ Database CRUD ═══

#[derive(Serialize)]
pub struct DatabaseResponse {
    #[serde(flatten)]
    entry: DatabaseEntry,
    /// Masked URL (hide password)
    masked_url: String,
    /// Live runtime status from SchedulerManager
    runtime_status: Option<crate::scheduler::manager::DbStatus>,
}

fn mask_url(url: &str) -> String {
    // Replace password in postgres://user:password@host/db
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            // Check it's after the scheme's ://
            if colon_pos > 10 {
                let mut masked = String::new();
                masked.push_str(&url[..colon_pos + 1]);
                masked.push_str("***");
                masked.push_str(&url[at_pos..]);
                return masked;
            }
        }
    }
    url.to_string()
}

pub async fn list_databases(State(state): State<AppState>) -> Json<Vec<DatabaseResponse>> {
    let entries = state.store.list_databases().await.unwrap_or_default();
    let statuses = state.scheduler.list_status().await;

    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let runtime_status = statuses.iter().find(|s| s.id == entry.id).cloned();
        let masked_url = mask_url(&entry.url);
        result.push(DatabaseResponse {
            entry,
            masked_url,
            runtime_status,
        });
    }

    Json(result)
}

pub async fn get_database(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let entry = state.store.get_database(&id).await.unwrap_or(None);
    match entry {
        Some(entry) => {
            let runtime_status = state.scheduler.get_status(&id).await;
            let masked_url = mask_url(&entry.url);
            Json(serde_json::json!({
                "ok": true,
                "database": DatabaseResponse { entry, masked_url, runtime_status },
            }))
        }
        None => Json(serde_json::json!({"ok": false, "error": "Not found"})),
    }
}

#[derive(Deserialize)]
pub struct AddDatabaseRequest {
    name: String,
    url: String,
    #[serde(default = "default_db_type")]
    db_type: String,
    #[serde(default = "default_environment")]
    environment: String,
    #[serde(default = "default_pool_size")]
    pool_size: u32,
    #[serde(default = "default_fast")]
    fast_interval: u64,
    #[serde(default = "default_slow")]
    slow_interval: u64,
    #[serde(default = "default_collectors")]
    collectors: Vec<String>,
    #[serde(default)]
    anonymize: Option<bool>,
    #[serde(default)]
    shippers: Vec<ShipperEntry>,
}

fn default_db_type() -> String {
    "postgres".into()
}
fn default_environment() -> String {
    "production".into()
}
fn default_pool_size() -> u32 {
    3
}
fn default_fast() -> u64 {
    30
}
fn default_slow() -> u64 {
    300
}
fn default_collectors() -> Vec<String> {
    // Default will be overridden in add_database based on db_type
    vec![]
}

/// Default anonymization based on environment: enabled for production/staging, disabled otherwise.
fn default_anonymize_for_env(env: &str) -> bool {
    matches!(env, "production" | "staging")
}

pub async fn add_database(
    State(state): State<AppState>,
    Json(req): Json<AddDatabaseRequest>,
) -> Json<serde_json::Value> {
    // Validate
    if req.name.is_empty() {
        return Json(serde_json::json!({"ok": false, "error": "Name is required"}));
    }

    // URL validation based on db_type
    match req.db_type.as_str() {
        "mongodb" => {
            if !req.url.starts_with("mongodb://") && !req.url.starts_with("mongodb+srv://") {
                return Json(
                    serde_json::json!({"ok": false, "error": "URL must start with mongodb:// or mongodb+srv://"}),
                );
            }
        }
        "postgres" | _ => {
            if !req.url.starts_with("postgres://") && !req.url.starts_with("postgresql://") {
                return Json(
                    serde_json::json!({"ok": false, "error": "URL must start with postgres:// or postgresql://"}),
                );
            }
        }
    }

    if req.fast_interval < 5 {
        return Json(serde_json::json!({"ok": false, "error": "Fast interval must be >= 5"}));
    }
    if req.slow_interval < req.fast_interval {
        return Json(
            serde_json::json!({"ok": false, "error": "Slow interval must be >= fast interval"}),
        );
    }

    let id = generate_id(&req.name);

    let anonymize = req
        .anonymize
        .unwrap_or_else(|| default_anonymize_for_env(&req.environment));

    // Default collectors per db_type if none specified
    let collectors = if req.collectors.is_empty() {
        crate::collector::registry::all_collector_names_for(&req.db_type)
            .into_iter()
            .map(String::from)
            .collect()
    } else {
        req.collectors
    };

    let entry = DatabaseEntry {
        id: id.clone(),
        name: req.name,
        url: req.url,
        db_type: req.db_type,
        environment: req.environment,
        pool_size: req.pool_size,
        fast_interval: req.fast_interval,
        slow_interval: req.slow_interval,
        collectors,
        anonymize,
        shippers: req.shippers,
        status: "stopped".into(),
        created_at: Utc::now(),
    };

    if let Err(e) = state.store.insert_database(&entry).await {
        return Json(serde_json::json!({"ok": false, "error": format!("Store error: {e}")}));
    }

    // Start the scheduler for this DB
    if let Err(e) = state.scheduler.add_db(&entry).await {
        return Json(
            serde_json::json!({"ok": true, "id": id, "warning": format!("Saved but failed to start: {e}")}),
        );
    }

    Json(serde_json::json!({"ok": true, "id": id}))
}

#[derive(Deserialize)]
pub struct UpdateDatabaseRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    db_type: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    pool_size: Option<u32>,
    #[serde(default)]
    fast_interval: Option<u64>,
    #[serde(default)]
    slow_interval: Option<u64>,
    #[serde(default)]
    collectors: Option<Vec<String>>,
    #[serde(default)]
    anonymize: Option<bool>,
    #[serde(default)]
    shippers: Option<Vec<ShipperEntry>>,
}

pub async fn update_database(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDatabaseRequest>,
) -> Json<serde_json::Value> {
    let existing = match state.store.get_database(&id).await {
        Ok(Some(db)) => db,
        Ok(None) => return Json(serde_json::json!({"ok": false, "error": "Not found"})),
        Err(e) => {
            return Json(serde_json::json!({"ok": false, "error": format!("Store error: {e}")}))
        }
    };

    let updated = DatabaseEntry {
        id: existing.id.clone(),
        name: req.name.unwrap_or(existing.name),
        url: req.url.unwrap_or(existing.url),
        db_type: req.db_type.unwrap_or(existing.db_type),
        environment: req.environment.unwrap_or(existing.environment),
        pool_size: req.pool_size.unwrap_or(existing.pool_size),
        fast_interval: req.fast_interval.unwrap_or(existing.fast_interval),
        slow_interval: req.slow_interval.unwrap_or(existing.slow_interval),
        collectors: req.collectors.unwrap_or(existing.collectors),
        anonymize: req.anonymize.unwrap_or(existing.anonymize),
        shippers: req.shippers.unwrap_or(existing.shippers),
        status: existing.status,
        created_at: existing.created_at,
    };

    if let Err(e) = state.store.update_database(&updated).await {
        return Json(serde_json::json!({"ok": false, "error": format!("Store error: {e}")}));
    }

    // Restart scheduler with new config
    state.scheduler.remove_db(&id).await.ok();
    if let Err(e) = state.scheduler.add_db(&updated).await {
        return Json(
            serde_json::json!({"ok": true, "warning": format!("Updated but restart failed: {e}")}),
        );
    }

    Json(serde_json::json!({"ok": true}))
}

pub async fn delete_database(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    // Stop scheduler first
    state.scheduler.remove_db(&id).await.ok();

    if let Err(e) = state.store.delete_database(&id).await {
        return Json(serde_json::json!({"ok": false, "error": format!("Store error: {e}")}));
    }

    Json(serde_json::json!({"ok": true}))
}

// ═══ Test Connection ═══

#[derive(Deserialize)]
pub struct TestConnectionRequest {
    url: String,
    #[serde(default = "default_db_type")]
    db_type: String,
}

pub async fn test_database(Json(req): Json<TestConnectionRequest>) -> Json<serde_json::Value> {
    match req.db_type.as_str() {
        "mongodb" => test_mongodb_connection(&req.url).await,
        _ => test_postgres_connection(&req.url).await,
    }
}

async fn test_postgres_connection(url: &str) -> Json<serde_json::Value> {
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Json(serde_json::json!({
            "ok": false,
            "error": "URL must start with postgres:// or postgresql://"
        }));
    }

    let pool = match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect(url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("Connection failed: {e}")
            }));
        }
    };

    let version: Result<(String,), _> = sqlx::query_as("SELECT version()").fetch_one(&pool).await;

    pool.close().await;

    match version {
        Ok((ver,)) => Json(serde_json::json!({
            "ok": true,
            "version": ver
        })),
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": format!("Query failed: {e}")
        })),
    }
}

async fn test_mongodb_connection(url: &str) -> Json<serde_json::Value> {
    if !url.starts_with("mongodb://") && !url.starts_with("mongodb+srv://") {
        return Json(serde_json::json!({
            "ok": false,
            "error": "URL must start with mongodb:// or mongodb+srv://"
        }));
    }

    let client_options = match mongodb::options::ClientOptions::parse(url).await {
        Ok(opts) => opts,
        Err(e) => {
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("Invalid connection string: {e}")
            }));
        }
    };

    let client = match mongodb::Client::with_options(client_options) {
        Ok(c) => c,
        Err(e) => {
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("Client creation failed: {e}")
            }));
        }
    };

    // Extract db name from URL, default to "admin"
    let db_name = url
        .split("://")
        .nth(1)
        .and_then(|s| s.split('/').nth(1))
        .and_then(|s| {
            let name = s.split('?').next().unwrap_or(s);
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .unwrap_or("admin");

    let db = client.database(db_name);
    match db.run_command(bson::doc! { "serverStatus": 1 }, None).await {
        Ok(doc) => {
            let version = doc.get_str("version").unwrap_or("unknown").to_string();
            Json(serde_json::json!({
                "ok": true,
                "version": format!("MongoDB {version}")
            }))
        }
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": format!("Connection failed: {e}")
        })),
    }
}

// ═══ Per-DB Data ═══

pub async fn get_collector_data(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    match state.store.get_latest_snapshot_for(&id, &name).await {
        Ok(Some(snap)) => Json(serde_json::json!({
            "ok": true,
            "snapshot": snap,
        })),
        Ok(None) => Json(serde_json::json!({"ok": true, "snapshot": null})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    }
}

pub async fn get_pipeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.store.get_pipeline_events(&id, 50).await {
        Ok(events) => Json(serde_json::json!({
            "ok": true,
            "events": events,
        })),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    }
}

pub async fn get_shipping(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.store.get_shipping_entries(&id, 50).await {
        Ok(entries) => Json(serde_json::json!({
            "ok": true,
            "entries": entries,
        })),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    }
}

// ═══ Shipper CRUD ═══

#[derive(Deserialize)]
pub struct AddShipperRequest {
    name: String,
    shipper_type: String,
    endpoint: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

pub async fn add_shipper(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AddShipperRequest>,
) -> Json<serde_json::Value> {
    let mut db = match state.store.get_database(&id).await {
        Ok(Some(db)) => db,
        Ok(None) => return Json(serde_json::json!({"ok": false, "error": "Not found"})),
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    };

    let shipper_id = generate_shipper_id(&req.name);
    let shipper = ShipperEntry {
        id: shipper_id.clone(),
        name: req.name,
        shipper_type: req.shipper_type,
        endpoint: req.endpoint,
        token: req.token,
        enabled: req.enabled,
    };

    db.shippers.push(shipper);

    if let Err(e) = state.store.update_database(&db).await {
        return Json(serde_json::json!({"ok": false, "error": format!("{e}")}));
    }

    // Restart scheduler
    state.scheduler.remove_db(&id).await.ok();
    state.scheduler.add_db(&db).await.ok();

    Json(serde_json::json!({"ok": true, "shipper_id": shipper_id}))
}

#[derive(Deserialize)]
pub struct UpdateShipperRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    shipper_type: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

pub async fn update_shipper(
    State(state): State<AppState>,
    Path((id, shipper_id)): Path<(String, String)>,
    Json(req): Json<UpdateShipperRequest>,
) -> Json<serde_json::Value> {
    let mut db = match state.store.get_database(&id).await {
        Ok(Some(db)) => db,
        Ok(None) => return Json(serde_json::json!({"ok": false, "error": "Not found"})),
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    };

    let shipper = match db.shippers.iter_mut().find(|s| s.id == shipper_id) {
        Some(s) => s,
        None => return Json(serde_json::json!({"ok": false, "error": "Shipper not found"})),
    };

    if let Some(name) = req.name {
        shipper.name = name;
    }
    if let Some(st) = req.shipper_type {
        shipper.shipper_type = st;
    }
    if let Some(ep) = req.endpoint {
        shipper.endpoint = ep;
    }
    if let Some(tok) = req.token {
        shipper.token = Some(tok);
    }
    if let Some(en) = req.enabled {
        shipper.enabled = en;
    }

    if let Err(e) = state.store.update_database(&db).await {
        return Json(serde_json::json!({"ok": false, "error": format!("{e}")}));
    }

    state.scheduler.remove_db(&id).await.ok();
    state.scheduler.add_db(&db).await.ok();

    Json(serde_json::json!({"ok": true}))
}

pub async fn delete_shipper(
    State(state): State<AppState>,
    Path((id, shipper_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    let mut db = match state.store.get_database(&id).await {
        Ok(Some(db)) => db,
        Ok(None) => return Json(serde_json::json!({"ok": false, "error": "Not found"})),
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    };

    let before = db.shippers.len();
    db.shippers.retain(|s| s.id != shipper_id);
    if db.shippers.len() == before {
        return Json(serde_json::json!({"ok": false, "error": "Shipper not found"}));
    }

    if let Err(e) = state.store.update_database(&db).await {
        return Json(serde_json::json!({"ok": false, "error": format!("{e}")}));
    }

    state.scheduler.remove_db(&id).await.ok();
    state.scheduler.add_db(&db).await.ok();

    Json(serde_json::json!({"ok": true}))
}

fn generate_shipper_id(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    let ts = Utc::now().timestamp_millis() % 10000;
    if slug.is_empty() {
        format!("ship-{ts}")
    } else {
        format!("{slug}-{ts}")
    }
}

// ═══ Helpers ═══

fn generate_id(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    let ts = Utc::now().timestamp_millis() % 10000;
    if slug.is_empty() {
        format!("db-{ts}")
    } else {
        format!("{slug}-{ts}")
    }
}
