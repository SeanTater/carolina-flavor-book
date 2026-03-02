# Installing Gallagher Kitchen

This guide sets up gk-server and Cloudflare Tunnel on a fresh Ubuntu/Debian host.

## Prerequisites

- Rust toolchain (`rustup`)
- A Cloudflare account with your domain's nameservers pointed to Cloudflare
- `cloudflared` installed: `curl -fsSL https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb -o /tmp/cloudflared.deb && sudo dpkg -i /tmp/cloudflared.deb`
- System libraries: `sudo apt install clang libclang-dev`

## 1. Build the server

```sh
cargo build --release -p gk-server
```

## 2. Create a system user

```sh
sudo useradd --system --no-create-home --shell /usr/sbin/nologin gk
```

## 3. Create directories

```sh
sudo mkdir -p /opt/gk-server /var/lib/gk-server /etc/gk-server
sudo chown gk:gk /var/lib/gk-server
```

## 4. Install the binary and config

```sh
sudo cp target/release/gk-server /opt/gk-server/
```

Copy and edit the production config:

```sh
sudo cp config/prod.yml /etc/gk-server/config.yml
sudo editor /etc/gk-server/config.yml
```

Fill in the secrets:

- `service_principal_secret`: generate with `openssl rand -hex 32`
- `password_hash`: generate with `python3 -c "import bcrypt; print(bcrypt.hashpw(b'YOUR_PASSWORD', bcrypt.gensalt()).decode())"`

## 5. Copy data (if migrating)

```sh
sudo cp data/recipes.db /var/lib/gk-server/
sudo chown gk:gk /var/lib/gk-server/recipes.db
```

For a fresh install, the server creates the database on first run.

## 6. Install the gk-server systemd service

```sh
sudo cp gk-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now gk-server
sudo systemctl status gk-server
```

## 7. Set up Cloudflare Tunnel

Authenticate (opens a browser):

```sh
cloudflared tunnel login
```

Create the tunnel:

```sh
cloudflared tunnel create gallagher-kitchen
```

This prints a tunnel ID (like `10929e4d-02af-417e-89c9-c6935b6cc66c`) and creates a credentials file at `~/.cloudflared/TUNNEL_ID.json`.

Route DNS:

```sh
cloudflared tunnel route dns gallagher-kitchen gallagher.kitchen
```

Install the config and credentials:

```sh
sudo mkdir -p /etc/cloudflared

# Copy the template and fill in your tunnel ID
sudo cp cloudflared.yml /etc/cloudflared/config.yml
sudo sed -i "s/TUNNEL_ID/YOUR_ACTUAL_TUNNEL_ID/g" /etc/cloudflared/config.yml

# Copy credentials
sudo cp ~/.cloudflared/YOUR_ACTUAL_TUNNEL_ID.json /etc/cloudflared/
sudo cp ~/.cloudflared/cert.pem /etc/cloudflared/
```

Install and start the systemd service:

```sh
sudo cloudflared service install
sudo systemctl enable --now cloudflared
sudo systemctl status cloudflared
```

## 8. Verify

```sh
curl https://gallagher.kitchen/health
```

## Updating

```sh
cargo build --release -p gk-server
sudo cp target/release/gk-server /opt/gk-server/
sudo systemctl restart gk-server
```

## Troubleshooting

Check logs:

```sh
sudo journalctl -u gk-server -f
sudo journalctl -u cloudflared -f
```

Test the server directly (bypassing the tunnel):

```sh
curl http://localhost:3000/health
```
