# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# Internal durable recording image. The gateway is the only installation
# ingest caller; native Rerun remains a container-loopback implementation port.
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libstdc++6 python3 python3-pip python3-venv netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# The `rerun` CLI (freeze optimize/verify + `rerun server` catalog).
RUN python3 -m venv /opt/rerun \
    && /opt/rerun/bin/pip install --no-cache-dir "rerun-sdk==0.36.2" \
    && ln -s /opt/rerun/bin/rerun /usr/local/bin/rerun \
    && rerun --version

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/spool \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=veoveo-rust-artifacts /bin/spooler /bin/sensor-sim /bin/hub-query /usr/local/bin/

USER veoveo
EXPOSE 9878
ENTRYPOINT []
CMD ["spooler", "--help"]
