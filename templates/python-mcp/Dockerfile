# syntax=docker/dockerfile:1.7
# Build context is the repository root; the template depends on
# sdk/python by path.
FROM ghcr.io/astral-sh/uv:python3.13-bookworm-slim AS builder

WORKDIR /build
COPY sdk/python sdk/python
COPY templates/python-mcp templates/python-mcp
WORKDIR /build/templates/python-mcp
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --no-dev --compile-bytecode

FROM python:3.13-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=builder /build/sdk/python /opt/veoveo/sdk/python
COPY --from=builder /build/templates/python-mcp /opt/veoveo/templates/python-mcp

ENV PATH="/opt/veoveo/templates/python-mcp/.venv/bin:${PATH}"

USER veoveo
EXPOSE 8798
ENTRYPOINT ["datasheet-mcp"]
