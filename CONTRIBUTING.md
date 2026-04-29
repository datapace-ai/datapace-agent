# Contributing to Datapace Agent

Thanks for considering a contribution! This document covers the commit
convention and the release flow — the build/test instructions live in the
[README](README.md).

## Commit / PR convention

We use [Conventional Commits](https://www.conventionalcommits.org/) for one
reason: the changelog and version bumps are automated from the commit history.
A non-conformant PR title will be **blocked by CI** (`PR Title` workflow), so
keep your titles in this shape:

```
<type>(<optional scope>): <subject>
```

| Type       | When to use it                               | Bumps version? |
| ---------- | -------------------------------------------- | -------------- |
| `feat`     | A new user-visible feature                   | minor          |
| `fix`      | A bug fix                                    | patch          |
| `perf`     | Performance-only change                      | patch          |
| `revert`   | Reverts a previous commit                    | patch          |
| `docs`     | Documentation only                           | no             |
| `refactor` | Code change without behavioural change       | no             |
| `style`    | Whitespace, formatting, lints                | no             |
| `test`     | Test additions / refactors                   | no             |
| `chore`    | Repo maintenance, deps, build config         | no             |
| `ci`       | CI-only changes                              | no             |
| `build`    | Build-system changes                         | no             |

**Breaking changes** — append `!` to the type or include a `BREAKING CHANGE:`
footer. Either form bumps the **major** version.

```
feat!: drop support for MongoDB 4.x

BREAKING CHANGE: minimum supported MongoDB is now 5.0.
```

### Squash-merge UX

We squash-merge PRs. GitHub defaults the squash commit message to the PR
title, so as long as your PR title is Conventional Commits format, the
commit on `main` will be too. **Don't edit the squash title at merge time** —
the CI check only runs against the PR title, not the merge dialog.

## Release flow

Releases are fully automated. You don't run `cargo release`, you don't bump
`Cargo.toml` by hand, and you don't tag manually.

```
                     ┌──────────────────────────────────────┐
  PR with `feat:` ─▶ │ release-plz watches main             │
                     │ Opens / updates a "Release v0.X.Y" PR │
                     │ that bumps Cargo.toml + CHANGELOG.md  │
                     └─────────────────┬────────────────────┘
                                       │ maintainer merges
                                       ▼
                     ┌──────────────────────────────────────┐
                     │ release-plz tags vX.Y.Z              │
                     └─────────────────┬────────────────────┘
                                       │ tag push
                                       ▼
                     ┌──────────────────────────────────────┐
                     │ release.yml builds:                  │
                     │  • 4 platform binaries               │
                     │  • multi-arch GHCR image             │
                     │  • GitHub Release w/ checksums       │
                     └──────────────────────────────────────┘
```

### As a contributor

- Open a PR. Use a Conventional Commits title.
- The `PR Title` check blocks the merge button until the title is valid.
- Once merged, you don't need to do anything — release-plz will pick up the
  commit on its next run.

### As a maintainer

- Watch for the auto-generated **"chore: release vX.Y.Z"** PR.
- Sanity-check the proposed CHANGELOG diff and version bump (occasionally
  you'll want to bump major instead of minor — edit the PR if so).
- Merge it. The tag and release fire automatically.

### One-time setup: `RELEASE_PLZ_TOKEN`

For end-to-end automation, the repo needs a `RELEASE_PLZ_TOKEN` secret —
a fine-grained Personal Access Token with `contents: write` and
`workflows: read` scoped to this repo. Without it, tags pushed by
release-plz won't trigger `release.yml` (a GitHub safety policy: actions
authenticated with the default `GITHUB_TOKEN` cannot fire other workflows).

If the secret isn't configured, the workflow falls back to `GITHUB_TOKEN` —
release-plz still opens/merges the release PR, but you'll need to push
the tag manually once to kick off the binary + GHCR build:

```bash
git fetch --tags
git push origin v0.X.Y
```

### Pre-releases

Tags containing `alpha`, `beta`, or `rc` (e.g. `v0.3.0-rc.1`) are auto-marked
as pre-releases by `release.yml`. release-plz won't propose pre-release tags
on its own — push them manually if you need one.

## Local development

See [README.md](README.md#development) for build and test commands. The
short version:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features
cargo test
```

CI runs the same three on every PR.

## What to do with the `[Unreleased]` section in CHANGELOG.md

Don't edit it by hand. release-plz regenerates it from commits on every push
to `main` and rolls it into the new versioned section when the release PR
merges. If you really need to add an unreleased note that doesn't map onto
a commit (e.g. an external dependency change worth flagging), edit the entry
**inside** the relevant Conventional Commits commit message body so it ends
up in the changelog naturally.
