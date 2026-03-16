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
}
