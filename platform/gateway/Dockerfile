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
    cargo build --release --jobs 4 -p veoveo-mcp-gateway --bin gateway \
    && mkdir -p /out/lib \
    && cp /app/target/release/gateway /out/gateway \
    && find /app/target -name 'libduckdb.so' -type f -exec cp {} /out/lib/libduckdb.so \; -quit \
    && test -f /out/lib/libduckdb.so

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /etc/veoveo \
    && chown -R veoveo:veoveo /etc/veoveo /var/lib/veoveo

COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so
RUN ldconfig

COPY --from=builder /out/gateway /usr/local/bin/gateway

USER veoveo
EXPOSE 8788
ENTRYPOINT ["/usr/local/bin/gateway"]
