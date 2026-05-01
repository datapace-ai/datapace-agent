# Configuration Guide

The Datapace Agent can be configured through environment variables, a YAML configuration file, or a combination of both.

## Quick Start

The minimum required configuration:

```bash
export DATAPACE_API_KEY=your_api_key
export DATAPACE_SIGNING_SECRET=your_signing_secret
export DATABASE_URL=postgres://user:password@host:5432/database
datapace-agent
```

## Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATAPACE_API_KEY` | Your Datapace Cloud API key | - | Yes |
| `DATABASE_URL` | Database connection string | - | Yes |
| `DATAPACE_SIGNING_SECRET` | Per-connection HMAC-SHA256 secret used to sign every payload. Provisioned alongside the API key. See [Payload signing](../README.md#payload-signing). | - | Yes |
| `DATAPACE_ENDPOINT` | API endpoint URL | `https://api.datapace.ai/v1/ingest` | No |
| `COLLECTION_INTERVAL` | How often to collect metrics (e.g. `30s`, `1m`, `5m`) | `60s` | No |
| `LOG_LEVEL` | Logging level (`trace`, `debug`, `info`, `warn`, `error`) | `info` | No |
| `LOG_FORMAT` | Log output format (`json`, `pretty`) | `json` | No |
| `DATAPACE_HEALTH_BIND_ADDRESS` | Health server bind address. Set to `0.0.0.0` inside containers to publish the port. | `127.0.0.1` | No |
| `DATAPACE_HEALTH_PORT` | Health server port | `8080` | No |

## Configuration File

For more advanced configuration, create a YAML file:

```yaml
# agent.yaml
datapace:
  api_key: ${DATAPACE_API_KEY}
  signing_secret: ${DATAPACE_SIGNING_SECRET}
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
  interval_secs: 60
  metrics:
    - query_stats
    - table_stats
    - index_stats
    - settings
    - schema_metadata

logging:
  level: info
  format: json

health:
  enabled: true
  bind_address: 127.0.0.1
  port: 8080
  path: /health
```

> The old PostgreSQL-specific metric names (`pg_stat_statements`, `pg_stat_user_tables`, `pg_stat_user_indexes`, `pg_settings`) are still accepted as aliases for the canonical agnostic names above.

Run with config file:

```bash
datapace-agent --config agent.yaml
```

## Configuration Options

### Datapace Section

| Option | Type | Description |
|--------|------|-------------|
| `api_key` | string | Your Datapace Cloud API key (required) |
| `signing_secret` | string | Per-connection HMAC-SHA256 secret used to sign payloads (required). See [Payload signing](../README.md#payload-signing). |
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
| `interval_secs` | integer | Collection interval in seconds (minimum 10). For string-duration syntax (e.g. `30s`, `1m`), use the `COLLECTION_INTERVAL` env var instead. |
| `metrics` | list | Metrics to collect |

Available metrics (database-agnostic names):
- `query_stats` - Query performance statistics (alias: `pg_stat_statements`)
- `table_stats` - Table-level statistics (alias: `pg_stat_user_tables`)
- `index_stats` - Index usage statistics (alias: `pg_stat_user_indexes`)
- `settings` - Database configuration (alias: `pg_settings`)
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
| `bind_address` | string | Bind address for the health server (`127.0.0.1` by default; set to `0.0.0.0` inside containers if you want to publish the port) |
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

### Authentication / signature errors (`401`)

If the agent boots but the cloud rejects requests with a `401`, run the agent with `LOG_LEVEL=debug` and inspect the response body:

- `error_kind=invalid_signature` — the HMAC didn't verify. The `DATAPACE_SIGNING_SECRET` doesn't match what the platform stores for this connection. Confirm the value matches what was shown in the dashboard at connection-creation time, or rotate the connection in the dashboard to get a new pair.
- `StaleTimestamp` — the host clock has drifted by more than ±5 minutes from server time. Sync via NTP.
- `401 Unauthorized` with no body — check `DATAPACE_API_KEY`.

If `DATAPACE_SIGNING_SECRET` is missing or empty the agent refuses to start with an actionable error before any request is made — see the [Payload signing](../README.md#payload-signing) section.

The signature scheme, for reference: `X-Signature = lowercase_hex( HMAC-SHA256( signing_secret, "<unix_timestamp>.<raw_body>" ) )`, with `X-Signature-Timestamp` carrying the unix-seconds timestamp.
