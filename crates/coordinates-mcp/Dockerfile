# syntax=docker/dockerfile:1.7
FROM rust:1.96.1 AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        build-essential \
        cmake \
        curl \
        libsqlite3-dev \
        libtiff-dev \
        pkg-config \
        sqlite3 \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
COPY showcase/sumo/mcp ./showcase/sumo/mcp
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-coordinates-mcp --bin server \
    && mkdir -p /out \
    && cp /app/target/release/server /out/coordinates-mcp

FROM debian:trixie

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libsqlite3-0 libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=builder /out/coordinates-mcp /usr/local/bin/coordinates-mcp

USER veoveo
EXPOSE 8793
ENTRYPOINT ["/usr/local/bin/coordinates-mcp"]
