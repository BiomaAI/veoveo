# syntax=docker/dockerfile:1.7
FROM rust:1.97.0-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates pkg-config \
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
RUN --mount=type=cache,id=time-mcp-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=time-mcp-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=time-mcp-target-bookworm,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-time-mcp --bin server \
    && cp /app/target/release/server /usr/local/bin/time-mcp

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && test -x /usr/sbin/zic \
    && test -f /usr/share/zoneinfo/leap-seconds.list \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/time/acquisitions /var/lib/veoveo/time/releases \
    && chown -R veoveo:veoveo /var/lib/veoveo/time
COPY --from=builder /usr/local/bin/time-mcp /usr/local/bin/time-mcp

USER veoveo
EXPOSE 8800
ENTRYPOINT ["/usr/local/bin/time-mcp"]
