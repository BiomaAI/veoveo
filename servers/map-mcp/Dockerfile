# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818 AS duckdb-spatial
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

FROM ghcr.io/valhalla/valhalla:3.8.3@sha256:70b45295d81035e3562e1bbf996a28d5fc55e1ccc5d7e3fff9f297d3b1a1359f AS valhalla-runtime

FROM ghcr.io/osgeo/gdal:ubuntu-full-3.13.2@sha256:de280a240ef4309e6fb64b943e0472e6098099b813a6b9ff0e182bc6976a3fdb

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
COPY --from=veoveo-rust-artifacts /lib/libduckdb.so /usr/local/lib/libduckdb.so
COPY --from=duckdb-spatial /out/extensions/spatial.duckdb_extension /usr/local/lib/duckdb/extensions/spatial.duckdb_extension
COPY --from=veoveo-rust-artifacts /bin/map-mcp /usr/local/bin/map-mcp
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
