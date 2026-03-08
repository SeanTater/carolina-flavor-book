# Agent Workflow

This document is the shared, tool-agnostic operating guide for autonomous work in this repository.

## Purpose

Use deterministic Rust tooling for anything that mutates or inspects repository/application state, and use the model for judgment-heavy tasks such as recipe writing, tagging, editorial grouping, and recovery planning.

## Read Order

1. `AGENTS.md`
2. `README.md`
3. `config/recipe-grid.toml` if the task involves recipe tags or content classification
4. `gk-content/src/main.rs` if the task involves content-pipeline commands
5. Relevant plan docs in `docs/plans/` for background only

## Safe Defaults

- Prefer `config/dev.toml`
- Treat `config/prod.toml` as opt-in and user-directed
- Prefer repo-scoped artifacts over `/tmp` when a task might need to be resumed or reviewed
- Treat `config/recipe-grid.toml` as the single source of truth for valid tags

## Canonical Content Workflow

### 1. Inspect current state

Use the CLI instead of reconstructing state from prose docs.

```sh
cargo run -p gk-content -- --config config/dev.toml gaps --json
cargo run -p gk-content -- --config config/dev.toml search "keyword"
```

Useful subcommands exposed by `gk-content` include:

- `search`
- `gaps`
- `ingest`
- `ingest-tags`
- `add-images`
- `missing-images`
- `rename`
- `patch`
- `upsert-author`
- `publish-article`
- `ingest-schedule`
- `retag`
- `apply-retag`

### 2. Generate or classify content

- Use model judgment for recipe generation, retagging, naming, summaries, and editorial choices
- Keep output structured as JSON whenever possible
- Validate any tag assignment against `config/recipe-grid.toml`

### 3. Persist artifacts in a resumable place

Preferred locations:

- `docs/plans/` for reviewed plans and long-lived operator notes
- a task-specific directory under the repo for batch manifests or generated JSON that may need review
- `/tmp` only for disposable intermediate files

Suggested pattern for resumable jobs:

- manifest file describing the task, inputs, outputs, config, and status
- generated JSON files alongside the manifest
- brief verification notes in the same directory

### 4. Apply deterministic mutations with the CLI

Examples:

```sh
cargo run -p gk-content -- --config config/dev.toml ingest path/to/recipes.json
cargo run -p gk-content -- --config config/dev.toml ingest-tags path/to/tags.json
cargo run -p gk-content -- --config config/dev.toml patch path/to/patches.json
```

Only switch to `config/prod.toml` when the user has clearly asked for production-facing changes.

### 5. Verify results

- Re-run `gaps` or `search` after bulk changes
- Spot-check representative recipes rather than trusting a single success message
- For code changes, run relevant tests plus the checks in `CLAUDE.md`

## Resuming Interrupted Work

When you inherit an in-progress workflow:

1. Read the repo docs first; do not assume old prompt files are current
2. Verify whether expected inputs and outputs actually exist
3. Inspect the CLI surface in `gk-content/src/main.rs` if the workflow description looks stale
4. Recreate missing ephemeral files with the deterministic command that produced them
5. Record resumed work in a repo-scoped note or manifest if the task is likely to continue across sessions

For retagging specifically, `gk-content retag` can regenerate batch inputs and `gk-content apply-retag` can re-apply outputs. Do not assume `/tmp/retag-*` files still exist without checking.

## Source Hierarchy

When instructions disagree, prefer sources in this order:

1. Current code and config
2. `AGENTS.md`
3. `docs/agent-workflow.md`
4. `README.md`
5. Historical plan docs
6. Tool-specific prompt files under `.claude/`

## Known Pitfalls

- Older prompt files duplicate tag lists and may be stale relative to `config/recipe-grid.toml`
- Some historical workflows reference `config/prod.toml`; that is not the default
- `/tmp` state is convenient but fragile for multi-session work
- Agent memory files can contain useful patterns, but they are not the source of truth
