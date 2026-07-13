# syntax=docker/dockerfile:1.7
FROM node:24-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g flint-chart-mcp@0.2.0 \
    && printf '#!/bin/sh\nexec flint-chart-mcp "$@"\n' > /usr/local/bin/chart-mcp \
    && chmod +x /usr/local/bin/chart-mcp \
    && npm cache clean --force

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

USER veoveo
EXPOSE 8795
ENTRYPOINT ["chart-mcp"]
