# syntax=docker/dockerfile:1.7
FROM rust:1.96.1-slim AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
COPY showcase/sumo/mcp ./showcase/sumo/mcp
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-mcp-stdio-bridge --bin bridge \
    && cp /app/target/release/bridge /out-bridge

FROM debian:trixie-slim

ARG RERUN_VERSION=0.34.0
ARG TARGETARCH

# libvulkan1 + mesa-vulkan-drivers provide lavapipe software rendering for the
# headless viewer's offscreen frames and screenshots.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libvulkan1 mesa-vulkan-drivers \
    && rm -rf /var/lib/apt/lists/*

RUN case "${TARGETARCH}" in \
        arm64) rerun_arch=aarch64 ;; \
        amd64) rerun_arch=x86_64 ;; \
        *) echo "unsupported TARGETARCH: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && curl -fsSL -o /usr/local/bin/rerun \
        "https://github.com/rerun-io/rerun/releases/download/${RERUN_VERSION}/rerun-cli-${RERUN_VERSION}-${rerun_arch}-unknown-linux-gnu" \
    && chmod +x /usr/local/bin/rerun \
    && if ldd /usr/local/bin/rerun | grep "not found"; then exit 1; fi

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=builder /out-bridge /usr/local/bin/bridge
COPY crates/mcp-stdio-bridge/docker-entrypoint.sh /usr/local/bin/rerun-mcp-entrypoint
RUN chmod +x /usr/local/bin/rerun-mcp-entrypoint

USER veoveo
EXPOSE 8790 9876
ENTRYPOINT ["/usr/local/bin/rerun-mcp-entrypoint"]
