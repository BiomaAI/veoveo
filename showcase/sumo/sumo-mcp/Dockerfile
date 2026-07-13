# syntax=docker/dockerfile:1.7
# Eclipse SUMO publishes this release as amd64-only. Build the Rust entrypoint
# for that same platform so multi-stage builds cannot mix architectures on an
# arm64 workstation. Bullseye's glibc is intentionally older than the Ubuntu
# 22.04 SUMO runtime, keeping the resulting binary ABI-compatible.
ARG SUMO_PLATFORM=linux/amd64
FROM --platform=${SUMO_PLATFORM} rust:1.96.1-slim-bullseye AS builder

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
    --mount=type=cache,id=veoveo-sumo-mcp-amd64-bullseye,target=/app/target \
    mkdir -p /out \
    && cargo build --release --jobs 4 -p veoveo-sumo-mcp --bin sumo-mcp \
    && cp /app/target/release/sumo-mcp /out/sumo-mcp

# Keep the SUMO command-line tools in the same image as the server. There is no
# fake generator fallback: missing or failing tools fail the durable task.
FROM --platform=${SUMO_PLATFORM} ghcr.io/eclipse-sumo/sumo:v1_27_1@sha256:87623396d3501ca8d0ac25154e202bc38baa55a31c724e4dfd6ae60f297c6bf2

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/sumo \
    && chown -R veoveo:veoveo /var/lib/veoveo \
    && ln -sf /usr/share/sumo/tools/tlsCoordinator.py /usr/local/bin/tlsCoordinator.py
COPY --from=builder /out/sumo-mcp /usr/local/bin/sumo-mcp

USER veoveo
EXPOSE 8795
ENTRYPOINT ["/usr/local/bin/sumo-mcp"]
