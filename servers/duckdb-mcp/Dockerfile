# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd AS duckdb-spatial
ARG TARGETARCH
ARG DUCKDB_VERSION=1.5.5
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gzip \
    && rm -rf /var/lib/apt/lists/*
RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) duckdb_platform=linux_amd64; spatial_sha256=832edb1b189d53281baf552034028a4dfe317c5381a7e7d2d9de74a4e875572e ;; \
      arm64) duckdb_platform=linux_arm64; spatial_sha256=ccf86767b4f28471963e0950cb6a9d9bf2e38f6f9a476de32f5b59ffdacfc239 ;; \
      *) echo "unsupported DuckDB Spatial architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    mkdir -p /out/extensions; \
    curl -fsSL "https://extensions.duckdb.org/v${DUCKDB_VERSION}/${duckdb_platform}/spatial.duckdb_extension.gz" \
      -o /tmp/spatial.duckdb_extension.gz; \
    echo "${spatial_sha256}  /tmp/spatial.duckdb_extension.gz" | sha256sum -c -; \
    gzip -dc /tmp/spatial.duckdb_extension.gz > /out/extensions/spatial.duckdb_extension; \
    chmod 0444 /out/extensions/spatial.duckdb_extension

FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=veoveo-rust-artifacts /lib/libduckdb.so /usr/local/lib/libduckdb.so
COPY --from=duckdb-spatial /out/extensions/spatial.duckdb_extension /usr/local/lib/duckdb/extensions/spatial.duckdb_extension
RUN ldconfig

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/duckdb \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=veoveo-rust-artifacts /bin/duckdb-mcp /usr/local/bin/duckdb-mcp

USER veoveo
EXPOSE 8791
ENTRYPOINT ["/usr/local/bin/duckdb-mcp"]
