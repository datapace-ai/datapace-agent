# Security Policy

## Reporting Security Vulnerabilities

If you discover a security vulnerability, please report it responsibly:

1. **Do not** open a public GitHub issue
2. Email security@datapace.ai with details
3. Include steps to reproduce if possible
4. Allow reasonable time for a fix before public disclosure

## Security Model

### Database Access

The Datapace Agent requires **read-only** access to your database. It only queries system catalogs and statistics views:

**Required Permissions:**
- `SELECT` on `pg_catalog` schema
- `SELECT` on `pg_stat_*` views
- `SELECT` on `information_schema`

**Recommended Setup:**
```sql
-- Create dedicated read-only user
CREATE USER datapace_agent WITH PASSWORD 'secure_password';

-- Grant access to statistics
GRANT pg_read_all_stats TO datapace_agent;

-- Grant schema access (for metadata collection)
GRANT USAGE ON SCHEMA public TO datapace_agent;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO datapace_agent;
```

### Data Collection

The agent collects **only metadata and statistics**, never actual row data:

**Collected:**
- Query patterns (from `pg_stat_statements`, normalized - no literals)
- Table and index statistics (counts, sizes, access patterns)
- Database configuration settings
- Schema structure (table names, column types, indexes)

**Never Collected:**
- Actual data rows
- Query parameters or literals
- Passwords or credentials
- Personal or sensitive data

### Network Security

**Outbound Connections:**
- All connections to Datapace Cloud use TLS 1.2+
- Certificate validation is enforced
- API keys are sent in HTTP headers, never in URLs

**Inbound Connections:**
- Optional health check endpoint on configurable port
- No other inbound connections required

### Credential Handling

**API Keys:**
- Stored only in memory during runtime
- Can be provided via environment variables (recommended) or config file
- Never logged, even at debug level

**Database Credentials:**
- Passed via connection URL
- Never logged or sent to cloud
- Use environment variables: `DATABASE_URL=postgres://...`

### Container Security

**Docker Image:**
- Based on minimal Alpine Linux
- Runs as non-root user (`datapace:datapace`)
- No shell access in production image
- Regularly scanned for vulnerabilities

**Recommended Deployment:**
```yaml
services:
  datapace-agent:
    image: ghcr.io/datapace-ai/datapace-agent:latest
    user: "1000:1000"
    read_only: true
    security_opt:
      - no-new-privileges:true
```

## Dependency Security

We use automated tools to monitor dependencies:

- `cargo audit` - Checks for known vulnerabilities
- Dependabot - Automatic security updates
- GitHub Security Advisories - Vulnerability alerts

All dependencies are regularly updated to patch security issues.

## Secure Configuration

### Production Recommendations

1. **Use environment variables for secrets:**
   ```bash
   export DATAPACE_API_KEY=your_key
   export DATABASE_URL=postgres://...
   ```

2. **Restrict database user permissions:**
   ```sql
   -- Only grant what's needed
   GRANT pg_read_all_stats TO datapace_agent;
   -- No INSERT, UPDATE, DELETE permissions
   ```

3. **Use TLS for database connections:**
   ```bash
   DATABASE_URL=postgres://user:pass@host:5432/db?sslmode=require
   ```

4. **Run as non-root:**
   ```bash
   docker run --user 1000:1000 ghcr.io/datapace-ai/datapace-agent
   ```

5. **Enable health checks for monitoring:**
   ```yaml
   health:
     enabled: true
     port: 8080
   ```

### Network Isolation

For maximum security, deploy the agent in the same network as your database:

```
┌─────────────────────────────────────┐
│           Private Network           │
│  ┌──────────┐    ┌──────────────┐  │
│  │ Database │◄───│   Agent      │  │
│  └──────────┘    └──────┬───────┘  │
└─────────────────────────│───────────┘
                          │ HTTPS
                          ▼
              ┌────────────────────┐
              │  Datapace Cloud    │
              └────────────────────┘
```

## Compliance

The agent is designed with compliance in mind:

- **GDPR**: No personal data collected
- **SOC 2**: Secure credential handling, TLS encryption
- **HIPAA**: Can be configured for healthcare environments

## Security Updates

Subscribe to security announcements:
- Watch the GitHub repository
- Join our Discord for security alerts
- Check release notes for security fixes

## Audit Logging

Enable debug logging to audit agent activity:

```yaml
logging:
  level: debug
  format: json
```

All database queries and API calls are logged (without sensitive data).
