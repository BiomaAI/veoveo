# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
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
ARG VEOVEO_TARGET_CACHE_ID
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
RUN --mount=type=cache,id=veoveo-cargo-registry-v1,target=/usr/local/cargo/registry \
    --mount=type=cache,id=veoveo-cargo-git-v1,target=/usr/local/cargo/git \
    --mount=type=cache,id=${VEOVEO_TARGET_CACHE_ID},target=/app/target,sharing=locked \
    toolchain="$(rustup default | awk 'NR == 1 { print $1 }')" \
    && test -n "${toolchain}" \
    && UAV_SIM_WEBRTC_CLIENT_BUNDLE=/tmp/ov-web-rtc.umd.cjs \
        RUSTUP_TOOLCHAIN="${toolchain}" \
        cargo build --release --locked -p veoveo-uav-sim-mcp --bin uav-sim-mcp \
    && cp /app/target/release/uav-sim-mcp /tmp/uav-sim-mcp

FROM --platform=${TARGETPLATFORM} debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo
COPY --from=builder /tmp/uav-sim-mcp /usr/local/bin/uav-sim-mcp

USER 10001:10001
EXPOSE 8802
ENTRYPOINT ["/usr/local/bin/uav-sim-mcp"]
