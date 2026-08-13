# syntax=docker/dockerfile:1.7
FROM node:26-slim@sha256:715e55e4b84e4bb0ff48e49b398a848f08e55daed8eb6a0ea1839ae53bc57583

RUN npm install --global npm@12.0.1 \
    && npm install --global flint-chart-mcp@0.5.0 \
    && cd /usr/local/lib/node_modules/flint-chart-mcp \
    && node -e 'const fs=require("node:fs"); const p=JSON.parse(fs.readFileSync("package.json","utf8")); for (const leaf of ["sdk", ["ext","apps"].join("-")]) delete p.dependencies[["@modelcontextprotocol",leaf].join("/")]; fs.writeFileSync("package.json", JSON.stringify(p, null, 2)+"\n")' \
    && npm install --save-exact @modelcontextprotocol/server@2.0.0 @modelcontextprotocol/node@2.0.0 zod@4.2.1 \
    && printf '#!/bin/sh\nexec node /usr/local/lib/node_modules/flint-chart-mcp/veoveo-server.mjs "$@"\n' > /usr/local/bin/chart-mcp \
    && chmod +x /usr/local/bin/chart-mcp \
    && npm cache clean --force \
    && rm -rf /root/.npm /tmp/node-compile-cache

COPY --chown=10001:10001 server.mjs /usr/local/lib/node_modules/flint-chart-mcp/veoveo-server.mjs
COPY --chown=10001:10001 flint-v2.mjs /usr/local/lib/node_modules/flint-chart-mcp/flint-v2.mjs
COPY --chown=10001:10001 internal-auth.mjs /usr/local/lib/node_modules/flint-chart-mcp/internal-auth.mjs
COPY --chown=10001:10001 AGENTS.md DESIGN.md /usr/local/lib/node_modules/flint-chart-mcp/

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

USER veoveo
EXPOSE 8795
ENTRYPOINT ["chart-mcp"]
