# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=veoveo-rust-artifacts /bin/uav-sim-mcp /usr/local/bin/uav-sim-mcp
COPY --chmod=0555 servers/uav-sim-mcp/entrypoint.sh /usr/local/bin/uav-sim-mcp-entrypoint

USER veoveo
EXPOSE 8802
ENTRYPOINT ["/usr/local/bin/uav-sim-mcp-entrypoint"]
