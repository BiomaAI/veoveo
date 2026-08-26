# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM node:26-bookworm-slim@sha256:4db36457f406501e6f608802e5da617e5fbd0e80b75901b6a09de1ae5a667d32 AS console-builder

WORKDIR /app/apps/console/web
COPY apps/console/web/package.json apps/console/web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm install --global npm@12.0.2 \
    && npm ci
COPY apps/console/web ./
RUN npm run build

FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=veoveo-rust-artifacts /bin/console-bff /usr/local/bin/console-bff
COPY --from=console-builder /app/apps/console/web/dist /app/console

USER veoveo
EXPOSE 8786
ENTRYPOINT ["/usr/local/bin/console-bff"]
