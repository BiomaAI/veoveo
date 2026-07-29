# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM nvidia/cuopt:26.6.0-cuda13.2-py3.14@sha256:0faac7182b32f5be747e30f081427e80e8dfeccd8f49613814d3d217167fe1ad

ENV PYTHONPATH=/opt/veoveo-cuopt \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    CUPY_CACHE_DIR=/run/veoveo-cuopt/cache/cupy \
    NUMBA_CACHE_DIR=/run/veoveo-cuopt/cache/numba \
    XDG_CACHE_HOME=/run/veoveo-cuopt/cache

COPY --chown=10001:10001 servers/optimization-mcp/executor/veoveo_cuopt_executor /opt/veoveo-cuopt/veoveo_cuopt_executor

USER 10001:10001
ENTRYPOINT ["python", "-m", "veoveo_cuopt_executor"]
