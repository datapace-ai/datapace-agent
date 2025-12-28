//! Datapace Agent - Database metrics collector for Datapace Cloud.
//!
//! This crate provides a lightweight agent that collects metrics from databases
//! and sends them to Datapace Cloud for analysis and monitoring.
//!
//! # Features
//!
//! - **PostgreSQL Support**: Full support for PostgreSQL with auto-detection
//!   of cloud providers (RDS, Aurora, Supabase, Neon).
//! - **Lightweight**: Minimal resource usage, small binary size.
//! - **Secure**: Read-only database access, TLS encryption.
//!
//! # Example
//!
//! ```no_run
//! use datapace_agent::{config::Config, collector, uploader};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load configuration
//!     let config = Config::from_env()?;
//!
//!     // Create collector
//!     let collector = collector::create_collector(
//!         &config.database.url,
//!         config.database.provider,
//!     ).await?;
//!
//!     // Collect metrics
//!     let payload = collector.collect().await?;
//!
//!     println!("{}", payload.to_json_pretty()?);
//!     Ok(())
//! }
//! ```

pub mod collector;
pub mod config;
pub mod payload;
pub mod scheduler;
pub mod uploader;

pub use config::Config;
