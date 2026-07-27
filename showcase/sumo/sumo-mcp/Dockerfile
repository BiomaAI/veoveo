# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# Eclipse SUMO publishes this release as amd64-only. Build the Rust entrypoint
# for that same platform so multi-stage builds cannot mix architectures on an
# arm64 workstation. Bullseye's glibc is intentionally older than the Ubuntu
# 22.04 SUMO runtime, keeping the resulting binary ABI-compatible.
ARG SUMO_PLATFORM=linux/amd64
ARG SUMO_BASE_IMAGE=sumo-base
FROM --platform=${SUMO_PLATFORM} rust:1.97.1-slim-bullseye@sha256:dd159e8e44aaa84dffbb7f24a742a96b8d0a26920bd3b46ec2b1778be0da1cdf AS builder

WORKDIR /src
ARG VEOVEO_TARGET_CACHE_ID
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN --mount=type=bind,source=.,target=/src,readonly \
    --mount=type=cache,id=veoveo-cargo-registry-v1,target=/usr/local/cargo/registry \
    --mount=type=cache,id=veoveo-cargo-git-v1,target=/usr/local/cargo/git \
    --mount=type=cache,id=${VEOVEO_TARGET_CACHE_ID},target=/target,sharing=locked \
    CARGO_TARGET_DIR=/target \
        cargo build --release --locked -p veoveo-sumo-mcp --bin sumo-mcp \
    && mkdir -p /out/bin \
    && install -m 0755 /target/release/sumo-mcp /out/bin/sumo-mcp

# Keep the SUMO command-line tools in the same image as the server. There is no
# fake generator fallback: missing or failing tools fail the durable task.
FROM --platform=${SUMO_PLATFORM} ${SUMO_BASE_IMAGE}

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/sumo \
    && chown -R veoveo:veoveo /var/lib/veoveo \
    && ln -sf /usr/share/sumo/tools/tlsCoordinator.py /usr/local/bin/tlsCoordinator.py
COPY --from=builder /out/bin/sumo-mcp /usr/local/bin/sumo-mcp

USER 10001:10001
EXPOSE 8795
ENTRYPOINT ["/usr/local/bin/sumo-mcp"]
