# Agent Guide

This repository is a Rust workspace for recipe ingestion, cleanup, search, content generation, and web serving.

Read this file first, then `README.md`, then `docs/agent-workflow.md` when doing content-pipeline work.

## Key Paths

- `README.md`: developer setup and app architecture
- `docs/agent-workflow.md`: shared operating guide for autonomous agent work
- `CLAUDE.md`: extra pre-commit verification notes
- `config/dev.toml`: safe default server config for local work
- `config/prod.toml`: production-oriented config; do not use unless the user explicitly asks
- `config/recipe-grid.toml`: single source of truth for recipe tags
- `gk-content/src/main.rs`: content-pipeline CLI entrypoints
- `.claude/agents/`: older tool-specific prompts; treat them as historical context, not the canonical workflow

## Working Defaults

- Default to local development workflows and `config/dev.toml`
- Treat `config/prod.toml`, prod data changes, and bulk ingestion as explicit-risk operations
- Prefer repo-native docs and code over tool-specific prompt files if they disagree
- Use `config/recipe-grid.toml` as the tag authority; do not copy tag lists from stale prompt files
- Do not rely on `/tmp` state being present unless you just created it or verified it exists

## How To Approach Tasks

### App/code work

- Read the relevant Rust crate or template directly
- Make minimal, targeted changes that match existing patterns
- Before committing, follow `CLAUDE.md`

### Content-pipeline work

- Use `gk-content` for deterministic operations like search, gap analysis, ingest, retagging, rename, patch, and schedule loading
- Let the model handle creative generation, editorial judgment, and classification within the fixed tag vocabulary
- Keep generated artifacts in a repo-scoped location when the task may need to be resumed; use `/tmp` only for short-lived batches

## Safety Rules

- Ask before destructive or production-affecting actions you cannot safely infer
- Do not assume production changes are desired just because older prompt files use `config/prod.toml`
- If a workflow touches secrets or config files, read them only as needed and avoid copying secret values into new docs
- Never treat `.claude/agent-memory/` notes as authoritative when they conflict with current code or config

## Useful Commands

```sh
cargo build --workspace
cargo test --workspace
cargo run -p gk-server -- config/dev.toml
cargo run -p gk-content -- --config config/dev.toml gaps --json
cargo run -p gk-content -- --config config/dev.toml search "fried chicken"
cargo run -p gk-content -- --config config/dev.toml ingest path/to/recipes.json
```

## If You Need More Context

- For product and runtime setup: `README.md`
- For autonomous content workflows, resumability, and safety conventions: `docs/agent-workflow.md`
- For historical background on the content pipeline: `docs/plans/2026-03-03-content-pipeline-design.md`
