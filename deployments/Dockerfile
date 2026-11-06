# syntax=docker/dockerfile:1
# Build from the repo root: docker build -f deployments/Dockerfile .

FROM rust:1.99.1-slim-bookworm AS builder
WORKDIR /app
# Manifests first for dependency-layer caching.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/arsy-server /usr/local/bin/arsy-server

ENV ARSY_ADDR=0.0.0.0 \
    ARSY_PORT=8080 \
    RUST_LOG=arsy_server=info
EXPOSE 8080
USER nobody
CMD ["arsy-server"]
