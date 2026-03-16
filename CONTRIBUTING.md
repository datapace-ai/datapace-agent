# Contributing to Datapace Agent

## How to Add a New PostgreSQL Collector

Adding a new collector requires **one new file** and **one line** in the registry.

### 1. Create the collector file

Create `src/collector/your_collector.rs`:

```rust
use super::pool::{require_postgres, DatabasePool};
use super::{Collector, CollectorError, CollectorInterval};
use crate::store::Snapshot;
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

pub struct YourCollector;

#[derive(Debug, FromRow, Serialize)]
struct YourRow {
    // fields matching your SQL query
}

const QUERY: &str = r#"
SELECT ... FROM your_view
"#;

#[async_trait]
impl Collector for YourCollector {
    fn name(&self) -> &'static str {
        "your_collector"
    }

    fn interval(&self) -> CollectorInterval {
        CollectorInterval::Fast // or CollectorInterval::Slow
    }

    fn requires(&self) -> &[&'static str] {
        &["your_capability"] // or &[] if always available
    }

    async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
        let pg = require_postgres(pool)?;
        let rows = sqlx::query_as::<_, YourRow>(QUERY).fetch_all(pg).await?;
        Ok(Snapshot {
            collector: self.name().into(),
            data: serde_json::to_value(&rows).unwrap_or_default(),
            collected_at: Utc::now(),
        })
    }
}
```

### 2. Register it

Add `pub mod your_collector;` to `src/collector/mod.rs`.

Add one entry to `all_collectors()` in `src/collector/registry.rs`:

```rust
CollectorInfo {
    name: "your_collector",
    interval: CollectorInterval::Fast,
    factory: || Box::new(YourCollector),
},
```

That's it. The scheduler, API defaults, and UI will pick it up automatically.

### 3. Choose the right interval

- **Fast** (~30s): Volatile metrics that change rapidly — active queries, locks, running statements.
- **Slow** (~300s): Structural data that changes infrequently — table sizes, schema metadata, I/O stats.

## How to Add a Capability

Capabilities represent PostgreSQL features (extensions, views) that may or may not be available.

In `Capabilities::probe()` in `src/collector/capability.rs`, add one line:

```rust
caps.insert("your_capability".into(), probe_extension(pool, "your_extension").await);
// or
caps.insert("your_capability".into(), probe_view(pool, "your_view").await);
```

Collectors reference capabilities via `fn requires(&self) -> &[&'static str]`. If a required capability is unavailable, the collector is skipped at runtime.

## Adding a New Database Type

The `Collector` trait accepts `&dyn DatabasePool`, making it fully extensible to any database technology without changing core code.

### 1. Implement `DatabasePool` for your database's pool type

Create `src/collector/pool_mysql.rs` (or similar):

```rust
use super::pool::DatabasePool;
use async_trait::async_trait;
use std::any::Any;

pub struct MySqlPool(pub sqlx::MySqlPool);

#[async_trait]
impl DatabasePool for MySqlPool {
    fn as_any(&self) -> &dyn Any { self }
    fn db_type(&self) -> &'static str { "mysql" }
    async fn close(&self) { self.0.close().await; }
}

pub fn require_mysql(pool: &dyn DatabasePool) -> Result<&sqlx::MySqlPool, CollectorError> {
    pool.as_any()
        .downcast_ref::<MySqlPool>()
        .map(|m| &m.0)
        .ok_or_else(|| CollectorError::NotAvailable(
            format!("requires MySQL, got {}", pool.db_type())
        ))
}
```

### 2. Write collectors using your `require_*` helper

```rust
async fn collect(&self, pool: &dyn DatabasePool) -> Result<Snapshot, CollectorError> {
    let mysql = require_mysql(pool)?;
    // ... MySQL-specific query logic
}
```

### 3. Wire it up

Wrap your pool when creating a scheduler: `Arc::new(MySqlPool(raw_pool))`. The scheduler, collectors, and shipping pipeline work identically for all database types.

## Running Tests

```bash
cargo test        # all tests
cargo fmt --check # formatting
cargo build       # compile check
```
