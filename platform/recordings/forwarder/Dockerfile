# syntax=docker/dockerfile:1.7
FROM rust:1.97.1-slim AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY agents ./agents
COPY apps/console/bff ./apps/console/bff
COPY mcp ./mcp
COPY platform ./platform
COPY servers ./servers
COPY testing ./testing
COPY showcase/sumo/sumo-mcp ./showcase/sumo/sumo-mcp
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target \
    cargo build --release --jobs 4 \
        -p veoveo-recording-forwarder --bin recording-forwarder \
    && cp /app/target/release/recording-forwarder /tmp/recording-forwarder

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libstdc++6 netcat-openbsd \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home \
        --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo-recording-forwarder \
    && chown -R veoveo:veoveo \
        /var/lib/veoveo /var/lib/veoveo-recording-forwarder

COPY --from=builder /tmp/recording-forwarder /usr/local/bin/recording-forwarder

USER 10001:10001
ENTRYPOINT ["/usr/local/bin/recording-forwarder"]
