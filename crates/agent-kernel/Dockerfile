# syntax=docker/dockerfile:1.7
FROM rust:1.96-slim AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-agent-kernel --bin agent \
    && mkdir -p /out/lib \
    && cp /app/target/release/agent /out/agent \
    && find /app/target -name 'libduckdb.so' -type f -exec cp {} /out/lib/libduckdb.so \; -quit \
    && test -f /out/lib/libduckdb.so

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so
RUN ldconfig

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/pilot \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=builder /out/agent /usr/local/bin/agent

USER veoveo
EXPOSE 8794
ENTRYPOINT ["/usr/local/bin/agent"]
