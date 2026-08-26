# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libstdc++6 netcat-openbsd \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home \
        --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo-recording-forwarder \
    && chown -R veoveo:veoveo \
        /var/lib/veoveo /var/lib/veoveo-recording-forwarder

COPY --from=veoveo-rust-artifacts /bin/recording-forwarder /usr/local/bin/recording-forwarder

USER 10001:10001
ENTRYPOINT ["/usr/local/bin/recording-forwarder"]
