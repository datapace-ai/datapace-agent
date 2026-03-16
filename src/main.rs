use anyhow::{Context, Result};
use clap::Parser;
use datapace_agent::{
    collector::{capability::Capabilities, pool::PostgresPool},
    config::Config,
    scheduler::{Scheduler, SchedulerManager},
    store::{DatabaseEntry, Store},
    ui,
};
use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn, Level};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(
    name = "datapace-agent",
    version,
    about = "PostgreSQL monitoring agent with local storage and web UI"
)]
struct Args {
    /// Path to agent.toml config file
    #[arg(short, long, value_name = "FILE", default_value = "agent.toml")]
    config: PathBuf,

    /// Collect once and print to stdout, then exit
    #[arg(long)]
    dry_run: bool,

    /// Test PostgreSQL connection, then exit
    #[arg(long)]
    test_connection: bool,

    /// Enable verbose (debug) logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Try to load config
    let config = match Config::from_file(&args.config) {
        Ok(c) => Some(c),
        Err(e) => {
            setup_logging_default(args.verbose);
            warn!("Config not loaded ({e}) — starting in UI-only mode");
            None
        }
    };

    if let Some(ref config) = config {
        setup_logging(&args, config);
    }

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting Datapace Agent"
    );

    // Determine data_dir and UI listen address
    let data_dir = config
        .as_ref()
        .map(|c| c.agent.data_dir.clone())
        .unwrap_or_else(|| PathBuf::from("/tmp/datapace"));
    let ui_listen = config
        .as_ref()
        .map(|c| c.ui.listen.clone())
        .unwrap_or_else(|| "0.0.0.0:7080".into());

    // Ensure data directory exists
    std::fs::create_dir_all(&data_dir).ok();

    // Open local SQLite store
    let sqlite_path = data_dir.join("datapace.db");
    let retention = config
        .as_ref()
        .map(|c| c.agent.retention_days)
        .unwrap_or(90);
    let store = Arc::new(
        Store::open(&sqlite_path, retention)
            .await
            .context("Failed to open SQLite store")?,
    );

    store
        .log_event(
            "startup",
            &format!("Agent v{} started", env!("CARGO_PKG_VERSION")),
        )
        .await
        .ok();

    // Create the scheduler manager
    let scheduler = Arc::new(SchedulerManager::new(store.clone()));

    // If config has a [postgres] section, handle legacy single-DB mode for dry-run/test
    if let Some(ref config) = config {
        if let Some(ref pg_config) = config.postgres {
            // Test connection mode
            if args.test_connection {
                let pool = PgPoolOptions::new()
                    .min_connections(1)
                    .max_connections(pg_config.pool_size)
                    .acquire_timeout(Duration::from_secs(30))
                    .connect(&pg_config.url)
                    .await
                    .context("Failed to connect to PostgreSQL")?;
                sqlx::query("SELECT 1").execute(&pool).await?;
                println!("PostgreSQL connection: OK");
                return Ok(());
            }

            // Dry-run mode uses legacy scheduler
            if args.dry_run {
                let pool = PgPoolOptions::new()
                    .min_connections(1)
                    .max_connections(pg_config.pool_size)
                    .acquire_timeout(Duration::from_secs(30))
                    .connect(&pg_config.url)
                    .await
                    .context("Failed to connect to PostgreSQL")?;

                let capabilities = Capabilities::probe(&pool)
                    .await
                    .context("Failed to probe PostgreSQL capabilities")?;

                let pool: Arc<dyn datapace_agent::collector::pool::DatabasePool> =
                    Arc::new(PostgresPool(pool));
                let (_tx, rx) = watch::channel(false);
                let sched = Scheduler::new(
                    pool,
                    store.clone(),
                    capabilities,
                    Duration::from_secs(pg_config.fast_interval),
                    Duration::from_secs(pg_config.slow_interval),
                    rx,
                );
                sched.run_once().await;
                let latest = store.get_latest_snapshots().await?;
                println!("{}", serde_json::to_string_pretty(&latest)?);
                return Ok(());
            }

            // Pre-seed this DB into the databases table if not already there
            let db_name = config
                .agent
                .name
                .clone()
                .unwrap_or_else(|| "default".into());
            let db_id = format!("config-{}", slug(&db_name));

            if store.get_database(&db_id).await?.is_none() {
                let all_collectors = vec![
                    "statements".into(),
                    "activity".into(),
                    "locks".into(),
                    "explain".into(),
                    "tables".into(),
                    "schema".into(),
                    "io".into(),
                ];

                let mut shippers = vec![];
                if config.shipper.target != datapace_agent::config::ShipperTarget::None {
                    let endpoint = config
                        .shipper
                        .endpoint
                        .clone()
                        .or(config.shipper.generic_url.clone())
                        .unwrap_or_default();
                    let token = config
                        .shipper
                        .api_key
                        .clone()
                        .or(config.shipper.generic_token.clone());
                    if !endpoint.is_empty() {
                        shippers.push(datapace_agent::store::ShipperEntry {
                            id: "config".into(),
                            name: format!("{:?}", config.shipper.target),
                            shipper_type: format!("{:?}", config.shipper.target).to_lowercase(),
                            endpoint,
                            token,
                            enabled: true,
                        });
                    }
                }

                let entry = DatabaseEntry {
                    id: db_id.clone(),
                    name: db_name,
                    url: pg_config.url.clone(),
                    db_type: "postgres".into(),
                    environment: "production".into(),
                    pool_size: pg_config.pool_size,
                    fast_interval: pg_config.fast_interval,
                    slow_interval: pg_config.slow_interval,
                    collectors: all_collectors,
                    anonymize: true,
                    shippers,
                    status: "stopped".into(),
                    created_at: chrono::Utc::now(),
                };

                store.insert_database(&entry).await.ok();
                info!(id = %db_id, "Pre-seeded database from TOML config");
            }
        }
    }

    // Boot all databases from SQLite
    let databases = store.list_databases().await.unwrap_or_default();
    info!(count = databases.len(), "Loading databases from store");

    for db in &databases {
        info!(id = %db.id, name = %db.name, "Starting scheduler for database");
        if let Err(e) = scheduler.add_db(db).await {
            error!(id = %db.id, error = %e, "Failed to start database scheduler");
        }
    }

    // Start web UI
    let app = ui::router(store.clone(), scheduler.clone());
    let listener = tokio::net::TcpListener::bind(&ui_listen)
        .await
        .with_context(|| format!("Failed to bind UI to {ui_listen}"))?;

    info!(addr = %ui_listen, "Web UI available");
    if databases.is_empty() {
        println!("\n  Open http://{ui_listen} to add your first database\n");
    } else {
        println!(
            "\n  Open http://{ui_listen} to manage {} database(s)\n",
            databases.len()
        );
    }

    // Spawn shutdown signal handler
    let scheduler_shutdown = scheduler.clone();
    let store_shutdown = store.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("Shutdown signal received");
        scheduler_shutdown.shutdown_all().await;
        store_shutdown
            .log_event("shutdown", "Agent stopped gracefully")
            .await
            .ok();
    });

    // Daily prune task
    let prune_store = store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(86400));
        loop {
            interval.tick().await;
            match prune_store.prune().await {
                Ok(n) if n > 0 => info!(pruned = n, "Daily prune completed"),
                Err(e) => error!(error = %e, "Prune failed"),
                _ => {}
            }
        }
    });

    // Serve UI (blocks until shutdown)
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_signal().await;
        })
        .await?;

    info!("Datapace Agent stopped");
    Ok(())
}

fn slug(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn setup_logging(args: &Args, config: &Config) {
    let level: Level = if args.verbose {
        Level::DEBUG
    } else {
        config.agent.log_level.into()
    };
    init_tracing(level);
}

fn setup_logging_default(verbose: bool) {
    let level = if verbose { Level::DEBUG } else { Level::INFO };
    init_tracing(level);
}

fn init_tracing(level: Level) {
    let filter = EnvFilter::from_default_env()
        .add_directive(format!("datapace_agent={level}").parse().unwrap())
        .add_directive("sqlx=warn".parse().unwrap())
        .add_directive("reqwest=warn".parse().unwrap())
        .add_directive("hyper=warn".parse().unwrap());

    fmt().with_env_filter(filter).with_target(false).init();
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
