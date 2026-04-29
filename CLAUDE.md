# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Release pipeline — read this before touching `Cargo.toml` or `CHANGELOG.md`

This repo is wired to **[release-plz](https://release-plz.dev/)**, which automates version bumps, CHANGELOG entries, and tagging from Conventional Commits on `main`. Two rules to avoid breaking it:

1. **Never edit `version` in `Cargo.toml`** by hand. release-plz manages it via the auto-generated "Release vX.Y.Z" PR.
2. **Never edit the `[Unreleased]` section in `CHANGELOG.md`** by hand. It is regenerated from commit messages on every push to `main`. Hand-written entries will be overwritten.

Adding noteworthy text to the changelog is done **through the commit message body** — release-plz's git-cliff parser reads commit bodies and folds them into the relevant section (mapped from Conventional Commits type → Keep a Changelog section in `release-plz.toml`).

## Conventional Commits are enforced

PR titles must start with one of: `feat`, `fix`, `perf`, `revert`, `docs`, `refactor`, `style`, `test`, `chore`, `ci`, `build`. CI workflow `.github/workflows/pr-title.yml` blocks the merge button otherwise. Append `!` to the type for breaking changes (e.g. `feat!: drop MongoDB 4.x`).

PRs are squash-merged — GitHub propagates the PR title to the squash commit by default. Don't edit the title in the merge dialog (the CI check ran on the PR title, not the commit message).

Only `feat`, `fix`, `perf`, and `revert` trigger version bumps. `chore`/`docs`/`ci` ship under "no version change" — useful for tooling work that needs to land before a release.

## Common commands

A `Makefile` wraps the canonical workflows. Use it directly:

```bash
make build           # cargo build --release
make test            # cargo test (all unit + integration tests)
make test-verbose    # cargo test -- --nocapture
make lint            # cargo clippy with -D warnings
make fmt             # cargo fmt
make run             # cargo run --release (requires DATABASE_URL + DATAPACE_API_KEY)
make run-debug       # RUST_LOG=debug cargo run
make watch           # cargo watch -x run (needs cargo-watch)
make docker          # docker build -t datapace-agent .
```

Single-test invocation: `cargo test <test_name_substring>` — e.g. `cargo test schema::tests::flat_document`. Integration tests under `tests/integration/` use testcontainers and require Docker running locally; they're auto-skipped if Docker isn't available.

## Architecture

The agent is a single async tokio binary with a clean trait-based collector abstraction. Reading `src/lib.rs` first is the fastest way to map the modules.

```
main.rs ─▶ create_collector(url, provider)              # factory in src/collector/mod.rs
            │
            ├─▶ PostgresCollector  (src/collector/postgres/)   # stable
            └─▶ Other collectors   (e.g. mongodb/, mysql/)     # add via the same trait
            │
            ▼
          Payload  (src/payload/mod.rs)                  # database-agnostic struct
            │  query_stats, table_stats, index_stats,
            │  settings, schema (TableMetadata + IndexMetadata)
            ▼
          Uploader (src/uploader/mod.rs)                # HMAC-signed POST → Datapace Cloud
            │
          Scheduler (src/scheduler/mod.rs)              # tick loop + graceful shutdown
            │
          Health   (src/health/mod.rs)                  # loopback /health for k8s probes
```

Critical contracts:

- **`Collector` trait** (`src/collector/mod.rs`) — every database backend implements `collect()` → `Payload` plus `test_connection`, `provider`, `version`, `database_type`. Errors map onto the shared `CollectorError` taxonomy (`ConnectionError` / `PermissionError` / `QueryError` / etc.) — preserve this mapping in new collectors.
- **`Payload` shape** (`src/payload/mod.rs`) — extending the payload schema must be **additive and `Option<T>`** with `#[serde(skip_serializing_if = "Option::is_none")]`, plus `#[derive(Default)]` on the struct, so existing collectors don't have to change. The platform-side ingest is JSONB-tolerant; new fields land non-breakingly.
- **`DatabaseType::from_url`** (`src/config/mod.rs`) — URL pattern → `DatabaseType` enum is the single dispatch point. New collectors register here and in the factory match in `src/collector/mod.rs`.
- **Provider detection** lives per-collector (e.g. `src/collector/postgres/providers.rs`) — URL fast path then runtime probe; default to `"generic"`.

## CI workflows

- `.github/workflows/ci.yml` — runs on push/PR: fmt + clippy + tests (PG service container) + multi-platform build artifacts + Docker buildx smoke test (no push) + cargo-audit
- `.github/workflows/pr-title.yml` — Conventional Commits check
- `.github/workflows/release-plz.yml` — on push to `main`, opens/refreshes the release PR
- `.github/workflows/release.yml` — fires on `v*` tag push (which release-plz creates): builds binaries for 4 platforms, pushes multi-arch image to `ghcr.io/datapace-ai/datapace-agent`, creates a GitHub Release with checksums

For end-to-end automation, the repo needs a `RELEASE_PLZ_TOKEN` secret (PAT with `contents: write` + `workflows: read`) — without it, tags pushed by `GITHUB_TOKEN` cannot trigger downstream workflows.

## More detail

- [`README.md`](README.md) — user-facing config, supported databases, deployment
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — full commit convention, release flow diagram, `RELEASE_PLZ_TOKEN` setup
- [`CHANGELOG.md`](CHANGELOG.md) — release history (don't hand-edit `[Unreleased]`)
