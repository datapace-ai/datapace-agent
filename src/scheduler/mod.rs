//! Metrics collection scheduler.
//!
//! Manages periodic collection and upload of database metrics.

use crate::collector::{Collector, CollectorError};
use crate::health::SharedHealthState;
use crate::uploader::{Upload, UploaderError};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::watch;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, warn};

/// Errors that can occur in the scheduler
#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Collection error: {0}")]
    CollectionError(#[from] CollectorError),

    #[error("Upload error: {0}")]
    UploadError(#[from] UploaderError),

    #[error("Scheduler was stopped")]
    Stopped,
}

/// Result of a single collect-and-upload cycle, used to update health state.
struct CycleResult {
    duration_ms: u64,
    collection_ok: bool,
    upload_error: Option<String>,
}

/// Scheduler for periodic metrics collection
pub struct Scheduler {
    collector: Arc<dyn Collector>,
    uploader: Arc<dyn Upload>,
    interval: Duration,
    shutdown_rx: watch::Receiver<bool>,
    health_state: Option<SharedHealthState>,
    start_time: std::time::Instant,
}

impl Scheduler {
    /// Create a new scheduler
    pub fn new(
        collector: Arc<dyn Collector>,
        uploader: Arc<dyn Upload>,
        interval: Duration,
        shutdown_rx: watch::Receiver<bool>,
        health_state: Option<SharedHealthState>,
    ) -> Self {
        Self {
            collector,
            uploader,
            interval,
            shutdown_rx,
            health_state,
            start_time: std::time::Instant::now(),
        }
    }

    /// Run the scheduler loop
    ///
    /// This will collect and upload metrics at the configured interval
    /// until a shutdown signal is received.
    pub async fn run(&mut self) -> Result<(), SchedulerError> {
        info!(
            interval_secs = self.interval.as_secs(),
            "Starting metrics collection scheduler"
        );

        // Add jitter to prevent thundering herd (use system time as simple randomness)
        let jitter_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_millis() as u64 % 5000)
            .unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(jitter_ms)).await;

        let mut interval = interval(self.interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // Collect immediately on start
        let result = self.collect_and_upload().await;
        self.update_health(&result).await;
        self.fire_heartbeat(&result).await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let result = self.collect_and_upload().await;
                    self.update_health(&result).await;
                    self.fire_heartbeat(&result).await;
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("Scheduler received shutdown signal");
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Update shared health state after a collection cycle.
    async fn update_health(&self, result: &CycleResult) {
        if let Some(ref health) = self.health_state {
            let mut state = health.write().await;
            state.uptime_secs = self.start_time.elapsed().as_secs();
            state.last_collection_time = Some(chrono::Utc::now());
            state.last_collection_duration_ms = Some(result.duration_ms);
            // database_connected reflects whether DB collection succeeded, not upload
            state.database_connected = result.collection_ok;
            if let Some(ref err) = result.upload_error {
                state.status = "degraded".to_string();
                state.last_collection_error = Some(err.clone());
            } else if !result.collection_ok {
                state.status = "degraded".to_string();
                state.last_collection_error =
                    Some("Database collection failed".to_string());
            } else {
                state.status = "ok".to_string();
                state.last_collection_error = None;
            }
        }
    }

    /// Fire a heartbeat to the cloud endpoint (non-blocking, log errors only).
    async fn fire_heartbeat(&self, result: &CycleResult) {
        let status = if result.upload_error.is_some() || !result.collection_ok {
            "degraded"
        } else {
            "ok"
        };
        if let Err(e) = self
            .uploader
            .send_heartbeat(status, env!("CARGO_PKG_VERSION"))
            .await
        {
            warn!(error = %e, "Heartbeat failed");
        }
    }

    /// Perform a single collection and upload cycle, returning status info.
    async fn collect_and_upload(&self) -> CycleResult {
        debug!("Starting metrics collection cycle");

        let start = std::time::Instant::now();

        match self.collector.collect().await {
            Ok(payload) => {
                let collection_time = start.elapsed();
                debug!(
                    duration_ms = collection_time.as_millis(),
                    "Metrics collection completed"
                );

                match self.uploader.upload(&payload).await {
                    Ok(()) => {
                        let total_time = start.elapsed();
                        info!(
                            collection_ms = collection_time.as_millis(),
                            total_ms = total_time.as_millis(),
                            "Metrics cycle completed successfully"
                        );
                        CycleResult {
                            duration_ms: total_time.as_millis() as u64,
                            collection_ok: true,
                            upload_error: None,
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to upload metrics");
                        CycleResult {
                            duration_ms: start.elapsed().as_millis() as u64,
                            collection_ok: true,
                            upload_error: Some(e.to_string()),
                        }
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to collect metrics");
                CycleResult {
                    duration_ms: start.elapsed().as_millis() as u64,
                    collection_ok: false,
                    upload_error: None,
                }
            }
        }
    }

    /// Run a single collection cycle (for dry-run mode)
    pub async fn run_once(&self) -> Result<(), SchedulerError> {
        info!("Running single metrics collection (dry-run mode)");

        let payload = self.collector.collect().await?;

        // In dry-run mode, just print the payload
        match payload.to_json_pretty() {
            Ok(json) => {
                println!("{}", json);
            }
            Err(e) => {
                warn!(error = %e, "Failed to serialize payload");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::MockCollector;
    use crate::config::DatabaseType;
    use crate::payload::{DatabaseInfo, Payload};
    use crate::uploader::MockUpload;
    use std::collections::HashMap;

    #[test]
    fn test_scheduler_error_display() {
        let err = SchedulerError::Stopped;
        assert!(err.to_string().contains("stopped"));
    }

    /// Helper: build a minimal valid Payload for mocks.
    fn mock_payload() -> Payload {
        Payload::new(DatabaseInfo {
            database_type: "postgres".to_string(),
            version: Some("16.1".to_string()),
            provider: "generic".to_string(),
            provider_metadata: HashMap::new(),
        })
        .with_instance_id("test://localhost/testdb")
        .with_settings(HashMap::new())
    }

    #[tokio::test]
    async fn test_run_once_success() {
        let payload = mock_payload();

        let mut mock_collector = MockCollector::new();
        mock_collector
            .expect_collect()
            .times(1)
            .returning(move || Ok(payload.clone()));
        // run_once only collects and prints; it does not call version/provider/database_type
        // but let's supply them in case the implementation changes.
        mock_collector
            .expect_provider()
            .return_const("generic".to_string());
        mock_collector
            .expect_version()
            .returning(|| Some("PostgreSQL 16.1".to_string()));
        mock_collector
            .expect_database_type()
            .returning(|| DatabaseType::Postgres);

        let mut mock_uploader = MockUpload::new();
        // run_once does not upload, but Scheduler requires an Upload Arc.
        mock_uploader.expect_upload().returning(|_| Ok(()));
        mock_uploader
            .expect_send_heartbeat()
            .returning(|_, _| Ok(()));

        let (_shutdown_tx, shutdown_rx) = watch::channel(false);

        let scheduler = Scheduler::new(
            Arc::new(mock_collector),
            Arc::new(mock_uploader),
            Duration::from_secs(60),
            shutdown_rx,
            None,
        );

        let result = scheduler.run_once().await;
        assert!(
            result.is_ok(),
            "run_once should complete without error: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_shutdown_signal() {
        let payload = mock_payload();

        let mut mock_collector = MockCollector::new();
        mock_collector
            .expect_collect()
            .returning(move || Ok(payload.clone()));
        mock_collector
            .expect_provider()
            .return_const("generic".to_string());
        mock_collector
            .expect_version()
            .returning(|| Some("PostgreSQL 16.1".to_string()));
        mock_collector
            .expect_database_type()
            .returning(|| DatabaseType::Postgres);
        mock_collector.expect_test_connection().returning(|| Ok(()));

        let mut mock_uploader = MockUpload::new();
        mock_uploader.expect_upload().returning(|_| Ok(()));
        mock_uploader
            .expect_send_heartbeat()
            .returning(|_, _| Ok(()));
        mock_uploader.expect_test_connection().returning(|| Ok(()));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let mut scheduler = Scheduler::new(
            Arc::new(mock_collector),
            Arc::new(mock_uploader),
            Duration::from_secs(3600), // long interval so the loop blocks on tick
            shutdown_rx,
            None,
        );

        // Send shutdown signal immediately after spawning the scheduler run.
        let handle = tokio::spawn(async move { scheduler.run().await });

        // Give the scheduler a moment to start and complete its first collect_and_upload.
        tokio::time::sleep(Duration::from_millis(200)).await;

        shutdown_tx.send(true).expect("Failed to send shutdown");

        let result = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .expect("Scheduler did not exit in time")
            .expect("Scheduler task panicked");

        assert!(
            result.is_ok(),
            "Scheduler should exit Ok on shutdown: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_collect_failure_no_upload() {
        let mut mock_collector = MockCollector::new();
        mock_collector.expect_collect().times(1).returning(|| {
            Err(CollectorError::ConnectionError(
                "connection refused".to_string(),
            ))
        });
        mock_collector
            .expect_provider()
            .return_const("generic".to_string());
        mock_collector
            .expect_version()
            .returning(|| Some("PostgreSQL 16.1".to_string()));
        mock_collector
            .expect_database_type()
            .returning(|| DatabaseType::Postgres);

        let mut mock_uploader = MockUpload::new();
        // Upload must NEVER be called when collection fails
        mock_uploader.expect_upload().times(0);
        mock_uploader
            .expect_send_heartbeat()
            .returning(|_, _| Ok(()));

        let (_shutdown_tx, shutdown_rx) = watch::channel(false);

        let scheduler = Scheduler::new(
            Arc::new(mock_collector),
            Arc::new(mock_uploader),
            Duration::from_secs(60),
            shutdown_rx,
            None,
        );

        let result = scheduler.run_once().await;
        assert!(
            result.is_err(),
            "run_once should return an error when collection fails"
        );
    }

    #[tokio::test]
    async fn test_upload_failure_does_not_panic() {
        let payload = mock_payload();

        let mut mock_collector = MockCollector::new();
        mock_collector
            .expect_collect()
            .returning(move || Ok(payload.clone()));
        mock_collector
            .expect_provider()
            .return_const("generic".to_string());
        mock_collector
            .expect_version()
            .returning(|| Some("PostgreSQL 16.1".to_string()));
        mock_collector
            .expect_database_type()
            .returning(|| DatabaseType::Postgres);
        mock_collector.expect_test_connection().returning(|| Ok(()));

        let mut mock_uploader = MockUpload::new();
        mock_uploader
            .expect_upload()
            .returning(|_| Err(UploaderError::MaxRetriesExceeded));
        mock_uploader
            .expect_send_heartbeat()
            .returning(|_, _| Ok(()));
        mock_uploader.expect_test_connection().returning(|| Ok(()));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let mut scheduler = Scheduler::new(
            Arc::new(mock_collector),
            Arc::new(mock_uploader),
            Duration::from_secs(3600),
            shutdown_rx,
            None,
        );

        // Spawn the scheduler and let it run one cycle (which includes a failed upload).
        let handle = tokio::spawn(async move { scheduler.run().await });

        // Give the scheduler time to start and complete the first collect_and_upload cycle.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Send shutdown signal
        shutdown_tx.send(true).expect("Failed to send shutdown");

        let result = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .expect("Scheduler did not exit in time")
            .expect("Scheduler task should NOT panic on upload failure");

        assert!(
            result.is_ok(),
            "Scheduler should exit Ok on shutdown even after upload failure: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_once_no_upload() {
        let payload = mock_payload();

        let mut mock_collector = MockCollector::new();
        mock_collector
            .expect_collect()
            .times(1)
            .returning(move || Ok(payload.clone()));
        mock_collector
            .expect_provider()
            .return_const("generic".to_string());
        mock_collector
            .expect_version()
            .returning(|| Some("PostgreSQL 16.1".to_string()));
        mock_collector
            .expect_database_type()
            .returning(|| DatabaseType::Postgres);

        let mut mock_uploader = MockUpload::new();
        // run_once must NEVER call upload or send_heartbeat
        mock_uploader.expect_upload().times(0);
        mock_uploader.expect_send_heartbeat().times(0);

        let (_shutdown_tx, shutdown_rx) = watch::channel(false);

        let scheduler = Scheduler::new(
            Arc::new(mock_collector),
            Arc::new(mock_uploader),
            Duration::from_secs(60),
            shutdown_rx,
            None,
        );

        let result = scheduler.run_once().await;
        assert!(
            result.is_ok(),
            "run_once should succeed: {:?}",
            result.err()
        );
    }
}
