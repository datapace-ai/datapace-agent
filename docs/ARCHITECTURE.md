# Architecture

This document describes the high-level architecture of the Datapace Agent.

## Overview

The Datapace Agent is a lightweight daemon that collects database metrics and sends them to Datapace Cloud for analysis. It's designed to be:

- **Single purpose**: Collect metrics, send to cloud
- **Database-agnostic core**: Plugin architecture for different databases
- **Lightweight**: Minimal dependencies, small Docker image (~10MB)

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         datapace-agent                           │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Collector Factory                      │   │
│  │              create_collector(db_type, url)               │   │
│  └─────────────────────────┬────────────────────────────────┘   │
│                            │                                     │
│          ┌─────────────────┼─────────────────┐                  │
│          ▼                 ▼                 ▼                  │
│   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐          │
│   │  PostgreSQL │   │    MySQL    │   │   MongoDB   │          │
│   │  Collector  │   │  Collector  │   │  Collector  │          │
│   │   (stable)  │   │  (planned)  │   │  (planned)  │          │
│   └──────┬──────┘   └──────┬──────┘   └──────┬──────┘          │
│          │                 │                 │                  │
│          └─────────────────┼─────────────────┘                  │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Payload (Database-Agnostic)                  │   │
│  │     query_stats, table_stats, index_stats, settings       │   │
│  └─────────────────────────┬────────────────────────────────┘   │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                       Uploader                            │   │
│  │              POST to api.datapace.ai/v1/ingest            │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼ HTTPS
                          ┌────────────────────────┐
                          │    Datapace Cloud      │
                          │  api.datapace.ai       │
                          └────────────────────────┘
```

## Data Flow

1. **Scheduler** triggers collection at configured intervals
2. **Collector** queries database for metrics and metadata
3. **Payload** normalizes data into a standard schema
4. **Uploader** sends payload to Datapace Cloud via HTTPS

## Module Structure

```
src/
├── main.rs           # CLI entry point, signal handling
├── lib.rs            # Library exports
├── config/           # Configuration loading (YAML, env vars)
│   └── mod.rs        # DatabaseType, MetricType, Provider enums
├── collector/        # Database collectors
│   ├── mod.rs        # Collector trait & factory function
│   ├── postgres/     # PostgreSQL implementation (stable)
│   │   ├── mod.rs    # Main collector
│   │   ├── queries.rs# SQL queries
│   │   └── providers.rs # Provider detection
│   └── mysql/        # MySQL implementation (skeleton)
│       ├── mod.rs    # Main collector
│       ├── queries.rs# SQL queries
│       └── providers.rs # Provider detection
├── payload/          # Data normalization
│   └── mod.rs        # Database-agnostic payload schema
├── uploader/         # Cloud API client
│   └── mod.rs        # HTTP client with retry logic
└── scheduler/        # Collection loop
    └── mod.rs        # Interval-based scheduling
```

## Key Components

### Collector Trait

```rust
#[async_trait]
pub trait Collector: Send + Sync {
    /// Collect all metrics and return a normalized payload
    async fn collect(&self) -> Result<Payload, CollectorError>;

    /// Test the database connection
    async fn test_connection(&self) -> Result<(), CollectorError>;

    /// Get the detected cloud provider (e.g., "rds", "neon", "generic")
    fn provider(&self) -> &str;

    /// Get the database version
    fn version(&self) -> Option<&str>;

    /// Get the database type (Postgres, MySQL, MongoDB, etc.)
    fn database_type(&self) -> DatabaseType;
}
```

All database collectors implement this trait, allowing for:
- Uniform collection interface
- Database type auto-detection from URL
- Easy addition of new databases
- Provider-specific optimizations

### Collector Factory

```rust
pub async fn create_collector(
    database_url: &str,
    provider: Provider,
) -> Result<Box<dyn Collector>, CollectorError> {
    // Auto-detect database type from URL scheme
    let db_type = DatabaseType::from_url(database_url)?;

    match db_type {
        DatabaseType::Postgres => { /* create PostgresCollector */ },
        DatabaseType::Mysql => { /* create MySQLCollector */ },
        DatabaseType::Mongodb => { /* create MongoDBCollector */ },
    }
}
```

### Database-Agnostic Metrics

| MetricType | Description | PostgreSQL | MySQL |
|------------|-------------|------------|-------|
| `query_stats` | Query performance | pg_stat_statements | performance_schema |
| `table_stats` | Table statistics | pg_stat_user_tables | information_schema |
| `index_stats` | Index usage | pg_stat_user_indexes | information_schema |
| `settings` | Configuration | pg_settings | SHOW VARIABLES |
| `schema_metadata` | Schema structure | information_schema | information_schema |

### Payload Schema

The normalized payload structure:

```json
{
  "agent_version": "0.1.0",
  "timestamp": "2024-12-28T10:00:00Z",
  "instance_id": "hash-of-connection",
  "database": {
    "type": "postgres",
    "version": "16.1",
    "provider": "supabase"
  },
  "schema": { ... },
  "query_stats": [ ... ],
  "table_stats": [ ... ],
  "index_stats": [ ... ],
  "settings": { ... }
}
```

### Provider Detection

The agent auto-detects cloud providers for each database type:

1. **URL patterns**: Check connection string for provider hints
2. **Extensions/Variables**: Query database for provider-specific features
3. **Settings**: Check for provider-specific configuration parameters

**Supported Providers by Database:**

| Database | Providers |
|----------|-----------|
| PostgreSQL | Generic, AWS RDS, AWS Aurora, Supabase, Neon |
| MySQL | Generic, AWS RDS, AWS Aurora, Google Cloud SQL, Azure, PlanetScale |
| MongoDB | Generic, MongoDB Atlas, AWS DocumentDB (planned) |

## Technology Choices

### Why Rust?

- **Performance**: Low resource usage, fast startup
- **Safety**: Memory safety, no garbage collection pauses
- **Single binary**: Easy distribution, no runtime dependencies
- **Async**: Excellent async support with Tokio
- **Ecosystem**: Great libraries (sqlx, reqwest, clap)

### Dependencies

| Crate | Purpose |
|-------|---------|
| tokio | Async runtime |
| sqlx | PostgreSQL driver |
| reqwest | HTTP client |
| clap | CLI parsing |
| serde | Serialization |
| tracing | Logging |

## Security Model

1. **Read-only access**: Agent only needs SELECT on system catalogs
2. **No data collection**: Only metadata and statistics, never row data
3. **TLS everywhere**: All cloud communication uses TLS 1.2+
4. **Minimal permissions**: Runs as non-root in container

## Extensibility

For detailed instructions, see **[EXTENDING.md](EXTENDING.md)**.

### Adding a New Database

1. Create a new module: `src/collector/{database}/`
2. Implement the `Collector` trait with all required methods
3. Add provider detection logic for cloud variants
4. Update the factory function in `src/collector/mod.rs`
5. Add `DatabaseType` variant to `src/config/mod.rs`
6. Add URL validation
7. Write tests

### Adding New Metrics

1. Add to `MetricType` enum in `src/config/mod.rs`
2. Add fields to payload structs in `src/payload/mod.rs`
3. Add query to `queries.rs` for each database
4. Update collector to call new query
5. Add configuration option to enable/disable

### Adding New Cloud Providers

1. Update provider detection in `providers.rs`
2. Add URL pattern matching for the provider
3. Optionally collect provider-specific metadata
4. Update `Provider` enum in config

## Future Considerations

- **MySQL support**: Complete MySQL collector implementation
- **MongoDB support**: Add MongoDB collector
- **Redis support**: Add Redis collector
- **Query plans**: Collect EXPLAIN output for slow queries
- **Schema diff**: Detect and report schema changes
- **Custom metrics**: User-defined SQL queries
