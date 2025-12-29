# Datapace Agent

A lightweight, open-source agent that collects database metrics and sends them to [Datapace Cloud](https://datapace.ai).

[![CI](https://github.com/datapace-ai/datapace-agent/actions/workflows/ci.yml/badge.svg)](https://github.com/datapace-ai/datapace-agent/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

## Features

- **Lightweight**: Single binary, minimal resource usage (~10MB Docker image)
- **PostgreSQL Support**: Full support for PostgreSQL with provider-specific optimizations
- **Auto-Detection**: Automatically detects RDS, Aurora, Supabase, Neon, and other providers
- **Secure**: Read-only database access, TLS encryption, no sensitive data collection
- **Easy Deployment**: Docker-first with simple environment variable configuration

## Quick Start

### Using Docker (Recommended)

```bash
docker run -d \
  --name datapace-agent \
  -e DATAPACE_API_KEY=your_api_key \
  -e DATABASE_URL=postgres://user:pass@host:5432/dbname \
  ghcr.io/datapace-ai/datapace-agent:latest
```

### Using Docker Compose

```yaml
services:
  datapace-agent:
    image: ghcr.io/datapace-ai/datapace-agent:latest
    environment:
      DATAPACE_API_KEY: ${DATAPACE_API_KEY}
      DATABASE_URL: ${DATABASE_URL}
    restart: unless-stopped
```

### Using Binary

```bash
# Download the latest release
curl -L https://github.com/datapace-ai/datapace-agent/releases/latest/download/datapace-agent-linux-amd64 -o datapace-agent
chmod +x datapace-agent

# Run with environment variables
export DATAPACE_API_KEY=your_api_key
export DATABASE_URL=postgres://user:pass@host:5432/dbname
./datapace-agent
```

## Configuration

The agent can be configured via environment variables or a YAML config file.

### Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `DATAPACE_API_KEY` | Your Datapace API key | Yes |
| `DATABASE_URL` | PostgreSQL connection string | Yes |
| `DATAPACE_ENDPOINT` | API endpoint (default: `https://api.datapace.ai`) | No |
| `COLLECTION_INTERVAL` | Metrics collection interval (default: `60s`) | No |
| `LOG_LEVEL` | Log level: `debug`, `info`, `warn`, `error` (default: `info`) | No |
| `LOG_FORMAT` | Log format: `json`, `pretty` (default: `json`) | No |

### Config File

```yaml
# agent.yaml
datapace:
  api_key: ${DATAPACE_API_KEY}
  endpoint: https://api.datapace.ai/v1/ingest

database:
  url: ${DATABASE_URL}
  provider: auto  # auto, rds, aurora, supabase, neon, generic

collection:
  interval_secs: 60
  metrics:
    - query_stats       # Query performance (pg_stat_statements, performance_schema)
    - table_stats       # Table statistics
    - index_stats       # Index usage
    - settings          # Database configuration
    - schema_metadata   # Schema structure
```

> **Note**: The old PostgreSQL-specific metric names (`pg_stat_statements`, etc.) are still supported for backward compatibility.

Run with config file:

```bash
./datapace-agent --config agent.yaml
```

## Database Permissions

The agent requires minimal read-only permissions:

```sql
-- Create a dedicated user for the agent
CREATE USER datapace_agent WITH PASSWORD 'secure_password';

-- Grant access to statistics views
GRANT pg_read_all_stats TO datapace_agent;

-- Grant access to schema information
GRANT USAGE ON SCHEMA public TO datapace_agent;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO datapace_agent;

-- For pg_stat_statements (if enabled)
GRANT EXECUTE ON FUNCTION pg_stat_statements_reset TO datapace_agent;
```

## Supported Databases

| Database | Status | Provider Detection |
|----------|--------|-------------------|
| PostgreSQL | Stable | Generic, RDS, Aurora, Supabase, Neon |
| MySQL/MariaDB | Coming Soon | Generic, RDS, Aurora, Cloud SQL, Azure, PlanetScale |
| MongoDB | Planned | Generic, Atlas, DocumentDB |
| Redis | Planned | Generic, ElastiCache |
| SQL Server | Planned | Generic, RDS, Azure SQL |

### Database-Agnostic Metrics

The agent collects these standard metrics across all supported databases:

| Metric | PostgreSQL Source | MySQL Source | Description |
|--------|-------------------|--------------|-------------|
| `query_stats` | pg_stat_statements | performance_schema | Query performance statistics |
| `table_stats` | pg_stat_user_tables | information_schema | Table-level statistics |
| `index_stats` | pg_stat_user_indexes | information_schema | Index usage statistics |
| `settings` | pg_settings | SHOW VARIABLES | Database configuration |
| `schema_metadata` | information_schema | information_schema | Schema structure |

## Building from Source

### Prerequisites

- Rust 1.75 or later
- OpenSSL development libraries (or use `rustls`)

### Build

```bash
# Clone the repository
git clone https://github.com/datapace-ai/datapace-agent.git
cd datapace-agent

# Build release binary
cargo build --release

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- --config agent.yaml
```

### Docker Build

```bash
# Build Docker image
docker build -t datapace-agent .

# Run locally
docker run -e DATAPACE_API_KEY=key -e DATABASE_URL=postgres://... datapace-agent
```

## Architecture

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
│   │   (stable)  │   │(coming soon)│   │  (planned)  │          │
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
```

### Key Components

| Component | Description |
|-----------|-------------|
| **Collector Trait** | Interface for database-specific metric collection |
| **Collector Factory** | Auto-detects database type from URL and creates appropriate collector |
| **Payload** | Normalized, database-agnostic data structure |
| **Uploader** | Sends data to Datapace Cloud with retry logic |
| **Scheduler** | Periodic collection loop with graceful shutdown |

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](docs/CONTRIBUTING.md) for guidelines.

### Adding New Database Support

Want to add support for MySQL, MongoDB, or another database? See our **[Extension Guide](docs/EXTENDING.md)** for step-by-step instructions.

The collector architecture makes it easy to add new databases:

1. Create a new collector module (`src/collector/mysql/`)
2. Implement the `Collector` trait
3. Add provider detection for cloud variants
4. Register in the collector factory

**Databases we'd love help with:**
- MySQL / MariaDB
- MongoDB
- Redis
- Microsoft SQL Server
- ClickHouse
- CockroachDB
- TimescaleDB

### Development Setup

```bash
# Install development dependencies
cargo install cargo-watch cargo-audit

# Run in watch mode
cargo watch -x run

# Run lints
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Security

- The agent only collects metadata and statistics, never actual row data
- All communication with Datapace Cloud uses TLS encryption
- API keys are scoped to individual projects
- See [SECURITY.md](docs/SECURITY.md) for our security policy

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.

## Support

- [Documentation](https://docs.datapace.ai/agent)
- [GitHub Issues](https://github.com/datapace-ai/datapace-agent/issues)
- [Discord Community](https://discord.gg/datapace)
