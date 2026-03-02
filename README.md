# Recipes: scanning and digitization for home recipes

This repository is a Rust workspace for ingesting, enhancing, and serving scanned family recipes.
It includes a CLI for capture + OCR + LLM cleanup, an Axum web server for browsing and search,
and a shared model crate used by both.

## Getting started (local dev)
1) Install system dependencies (if needed for build):
   - Ubuntu: `apt install clang libclang-dev`
2) Build the workspace:
   - `cargo build --workspace`
3) Start the server:
   - `cargo run -p gk-server -- config/dev.toml`
4) Set up your `.env` (for the CLI):
   - `PRINCIPAL_SECRET` must match `auth.service_principal_secret` in `config/dev.toml`
   - Optional: `OPENAI_API_KEY`, `LLM_PROVIDER`, `LLM_MODEL`, `OLLAMA_BASE_URL`
5) Add a recipe:
   - `cargo run -p gk-client --bin gk-add -- "Apple Cake" --dictation --tag dessert`

## Using the CLI (gk-client)
`gk-add` ingests and uploads recipes to the server.

Examples:
```sh
cargo run -p gk-client --bin gk-add -- "Pumpkin Bread" --webcam 0 --rotate r90 --illustrate
cargo run -p gk-client --bin gk-add -- "Apple Cake" --dictation --tag dessert
```

Required env vars:
- `PRINCIPAL_SECRET` must match `auth.service_principal_secret` in server config.

Optional env vars and flags:
- `LLM_PROVIDER` (`openai` or `ollama`), `LLM_MODEL`, `OPENAI_API_KEY`, `OLLAMA_BASE_URL`
- `DIFFUSION_BASE_URL` (defaults to `http://localhost:8000`)

Auth check:
```sh
cargo run -p gk-client --bin gk-auth-check -- --server http://localhost:3000
```

## Running the server (Docker)
```sh
task run-local
```
This builds the `gk-server` image and runs it with:
- `data/` mounted at `/app/data`
- `.env` mounted at `/app/.env`

## Repository layout
- `gk/` shared data models for client/server payloads.
- `gk-client/` CLI ingestion tools (`gk-add`, `gk-auth-check`).
- `gk-server/` Axum web/API server, templates, static assets, migrations.
- `config/` TOML config files for the server (`dev.toml`, `prod.toml`).
- `data/` runtime artifacts (SQLite database, session store, logs, sample images).
- `models/` local embedding model files for search.
- `Dockerfile`, `gk-server.service`, `Justfile` deployment helpers.

## Architecture overview
1) `gk-client` ingests recipes via webcam, dictation, or direct text.
2) OCR and LLM cleanup produce a clean Markdown revision.
3) Optional diffusion service generates illustrative images from LLM prompts.
4) `gk-server` stores recipes/images in SQLite and serves HTML + API endpoints.
5) A local embedding model indexes recipes for semantic search.

## Dependencies
Core:
- Rust toolchain (workspace builds `gk`, `gk-client`, `gk-server`).
- SQLite (via `rusqlite` + bundled sqlite).
- `models/snowflake-arctic-embed-xs` for embeddings.

System libraries:
- Clang/LLVM headers may be needed to build native dependencies (via bindgen).
  - Ubuntu: `apt install clang libclang-dev`
- v4l2 webcam support if using `gk-add --webcam`.
- `libssl3` at runtime for the server (Docker installs this).

External services:
- LLM provider for OCR cleanup / recipe rewrites:
  - `openai` (requires `OPENAI_API_KEY`)
  - or `ollama` (OpenAI-compatible API, default `http://localhost:11434/v1`)
- Diffusion service for image generation:
  - `gk-client` calls `POST {DIFFUSION_BASE_URL}/api/generate`.
  - Expected request/response shape is documented below.
- Browser login uses username/password (bcrypt-hashed credentials in `config/*.toml`).

## Server configuration
`gk-server` reads TOML config files (`config/dev.toml`, `config/prod.toml`) with:
- `server.address` and optional `server.tls` cert/key paths.
- `database.path` for the SQLite database.
- `auth.*` for username/password login, service principal token, and session storage.

Note: `config/*.toml` includes secrets in this repo today. Treat them as private, and prefer
local `.env` overrides or secret management when deploying.

Useful env vars:
- `RUST_LOG=info,tower_http::trace::on_response=debug` for request logging.

## Diffusion service contract
The CLI expects an HTTP service with:
- `POST /api/generate` that accepts:
  - `prompt` (string)
  - `negative_prompt` (string, optional)
  - `width`, `height`, `steps`, `cfg_scale`, `seed`
  - `include_base64` (bool)
- Response JSON:
  - `job_id` (string)
  - `base64_png` (string)

Images are converted to WebP and stored in SQLite with optional prompt metadata.

## Data model and storage
- SQLite DB at `data/recipes.db`.
- Migrations live in `gk-server/src/migrations/`.
- Images are stored as WebP blobs in SQLite.
- Sessions are persisted to `data/sessions.json`.

## Build and test
```sh
cargo build --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

## Deployment
- See [INSTALL.md](INSTALL.md) for production setup (systemd + Cloudflare Tunnel).
- `task run-local` builds and runs the server in Docker for development.
