# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132 AS duckdb-spatial
ARG TARGETARCH
ARG DUCKDB_VERSION=1.5.6
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gzip \
    && rm -rf /var/lib/apt/lists/*
RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) duckdb_platform=linux_amd64; spatial_sha256=3aa7e084efff1ceb71218b3bf75911a2ff519f9530547be87f6729f3cd7a1b6a ;; \
      arm64) duckdb_platform=linux_arm64; spatial_sha256=f9a39f333d23571f82ae17b87c65ceeebd06a52629c6535eb2072669012a70c7 ;; \
      *) echo "unsupported DuckDB Spatial architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    mkdir -p /out/extensions; \
    curl -fsSL "https://extensions.duckdb.org/v${DUCKDB_VERSION}/${duckdb_platform}/spatial.duckdb_extension.gz" \
      -o /tmp/spatial.duckdb_extension.gz; \
    echo "${spatial_sha256}  /tmp/spatial.duckdb_extension.gz" | sha256sum -c -; \
    gzip -dc /tmp/spatial.duckdb_extension.gz > /out/extensions/spatial.duckdb_extension; \
    chmod 0444 /out/extensions/spatial.duckdb_extension

FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log

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
