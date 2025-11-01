# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a recipe scanning and digitization system for home recipes, consisting of three Rust workspace members:

- **gk-server**: Web server that hosts recipes with search, image generation, and OAuth authentication
- **gk-client**: CLI tool for recipe ingestion via webcam, OCR, dictation, or LLM generation
- **gk**: Shared library containing basic models and LLM/illustration ingestion logic

## Development Setup

Install OpenCV dependencies first (Ubuntu):
```bash
apt install libopencv-dev clang libclang-dev
```

For Google Cloud deployment setup:
```bash
task setup-dev-env
```

## Common Commands

### Local Development

Run the server locally (native):
```bash
RUST_LOG=debug cargo run --bin gk-server -- config/dev.yml
```

Run the server locally (Docker):
```bash
task run-local
```

Build Docker image:
```bash
task build
```

### Client Tools

Add a recipe using the CLI:
```bash
cargo run --bin gk-add -- "Recipe Name" tag1 tag2 [OPTIONS]
```

Options include:
- `-w <device>` - webcam capture
- `-d` - dictation mode
- `-f` - freestyle (LLM generation)
- `--illustrate` - generate AI images
- `--server <url>` - target server (default: https://gallagher.kitchen)
- `--dry` - dry run without upload

### Deployment

Deploy to production:
```bash
task deploy
```

SSH to production VM:
```bash
task login
```

## Architecture

### Server (gk-server)

- **Web framework**: Axum with tower-http compression and tracing
- **Database**: SQLite with r2d2 connection pooling, migrations in `src/migrations/`
- **Storage**: Google Cloud Storage for images (via `storage.rs`)
- **Auth**: Google OAuth2 with JWT validation, session storage in JSON file
- **Search**: Vector embeddings using fastembed (snowflake-arctic-embed-xs model), background indexing
- **Templates**: Minijinja templates compiled at build time from `templates/` directory
- **Static files**: Embedded at compile time using `include_dir!` from `static/` directory

Key components:
- `config.rs` - YAML config loading (server, database, auth sections)
- `database.rs` - Database connection pool and migration system
- `models.rs` - Recipe, Image, and related database models
- `search/` - Vector search with embedding model and document indexing
- `auth/` - OAuth flow, JWT validation, session management
- `storage.rs` - Google Cloud Storage client for image uploads

### Client (gk-client)

Ingestion pipeline with multiple modes:
- Webcam capture with OpenCV (`ingestion/webcam.rs`)
- OCR using ocrs library (`ingestion/ocr.rs`)
- Scan cropping and image processing (`ingestion/scan_cropping.rs`)

### Shared Library (gk)

- `basic_models.rs` - Upload models (RecipeForUpload, RevisionForUpload, ImageForUpload)
- `ingestion/llm.rs` - OpenAI integration for recipe generation and improvement
- `ingestion/illustrate.rs` - Replicate API for AI image generation

## Configuration

Config files are in `config/`:
- `dev.yml` - Local development (localhost:3000, no TLS, SQLite at data/recipes.db)
- `prod.yml` - Production (0.0.0.0:443, TLS with Let's Encrypt certs)

Config structure:
```yaml
server:
  address: <host:port>
  tls: null | {cert_path, key_path}
database:
  path: <sqlite_file>
auth:
  client_id: <google_oauth_client_id>
  client_secret: <google_oauth_secret>
  redirect_uri: <callback_url>
  session_storage_path: <json_file>
  audiences: [<client_ids>]
```

## Database Migrations

Migrations are embedded SQL files in `gk-server/src/migrations/`. The system tracks schema version in metadata table and applies pending migrations on startup. To add a migration:

1. Create new file `gk-server/src/migrations/0X-description.sql`
2. Add to migrations array in `database.rs`
3. Server will auto-apply on next startup

## Testing Locally

1. Ensure `data/` directory exists for SQLite database and sessions
2. Use `config/dev.yml` which points to local paths
3. Set `RUST_LOG=debug` for verbose logging
4. Run server: `cargo run --bin gk-server -- config/dev.yml`
5. Access at http://localhost:3000

## API Keys

The client requires API keys in `.env` file for:
- `OPENAI_API_KEY` - recipe generation and improvement
- `REPLICATE_API_TOKEN` - AI image generation
