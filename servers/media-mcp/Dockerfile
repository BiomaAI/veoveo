# syntax=docker/dockerfile:1.7
FROM rust:1.96.1-slim AS builder

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
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    mkdir -p /out \
    && cargo build --release --jobs 4 -p veoveo-media-mcp --bin server \
    && cp /app/target/release/server /out/media-mcp

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/media \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=builder /out/media-mcp /usr/local/bin/media-mcp

USER veoveo
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/media-mcp"]
