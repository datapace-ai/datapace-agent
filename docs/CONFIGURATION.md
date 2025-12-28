# Configuration Guide

The Datapace Agent can be configured through environment variables, a YAML configuration file, or a combination of both.

## Quick Start

The minimum required configuration:

```bash
export DATAPACE_API_KEY=your_api_key
export DATABASE_URL=postgres://user:password@host:5432/database
datapace-agent
```

## Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATAPACE_API_KEY` | Your Datapace API key | - | Yes |
| `DATABASE_URL` | PostgreSQL connection string | - | Yes |
| `DATAPACE_ENDPOINT` | API endpoint URL | `https://api.datapace.ai/v1/ingest` | No |
| `COLLECTION_INTERVAL` | How often to collect metrics | `60s` | No |
| `LOG_LEVEL` | Logging level | `info` | No |
| `LOG_FORMAT` | Log output format | `json` | No |

## Configuration File

For more advanced configuration, create a YAML file:

```yaml
# agent.yaml
datapace:
  api_key: ${DATAPACE_API_KEY}
  endpoint: https://api.datapace.ai/v1/ingest
  timeout: 30
  retries: 3

database:
  url: ${DATABASE_URL}
  provider: auto
  pool:
    min_connections: 1
    max_connections: 5
    acquire_timeout: 30

collection:
  interval: 60s
  metrics:
    - pg_stat_statements
    - pg_stat_user_tables
    - pg_stat_user_indexes
    - pg_settings
    - schema_metadata

logging:
  level: info
  format: json

health:
  enabled: true
  port: 8080
  path: /health
```

Run with config file:

```bash
datapace-agent --config agent.yaml
```

## Configuration Options

### Datapace Section

| Option | Type | Description |
|--------|------|-------------|
| `api_key` | string | Your Datapace API key (required) |
| `endpoint` | string | API endpoint URL |
| `timeout` | integer | Request timeout in seconds |
| `retries` | integer | Number of retry attempts on failure |

### Database Section

| Option | Type | Description |
|--------|------|-------------|
| `url` | string | PostgreSQL connection URL (required) |
| `provider` | string | Database provider (`auto`, `generic`, `rds`, `aurora`, `supabase`, `neon`) |
| `pool.min_connections` | integer | Minimum connections in pool |
| `pool.max_connections` | integer | Maximum connections in pool |
| `pool.acquire_timeout` | integer | Connection acquire timeout in seconds |

### Collection Section

| Option | Type | Description |
|--------|------|-------------|
| `interval` | duration | Collection interval (e.g., `30s`, `1m`, `5m`) |
| `metrics` | list | Metrics to collect |

Available metrics:
- `pg_stat_statements` - Query statistics
- `pg_stat_user_tables` - Table statistics
- `pg_stat_user_indexes` - Index statistics
- `pg_settings` - Database configuration
- `schema_metadata` - Schema structure

### Logging Section

| Option | Type | Description |
|--------|------|-------------|
| `level` | string | Log level (`trace`, `debug`, `info`, `warn`, `error`) |
| `format` | string | Output format (`json`, `pretty`) |

### Health Section

| Option | Type | Description |
|--------|------|-------------|
| `enabled` | boolean | Enable health check endpoint |
| `port` | integer | Port for health check server |
| `path` | string | Path for health check endpoint |

## Environment Variable Substitution

You can use `${VAR}` syntax in YAML files to reference environment variables:

```yaml
datapace:
  api_key: ${DATAPACE_API_KEY}

database:
  url: ${DATABASE_URL}
```

## Provider-Specific Configuration

### AWS RDS

```yaml
database:
  url: postgres://user:pass@mydb.xxx.us-east-1.rds.amazonaws.com:5432/postgres
  provider: rds  # or 'auto' for detection
```

### AWS Aurora

```yaml
database:
  url: postgres://user:pass@mydb.cluster-xxx.us-east-1.rds.amazonaws.com:5432/postgres
  provider: aurora
```

### Supabase

```yaml
database:
  url: postgres://postgres:[password]@db.xxx.supabase.co:5432/postgres
  provider: supabase
```

### Neon

```yaml
database:
  url: postgres://user:pass@ep-xxx.us-east-2.aws.neon.tech/neondb
  provider: neon
```

## Docker Configuration

### Using Environment Variables

```bash
docker run -d \
  -e DATAPACE_API_KEY=your_key \
  -e DATABASE_URL=postgres://... \
  -e COLLECTION_INTERVAL=30s \
  ghcr.io/datapace-ai/datapace-agent:latest
```

### Using Config File

```bash
docker run -d \
  -v $(pwd)/agent.yaml:/app/agent.yaml:ro \
  ghcr.io/datapace-ai/datapace-agent:latest \
  --config /app/agent.yaml
```

## Troubleshooting

### Connection Issues

Enable debug logging to see detailed connection information:

```bash
LOG_LEVEL=debug datapace-agent
```

### Test Connection

Test database and cloud connections without starting the collection loop:

```bash
datapace-agent --test-connection
```

### Dry Run

Collect metrics once and print to stdout:

```bash
datapace-agent --dry-run
```
