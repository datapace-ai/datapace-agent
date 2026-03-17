pub mod manager;

pub use manager::SchedulerManager;

use crate::anonymizer;
use crate::collector::capability::{Capabilities, CapabilitySet};
use crate::collector::pool::DatabasePool;
use crate::collector::registry;
use crate::collector::Collector;
use crate::store::{QueryFingerprint, Store};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, warn};

/// Dual-interval scheduler: fast loop (30s) + slow loop (300s).
/// Legacy single-DB scheduler — used when running from TOML config.
pub struct Scheduler {
    pool: Arc<dyn DatabasePool>,
    store: Arc<Store>,
    capabilities: CapabilitySet,
    fast_interval: Duration,
    slow_interval: Duration,
    shutdown_rx: watch::Receiver<bool>,
    /// Called after each tick so shippers can pick up new data.
    on_tick: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Scheduler {
    pub fn new(
        pool: Arc<dyn DatabasePool>,
        store: Arc<Store>,
        capabilities: Capabilities,
        fast_interval: Duration,
        slow_interval: Duration,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            pool,
            store,
            capabilities: CapabilitySet::Postgres(capabilities),
            fast_interval,
            slow_interval,
            shutdown_rx,
            on_tick: None,
        }
    }

    /// Register a callback invoked after each collection tick.
    pub fn on_tick(&mut self, f: Arc<dyn Fn() + Send + Sync>) {
        self.on_tick = Some(f);
    }

    /// Run until shutdown signal is received.
    pub async fn run(&mut self) {
        info!(
            fast_s = self.fast_interval.as_secs(),
            slow_s = self.slow_interval.as_secs(),
            "Scheduler starting"
        );

        let mut fast = interval(self.fast_interval);
        fast.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut slow = interval(self.slow_interval);
        slow.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // Collect immediately on start
        self.run_fast_collectors().await;
        self.run_slow_collectors().await;

        loop {
            tokio::select! {
                _ = fast.tick() => {
                    self.run_fast_collectors().await;
                }
                _ = slow.tick() => {
                    self.run_slow_collectors().await;
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("Scheduler shutting down");
                        return;
                    }
                }
            }
        }
    }

    /// Run a single fast + slow tick (for dry-run / testing).
    pub async fn run_once(&self) {
        self.run_fast_collectors().await;
        self.run_slow_collectors().await;
    }

    async fn run_fast_collectors(&self) {
        debug!("Fast tick");
        let db_type = self.pool.db_type();
        let all_names: Vec<String> = registry::all_collector_names_for(db_type)
            .into_iter()
            .map(String::from)
            .collect();
        let collectors = registry::build_collectors_for_tick(db_type, "fast", &all_names);
        self.run_collectors(&collectors).await;
    }

    async fn run_slow_collectors(&self) {
        debug!("Slow tick");
        let db_type = self.pool.db_type();
        let all_names: Vec<String> = registry::all_collector_names_for(db_type)
            .into_iter()
            .map(String::from)
            .collect();
        let collectors = registry::build_collectors_for_tick(db_type, "slow", &all_names);
        self.run_collectors(&collectors).await;
    }

    async fn run_collectors(&self, collectors: &[Box<dyn Collector>]) {
        for collector in collectors {
            // Check capabilities
            let missing: Vec<_> = collector
                .requires()
                .iter()
                .filter(|req| !self.capabilities.has(req))
                .collect();

            if !missing.is_empty() {
                debug!(
                    collector = collector.name(),
                    missing = ?missing,
                    "Skipping collector — missing capabilities"
                );
                continue;
            }

            match collector.collect(self.pool.as_ref()).await {
                Ok(mut snapshot) => {
                    // Anonymize query text in the snapshot data
                    anonymize_snapshot_queries(&mut snapshot);

                    // Extract and store query fingerprints
                    if let Err(e) = store_fingerprints(&self.store, &snapshot).await {
                        warn!(collector = collector.name(), error = %e, "Failed to store fingerprints");
                    }

                    if let Err(e) = self.store.insert_snapshot(&snapshot).await {
                        error!(
                            collector = collector.name(),
                            error = %e,
                            "Failed to write snapshot to store"
                        );
                    }
                }
                Err(e) => {
                    error!(
                        collector = collector.name(),
                        error = %e,
                        "Collector failed"
                    );
                }
            }
        }

        // Signal shippers
        if let Some(ref f) = self.on_tick {
            f();
        }
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
