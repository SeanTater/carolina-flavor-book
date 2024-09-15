FROM docker.io/rust:bookworm as builder

WORKDIR /app
COPY Cargo.lock Cargo.toml /app/
COPY src/ /app/src/
RUN cargo build --release --bin gk-server

FROM debian:bookworm-slim

# TODO: Why does it not need to install openvino?
# RUN wget https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB \
#     && echo "deb https://apt.repos.intel.com/openvino/2024 ubuntu24 main" \
#     | sudo tee /etc/apt/sources.list.d/intel-openvino-2024.list \
#     && apt-get update \
#     && apt-get install -y intel-openvino-dev-ubuntu24-2024.3.0 \
#     && apt-get install -y ca-certificates libssl3 \
#     && rm -rf /var/lib/apt/lists/*

RUN apt-get update \
    && apt-get install -y ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/server /usr/local/bin/server
COPY models /app/models
WORKDIR /app
CMD [ "/usr/local/bin/gk-server" ]