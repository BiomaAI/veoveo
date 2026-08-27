# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# Internal durable recording image. The gateway is the only installation
# ingest caller; native Rerun remains a container-loopback implementation port.
FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

ARG RERUN_VERSION=0.36.3
ARG ATTRS_VERSION=26.1.0
ARG NUMPY_VERSION=2.5.2
ARG PILLOW_VERSION=12.3.0
ARG PSUTIL_VERSION=7.2.2
ARG PYARROW_VERSION=25.0.1
ARG TYPING_EXTENSIONS_VERSION=4.16.0

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libstdc++6 python3 python3-pip python3-venv netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# The `rerun` CLI (freeze optimize/verify + `rerun server` catalog).
RUN python3 -m venv /opt/rerun \
    && /opt/rerun/bin/pip install --no-cache-dir \
        "attrs==${ATTRS_VERSION}" \
        "numpy==${NUMPY_VERSION}" \
        "pillow==${PILLOW_VERSION}" \
        "psutil==${PSUTIL_VERSION}" \
        "pyarrow==${PYARROW_VERSION}" \
        "typing-extensions==${TYPING_EXTENSIONS_VERSION}" \
        "rerun-sdk==${RERUN_VERSION}" \
    && ln -s /opt/rerun/bin/rerun /usr/local/bin/rerun

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/spool \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=veoveo-rust-artifacts /bin/spooler /bin/sensor-sim /usr/local/bin/

USER veoveo
RUN rerun analytics disable \
    && rerun analytics config | grep -F '"analytics_enabled": false' \
    && rerun --version | grep -F "rerun-cli ${RERUN_VERSION} "
EXPOSE 9878
ENTRYPOINT []
CMD ["spooler", "--help"]
