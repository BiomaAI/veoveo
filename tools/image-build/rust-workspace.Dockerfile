# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG RUST_IMAGE=docker.io/library/rust:1.97.1-slim-trixie@sha256:5c6f46a6e4472ab1ca7ba7d494e6677f2f219ebc02f32025d3986f057635ec9c
FROM ${RUST_IMAGE} AS compile

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        cmake \
        curl \
        gzip \
        libsqlite3-dev \
        libtiff-dev \
        pkg-config \
        sqlite3 \
    && rm -rf /var/lib/apt/lists/*

ARG VEOVEO_CARGO_PACKAGES
ARG VEOVEO_CARGO_BINARIES
ARG VEOVEO_AUXILIARY
ARG VEOVEO_CARGO_CACHE_ID
ARG VEOVEO_TARGET_CACHE_ID

WORKDIR /src
RUN --mount=type=bind,source=.,target=/src,readonly \
    --mount=type=cache,id=${VEOVEO_CARGO_CACHE_ID}-registry-v1,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=${VEOVEO_CARGO_CACHE_ID}-git-v1,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=${VEOVEO_TARGET_CACHE_ID},target=/target,sharing=locked \
    bash -euc '\
        [[ -n "${VEOVEO_CARGO_PACKAGES}" ]] || { echo "no Cargo packages selected" >&2; exit 1; }; \
        [[ -n "${VEOVEO_CARGO_BINARIES}" ]] || { echo "no Cargo binaries selected" >&2; exit 1; }; \
        if [[ ",${VEOVEO_CARGO_PACKAGES}," == *,veoveo-simulation-view-mcp,* ]]; then \
            readonly bundle_root=/tmp/nvidia-ov-web-rtc; \
            readonly bundle_url=https://edge.urm.nvidia.com/artifactory/api/npm/omniverse-client-npm/@nvidia/ov-web-rtc/-/@nvidia/ov-web-rtc-6.6.0.tgz; \
            readonly bundle_tar_sha=77be78cd4799f797d320d386461834737f5a8368deacfb3b27ae26612f39c9a5; \
            readonly bundle_umd_sha=ef2bab07d13bee861c30922100f9c98fd61982826fdc8cedc6e43f032d8fa70d; \
            mkdir -p "${bundle_root}"; \
            curl --fail --location --silent --show-error \
                "${bundle_url}" \
                --output "${bundle_root}/package.tgz"; \
            echo "${bundle_tar_sha}  ${bundle_root}/package.tgz" \
                | sha256sum --check --strict; \
            tar --extract --gzip --file "${bundle_root}/package.tgz" \
                --directory "${bundle_root}" package/dist/ov-web-rtc.umd.cjs; \
            echo "${bundle_umd_sha}  ${bundle_root}/package/dist/ov-web-rtc.umd.cjs" \
                | sha256sum --check --strict; \
            export SIMULATION_VIEW_WEBRTC_CLIENT_BUNDLE="${bundle_root}/package/dist/ov-web-rtc.umd.cjs"; \
        fi; \
        cargo_args=(build --release --locked); \
        IFS=, read -r -a packages <<< "${VEOVEO_CARGO_PACKAGES}"; \
        for package in "${packages[@]}"; do \
            [[ "${package}" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo "invalid Cargo package: ${package}" >&2; exit 1; }; \
            cargo_args+=(-p "${package}"); \
        done; \
        IFS=, read -r -a binaries <<< "${VEOVEO_CARGO_BINARIES}"; \
        for binary in "${binaries[@]}"; do \
            [[ "${binary}" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo "invalid Cargo binary: ${binary}" >&2; exit 1; }; \
            cargo_args+=(--bin "${binary}"); \
        done; \
        if [[ ",${VEOVEO_CARGO_PACKAGES}," == *,veoveo-recording-mcp,* ]]; then \
            cargo_args+=(--features veoveo-recording-mcp/redap); \
        fi; \
        toolchain="$(rustup default | cut -d" " -f1)"; \
        [[ -n "${toolchain}" ]] || { echo "the builder image has no default Rust toolchain" >&2; exit 1; }; \
        RUSTUP_TOOLCHAIN="${toolchain}" CARGO_TARGET_DIR=/target cargo "${cargo_args[@]}"; \
        mkdir -p /out/bin; \
        for binary in "${binaries[@]}"; do \
            install -m 0755 "/target/release/${binary}" "/out/bin/${binary}"; \
        done; \
        if [[ ",${VEOVEO_AUXILIARY}," == *,libduckdb,* ]]; then \
            mkdir -p /out/lib; \
            library="$(find /target -name libduckdb.so -type f -print -quit)"; \
            [[ -n "${library}" ]] || { echo "libduckdb.so was requested but not produced" >&2; exit 1; }; \
            install -m 0755 "${library}" /out/lib/libduckdb.so; \
        fi'

FROM scratch AS artifacts
COPY --from=compile /out/ /
