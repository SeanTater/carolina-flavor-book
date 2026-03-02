# See README.md for project documentation.

# Before committing
- Run `cargo test --workspace`
- Smoke-test the server visually:
  ```sh
  cargo run -p gk-server -- config/dev.toml &
  npx playwright screenshot --browser chromium http://localhost:3001/ /tmp/gk-smoke.png
  # Then read /tmp/gk-smoke.png to verify it looks right
  ```
