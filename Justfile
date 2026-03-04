# Run the dev server
dev:
    cargo run -- config/dev.toml

# Build the release binary
build:
    cargo build --release -p gk-server

# Deploy: build, stop service, copy binary, start service
deploy: build
    sudo systemctl stop gk-server
    sudo cp target/release/gk-server /opt/gk-server/
    sudo systemctl start gk-server

# Run workspace tests (including doctests)
test:
    cargo test --workspace
    cargo test --workspace --doc

# Generate HTML coverage report
coverage:
    cargo llvm-cov --workspace --html
    @echo "Report at target/llvm-cov/html/index.html"

# Print coverage summary to terminal
coverage-summary:
    cargo llvm-cov --workspace

# Check prod server status
status:
    sudo systemctl status gk-server

# Tag a release: just release 0.3.1
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    # Update workspace version
    sed -i 's/^version = ".*"/version = "{{version}}"/' Cargo.toml
    cargo check --workspace
    git add -A
    git commit -m "Release v{{version}}"
    git tag "v{{version}}"
    echo "Tagged v{{version}}. Push with: git push && git push --tags"
