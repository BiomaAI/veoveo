# syntax=docker/dockerfile:1.7
ARG TARGETPLATFORM=linux/amd64
FROM --platform=${TARGETPLATFORM} rust:1.97.1-slim-bookworm@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777 AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
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
    --mount=type=cache,id=veoveo-uav-sim-mcp-amd64,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-uav-sim-mcp --bin uav-sim-mcp \
    && cp /app/target/release/uav-sim-mcp /tmp/uav-sim-mcp

FROM --platform=${TARGETPLATFORM} debian:bookworm-slim@sha256:63a496b5d3b99214b39f5ed70eb71a61e590a77979c79cbee4faf991f8c0783e

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo
COPY --from=builder /tmp/uav-sim-mcp /usr/local/bin/uav-sim-mcp

USER 10001:10001
EXPOSE 8802
ENTRYPOINT ["/usr/local/bin/uav-sim-mcp"]
