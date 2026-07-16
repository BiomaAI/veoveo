# syntax=docker/dockerfile:1.7
FROM rust:1.97.0-slim AS builder

ARG TARGETARCH
ARG DUCKDB_VERSION=1.5.4

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gzip \
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
    cargo build --release --jobs 4 -p veoveo-duckdb-mcp --bin server \
    && mkdir -p /out/lib \
    && cp /app/target/release/server /out/duckdb-mcp \
    && find /app/target -name 'libduckdb.so' -type f -exec cp {} /out/lib/libduckdb.so \; -quit \
    && test -f /out/lib/libduckdb.so
RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) duckdb_platform=linux_amd64; spatial_sha256=be5c95a1921d5bf66dbcee450272927a93a2839cff7c1e9c2ee815e5461616ac ;; \
      arm64) duckdb_platform=linux_arm64; spatial_sha256=38eca92a690de960e64c749f1d8e5e0cd3d8a3f1c4af647d8d0fc66cb7c4c565 ;; \
      *) echo "unsupported DuckDB Spatial architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    mkdir -p /out/extensions; \
    curl -fsSL "https://extensions.duckdb.org/v${DUCKDB_VERSION}/${duckdb_platform}/spatial.duckdb_extension.gz" \
      -o /tmp/spatial.duckdb_extension.gz; \
    echo "${spatial_sha256}  /tmp/spatial.duckdb_extension.gz" | sha256sum -c -; \
    gzip -dc /tmp/spatial.duckdb_extension.gz > /out/extensions/spatial.duckdb_extension; \
    chmod 0444 /out/extensions/spatial.duckdb_extension

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so
COPY --from=builder /out/extensions/spatial.duckdb_extension /usr/local/lib/duckdb/extensions/spatial.duckdb_extension
RUN ldconfig

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/duckdb \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=builder /out/duckdb-mcp /usr/local/bin/duckdb-mcp

USER veoveo
EXPOSE 8791
ENTRYPOINT ["/usr/local/bin/duckdb-mcp"]
