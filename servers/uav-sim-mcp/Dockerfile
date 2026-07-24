# syntax=docker/dockerfile:1.7
ARG TARGETPLATFORM=linux/amd64
FROM --platform=${TARGETPLATFORM} node:26.5.0-bookworm-slim@sha256:2d49d876e96237d76de412761cf05dbfe5aee325cc4406a4d41d5824c5bb8beb AS webrtc-client

WORKDIR /client
RUN archive="$(npm pack @nvidia/ov-web-rtc@6.6.0 \
      --registry=https://edge.urm.nvidia.com/artifactory/api/npm/omniverse-client-npm/ \
      --silent)" \
    && echo "77be78cd4799f797d320d386461834737f5a8368deacfb3b27ae26612f39c9a5  ${archive}" | sha256sum -c - \
    && tar -xzf "${archive}" package/dist/ov-web-rtc.umd.cjs \
    && echo "ef2bab07d13bee861c30922100f9c98fd61982826fdc8cedc6e43f032d8fa70d  package/dist/ov-web-rtc.umd.cjs" | sha256sum -c -

FROM --platform=${TARGETPLATFORM} rust:1.97.1-slim-bookworm@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777 AS builder

WORKDIR /app
COPY --from=webrtc-client /client/package/dist/ov-web-rtc.umd.cjs /tmp/ov-web-rtc.umd.cjs
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY agents ./agents
COPY apps/console/bff ./apps/console/bff
COPY mcp ./mcp
COPY platform ./platform
COPY servers ./servers
COPY testing ./testing
COPY showcase/sumo/sumo-mcp ./showcase/sumo/sumo-mcp
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=veoveo-uav-sim-mcp-amd64,target=/app/target \
    UAV_SIM_WEBRTC_CLIENT_BUNDLE=/tmp/ov-web-rtc.umd.cjs \
    cargo build --release --jobs 4 -p veoveo-uav-sim-mcp --bin uav-sim-mcp \
    && cp /app/target/release/uav-sim-mcp /tmp/uav-sim-mcp

FROM --platform=${TARGETPLATFORM} debian:bookworm-slim@sha256:63a496b5d3b99214b39f5ed70eb71a61e590a77979c79cbee4faf991f8c0783e

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo
COPY --from=builder /tmp/uav-sim-mcp /usr/local/bin/uav-sim-mcp

USER 10001:10001
EXPOSE 8802
ENTRYPOINT ["/usr/local/bin/uav-sim-mcp"]
