# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && test -x /usr/sbin/zic \
    && test -f /usr/share/zoneinfo/leap-seconds.list \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/time/acquisitions /var/lib/veoveo/time/releases \
    && chown -R veoveo:veoveo /var/lib/veoveo/time
COPY --from=veoveo-rust-artifacts /bin/time-mcp /usr/local/bin/time-mcp

USER veoveo
EXPOSE 8800
ENTRYPOINT ["/usr/local/bin/time-mcp"]
