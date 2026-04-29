# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintenance is automated via [release-plz](https://release-plz.dev/) — entries
under `[Unreleased]` are derived from [Conventional Commits](https://www.conventionalcommits.org/)
on `main` and are promoted to a new versioned section when a release PR merges.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow.

## [Unreleased]

### Added
- MongoDB collector with migration-grade schema profiling. Samples each
  collection (adaptive size, `$sample`-driven) and recursively flattens nested
  documents and arrays into dot/bracket paths (`address.street`, `photos[].url`).
  Per path: BSON type variance, presence rate, null rate, distinct value count
  (capped), reservoir-sampled examples, array-element flag, and max array
  length. Memory-bounded with per-path/per-collection/global ceilings so even
  pathologically polymorphic collections stay safe.
- Payload extensions: `ColumnMetadata` and `TableMetadata` gained optional
  MongoDB-specific fields (`presence_rate`, `null_rate`, `bson_types`,
  `distinct_count`, `distinct_capped`, `sample_values`, `is_array_element`,
  `array_max_len` on columns; `document_count_sampled`,
  `avg_document_size_bytes`, `storage_size_bytes`, `is_capped`, `is_view`,
  `is_timeseries` on tables). All additive — Postgres collector unaffected.
- Provider detection for MongoDB Atlas, AWS DocumentDB, and Azure Cosmos DB
  Mongo API (URL fast path plus `buildInfo` probe fallback).
- testcontainers-driven MongoDB integration tests covering connection,
  version detection, schema inference of nested + polymorphic fixtures, and
  unique-index emission.

## [0.1.0] - 2026-04-29

The initial release. Lightweight Rust agent that connects to a database,
collects metrics + schema metadata, signs the payload, and pushes it to
Datapace Cloud.

### Added
- **PostgreSQL collector** — query stats (`pg_stat_statements`), table stats
  (`pg_stat_user_tables`), index stats (`pg_stat_user_indexes`), settings
  (`pg_settings`), and schema metadata (tables, columns, indexes).
- **Cloud provider auto-detection** — RDS, Aurora, Supabase, Neon, plus
  generic fallback. Provider-specific metadata captured where available.
- **Database-type detection from URL** — 32 database types recognised across
  relational, document, analytics, key-value, time-series, NewSQL, vector,
  and graph categories. PostgreSQL-compatible variants (TimescaleDB,
  CockroachDB, YugabyteDB, Redshift, pgvector) reuse the PostgreSQL collector.
- **HMAC-SHA256 payload signing** with replay-protection timestamp header.
  Falls back to API key if `DATAPACE_SIGNING_SECRET` is unset, with a warning.
- **Health endpoint** (loopback-only by default) for Kubernetes liveness and
  readiness probes; reports last collection error and instance ID.
- **Heartbeat** posted asynchronously alongside collection so a missed
  collection doesn't drop the agent off the dashboard.
- **Configurable collection interval** with reasonable defaults (60 s).
- **Graceful shutdown** on `SIGINT` / `SIGTERM`; in-flight collection
  completes before exit.
- **Multi-platform release artifacts** — Linux x86_64, macOS x86_64 + arm64,
  Windows x86_64. Multi-arch (`linux/amd64`, `linux/arm64`) Docker image
  published to `ghcr.io/datapace-ai/datapace-agent`.

### Security
- Payloads signed with a distinct signing secret separate from the API key,
  so tamper detection survives a leaked transport token.
- Health endpoint binds to `127.0.0.1` by default to avoid leaking last-error
  diagnostics to the network.
- Read-only database access — no writes, no DDL, no data exfiltration.

[Unreleased]: https://github.com/datapace-ai/datapace-agent/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/datapace-ai/datapace-agent/releases/tag/v0.1.0
