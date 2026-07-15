# syntax=docker/dockerfile:1.7
FROM rust:1.97.0-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY agents ./agents
COPY apps/console/bff ./apps/console/bff
COPY mcp ./mcp
COPY platform ./platform
COPY servers ./servers
COPY testing ./testing
COPY showcase/sumo/sumo-mcp ./showcase/sumo/sumo-mcp
RUN --mount=type=cache,id=view-mcp-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=view-mcp-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=view-mcp-target,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-view-mcp --bin server \
    && cp /app/target/release/server /tmp/view-mcp

# This runtime provides NVIDIA's CUDA user-space baseline. The host driver and
# Vulkan ICD are injected by NVIDIA Container Toolkit through the graphics
# driver capability; no Mesa Vulkan ICD or CPU rendering fallback is installed.
FROM nvidia/cuda:13.3.0-runtime-ubuntu24.04

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libegl1 \
        libvulkan1 \
        libx11-6 \
        libxext6 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /etc/veoveo/view \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=builder /tmp/view-mcp /usr/local/bin/view-mcp
COPY configs/view/layers.json /etc/veoveo/view/layers.json
COPY configs/view/local-layers.json /etc/veoveo/view/local-layers.json
COPY configs/view/local-tileset.json /etc/veoveo/view/local-tileset.json

ENV NVIDIA_VISIBLE_DEVICES=all \
    NVIDIA_DRIVER_CAPABILITIES=graphics,compute,utility \
    WGPU_BACKEND=vulkan \
    WGPU_POWER_PREF=high \
    HOME=/tmp \
    XDG_CACHE_HOME=/tmp/wgpu-cache

USER veoveo
EXPOSE 8801
ENTRYPOINT ["/usr/local/bin/view-mcp"]
