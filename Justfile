# Run the dev server
dev:
    cargo run -- config/dev.yml

# Build the release binary
build:
    cargo build --release -p gk-server

# Deploy: build, stop service, copy binary, start service
deploy: build
    sudo systemctl stop gk-server
    sudo cp target/release/gk-server /opt/gk-server/
    sudo systemctl start gk-server

# Run workspace tests
test:
    cargo test --workspace

# Check prod server status
status:
    sudo systemctl status gk-server
