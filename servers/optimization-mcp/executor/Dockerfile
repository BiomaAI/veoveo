# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM ghcr.io/astral-sh/uv:0.12.18-python3.13-trixie-slim@sha256:8891323e7ddaddc86d08c91f8af845ac5774a07d6f98e6765883030e1ad16fbc AS python-package-tools
FROM nvidia/cuopt:26.8.0-cuda13.3-py3.14@sha256:81441d50797ffaf6352552d94bd14560c53df28370bd0c9bf413fd7eeebbf178

USER root
RUN --mount=type=bind,from=python-package-tools,source=/usr/local/bin/uv,target=/usr/local/bin/uv,readonly \
    --mount=type=bind,source=servers/optimization-mcp/executor/pyproject.toml,target=/tmp/executor-package/pyproject.toml,readonly \
    --mount=type=bind,source=servers/optimization-mcp/executor/uv.lock,target=/tmp/executor-package/uv.lock,readonly \
    uv export --project /tmp/executor-package --python python --no-python-downloads --locked --no-dev --no-emit-project --output-file /tmp/executor-requirements.txt \
    && uv pip install --python python --target /opt/veoveo-cuopt-deps \
      --only-binary :all: --require-hashes --requirement /tmp/executor-requirements.txt \
    && rm /tmp/executor-requirements.txt
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        libgomp1=12.3.0-1ubuntu1~22.04.3 \
    && rm -rf /var/lib/apt/lists/* \
    && cudss_threading_layer=/usr/local/lib/python3.14/dist-packages/nvidia/cu13/lib/libcudss_mtlayer_gomp.so.0 \
    && test -f "${cudss_threading_layer}" \
    && ! ldd "${cudss_threading_layer}" | grep --quiet 'not found'

ENV PYTHONPATH=/opt/veoveo-cuopt:/opt/veoveo-cuopt-deps \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    CUPY_CACHE_DIR=/run/veoveo-cuopt/cache/cupy \
    NUMBA_CACHE_DIR=/run/veoveo-cuopt/cache/numba \
    XDG_CACHE_HOME=/run/veoveo-cuopt/cache

COPY --chown=10001:10001 servers/optimization-mcp/executor/veoveo_cuopt_executor /opt/veoveo-cuopt/veoveo_cuopt_executor

USER 10001:10001
ENTRYPOINT ["python", "-m", "veoveo_cuopt_executor"]
