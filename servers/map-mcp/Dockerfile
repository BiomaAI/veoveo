# syntax=docker/dockerfile:1.7
FROM rust:1.97.1-slim-bookworm AS builder

ARG TARGETARCH
ARG DUCKDB_VERSION=1.5.4

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      build-essential ca-certificates cmake curl gzip libsqlite3-dev libtiff-dev pkg-config sqlite3 \
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
RUN --mount=type=cache,id=map-mcp-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=map-mcp-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=map-mcp-target-bookworm,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-map-mcp --bin server \
    && mkdir -p /out/lib \
    && cp /app/target/release/server /out/map-mcp \
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

FROM ghcr.io/valhalla/valhalla:3.8.2 AS valhalla-runtime

FROM ghcr.io/osgeo/gdal:ubuntu-full-3.11.4

ARG GTFS_VALIDATOR_VERSION=8.0.1
ARG GTFS_VALIDATOR_SHA256=19293ddd9b6f954f216d4f12054bd8a3232921751c4484339e339764a91000e2

USER root
RUN rm -f /etc/apt/sources.list.d/apache-arrow.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates curl libczmq4 libluajit-5.1-2 libprotobuf-lite32t64 libstdc++6 \
      libzmq5 openjdk-21-jre-headless osmium-tool python3 \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL \
      "https://github.com/MobilityData/gtfs-validator/releases/download/v${GTFS_VALIDATOR_VERSION}/gtfs-validator-${GTFS_VALIDATOR_VERSION}-cli.jar" \
      -o /usr/local/lib/gtfs-validator.jar \
    && echo "${GTFS_VALIDATOR_SHA256}  /usr/local/lib/gtfs-validator.jar" | sha256sum -c - \
    && chmod 0444 /usr/local/lib/gtfs-validator.jar

COPY --from=valhalla-runtime /usr/local/bin/valhalla_build_config /usr/local/bin/valhalla_build_config
COPY --from=valhalla-runtime /usr/local/bin/valhalla_build_tiles /usr/local/bin/valhalla_build_tiles
COPY --from=valhalla-runtime /usr/local/bin/valhalla_service /usr/local/bin/valhalla_service
COPY --from=valhalla-runtime /usr/local/lib/libprime_server.so.0.0.0 /usr/local/lib/libprime_server.so.0.0.0
COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so
COPY --from=builder /out/extensions/spatial.duckdb_extension /usr/local/lib/duckdb/extensions/spatial.duckdb_extension
COPY --from=builder /out/map-mcp /usr/local/bin/map-mcp
COPY servers/map-mcp/data/src/map_data /opt/veoveo/map-data/map_data
RUN ln -s libprime_server.so.0.0.0 /usr/local/lib/libprime_server.so.0 \
    && ln -s libprime_server.so.0.0.0 /usr/local/lib/libprime_server.so \
    && ldconfig \
    && ogr2ogr --formats | grep -q 'Parquet' \
    && ! ldd /usr/local/bin/valhalla_build_tiles | grep -q 'not found' \
    && ! ldd /usr/local/bin/valhalla_service | grep -q 'not found' \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p \
      /data/map-sources \
      /etc/veoveo/map \
      /run/secrets/map \
      /var/lib/veoveo/map/acquisitions \
      /var/lib/veoveo/map/spill \
      /var/lib/veoveo/map/valhalla/active \
    && valhalla_build_config \
      --mjolnir-tile-dir /var/lib/veoveo/map/valhalla/active \
      --mjolnir-tile-extract /var/lib/veoveo/map/valhalla/active.tar \
      --mjolnir-admin /var/lib/veoveo/map/valhalla/active/admins.sqlite \
      --mjolnir-timezone /var/lib/veoveo/map/valhalla/active/timezones.sqlite \
      > /etc/veoveo/map/valhalla.json \
    && chown -R veoveo:veoveo /var/lib/veoveo /data/map-sources \
    && chmod 0555 /opt/veoveo/map-data \
    && chmod 0444 /etc/veoveo/map/valhalla.json

ENV PYTHONPATH=/opt/veoveo/map-data \
    MAP_GTFS_VALIDATOR_JAR=/usr/local/lib/gtfs-validator.jar \
    MAP_VALHALLA_BUILD_CONFIG=/etc/veoveo/map/valhalla.json \
    PROJ_NETWORK=OFF

USER veoveo
EXPOSE 8799
ENTRYPOINT ["/usr/local/bin/map-mcp"]
