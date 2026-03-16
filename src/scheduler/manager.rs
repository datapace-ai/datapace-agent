use crate::anonymizer;
use crate::collector::capability::Capabilities;
use crate::collector::pool::{DatabasePool, PostgresPool};
use crate::collector::registry;
use crate::collector::Collector;
use crate::store::{DatabaseEntry, PipelineEvent, QueryFingerprint, ShippingEntry, Store};
use chrono::Utc;
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, warn};

/// Status of a running database pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct DbStatus {
    pub id: String,
    pub name: String,
    pub status: String, // "running", "error", "stopped", "connecting"
    pub last_tick: Option<String>,
    pub error: Option<String>,
    /// Per-collector latest results: [{name, rows, duration_ms, error}]
    pub collector_stats: Vec<CollectorStat>,
    /// Per-shipper latest status
    pub shipper_statuses: Vec<ShipperShippingStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectorStat {
    pub name: String,
    pub rows: i64,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShipperShippingStatus {
    pub shipper_id: String,
    pub name: String,
    pub shipper_type: String,
    pub status: String,
    pub bytes: u64,
    pub error: Option<String>,
    pub at: String,
}

struct RunningDatabase {
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
    status: Arc<RwLock<DbStatus>>,
}

/// Manages multiple database schedulers.
pub struct SchedulerManager {
    store: Arc<Store>,
    databases: Arc<RwLock<HashMap<String, RunningDatabase>>>,
}

impl SchedulerManager {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            databases: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a database scheduler. Connects to PG, probes capabilities, starts collection loop.
    pub async fn add_db(&self, entry: &DatabaseEntry) -> Result<(), String> {
        // Check if already running
        {
            let dbs = self.databases.read().await;
            if dbs.contains_key(&entry.id) {
                return Err("Database already running".into());
            }
        }

        let db_id = entry.id.clone();

        let status = Arc::new(RwLock::new(DbStatus {
            id: entry.id.clone(),
            name: entry.name.clone(),
            status: "connecting".into(),
            last_tick: None,
            error: None,
            collector_stats: vec![],
            shipper_statuses: vec![],
        }));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let store = self.store.clone();
        let entry_clone = entry.clone();
        let status_clone = status.clone();

        let handle = tokio::spawn(async move {
            run_db_scheduler(store, entry_clone, status_clone, shutdown_rx).await;
        });

        // Update status in SQLite
        self.store
            .update_database_status(&db_id, "running")
            .await
            .ok();

        let mut dbs = self.databases.write().await;
        dbs.insert(
            db_id,
            RunningDatabase {
                shutdown_tx,
                handle,
                status,
            },
        );

        Ok(())
    }

    /// Stop and remove a database scheduler.
    pub async fn remove_db(&self, id: &str) -> Result<(), String> {
        let running = {
            let mut dbs = self.databases.write().await;
            dbs.remove(id)
        };

        if let Some(running) = running {
            let _ = running.shutdown_tx.send(true);
            // Give it a moment to shut down gracefully
            tokio::time::timeout(Duration::from_secs(5), running.handle)
                .await
                .ok();
        }

        self.store.update_database_status(id, "stopped").await.ok();
        Ok(())
    }

    /// Get the status of all running databases.
    pub async fn list_status(&self) -> Vec<DbStatus> {
        let dbs = self.databases.read().await;
        let mut statuses = Vec::with_capacity(dbs.len());
        for running in dbs.values() {
            let status = running.status.read().await;
            statuses.push(status.clone());
        }
        statuses
    }

    /// Get the status of a specific database.
    pub async fn get_status(&self, id: &str) -> Option<DbStatus> {
        let dbs = self.databases.read().await;
        if let Some(running) = dbs.get(id) {
            let status = running.status.read().await;
            Some(status.clone())
        } else {
            None
        }
    }

    /// Check if a database is currently running.
    pub async fn is_running(&self, id: &str) -> bool {
        let dbs = self.databases.read().await;
        dbs.contains_key(id)
    }

    /// Stop all database schedulers.
    pub async fn shutdown_all(&self) {
        let mut dbs = self.databases.write().await;
        for (id, running) in dbs.drain() {
            info!(db = %id, "Stopping database scheduler");
            let _ = running.shutdown_tx.send(true);
            tokio::time::timeout(Duration::from_secs(5), running.handle)
                .await
                .ok();
        }
    }
}

/// The main loop for a single database.
async fn run_db_scheduler(
    store: Arc<Store>,
    entry: DatabaseEntry,
    status: Arc<RwLock<DbStatus>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    // Connect to PostgreSQL
    let pg_pool = match PgPoolOptions::new()
        .min_connections(1)
        .max_connections(entry.pool_size)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&entry.url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            let err_msg = format!("Connection failed: {e}");
            error!(db = %entry.id, error = %e, "Failed to connect to PostgreSQL");
            let mut s = status.write().await;
            s.status = "error".into();
            s.error = Some(err_msg);
            store.update_database_status(&entry.id, "error").await.ok();
            return;
        }
    };

    // Probe capabilities
    let capabilities = match Capabilities::probe(&pg_pool).await {
        Ok(caps) => caps,
        Err(e) => {
            let err_msg = format!("Capability probe failed: {e}");
            error!(db = %entry.id, error = %e, "Failed to probe capabilities");
            let mut s = status.write().await;
            s.status = "error".into();
            s.error = Some(err_msg);
            store.update_database_status(&entry.id, "error").await.ok();
            return;
        }
    };

    // Wrap the raw PgPool in the DatabasePool abstraction
    let pool: Arc<dyn DatabasePool> = Arc::new(PostgresPool(pg_pool));

    info!(db = %entry.id, "Database scheduler started");
    {
        let mut s = status.write().await;
        s.status = "running".into();
        s.error = None;
    }
    store
        .update_database_status(&entry.id, "running")
        .await
        .ok();

    let fast_dur = Duration::from_secs(entry.fast_interval);
    let slow_dur = Duration::from_secs(entry.slow_interval);

    let mut fast = interval(fast_dur);
    fast.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut slow = interval(slow_dur);
    slow.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Collect immediately on start
    run_tick(&store, &entry, &pool, &capabilities, &status, "fast").await;
    run_tick(&store, &entry, &pool, &capabilities, &status, "slow").await;

    loop {
        tokio::select! {
            _ = fast.tick() => {
                run_tick(&store, &entry, &pool, &capabilities, &status, "fast").await;
            }
            _ = slow.tick() => {
                run_tick(&store, &entry, &pool, &capabilities, &status, "slow").await;
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!(db = %entry.id, "Database scheduler shutting down");
                    pool.close().await;
                    let mut s = status.write().await;
                    s.status = "stopped".into();
                    return;
                }
            }
        }
    }
}

/// Run a single tick (fast or slow) for a database.
async fn run_tick(
    store: &Store,
    entry: &DatabaseEntry,
    pool: &Arc<dyn DatabasePool>,
    capabilities: &Capabilities,
    status: &Arc<RwLock<DbStatus>>,
    tick_type: &str,
) {
    debug!(db = %entry.id, tick = tick_type, "Tick");

    let collectors = build_collectors(entry, tick_type);
    let mut stats: Vec<CollectorStat> = Vec::new();

    for collector in &collectors {
        // Check capabilities
        let missing: Vec<_> = collector
            .requires()
            .iter()
            .filter(|req| !capabilities.has(req))
            .collect();

        if !missing.is_empty() {
            debug!(
                db = %entry.id,
                collector = collector.name(),
                missing = ?missing,
                "Skipping collector — missing capabilities"
            );
            continue;
        }

        let start = Instant::now();
        match collector.collect(pool.as_ref()).await {
            Ok(mut snapshot) => {
                let duration = start.elapsed();
                let row_count = match &snapshot.data {
                    serde_json::Value::Array(arr) => arr.len() as i64,
                    _ => 1,
                };

                // Anonymize query text (if enabled for this database)
                if entry.anonymize {
                    anonymize_snapshot_queries(&mut snapshot);
                }

                // Store fingerprints
                if let Err(e) = store_fingerprints(store, &snapshot).await {
                    warn!(db = %entry.id, collector = collector.name(), error = %e, "Failed to store fingerprints");
                }

                // Store snapshot
                if let Err(e) = store.insert_snapshot_for(&entry.id, &snapshot).await {
                    error!(db = %entry.id, collector = collector.name(), error = %e, "Failed to write snapshot");
                }

                stats.push(CollectorStat {
                    name: collector.name().into(),
                    rows: row_count,
                    duration_ms: duration.as_millis() as u64,
                    error: None,
                });
            }
            Err(e) => {
                let duration = start.elapsed();
                error!(db = %entry.id, collector = collector.name(), error = %e, "Collector failed");
                stats.push(CollectorStat {
                    name: collector.name().into(),
                    rows: 0,
                    duration_ms: duration.as_millis() as u64,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    // Record pipeline event
    let pipeline_event = PipelineEvent {
        source_id: entry.id.clone(),
        tick_type: tick_type.into(),
        collectors_json: serde_json::to_value(&stats).unwrap_or_default(),
        created_at: Utc::now(),
    };
    store.insert_pipeline_event(&pipeline_event).await.ok();

    // Update live status
    {
        let mut s = status.write().await;
        s.last_tick = Some(Utc::now().to_rfc3339());
        // Merge stats with existing (update fast collectors, keep slow ones and vice versa)
        for stat in &stats {
            if let Some(existing) = s.collector_stats.iter_mut().find(|c| c.name == stat.name) {
                *existing = stat.clone();
            } else {
                s.collector_stats.push(stat.clone());
            }
        }
    }

    // Ship data to all enabled shippers
    for shipper in &entry.shippers {
        if shipper.enabled {
            ship_data(store, entry, shipper, status).await;
        }
    }
}

/// Build the list of collectors for a tick based on the database's configuration.
fn build_collectors(entry: &DatabaseEntry, tick_type: &str) -> Vec<Box<dyn Collector>> {
    registry::build_collectors_for_tick(tick_type, &entry.collectors)
}

/// Ship collected data to a specific shipper destination.
async fn ship_data(
    store: &Store,
    entry: &DatabaseEntry,
    shipper: &crate::store::ShipperEntry,
    status: &Arc<RwLock<DbStatus>>,
) {
    let snapshots = match store.get_latest_snapshots_for(&entry.id).await {
        Ok(s) => s,
        Err(e) => {
            warn!(db = %entry.id, shipper = %shipper.id, error = %e, "Failed to get snapshots for shipping");
            return;
        }
    };

    if snapshots.is_empty() {
        return;
    }

    let payload = serde_json::json!({
        "source_id": entry.id,
        "source_name": entry.name,
        "shipper_id": shipper.id,
        "snapshots": snapshots,
        "shipped_at": Utc::now().to_rfc3339(),
    });

    let body = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => {
            let ship_entry = ShippingEntry {
                source_id: entry.id.clone(),
                shipper_id: shipper.id.clone(),
                status: "error".into(),
                bytes: 0,
                error: Some(format!("Serialization failed: {e}")),
                created_at: Utc::now(),
            };
            store.insert_shipping_entry(&ship_entry).await.ok();
            update_shipper_status(status, shipper, &ship_entry).await;
            return;
        }
    };

    let bytes = body.len() as u64;
    let client = reqwest::Client::new();
    let mut req = client
        .post(&shipper.endpoint)
        .header("Content-Type", "application/json");

    if let Some(ref token) = shipper.token {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }

    let ship_entry = match req.body(body).send().await {
        Ok(resp) => {
            let status_code = resp.status();
            if status_code.is_success() {
                ShippingEntry {
                    source_id: entry.id.clone(),
                    shipper_id: shipper.id.clone(),
                    status: "ok".into(),
                    bytes,
                    error: None,
                    created_at: Utc::now(),
                }
            } else {
                let err_text = resp.text().await.unwrap_or_default();
                ShippingEntry {
                    source_id: entry.id.clone(),
                    shipper_id: shipper.id.clone(),
                    status: "error".into(),
                    bytes,
                    error: Some(format!("HTTP {status_code}: {err_text}")),
                    created_at: Utc::now(),
                }
            }
        }
        Err(e) => ShippingEntry {
            source_id: entry.id.clone(),
            shipper_id: shipper.id.clone(),
            status: "error".into(),
            bytes: 0,
            error: Some(format!("Request failed: {e}")),
            created_at: Utc::now(),
        },
    };

    store.insert_shipping_entry(&ship_entry).await.ok();
    update_shipper_status(status, shipper, &ship_entry).await;
}

/// Update the live shipper status in DbStatus.
async fn update_shipper_status(
    status: &Arc<RwLock<DbStatus>>,
    shipper: &crate::store::ShipperEntry,
    ship_entry: &ShippingEntry,
) {
    let mut s = status.write().await;
    let ss = ShipperShippingStatus {
        shipper_id: shipper.id.clone(),
        name: shipper.name.clone(),
        shipper_type: shipper.shipper_type.clone(),
        status: ship_entry.status.clone(),
        bytes: ship_entry.bytes,
        error: ship_entry.error.clone(),
        at: ship_entry.created_at.to_rfc3339(),
    };
    if let Some(existing) = s
        .shipper_statuses
        .iter_mut()
        .find(|x| x.shipper_id == shipper.id)
    {
        *existing = ss;
    } else {
        s.shipper_statuses.push(ss);
    }
}

/// Sanitize query text fields within snapshot JSON data.
fn anonymize_snapshot_queries(snapshot: &mut crate::store::Snapshot) {
    if let serde_json::Value::Array(ref mut items) = snapshot.data {
        for item in items.iter_mut() {
            if let Some(q) = item.get("query").and_then(|v| v.as_str()).map(String::from) {
                let sanitized = anonymizer::sanitize_query(&q);
                item["query"] = serde_json::Value::String(sanitized);
            }
        }
    }
}

/// Extract query fingerprints from statements/activity snapshots and upsert them.
async fn store_fingerprints(
    store: &Store,
    snapshot: &crate::store::Snapshot,
) -> Result<(), sqlx::Error> {
    if snapshot.collector != "statements" && snapshot.collector != "activity" {
        return Ok(());
    }

    if let serde_json::Value::Array(ref items) = snapshot.data {
        for item in items {
            if let Some(query) = item.get("query").and_then(|v| v.as_str()) {
                if query.is_empty() {
                    continue;
                }
                let fp = anonymizer::fingerprint_query(query);
                let now = Utc::now();
                store
                    .upsert_fingerprint(&QueryFingerprint {
                        fingerprint: fp,
                        sanitized_query: query.to_string(),
                        first_seen: now,
                        last_seen: now,
                    })
                    .await?;
            }
        }
    }
    Ok(())
}
