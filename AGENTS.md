# Repository Guidelines

## Project Structure & Module Organization
- `gk/`: shared recipe models; update here when adding fields that both client and server consume.
- `gk-client/`: ingestion CLI (`src/bin` entry points, `src/ingestion` for OCR, webcam, LLM, diffusion steps).
- `gk-server/`: Axum web/API surface (`src` handlers, `templates/` MiniJinja views, `static/` assets, `config/*.yml` environment configs).
- Long-lived artifacts live under `data/` (SQLite snapshots, sample images) and `models/`; treat `.db` files as runtime state, not fixtures.

## Build, Test, and Development Commands
- `cargo build --workspace` — compile all crates and surface linker issues early.
- `cargo run -p gk-server -- config/dev.yml` — boot the server against the dev SQLite database.
- `cargo fmt --all` then `cargo clippy --workspace --all-targets` — enforce formatting and catch lint regressions.
- `cargo test --workspace` — run available tests; add coverage when touching ingestion, search, or auth flows.
- `task run-local` — Dockerized server that mirrors production mounts; requires Taskfile and Docker.

## Coding Style & Naming Conventions
- Keep to default `rustfmt` style (4-space indent, newline-at-end); rerun before pushing.
- Use `snake_case` for modules/functions, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Align handler names with routes (`search_recipes`, `get_image`) and keep modules small; add `///` comments only for non-obvious behavior or invariants.

## Testing Guidelines
- Co-locate `#[cfg(test)]` modules with the code they cover; integration tests belong under `gk-server/tests/`.
- Prefer `tokio::test` for async code and use temporary SQLite databases from `Database::connect` to avoid mutating `data/recipes.db`.
- Fixture data should be lightweight YAML/JSON checked into the crate; document remaining gaps when full coverage is not feasible.

## Commit & Pull Request Guidelines
- Mirror existing history with short, imperative subjects (`Add`, `Refactor`, `Remove`), capitalized and under 72 characters.
- Reference issues in the body (`Refs #123`) and state expected impact plus rollback notes.
- UI or ingestion changes should include screenshots or sample CLI transcripts; schema/config updates need migration notes or ops callouts.
- Keep PR scope tight and flag secrets or OAuth changes so deployments can be coordinated.

## Environment & Configuration Tips
- `config/dev.yml` is the default local config; never commit real secrets—store machine-specific values in `.env`, which Docker mounts for local runs.
- Clean sample assets before adding to `data/`; include a README snippet in the PR explaining provenance and intended usage.
- Rotate API keys or prompt files committed under `gk-client/src/prompts/`; redact tokens in logs before sharing.
