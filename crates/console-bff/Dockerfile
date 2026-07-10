# syntax=docker/dockerfile:1.7
FROM node:24-bookworm-slim AS console-builder

WORKDIR /app/console
COPY console/package.json console/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY console ./
RUN npm run build

FROM rust:1.96.1-slim AS rust-builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
COPY showcase/sumo/mcp ./showcase/sumo/mcp
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-console-bff --bin console-bff \
    && cp /app/target/release/console-bff /out-console-bff

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=rust-builder /out-console-bff /usr/local/bin/console-bff
COPY --from=console-builder /app/console/dist /app/console

USER veoveo
EXPOSE 8786
ENTRYPOINT ["/usr/local/bin/console-bff"]
