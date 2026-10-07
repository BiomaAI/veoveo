# syntax=docker/dockerfile:1.7
FROM node:26-slim@sha256:5758d367d7b4f48b73a9bb3530e687e47efb289f3b43f9c0450a25225ae0db5d

# Preserve the pinned upstream assets/render exports; install the reviewed owner lock
# over its package metadata so no obsolete SDK/Apps dependency enters the final tree.
RUN npm install --global npm@12.0.2 \
    && npm install --global flint-chart-mcp@0.5.1
COPY package.json package-lock.json /usr/local/lib/node_modules/flint-chart-mcp/
RUN cd /usr/local/lib/node_modules/flint-chart-mcp \
    && npm ci --omit=dev \
    && rm -f /usr/local/bin/flint-chart-mcp \
    && printf '#!/bin/sh\nexec node /usr/local/lib/node_modules/flint-chart-mcp/server.mjs "$@"\n' > /usr/local/bin/chart-mcp \
    && chmod +x /usr/local/bin/chart-mcp \
    && npm cache clean --force \
    && rm -rf /root/.npm /tmp/node-compile-cache

COPY --chown=10001:10001 server.mjs /usr/local/lib/node_modules/flint-chart-mcp/server.mjs
COPY --chown=10001:10001 flint-v2.mjs /usr/local/lib/node_modules/flint-chart-mcp/flint-v2.mjs
COPY --chown=10001:10001 composer.html /usr/local/lib/node_modules/flint-chart-mcp/assets/composer.html
COPY --chown=10001:10001 internal-auth.mjs /usr/local/lib/node_modules/flint-chart-mcp/internal-auth.mjs
COPY --chown=10001:10001 documents.mjs compliance.mjs well-known.mjs knowledge.mjs build-docs.mjs /usr/local/lib/node_modules/flint-chart-mcp/
COPY --chown=10001:10001 AGENTS.md DESIGN.md contract-compliance.json requirements.json compliance-profile.schema.json /usr/local/lib/node_modules/flint-chart-mcp/

RUN node /usr/local/lib/node_modules/flint-chart-mcp/build-docs.mjs

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

USER veoveo
EXPOSE 8795
ENTRYPOINT ["chart-mcp"]
