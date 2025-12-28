//! Datapace Agent - Database metrics collector for Datapace Cloud.
//!
//! Usage:
//!   datapace-agent [OPTIONS]
//!
//! Options:
//!   -c, --config <FILE>    Path to configuration file
//!   --dry-run              Collect metrics once and print to stdout
//!   --test-connection      Test database and cloud connections
//!   -v, --verbose          Enable verbose logging
//!   -V, --version          Print version information
//!   -h, --help             Print help

use anyhow::{Context, Result};
use clap::Parser;
use datapace_agent::{
    collector, config::Config, scheduler::Scheduler, uploader::{Uploader, UploaderConfig},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, EnvFilter};

/// Datapace Agent - Database metrics collector
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Collect metrics once and print to stdout (no upload)
    #[arg(long)]
    dry_run: bool,

    /// Test database and cloud connections, then exit
    #[arg(long)]
    test_connection: bool,

    /// Enable verbose logging (debug level)
    #[arg(short, long)]
    verbose: bool,

    /// Output logs in JSON format
    #[arg(long)]
    json_logs: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config = load_config(&args)?;

    // Setup logging
    setup_logging(&args, &config);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting Datapace Agent"
    );

    // Create collector
    let collector = collector::create_collector(&config.database.url, config.database.provider)
        .await
        .context("Failed to create database collector")?;

    info!(
        provider = collector.provider(),
        version = collector.version(),
        "Connected to database"
    );

    // Handle test connection mode
    if args.test_connection {
        return test_connections(&config, collector.as_ref()).await;
    }

    // Create uploader
    let uploader_config = UploaderConfig::new(
        config.datapace.endpoint.clone(),
        config.datapace.api_key.clone(),
    );
    let uploader = Uploader::new(uploader_config).context("Failed to create uploader")?;

    // Handle dry-run mode
    if args.dry_run {
        let (_, shutdown_rx) = watch::channel(false);
        let scheduler = Scheduler::new(
            Arc::from(collector),
            Arc::new(uploader),
            config.collection.interval,
            shutdown_rx,
        );
        return scheduler.run_once().await.map_err(Into::into);
    }

    // Setup shutdown signal handling
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn signal handler
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("Shutdown signal received");
        let _ = shutdown_tx.send(true);
    });

    // Run the scheduler
    let mut scheduler = Scheduler::new(
        Arc::from(collector),
        Arc::new(uploader),
        config.collection.interval,
        shutdown_rx,
    );

    info!(
        interval_secs = config.collection.interval.as_secs(),
        "Starting metrics collection loop"
    );

    scheduler.run().await?;

    info!("Datapace Agent stopped");
    Ok(())
}

fn load_config(args: &Args) -> Result<Config> {
    if let Some(ref path) = args.config {
        Config::from_file(path).context(format!("Failed to load config from {:?}", path))
    } else {
        Config::from_env().context("Failed to load config from environment")
    }
}

fn setup_logging(args: &Args, config: &Config) {
    let level = if args.verbose {
        Level::DEBUG
    } else {
        config.logging.level.into()
    };

    let filter = EnvFilter::from_default_env()
        .add_directive(format!("datapace_agent={}", level).parse().unwrap())
        .add_directive("sqlx=warn".parse().unwrap())
        .add_directive("reqwest=warn".parse().unwrap());

    let use_json = args.json_logs || config.logging.format == datapace_agent::config::LogFormat::Json;

    if use_json {
        fmt()
            .json()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    } else {
        fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    }
}

async fn test_connections(
    config: &Config,
    collector: &dyn collector::Collector,
) -> Result<()> {
    println!("Testing database connection...");

    match collector.test_connection().await {
        Ok(()) => println!("  Database: OK (provider: {}, version: {})",
            collector.provider(),
            collector.version().unwrap_or("unknown")),
        Err(e) => {
            eprintln!("  Database: FAILED - {}", e);
            return Err(e.into());
        }
    }

    println!("\nTesting Datapace Cloud connection...");

    let uploader_config = UploaderConfig::new(
        config.datapace.endpoint.clone(),
        config.datapace.api_key.clone(),
    );
    let uploader = Uploader::new(uploader_config)?;

    match uploader.test_connection().await {
        Ok(()) => println!("  Datapace Cloud: OK"),
        Err(e) => {
            eprintln!("  Datapace Cloud: FAILED - {}", e);
            return Err(e.into());
        }
    }

    println!("\nAll connections verified successfully!");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
