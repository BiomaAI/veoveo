# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
FROM ghcr.io/astral-sh/uv:0.12.18-python3.13-trixie-slim@sha256:8891323e7ddaddc86d08c91f8af845ac5774a07d6f98e6765883030e1ad16fbc

ARG SOURCE_DATE_EPOCH
ENV UV_PROJECT_ENVIRONMENT=/opt/speech UV_LINK_MODE=copy UV_PYTHON_DOWNLOADS=never
# Runtime dependencies and weights are independent of Rust and application edits.
RUN --mount=type=bind,from=veoveo-image-assets,source=servers/speech-mcp/runner/pyproject.toml,target=/tmp/speech-package/pyproject.toml,readonly \
    --mount=type=bind,from=veoveo-image-assets,source=servers/speech-mcp/runner/uv.lock,target=/tmp/speech-package/uv.lock,readonly \
    --mount=type=cache,id=veoveo-speech-uv,target=/root/.cache/uv \
    uv sync --project /tmp/speech-package --locked --no-dev --no-install-project --python /usr/local/bin/python3

ENV HF_HUB_CACHE=/opt/speech-models
RUN --mount=type=bind,from=veoveo-image-assets,source=servers/speech-mcp/runner/src/speech_runner/protocol.py,target=/tmp/speech-source/speech_runner/protocol.py,readonly \
    --mount=type=bind,from=veoveo-image-assets,source=servers/speech-mcp/runner/src/speech_runner/cache_model.py,target=/tmp/speech-source/speech_runner/cache_model.py,readonly \
    PYTHONPATH=/tmp/speech-source /opt/speech/bin/python -m speech_runner.cache_model

COPY --from=veoveo-image-assets /servers/speech-mcp/runner/src /opt/speech-source
COPY --from=veoveo-image-assets /servers/speech-mcp/runner/ATTRIBUTION.md /usr/share/doc/speech/ATTRIBUTION.md
COPY --from=veoveo-rust-artifacts /bin/speech-mcp /usr/local/bin/speech-mcp
RUN useradd --system --uid 10001 --no-create-home --home-dir /tmp veoveo \
    && chage --lastday "$(date -u --date=@${SOURCE_DATE_EPOCH} +%F)" veoveo \
    && chmod -R a+rX /opt/speech-models
ENV PYTHONPATH=/opt/speech-source HF_HUB_OFFLINE=1 PYTHONDONTWRITEBYTECODE=1 \
    NVIDIA_DRIVER_CAPABILITIES=compute,utility XDG_CACHE_HOME=/tmp/speech-cache \
    TORCHINDUCTOR_CACHE_DIR=/tmp/speech-inductor TRITON_CACHE_DIR=/tmp/speech-triton
USER 10001
RUN speech-mcp --help >/dev/null
EXPOSE 8811
ENTRYPOINT ["/usr/local/bin/speech-mcp"]
