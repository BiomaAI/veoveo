# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM ghcr.io/astral-sh/uv:0.11.32@sha256:df4cae8f3a96d175e2e5f992e597550000edbe78fdc2594d5cd8de1a217f504c AS uv
FROM python:3.13.14-slim-bookworm@sha256:9d7f287598e1a5a978c015ee176d8216435aaf335ed69ac3c38dd1bbb10e8d64 AS builder

COPY --from=uv /uv /uvx /bin/
WORKDIR /opt/extension
COPY pyproject.toml uv.lock ./
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=secret,id=veoveo-python-index,required=true \
    UV_DEFAULT_INDEX="$(cat /run/secrets/veoveo-python-index)" \
    uv sync --locked --no-dev --no-install-project
COPY src src
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=secret,id=veoveo-python-index,required=true \
    UV_DEFAULT_INDEX="$(cat /run/secrets/veoveo-python-index)" \
    uv sync --locked --no-dev --no-editable --compile-bytecode

FROM python:3.13.14-slim-bookworm@sha256:9d7f287598e1a5a978c015ee176d8216435aaf335ed69ac3c38dd1bbb10e8d64

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=builder --chown=10001:10001 /opt/extension/.venv /opt/extension/.venv

ENV PATH="/opt/extension/.venv/bin:${PATH}"

USER veoveo
EXPOSE 8798
ENTRYPOINT ["datasheet-mcp"]
