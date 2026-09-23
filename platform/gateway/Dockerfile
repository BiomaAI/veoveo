# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
FROM debian:trixie-slim@sha256:a99cfc517144bc59b1978475ec53b46ecabec7e43635402ee5b77cc54cd1b20a

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /etc/veoveo \
    && chown -R veoveo:veoveo /etc/veoveo /var/lib/veoveo

COPY --from=veoveo-rust-artifacts /bin/gateway /usr/local/bin/gateway

USER veoveo
EXPOSE 8788
ENTRYPOINT ["/usr/local/bin/gateway"]
