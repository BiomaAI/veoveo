# syntax=docker/dockerfile:1.7
FROM node:26-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*

RUN npm install --global npm@12.0.1 \
    && npm install --global flint-chart-mcp@0.3.0 \
    && printf '#!/bin/sh\nexec node /usr/local/lib/node_modules/flint-chart-mcp/veoveo-server.mjs "$@"\n' > /usr/local/bin/chart-mcp \
    && chmod +x /usr/local/bin/chart-mcp \
    && npm cache clean --force

COPY --chown=10001:10001 server.mjs /usr/local/lib/node_modules/flint-chart-mcp/veoveo-server.mjs

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

USER veoveo
EXPOSE 8795
ENTRYPOINT ["chart-mcp"]
