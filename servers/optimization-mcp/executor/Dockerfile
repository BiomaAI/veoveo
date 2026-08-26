# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM nvidia/cuopt:26.8.0-cuda13.3-py3.14@sha256:81441d50797ffaf6352552d94bd14560c53df28370bd0c9bf413fd7eeebbf178

USER root
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        libgomp1=12.3.0-1ubuntu1~22.04.3 \
    && rm -rf /var/lib/apt/lists/* \
    && cudss_threading_layer=/usr/local/lib/python3.14/dist-packages/nvidia/cu13/lib/libcudss_mtlayer_gomp.so.0 \
    && test -f "${cudss_threading_layer}" \
    && ! ldd "${cudss_threading_layer}" | grep --quiet 'not found'

ENV PYTHONPATH=/opt/veoveo-cuopt \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    CUPY_CACHE_DIR=/run/veoveo-cuopt/cache/cupy \
    NUMBA_CACHE_DIR=/run/veoveo-cuopt/cache/numba \
    XDG_CACHE_HOME=/run/veoveo-cuopt/cache

COPY --chown=10001:10001 servers/optimization-mcp/executor/veoveo_cuopt_executor /opt/veoveo-cuopt/veoveo_cuopt_executor

USER 10001:10001
ENTRYPOINT ["python", "-m", "veoveo_cuopt_executor"]
