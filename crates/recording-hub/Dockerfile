# syntax=docker/dockerfile:1.7
# Internal durable spooler image. The governed recording-mcp process is the
# only public protocol reader; this image exposes raw Rerun ingest internally.
FROM rust:1.96.1-slim AS builder

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
    cargo build --release --jobs 4 -p veoveo-recording-hub \
    --bin spooler --bin sensor-sim --bin hub-query \
    && mkdir -p /out \
    && cp /app/target/release/spooler /app/target/release/sensor-sim \
          /app/target/release/hub-query /out/

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libstdc++6 python3 python3-pip python3-venv netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# The `rerun` CLI (freeze optimize/verify + `rerun server` catalog).
RUN python3 -m venv /opt/rerun \
    && /opt/rerun/bin/pip install --no-cache-dir "rerun-sdk==0.34.0" \
    && ln -s /opt/rerun/bin/rerun /usr/local/bin/rerun \
    && rerun --version

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/spool \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=builder /out/spooler /out/sensor-sim /out/hub-query /usr/local/bin/

USER veoveo
# Raw proxy ingest is internal-network only.
EXPOSE 9876
ENTRYPOINT []
CMD ["spooler", "--help"]
