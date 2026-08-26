# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

ARG RERUN_VERSION=0.36.3
ARG ATTRS_VERSION=26.1.0
ARG NUMPY_VERSION=2.5.2
ARG PILLOW_VERSION=12.3.0
ARG PSUTIL_VERSION=7.2.2
ARG PYARROW_VERSION=25.0.1
ARG TYPING_EXTENSIONS_VERSION=4.16.0

# The Vulkan loader resolves the hardware driver mounted by the GPU runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libegl1 \
        libvulkan1 \
        libx11-6 \
        libxext6 \
        python3 \
        python3-pip \
        python3-venv \
    && rm -rf /var/lib/apt/lists/*

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

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=veoveo-rust-artifacts /bin/legacy-bridge /usr/local/bin/legacy-bridge
COPY mcp/bridges/legacy/docker-entrypoint.sh /usr/local/bin/rerun-legacy-mcp-entrypoint
RUN chmod +x /usr/local/bin/rerun-legacy-mcp-entrypoint

USER veoveo
RUN rerun analytics disable \
    && rerun analytics config | grep -F '"analytics_enabled": false' \
    && rerun --version | grep -F "rerun-cli ${RERUN_VERSION} "
EXPOSE 8790 9876
ENTRYPOINT ["/usr/local/bin/rerun-legacy-mcp-entrypoint"]
