use super::{
    AgentEvent, DatabaseEntry, PipelineEvent, QueryFingerprint, ShipperEntry, ShippingEntry,
    Snapshot,
};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;
use tracing::{debug, info};

/// Local SQLite time-series store — the single source of truth for the UI.
pub struct Store {
    pool: SqlitePool,
    retention_days: u32,
}

impl Store {
    /// Open (or create) the SQLite database at `path`.
    pub async fn open(path: &Path, retention_days: u32) -> Result<Self, sqlx::Error> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await?;

        let store = Self {
            pool,
            retention_days,
        };
        store.run_migrations().await?;
        Ok(store)
    }

    async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS metric_snapshots (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id   TEXT    NOT NULL DEFAULT '',
                collector   TEXT    NOT NULL,
                data        TEXT    NOT NULL,
                collected_at TEXT   NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // Add source_id column if missing (migration from old schema)
        self.add_column_if_missing("metric_snapshots", "source_id", "TEXT NOT NULL DEFAULT ''")
            .await;

        self.add_column_if_missing(
            "metric_snapshots",
            "idempotency_key",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_source_collector_time
             ON metric_snapshots(source_id, collector, collected_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS query_fingerprints (
                fingerprint     TEXT PRIMARY KEY,
                source_id       TEXT NOT NULL DEFAULT '',
                sanitized_query TEXT NOT NULL,
                first_seen      TEXT NOT NULL,
                last_seen       TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        self.add_column_if_missing(
            "query_fingerprints",
            "source_id",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS agent_events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id   TEXT NOT NULL DEFAULT '',
                event_type  TEXT NOT NULL,
                message     TEXT NOT NULL,
                created_at  TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        self.add_column_if_missing("agent_events", "source_id", "TEXT NOT NULL DEFAULT ''")
            .await;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS capabilities_cache (
                id          INTEGER PRIMARY KEY CHECK (id = 1),
                data        TEXT NOT NULL,
                probed_at   TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // ── New tables for multi-DB ──

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS databases (
                id              TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                url             TEXT NOT NULL,
                db_type         TEXT NOT NULL DEFAULT 'postgres',
                environment     TEXT NOT NULL DEFAULT 'production',
                pool_size       INTEGER NOT NULL DEFAULT 3,
                fast_interval   INTEGER NOT NULL DEFAULT 30,
                slow_interval   INTEGER NOT NULL DEFAULT 300,
                collectors      TEXT NOT NULL DEFAULT '[]',
                anonymize       INTEGER NOT NULL DEFAULT 1,
                shipper_enabled INTEGER NOT NULL DEFAULT 0,
                shipper_endpoint TEXT,
                shipper_token   TEXT,
                status          TEXT NOT NULL DEFAULT 'stopped',
                created_at      TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // Migrations for new columns on existing databases
        self.add_column_if_missing("databases", "db_type", "TEXT NOT NULL DEFAULT 'postgres'")
            .await;
        self.add_column_if_missing(
            "databases",
            "environment",
            "TEXT NOT NULL DEFAULT 'production'",
        )
        .await;
        self.add_column_if_missing("databases", "anonymize", "INTEGER NOT NULL DEFAULT 1")
            .await;
        self.add_column_if_missing("databases", "shippers", "TEXT NOT NULL DEFAULT '[]'")
            .await;

        // Migrate legacy shipper_enabled rows → shippers JSON
        self.migrate_legacy_shippers().await;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS pipeline_events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id   TEXT NOT NULL,
                tick_type   TEXT NOT NULL,
                collectors  TEXT NOT NULL,
                created_at  TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pipeline_source_time
             ON pipeline_events(source_id, created_at DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS shipping_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id   TEXT NOT NULL,
                status      TEXT NOT NULL,
                bytes       INTEGER NOT NULL DEFAULT 0,
                error       TEXT,
                created_at  TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_shipping_source_time
             ON shipping_log(source_id, created_at DESC)",
        )
        .execute(&self.pool)
        .await?;

        self.add_column_if_missing("shipping_log", "shipper_id", "TEXT NOT NULL DEFAULT ''")
            .await;

        info!("SQLite store initialized");
        Ok(())
    }

    /// Safely add a column if it doesn't exist yet.
    async fn add_column_if_missing(&self, table: &str, column: &str, col_type: &str) {
        // Check if column exists by querying pragma
        let query = format!("PRAGMA table_info({})", table);
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> = sqlx::query_as(&query)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let has_column = rows.iter().any(|(_, name, _, _, _, _)| name == column);
        if !has_column {
            let alter = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, col_type);
            sqlx::query(&alter).execute(&self.pool).await.ok();
            debug!("Added column {}.{}", table, column);
        }
    }

    /// One-time migration: convert legacy shipper_enabled rows → shippers JSON.
    async fn migrate_legacy_shippers(&self) {
        // Check if old columns exist
        let query = "PRAGMA table_info(databases)";
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> = sqlx::query_as(query)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let has_shipper_enabled = rows
            .iter()
            .any(|(_, name, _, _, _, _)| name == "shipper_enabled");
        if !has_shipper_enabled {
            return;
        }

        // Read rows with shipper_enabled=1 that have empty shippers
        let legacy_rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id, shipper_endpoint, shipper_token FROM databases WHERE shipper_enabled = 1 AND (shippers = '[]' OR shippers IS NULL)",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        for (id, endpoint, token) in legacy_rows {
            if let Some(ep) = endpoint {
                if !ep.is_empty() {
                    let shipper = ShipperEntry {
                        id: "legacy".into(),
                        name: "Legacy Shipper".into(),
                        shipper_type: "webhook".into(),
                        endpoint: ep,
                        token,
                        enabled: true,
                    };
                    let json =
                        serde_json::to_string(&vec![shipper]).unwrap_or_else(|_| "[]".into());
                    sqlx::query("UPDATE databases SET shippers = ? WHERE id = ?")
                        .bind(&json)
                        .bind(&id)
                        .execute(&self.pool)
                        .await
                        .ok();
                    debug!("Migrated legacy shipper for database {}", id);
                }
            }
        }
    }

    // ── Databases ─────────────────────────────────────────────

    /// Insert a new database entry.
    pub async fn insert_database(&self, db: &DatabaseEntry) -> Result<(), sqlx::Error> {
        let collectors_json = serde_json::to_string(&db.collectors).unwrap_or_else(|_| "[]".into());
        let shippers_json = serde_json::to_string(&db.shippers).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "INSERT INTO databases (id, name, url, db_type, environment, pool_size, fast_interval, slow_interval, collectors, anonymize, shippers, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&db.id)
        .bind(&db.name)
        .bind(&db.url)
        .bind(&db.db_type)
        .bind(&db.environment)
        .bind(db.pool_size)
        .bind(db.fast_interval as i64)
        .bind(db.slow_interval as i64)
        .bind(&collectors_json)
        .bind(db.anonymize)
        .bind(&shippers_json)
        .bind(&db.status)
        .bind(db.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update an existing database entry.
    pub async fn update_database(&self, db: &DatabaseEntry) -> Result<(), sqlx::Error> {
        let collectors_json = serde_json::to_string(&db.collectors).unwrap_or_else(|_| "[]".into());
        let shippers_json = serde_json::to_string(&db.shippers).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "UPDATE databases SET name=?, url=?, db_type=?, environment=?, pool_size=?, fast_interval=?, slow_interval=?, collectors=?, anonymize=?, shippers=?, status=?
             WHERE id=?",
        )
        .bind(&db.name)
        .bind(&db.url)
        .bind(&db.db_type)
        .bind(&db.environment)
        .bind(db.pool_size)
        .bind(db.fast_interval as i64)
        .bind(db.slow_interval as i64)
        .bind(&collectors_json)
        .bind(db.anonymize)
        .bind(&shippers_json)
        .bind(&db.status)
        .bind(&db.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a database entry and its associated data.
    pub async fn delete_database(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM databases WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM metric_snapshots WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM pipeline_events WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM shipping_log WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM query_fingerprints WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM agent_events WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// List all database entries.
    pub async fn list_databases(&self) -> Result<Vec<DatabaseEntry>, sqlx::Error> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(String, String, String, String, String, i64, i64, i64, String, bool, String, String, String)> = sqlx::query_as(
            "SELECT id, name, url, db_type, environment, pool_size, fast_interval, slow_interval, collectors, anonymize, shippers, status, created_at
             FROM databases ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(
                |(
                    id,
                    name,
                    url,
                    db_type,
                    environment,
                    pool_size,
                    fast,
                    slow,
                    collectors,
                    anonymize,
                    shippers,
                    status,
                    ts,
                )| {
                    let created_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                    let collectors: Vec<String> =
                        serde_json::from_str(&collectors).unwrap_or_default();
                    let shippers: Vec<ShipperEntry> =
                        serde_json::from_str(&shippers).unwrap_or_default();
                    Some(DatabaseEntry {
                        id,
                        name,
                        url,
                        db_type,
                        environment,
                        pool_size: pool_size as u32,
                        fast_interval: fast as u64,
                        slow_interval: slow as u64,
                        collectors,
                        anonymize,
                        shippers,
                        status,
                        created_at,
                    })
                },
            )
            .collect())
    }

    /// Get a single database entry by ID.
    pub async fn get_database(&self, id: &str) -> Result<Option<DatabaseEntry>, sqlx::Error> {
        #[allow(clippy::type_complexity)]
        let row: Option<(String, String, String, String, String, i64, i64, i64, String, bool, String, String, String)> = sqlx::query_as(
            "SELECT id, name, url, db_type, environment, pool_size, fast_interval, slow_interval, collectors, anonymize, shippers, status, created_at
             FROM databases WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(
            |(
                id,
                name,
                url,
                db_type,
                environment,
                pool_size,
                fast,
                slow,
                collectors,
                anonymize,
                shippers,
                status,
                ts,
            )| {
                let created_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                let collectors: Vec<String> = serde_json::from_str(&collectors).unwrap_or_default();
                let shippers: Vec<ShipperEntry> =
                    serde_json::from_str(&shippers).unwrap_or_default();
                Some(DatabaseEntry {
                    id,
                    name,
                    url,
                    db_type,
                    environment,
                    pool_size: pool_size as u32,
                    fast_interval: fast as u64,
                    slow_interval: slow as u64,
                    collectors,
                    anonymize,
                    shippers,
                    status,
                    created_at,
                })
            },
        ))
    }

    /// Update the status of a database.
    pub async fn update_database_status(&self, id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE databases SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Snapshots ───────────────────────────────────────────────

    /// Insert a metric snapshot (with source_id).
    pub async fn insert_snapshot_for(
        &self,
        source_id: &str,
        snap: &Snapshot,
    ) -> Result<(), sqlx::Error> {
        let data_str = serde_json::to_string(&snap.data).unwrap_or_default();
        let ts = snap.collected_at.to_rfc3339();
        sqlx::query(
            "INSERT INTO metric_snapshots (source_id, collector, data, collected_at, idempotency_key) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(source_id)
        .bind(&snap.collector)
        .bind(&data_str)
        .bind(&ts)
        .bind(&snap.idempotency_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Insert a metric snapshot (legacy, empty source_id).
    pub async fn insert_snapshot(&self, snap: &Snapshot) -> Result<(), sqlx::Error> {
        self.insert_snapshot_for("", snap).await
    }

    /// Get time-series snapshots for a collector within a time range.
    pub async fn get_series(
        &self,
        collector: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Snapshot>, sqlx::Error> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT collector, data, collected_at, idempotency_key
             FROM metric_snapshots
             WHERE collector = ? AND collected_at >= ? AND collected_at <= ?
             ORDER BY collected_at ASC",
        )
        .bind(collector)
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(coll, data, ts, key)| {
                let collected_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                let data: serde_json::Value = serde_json::from_str(&data).ok()?;
                Some(Snapshot {
                    collector: coll,
                    data,
                    collected_at,
                    idempotency_key: key,
                })
            })
            .collect())
    }

    /// Get the latest snapshot for each collector.
    pub async fn get_latest_snapshots(&self) -> Result<Vec<Snapshot>, sqlx::Error> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT collector, data, collected_at, idempotency_key
             FROM metric_snapshots
             WHERE id IN (
                SELECT MAX(id) FROM metric_snapshots GROUP BY collector
             )",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(coll, data, ts, key)| {
                let collected_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                let data: serde_json::Value = serde_json::from_str(&data).ok()?;
                Some(Snapshot {
                    collector: coll,
                    data,
                    collected_at,
                    idempotency_key: key,
                })
            })
            .collect())
    }

    /// Get the latest snapshot for a specific collector and source.
    pub async fn get_latest_snapshot_for(
        &self,
        source_id: &str,
        collector: &str,
    ) -> Result<Option<Snapshot>, sqlx::Error> {
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT collector, data, collected_at, idempotency_key
             FROM metric_snapshots
             WHERE source_id = ? AND collector = ?
             ORDER BY collected_at DESC
             LIMIT 1",
        )
        .bind(source_id)
        .bind(collector)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(coll, data, ts, key)| {
            let collected_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
            let data: serde_json::Value = serde_json::from_str(&data).ok()?;
            Some(Snapshot {
                collector: coll,
                data,
                collected_at,
                idempotency_key: key,
            })
        }))
    }

    /// Get the latest snapshots for all collectors of a given source.
    pub async fn get_latest_snapshots_for(
        &self,
        source_id: &str,
    ) -> Result<Vec<Snapshot>, sqlx::Error> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT collector, data, collected_at, idempotency_key
             FROM metric_snapshots
             WHERE source_id = ? AND id IN (
                SELECT MAX(id) FROM metric_snapshots WHERE source_id = ? GROUP BY collector
             )",
        )
        .bind(source_id)
        .bind(source_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(coll, data, ts, key)| {
                let collected_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                let data: serde_json::Value = serde_json::from_str(&data).ok()?;
                Some(Snapshot {
                    collector: coll,
                    data,
                    collected_at,
                    idempotency_key: key,
                })
            })
            .collect())
    }

    // ── Query Fingerprints ──────────────────────────────────────

    /// Upsert a query fingerprint.
    pub async fn upsert_fingerprint(&self, fp: &QueryFingerprint) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO query_fingerprints (fingerprint, sanitized_query, first_seen, last_seen)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
                sanitized_query = excluded.sanitized_query,
                last_seen = excluded.last_seen",
        )
        .bind(&fp.fingerprint)
        .bind(&fp.sanitized_query)
        .bind(fp.first_seen.to_rfc3339())
        .bind(fp.last_seen.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get top queries by last_seen (most recent first).
    pub async fn get_top_queries(&self, limit: u32) -> Result<Vec<QueryFingerprint>, sqlx::Error> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT fingerprint, sanitized_query, first_seen, last_seen
             FROM query_fingerprints
             ORDER BY last_seen DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(fp, query, first, last)| {
                Some(QueryFingerprint {
                    fingerprint: fp,
                    sanitized_query: query,
                    first_seen: DateTime::parse_from_rfc3339(&first)
                        .ok()?
                        .with_timezone(&Utc),
                    last_seen: DateTime::parse_from_rfc3339(&last)
                        .ok()?
                        .with_timezone(&Utc),
                })
            })
            .collect())
    }

    // ── Agent Events ────────────────────────────────────────────

    /// Log an agent event.
    pub async fn log_event(&self, event_type: &str, message: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO agent_events (event_type, message, created_at) VALUES (?, ?, ?)")
            .bind(event_type)
            .bind(message)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get recent agent events.
    pub async fn get_events(&self, limit: u32) -> Result<Vec<AgentEvent>, sqlx::Error> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT event_type, message, created_at
             FROM agent_events
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(t, m, ts)| {
                Some(AgentEvent {
                    event_type: t,
                    message: m,
                    created_at: DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc),
                })
            })
            .collect())
    }

    // ── Pipeline Events ──────────────────────────────────────────

    /// Record a pipeline tick event.
    pub async fn insert_pipeline_event(&self, event: &PipelineEvent) -> Result<(), sqlx::Error> {
        let collectors_str = serde_json::to_string(&event.collectors_json).unwrap_or_default();
        sqlx::query(
            "INSERT INTO pipeline_events (source_id, tick_type, collectors, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&event.source_id)
        .bind(&event.tick_type)
        .bind(&collectors_str)
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get recent pipeline events for a source.
    pub async fn get_pipeline_events(
        &self,
        source_id: &str,
        limit: u32,
    ) -> Result<Vec<PipelineEvent>, sqlx::Error> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT source_id, tick_type, collectors, created_at
             FROM pipeline_events
             WHERE source_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(source_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(sid, tick, colls, ts)| {
                let created_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                let collectors_json: serde_json::Value = serde_json::from_str(&colls).ok()?;
                Some(PipelineEvent {
                    source_id: sid,
                    tick_type: tick,
                    collectors_json,
                    created_at,
                })
            })
            .collect())
    }

    // ── Shipping Log ─────────────────────────────────────────────

    /// Record a shipping event.
    pub async fn insert_shipping_entry(&self, entry: &ShippingEntry) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO shipping_log (source_id, shipper_id, status, bytes, error, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.source_id)
        .bind(&entry.shipper_id)
        .bind(&entry.status)
        .bind(entry.bytes as i64)
        .bind(&entry.error)
        .bind(entry.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get recent shipping entries for a source.
    pub async fn get_shipping_entries(
        &self,
        source_id: &str,
        limit: u32,
    ) -> Result<Vec<ShippingEntry>, sqlx::Error> {
        let rows: Vec<(String, String, String, i64, Option<String>, String)> = sqlx::query_as(
            "SELECT source_id, shipper_id, status, bytes, error, created_at
             FROM shipping_log
             WHERE source_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(source_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(sid, shipper_id, status, bytes, error, ts)| {
                let created_at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                Some(ShippingEntry {
                    source_id: sid,
                    shipper_id,
                    status,
                    bytes: bytes as u64,
                    error,
                    created_at,
                })
            })
            .collect())
    }

    // ── Capabilities Cache ──────────────────────────────────────

    /// Cache capabilities JSON.
    pub async fn cache_capabilities(&self, data: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO capabilities_cache (id, data, probed_at) VALUES (1, ?, ?)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, probed_at = excluded.probed_at",
        )
        .bind(data)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Load cached capabilities.
    pub async fn load_capabilities(&self) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT data FROM capabilities_cache WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(d,)| d))
    }

    // ── Maintenance ─────────────────────────────────────────────

    /// Prune data older than `retention_days`.
    pub async fn prune(&self) -> Result<u64, sqlx::Error> {
        let cutoff = Utc::now() - chrono::Duration::days(self.retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let r1 = sqlx::query("DELETE FROM metric_snapshots WHERE collected_at < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await?;

        let r2 = sqlx::query("DELETE FROM agent_events WHERE created_at < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await?;

        let r3 = sqlx::query("DELETE FROM pipeline_events WHERE created_at < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await?;

        let r4 = sqlx::query("DELETE FROM shipping_log WHERE created_at < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await?;

        let total =
            r1.rows_affected() + r2.rows_affected() + r3.rows_affected() + r4.rows_affected();
        if total > 0 {
            debug!(
                "Pruned {total} old rows (retention = {} days)",
                self.retention_days
            );
        }
        Ok(total)
    }

    /// Get a reference to the underlying pool (for tests / advanced queries).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_store() -> (Store, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(&path, 90).await.unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn insert_and_query_snapshot() {
        let (store, _dir) = test_store().await;
        let snap = Snapshot {
            collector: "test".into(),
            data: serde_json::json!({"value": 42}),
            collected_at: Utc::now(),
            idempotency_key: String::new(),
        };
        store.insert_snapshot(&snap).await.unwrap();

        let from = Utc::now() - chrono::Duration::hours(1);
        let to = Utc::now() + chrono::Duration::hours(1);
        let results = store.get_series("test", from, to).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data["value"], 42);
    }

    #[tokio::test]
    async fn latest_snapshots() {
        let (store, _dir) = test_store().await;
        for i in 0..3 {
            let snap = Snapshot {
                collector: "test".into(),
                data: serde_json::json!({"i": i}),
                collected_at: Utc::now(),
                idempotency_key: String::new(),
            };
            store.insert_snapshot(&snap).await.unwrap();
        }
        let latest = store.get_latest_snapshots().await.unwrap();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].data["i"], 2);
    }

    #[tokio::test]
    async fn fingerprint_upsert() {
        let (store, _dir) = test_store().await;
        let now = Utc::now();
        let fp = QueryFingerprint {
            fingerprint: "abc123".into(),
            sanitized_query: "SELECT ?".into(),
            first_seen: now,
            last_seen: now,
        };
        store.upsert_fingerprint(&fp).await.unwrap();
        store.upsert_fingerprint(&fp).await.unwrap(); // should not fail

        let top = store.get_top_queries(10).await.unwrap();
        assert_eq!(top.len(), 1);
    }

    #[tokio::test]
    async fn event_logging() {
        let (store, _dir) = test_store().await;
        store.log_event("startup", "Agent started").await.unwrap();
        let events = store.get_events(10).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "startup");
    }

    #[tokio::test]
    async fn capabilities_cache() {
        let (store, _dir) = test_store().await;
        assert!(store.load_capabilities().await.unwrap().is_none());
        store.cache_capabilities("{\"test\": true}").await.unwrap();
        let cached = store.load_capabilities().await.unwrap().unwrap();
        assert!(cached.contains("test"));
    }

    #[tokio::test]
    async fn database_crud() {
        let (store, _dir) = test_store().await;
        let db = DatabaseEntry {
            id: "test-db".into(),
            name: "Test DB".into(),
            url: "postgres://localhost/test".into(),
            db_type: "postgres".into(),
            environment: "development".into(),
            pool_size: 3,
            fast_interval: 30,
            slow_interval: 300,
            collectors: vec!["statements".into(), "activity".into()],
            anonymize: false,
            shippers: vec![ShipperEntry {
                id: "s1".into(),
                name: "Test Webhook".into(),
                shipper_type: "webhook".into(),
                endpoint: "https://example.com/ingest".into(),
                token: None,
                enabled: true,
            }],
            status: "stopped".into(),
            created_at: Utc::now(),
        };
        store.insert_database(&db).await.unwrap();

        let list = store.list_databases().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test DB");
        assert_eq!(list[0].shippers.len(), 1);
        assert_eq!(list[0].shippers[0].id, "s1");

        let got = store.get_database("test-db").await.unwrap().unwrap();
        assert_eq!(got.collectors, vec!["statements", "activity"]);

        store.delete_database("test-db").await.unwrap();
        let list = store.list_databases().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn pipeline_events() {
        let (store, _dir) = test_store().await;
        let event = PipelineEvent {
            source_id: "db1".into(),
            tick_type: "fast".into(),
            collectors_json: serde_json::json!([{"name": "statements", "rows": 42, "duration_ms": 23}]),
            created_at: Utc::now(),
        };
        store.insert_pipeline_event(&event).await.unwrap();
        let events = store.get_pipeline_events("db1", 10).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tick_type, "fast");
    }

    #[tokio::test]
    async fn shipping_log() {
        let (store, _dir) = test_store().await;
        let entry = ShippingEntry {
            source_id: "db1".into(),
            shipper_id: "s1".into(),
            status: "ok".into(),
            bytes: 1234,
            error: None,
            created_at: Utc::now(),
        };
        store.insert_shipping_entry(&entry).await.unwrap();
        let entries = store.get_shipping_entries("db1", 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].bytes, 1234);
        assert_eq!(entries[0].shipper_id, "s1");
    }
}
