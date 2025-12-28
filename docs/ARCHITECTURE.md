# Architecture

This document describes the high-level architecture of the Datapace Agent.

## Overview

The Datapace Agent is a lightweight daemon that collects database metrics and sends them to Datapace Cloud for analysis. It's designed to be:

- **Single purpose**: Collect metrics, send to cloud
- **Database-agnostic core**: Plugin architecture for different databases
- **Lightweight**: Minimal dependencies, small Docker image (~10MB)

## System Architecture

```
┌─────────────────────────────────────────────────────┐
│                   User's Environment                 │
│  ┌───────────────────────────────────────────────┐  │
│  │            datapace-agent (Docker)            │  │
│  │  ┌─────────┐  ┌──────────┐  ┌─────────────┐  │  │
│  │  │Collector│→ │ Payload  │→ │  Uploader   │  │  │
│  │  │         │  │          │  │ (to Cloud)  │  │  │
│  │  └────┬────┘  └──────────┘  └──────┬──────┘  │  │
│  └───────│───────────────────────────│──────────┘  │
│          │                           │              │
│          ▼                           │              │
│  ┌───────────────┐                   │              │
│  │   PostgreSQL  │                   │              │
│  │  (RDS/Aurora/ │                   │              │
│  │  Supabase/etc)│                   │              │
│  └───────────────┘                   │              │
└──────────────────────────────────────│──────────────┘
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
├── collector/        # Database collectors
│   ├── mod.rs        # Collector trait definition
│   └── postgres/     # PostgreSQL implementation
│       ├── mod.rs    # Main collector
│       ├── queries.rs# SQL queries
│       └── providers.rs # Provider detection
├── payload/          # Data normalization
│   └── mod.rs        # Payload schema
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
    async fn collect(&self) -> Result<Payload, CollectorError>;
    async fn test_connection(&self) -> Result<(), CollectorError>;
    fn provider(&self) -> &str;
    fn version(&self) -> Option<&str>;
}
```

All database collectors implement this trait, allowing for:
- Uniform collection interface
- Easy addition of new databases
- Provider-specific optimizations

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

The agent auto-detects database providers:

1. **URL patterns**: Check connection string for provider hints
2. **Extensions**: Query `pg_extension` for provider-specific extensions
3. **Settings**: Check for provider-specific GUC parameters

Supported providers:
- Generic PostgreSQL
- AWS RDS
- AWS Aurora
- Supabase
- Neon

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

### Adding a New Database

1. Create a new module: `src/collector/mysql/`
2. Implement the `Collector` trait
3. Add provider detection logic
4. Update the factory function
5. Add configuration options

### Adding New Metrics

1. Add query to `queries.rs`
2. Add fields to payload structs
3. Update collector to call new query
4. Add configuration option to enable/disable

## Future Considerations

- **MySQL support**: Add MySQL collector
- **Query plans**: Collect EXPLAIN output for slow queries
- **Schema diff**: Detect and report schema changes
- **Custom metrics**: User-defined SQL queries
