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
  -e DATAPACE_SIGNING_SECRET=your_signing_secret \
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
      DATAPACE_SIGNING_SECRET: ${DATAPACE_SIGNING_SECRET}
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
export DATAPACE_SIGNING_SECRET=your_signing_secret
export DATABASE_URL=postgres://user:pass@host:5432/dbname
./datapace-agent
```

> Both `DATAPACE_API_KEY` and `DATAPACE_SIGNING_SECRET` are issued together in the Datapace dashboard when you create a connection — see [Payload signing](#payload-signing) for why two distinct keys are needed. The repo's [`docker-compose.yml`](docker-compose.yml) is the canonical reference for a complete env-var setup.

## Configuration

The agent can be configured via environment variables or a YAML config file.

### Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `DATAPACE_API_KEY` | Your Datapace Cloud API key | Yes |
| `DATABASE_URL` | Database connection string (`postgres://…`, `mongodb://…`) | Yes |
| `DATAPACE_SIGNING_SECRET` | Per-connection HMAC-SHA256 secret used to sign every payload. Provisioned alongside the API key in the Datapace dashboard at connection-creation time. See [Payload signing](#payload-signing). | Yes |
| `DATAPACE_ENDPOINT` | Override the cloud endpoint (default: `https://api.datapace.ai/v1/ingest`) | No |
| `COLLECTION_INTERVAL` | Metrics collection interval, e.g. `30s`, `1m`, `5m` (default: `60s`) | No |
| `LOG_LEVEL` | Log level: `trace`, `debug`, `info`, `warn`, `error` (default: `info`) | No |
| `LOG_FORMAT` | Log format: `json`, `pretty` (default: `json`) | No |
| `DATAPACE_HEALTH_BIND_ADDRESS` | Health server bind address (default: `127.0.0.1`). Set to `0.0.0.0` inside containers if you want to publish the health port. | No |
| `DATAPACE_HEALTH_PORT` | Health server port (default: `8080`) | No |

### Payload signing

Every request to Datapace Cloud is signed by the agent and verified by the platform:

```
X-Signature           = lowercase_hex( HMAC-SHA256( signing_secret, "<unix_timestamp>.<raw_body>" ) )
X-Signature-Timestamp = unix_timestamp (seconds)
Authorization         = Bearer <api_key>
```

The platform requires the timestamp within ±300 s of server time and uses constant-time comparison to verify the signature.

**Why two separate keys?** The API key and the signing secret play different roles, even though the platform issues both. The API key authenticates the connection and travels in the `Authorization` header on every request — so it is observable to any TLS-terminating intermediary on the path (proxies, WAFs, gateways, sidecars) and to platform-side request logs. The signing secret never travels; only the HMAC output does, and HMAC is one-way. If the signing secret were the same as the API key, every middlebox that sees the API key would also be able to forge valid signatures, and the integrity layer would be ceremonial.

**Where do I get it?** The signing secret is generated and shown to you **once** in the Datapace dashboard at connection-creation time, and stored AES-256-GCM-encrypted server-side. Lost secrets cannot be recovered, only rotated.

### Config File

```yaml
# agent.yaml
datapace:
  api_key: ${DATAPACE_API_KEY}
  signing_secret: ${DATAPACE_SIGNING_SECRET}
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

### Relational Databases

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **PostgreSQL** | Stable | Generic, AWS RDS, Aurora, Supabase, Neon |
| **MySQL/MariaDB** | Coming Soon | Generic, AWS RDS, Aurora, Cloud SQL, Azure, PlanetScale |
| **SQL Server** | Planned | Generic, AWS RDS, Azure SQL |
| **Oracle** | Planned | Generic, Oracle Cloud, AWS RDS |
| **IBM DB2** | Planned | Generic, IBM Cloud |

### Document Databases

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **MongoDB** | Stable | Generic, MongoDB Atlas, AWS DocumentDB |
| **Couchbase** | Planned | Generic, Couchbase Cloud |
| **Azure Cosmos DB** | Planned | Azure |

### Analytics & Search

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **Elasticsearch** | Planned | Generic, Elastic Cloud, AWS OpenSearch |
| **ClickHouse** | Planned | Generic, ClickHouse Cloud |
| **Snowflake** | Planned | Snowflake |
| **BigQuery** | Planned | Google Cloud |
| **Redshift** | Works* | AWS (*PostgreSQL-compatible) |

### Key-Value & Cache

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **Redis** | Planned | Generic, AWS ElastiCache, Upstash |
| **DynamoDB** | Planned | AWS |

### Time-Series

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **TimescaleDB** | Works* | Generic, Timescale Cloud (*PostgreSQL-compatible) |
| **InfluxDB** | Planned | Generic, InfluxDB Cloud |

### NewSQL (PostgreSQL-compatible)

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **CockroachDB** | Works* | Generic, Cockroach Cloud (*PostgreSQL-compatible) |
| **YugabyteDB** | Works* | Generic, Yugabyte Cloud (*PostgreSQL-compatible) |
| **TiDB** | Planned | Generic, TiDB Cloud (MySQL-compatible) |

### Vector Databases

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **pgvector** | Works* | PostgreSQL + pgvector extension (*PostgreSQL-compatible) |
| **Pinecone** | Planned | Pinecone |
| **Milvus** | Planned | Generic, Zilliz Cloud |
| **Weaviate** | Planned | Generic, Weaviate Cloud |
| **Qdrant** | Planned | Generic, Qdrant Cloud |
| **Chroma** | Planned | Generic |

### Graph Databases

| Database | Status | Cloud Providers |
|----------|--------|-----------------|
| **Neo4j** | Planned | Generic, Neo4j Aura |
| **Amazon Neptune** | Planned | AWS |
| **ArangoDB** | Planned | Generic, ArangoDB Oasis |
| **JanusGraph** | Planned | Generic |
| **TigerGraph** | Planned | Generic, TigerGraph Cloud |
| **Dgraph** | Planned | Generic, Dgraph Cloud |
| **Memgraph** | Planned | Generic, Memgraph Cloud |

> **Note**: Databases marked "Works*" use the PostgreSQL collector as they are wire-compatible.

### Database-Agnostic Metrics

The agent collects these standard metrics across all supported databases:

| Metric | Description | Example Sources |
|--------|-------------|-----------------|
| `query_stats` | Query performance statistics | pg_stat_statements, performance_schema, $currentOp |
| `table_stats` | Table-level statistics | pg_stat_user_tables, information_schema, collStats |
| `index_stats` | Index usage statistics | pg_stat_user_indexes, $indexStats |
| `settings` | Database configuration | pg_settings, SHOW VARIABLES, db.adminCommand |
| `schema_metadata` | Schema structure | information_schema, listCollections |

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

| Category | Databases |
|----------|-----------|
| Relational | MySQL, MariaDB, SQL Server, Oracle, IBM DB2 |
| Document | MongoDB, Couchbase, Azure Cosmos DB |
| Analytics | Elasticsearch, ClickHouse, Snowflake, BigQuery |
| Key-Value | Redis, DynamoDB |
| Time-Series | InfluxDB |
| NewSQL | TiDB |
| Vector | Pinecone, Milvus, Weaviate, Qdrant, Chroma |
| Graph | Neo4j, Neptune, ArangoDB, JanusGraph, TigerGraph, Dgraph, Memgraph |

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

## Releases & changelog

See [CHANGELOG.md](CHANGELOG.md) for the full release history. Versions follow
[Semantic Versioning](https://semver.org/) and are cut automatically by
[release-plz](https://release-plz.dev/) from [Conventional Commits](https://www.conventionalcommits.org/)
on `main`. PR titles must follow the Conventional Commits format — see
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.

## Support

- [Documentation](https://docs.datapace.ai/agent)
- [GitHub Issues](https://github.com/datapace-ai/datapace-agent/issues)
- [Discord Community](https://discord.gg/datapace)
