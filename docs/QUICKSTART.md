# Quick Start Guide

Get Datapace Agent running in 5 minutes.

## Prerequisites

- Docker installed
- A PostgreSQL database (local or cloud)
- A Datapace API key ([get one here](https://app.datapace.ai/settings/api-keys))

## Step 1: Get Your Connection Details

You'll need:
- **Database URL**: `postgres://user:password@host:port/database`
- **Datapace API Key**: From your Datapace dashboard

## Step 2: Run the Agent

### Option A: Docker (Recommended)

```bash
docker run -d \
  --name datapace-agent \
  -e DATAPACE_API_KEY=your_api_key \
  -e DATABASE_URL=postgres://user:pass@host:5432/database \
  ghcr.io/datapace-ai/datapace-agent:latest
```

### Option B: Docker Compose

Create a `.env` file:
```bash
DATAPACE_API_KEY=your_api_key
DATABASE_URL=postgres://user:pass@host:5432/database
```

Create `docker-compose.yml`:
```yaml
services:
  datapace-agent:
    image: ghcr.io/datapace-ai/datapace-agent:latest
    environment:
      DATAPACE_API_KEY: ${DATAPACE_API_KEY}
      DATABASE_URL: ${DATABASE_URL}
    restart: unless-stopped
```

Run:
```bash
docker-compose up -d
```

### Option C: Binary

```bash
# Download
curl -L https://github.com/datapace-ai/datapace-agent/releases/latest/download/datapace-agent-linux-amd64 -o datapace-agent
chmod +x datapace-agent

# Run
export DATAPACE_API_KEY=your_api_key
export DATABASE_URL=postgres://user:pass@host:5432/database
./datapace-agent
```

## Step 3: Verify It's Working

Check the agent logs:
```bash
docker logs datapace-agent
```

You should see:
```
{"level":"info","message":"Starting Datapace Agent","version":"0.1.0"}
{"level":"info","message":"Connected to database","provider":"generic","version":"PostgreSQL 16.1"}
{"level":"info","message":"Starting metrics collection loop","interval_secs":60}
{"level":"info","message":"Metrics uploaded successfully"}
```

## Step 4: View in Datapace

1. Go to [app.datapace.ai](https://app.datapace.ai)
2. Navigate to your project
3. See your database metrics appear within a minute

## Common Connection Strings

### AWS RDS
```
postgres://username:password@mydb.abc123.us-east-1.rds.amazonaws.com:5432/postgres
```

### Supabase
```
postgres://postgres:[YOUR-PASSWORD]@db.abcdefgh.supabase.co:5432/postgres
```

### Neon
```
postgres://username:password@ep-cool-river-123456.us-east-2.aws.neon.tech/neondb?sslmode=require
```

### Local PostgreSQL
```
postgres://postgres:password@localhost:5432/mydb
```

## Troubleshooting

### Test Your Connection

```bash
docker run --rm \
  -e DATAPACE_API_KEY=your_key \
  -e DATABASE_URL=postgres://... \
  ghcr.io/datapace-ai/datapace-agent:latest \
  --test-connection
```

### Enable Debug Logging

```bash
docker run -d \
  -e DATAPACE_API_KEY=your_key \
  -e DATABASE_URL=postgres://... \
  -e LOG_LEVEL=debug \
  ghcr.io/datapace-ai/datapace-agent:latest
```

### Common Issues

**"Connection refused"**
- Check that PostgreSQL is accessible from where the agent runs
- Verify the host and port in your connection string

**"Permission denied"**
- The database user needs `pg_read_all_stats` role
- Run: `GRANT pg_read_all_stats TO your_user;`

**"Invalid API key"**
- Check your API key in the Datapace dashboard
- Ensure no extra spaces or quotes

## Next Steps

- [Full Configuration Guide](CONFIGURATION.md)
- [Security Best Practices](SECURITY.md)
- [Contributing](CONTRIBUTING.md)

## Need Help?

- [GitHub Issues](https://github.com/datapace-ai/datapace-agent/issues)
- [Discord Community](https://discord.gg/datapace)
- Email: support@datapace.ai
