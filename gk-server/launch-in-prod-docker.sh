#!/bin/bash
gcloud auth configure-docker
docker pull us-central1-docker.pkg.dev/telepathicpenguins/gk-container-images/gk-server:latest
docker run \
    -d \
    --name gk-server \
    --restart unless-stopped \
    -p 443:3000 \
    -v /etc/letsencrypt:/etc/letsencrypt:ro \
    -v /app/data:/app/data:rw \
    -e  RUST_LOG=info,tower_http::trace::on_response=debug \
    --env-file /app/.env \
    us-central1-docker.pkg.dev/telepathicpenguins/gk-container-images/gk-server:latest \
    gk-server \
    --address 0.0.0.0:3000 \
    --tls