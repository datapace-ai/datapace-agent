use bson::doc;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{info, warn};

/// Detected PostgreSQL capabilities based on extensions, views, and version.
///
/// Uses a HashMap internally so adding a new capability requires only one
/// additional `probe_*` call in `probe()` — no struct field or match arm needed.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub server_version: u32, // e.g. 160001 for 16.1
    caps: HashMap<String, bool>,
}

impl Capabilities {
    /// Probe the connected PostgreSQL instance for available capabilities.
    pub async fn probe(pool: &PgPool) -> Result<Self, sqlx::Error> {
        let server_version = get_server_version(pool).await?;
        let mut caps = HashMap::new();

        caps.insert(
            "pg_stat_statements".into(),
            probe_extension(pool, "pg_stat_statements").await,
        );
        caps.insert(
            "pg_stat_activity".into(),
            probe_view(pool, "pg_stat_activity").await,
        );
        caps.insert(
            "pg_stat_io".into(),
            server_version >= 160000 && probe_view(pool, "pg_stat_io").await,
        );
        caps.insert("pg_locks".into(), probe_view(pool, "pg_locks").await);
        caps.insert(
            "pg_buffercache".into(),
            probe_extension(pool, "pg_buffercache").await,
        );
        caps.insert(
            "pg_stat_replication".into(),
            probe_view(pool, "pg_stat_replication").await,
        );
        caps.insert("schema_catalog".into(), probe_schema_access(pool).await);

        let result = Self {
            server_version,
            caps,
        };
        result.print_table();
        Ok(result)
    }

    /// Create capabilities from a list of (name, available) pairs. Useful for tests.
    pub fn from_probes(server_version: u32, probes: Vec<(&str, bool)>) -> Self {
        let caps = probes
            .into_iter()
            .map(|(name, available)| (name.to_string(), available))
            .collect();
        Self {
            server_version,
            caps,
        }
    }

    /// Check if a named capability is available.
    pub fn has(&self, name: &str) -> bool {
        self.caps.get(name).copied().unwrap_or(false)
    }

    fn print_table(&self) {
        info!(
            "PostgreSQL v{}.{}",
            self.server_version / 10000,
            (self.server_version / 100) % 100
        );
        let mut keys: Vec<_> = self.caps.keys().collect();
        keys.sort();
        for name in keys {
            let available = self.caps[name];
            let status = if available { "OK" } else { "n/a" };
            info!("  {:<25} {}", name, status);
        }
    }
}

async fn get_server_version(pool: &PgPool) -> Result<u32, sqlx::Error> {
    // Try SHOW server_version_num first (returns a string in sqlx)
    let result: Result<(String,), _> = sqlx::query_as("SHOW server_version_num")
        .fetch_one(pool)
        .await;

    if let Ok((num_str,)) = result {
        if let Ok(num) = num_str.trim().parse::<u32>() {
            return Ok(num);
        }
    }

    // Fallback: parse from version() string
    let (ver_str,): (String,) = sqlx::query_as("SELECT version()").fetch_one(pool).await?;

    // e.g. "PostgreSQL 16.1 ..."
    let num = ver_str
        .split_whitespace()
        .nth(1)
        .and_then(|v| {
            let parts: Vec<&str> = v.split('.').collect();
            if parts.len() >= 2 {
                let major: u32 = parts[0].parse().ok()?;
                let minor: u32 = parts[1].parse().ok()?;
                Some(major * 10000 + minor * 100)
            } else {
                None
            }
        })
        .unwrap_or(150000);

    Ok(num)
}

async fn probe_extension(pool: &PgPool, name: &str) -> bool {
    let result: Result<(bool,), _> =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = $1)")
            .bind(name)
            .fetch_one(pool)
            .await;
    match result {
        Ok((exists,)) => {
            if !exists {
                warn!("Extension '{name}' not installed — related metrics will be skipped");
            }
            exists
        }
        Err(e) => {
            warn!("Could not check extension '{name}': {e}");
            false
        }
    }
}

async fn probe_view(pool: &PgPool, view: &str) -> bool {
    let query = format!("SELECT 1 FROM {view} LIMIT 0");
    match sqlx::query(&query).execute(pool).await {
        Ok(_) => true,
        Err(e) => {
            warn!("Cannot access {view}: {e}");
            false
        }
    }
}

async fn probe_schema_access(pool: &PgPool) -> bool {
    match sqlx::query("SELECT 1 FROM information_schema.tables LIMIT 0")
        .execute(pool)
        .await
    {
        Ok(_) => true,
        Err(e) => {
            warn!("Cannot access information_schema: {e}");
            false
        }
    }
}

/// Detected MongoDB capabilities based on available commands and configuration.
#[derive(Debug, Clone)]
pub struct MongoCapabilities {
    pub server_version: String,
    caps: HashMap<String, bool>,
}

impl MongoCapabilities {
    /// Probe the connected MongoDB instance for available capabilities.
    pub async fn probe(
        client: &mongodb::Client,
        db: &mongodb::Database,
    ) -> Result<Self, mongodb::error::Error> {
        let admin = client.database("admin");

        // Get server version from serverStatus
        let server_version = match db.run_command(doc! { "serverStatus": 1 }, None).await {
            Ok(doc) => doc.get_str("version").unwrap_or("unknown").to_string(),
            Err(_) => "unknown".into(),
        };

        let mut caps = HashMap::new();

        // server_status
        caps.insert(
            "server_status".into(),
            db.run_command(doc! { "serverStatus": 1 }, None)
                .await
                .is_ok(),
        );

        // current_op
        caps.insert(
            "current_op".into(),
            db.run_command(doc! { "currentOp": 1, "$all": false }, None)
                .await
                .is_ok(),
        );

        // profiler — check if profiling level >= 1
        let profiler_enabled = match db.run_command(doc! { "profile": -1 }, None).await {
            Ok(doc) => doc.get_i32("was").unwrap_or(0) >= 1,
            Err(_) => false,
        };
        caps.insert("profiler".into(), profiler_enabled);

        // repl_set
        caps.insert(
            "repl_set".into(),
            admin
                .run_command(doc! { "replSetGetStatus": 1 }, None)
                .await
                .is_ok(),
        );

        // top (admin-only, not available on Atlas shared tier)
        caps.insert(
            "top".into(),
            admin.run_command(doc! { "top": 1 }, None).await.is_ok(),
        );

        // list_collections
        caps.insert(
            "list_collections".into(),
            db.run_command(doc! { "listCollections": 1, "nameOnly": true }, None)
                .await
                .is_ok(),
        );

        let result = Self {
            server_version,
            caps,
        };
        result.print_table();
        Ok(result)
    }

    /// Create capabilities from a list of (name, available) pairs. Useful for tests.
    pub fn from_probes(server_version: &str, probes: Vec<(&str, bool)>) -> Self {
        let caps = probes
            .into_iter()
            .map(|(name, available)| (name.to_string(), available))
            .collect();
        Self {
            server_version: server_version.to_string(),
            caps,
        }
    }

    /// Check if a named capability is available.
    pub fn has(&self, name: &str) -> bool {
        self.caps.get(name).copied().unwrap_or(false)
    }

    fn print_table(&self) {
        info!("MongoDB v{}", self.server_version);
        let mut keys: Vec<_> = self.caps.keys().collect();
        keys.sort();
        for name in keys {
            let available = self.caps[name];
            let status = if available { "OK" } else { "n/a" };
            info!("  {:<25} {}", name, status);
        }
    }
}

/// Unified capability set for any database type.
#[derive(Debug, Clone)]
pub enum CapabilitySet {
    Postgres(Capabilities),
    MongoDB(MongoCapabilities),
}

impl CapabilitySet {
    pub fn has(&self, name: &str) -> bool {
        match self {
            Self::Postgres(c) => c.has(name),
            Self::MongoDB(c) => c.has(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_has() {
        let caps = Capabilities::from_probes(
            160001,
            vec![
                ("pg_stat_statements", true),
                ("pg_stat_activity", true),
                ("pg_stat_io", true),
                ("pg_locks", false),
                ("pg_buffercache", false),
                ("pg_stat_replication", false),
                ("schema_catalog", true),
            ],
        );
        assert!(caps.has("pg_stat_statements"));
        assert!(caps.has("pg_stat_io"));
        assert!(!caps.has("pg_locks"));
        assert!(!caps.has("unknown"));
    }

    #[test]
    fn from_probes_constructor() {
        let caps = Capabilities::from_probes(150000, vec![("custom_ext", true)]);
        assert_eq!(caps.server_version, 150000);
        assert!(caps.has("custom_ext"));
        assert!(!caps.has("pg_stat_statements"));
    }

    #[test]
    fn mongo_capability_has() {
        let caps = MongoCapabilities::from_probes(
            "7.0.0",
            vec![
                ("server_status", true),
                ("current_op", true),
                ("profiler", false),
                ("repl_set", true),
                ("top", false),
                ("list_collections", true),
            ],
        );
        assert!(caps.has("server_status"));
        assert!(caps.has("current_op"));
        assert!(!caps.has("profiler"));
        assert!(!caps.has("top"));
        assert!(!caps.has("unknown"));
    }

    #[test]
    fn capability_set_delegates() {
        let pg = CapabilitySet::Postgres(Capabilities::from_probes(
            160001,
            vec![("pg_stat_statements", true)],
        ));
        assert!(pg.has("pg_stat_statements"));
        assert!(!pg.has("server_status"));

        let mongo = CapabilitySet::MongoDB(MongoCapabilities::from_probes(
            "7.0.0",
            vec![("server_status", true)],
        ));
        assert!(mongo.has("server_status"));
        assert!(!mongo.has("pg_stat_statements"));
    }
}
