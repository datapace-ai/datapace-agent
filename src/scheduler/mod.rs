//! Metrics collection scheduler.
//!
//! Manages periodic collection and upload of database metrics.

use crate::collector::{Collector, CollectorError};
use crate::uploader::{Uploader, UploaderError};
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

/// Scheduler for periodic metrics collection
pub struct Scheduler {
    collector: Arc<dyn Collector>,
    uploader: Arc<Uploader>,
    interval: Duration,
    shutdown_rx: watch::Receiver<bool>,
}

impl Scheduler {
    /// Create a new scheduler
    pub fn new(
        collector: Arc<dyn Collector>,
        uploader: Arc<Uploader>,
        interval: Duration,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            collector,
            uploader,
            interval,
            shutdown_rx,
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

        // Add jitter to prevent thundering herd
        let jitter = Duration::from_millis(rand::random::<u64>() % 5000);
        tokio::time::sleep(jitter).await;

        let mut interval = interval(self.interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // Collect immediately on start
        self.collect_and_upload().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.collect_and_upload().await;
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

    /// Perform a single collection and upload cycle
    async fn collect_and_upload(&self) {
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
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to upload metrics");
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to collect metrics");
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

// Simple random number for jitter - avoid pulling in full rand crate
mod rand {
    pub fn random<T: Default>() -> T {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();

        // This is a simplification - in real code use proper random
        unsafe { std::mem::transmute_copy(&nanos) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_error_display() {
        let err = SchedulerError::Stopped;
        assert!(err.to_string().contains("stopped"));
    }
}
