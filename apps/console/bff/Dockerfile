# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM node:26-bookworm-slim@sha256:2d49d876e96237d76de412761cf05dbfe5aee325cc4406a4d41d5824c5bb8beb AS console-builder

WORKDIR /app/apps/console/web
COPY apps/console/web/package.json apps/console/web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm install --global npm@12.0.1 \
    && npm ci
COPY apps/console/web ./
RUN npm run build

FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=veoveo-rust-artifacts /bin/console-bff /usr/local/bin/console-bff
COPY --from=console-builder /app/apps/console/web/dist /app/console

USER veoveo
EXPOSE 8786
ENTRYPOINT ["/usr/local/bin/console-bff"]
