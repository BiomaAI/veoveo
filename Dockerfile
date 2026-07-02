FROM rust:1.96-slim AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p veoveo-media-mcp --bin server

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/media \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=builder /app/target/release/server /usr/local/bin/media-mcp

USER veoveo
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/media-mcp"]
