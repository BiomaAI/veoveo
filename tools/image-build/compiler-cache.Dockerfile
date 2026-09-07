# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
# The named input is the existing family's compile stage, with the same compiler,
# system libraries and build flags as ordinary artifact production.
FROM compiler-environment AS experiment
ARG SCCACHE_VERSION
ARG SCCACHE_ARCHIVE_URL
ARG SCCACHE_ARCHIVE_SHA256
ADD --checksum=sha256:${SCCACHE_ARCHIVE_SHA256} ${SCCACHE_ARCHIVE_URL} /tmp/sccache.tar.gz
RUN tar -xzf /tmp/sccache.tar.gz -C /tmp \
    && install -m 0755 /tmp/sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl/sccache /usr/local/bin/sccache \
    && test "$(sccache --version)" = "sccache ${SCCACHE_VERSION}" \
    && rm -rf /tmp/sccache.tar.gz /tmp/sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl

ENV CARGO_INCREMENTAL=0 \
    SCCACHE_DIR=/compiler-cache \
    SCCACHE_CACHE_SIZE=8G \
    SCCACHE_SERVER_UDS=/tmp/veoveo-sccache.sock \
    SCCACHE_IDLE_TIMEOUT=0 \
    SCCACHE_CLIENT_SIDE=1
WORKDIR /src
ARG VEOVEO_CARGO_PACKAGES
ARG VEOVEO_CARGO_BINARIES
ARG VEOVEO_AUXILIARY
ARG VEOVEO_CARGO_CACHE_ID
ARG VEOVEO_COMPILER_CACHE_ID
ARG VEOVEO_CACHE_CASE
RUN --mount=type=bind,from=veoveo-rust-source,target=/src,readonly \
    --mount=type=cache,id=${VEOVEO_CARGO_CACHE_ID}-registry-v1,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=${VEOVEO_CARGO_CACHE_ID}-git-v1,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=${VEOVEO_COMPILER_CACHE_ID},target=/compiler-cache,sharing=locked \
    bash -euo pipefail -c '\
        mkdir -p /target; \
        [[ -z "$(ls -A /target)" ]] || { echo "experiment requires an empty Cargo target directory" >&2; exit 1; }; \
        rm -rf /out; mkdir -p /out/bin /out/evidence; \
        cache_empty=false; \
        [[ -n "$(find /compiler-cache -type f -print -quit)" ]] || cache_empty=true; \
        case "${VEOVEO_CACHE_CASE}" in \
            baseline) unset RUSTC_WRAPPER ;; \
            populate|restore) export RUSTC_WRAPPER=/usr/local/bin/sccache; sccache --start-server; sccache --zero-stats ;; \
            *) echo "unknown compiler cache case" >&2; exit 1 ;; \
        esac; \
        cargo_args=(build --release --locked); \
        IFS=, read -r -a packages <<< "${VEOVEO_CARGO_PACKAGES}"; \
        for package in "${packages[@]}"; do \
            [[ "${package}" =~ ^[a-z0-9][a-z0-9-]*$ ]] || exit 1; \
            cargo_args+=(-p "${package}"); \
        done; \
        IFS=, read -r -a binaries <<< "${VEOVEO_CARGO_BINARIES}"; \
        for binary in "${binaries[@]}"; do \
            [[ "${binary}" =~ ^[a-z0-9][a-z0-9-]*$ ]] || exit 1; \
            cargo_args+=(--bin "${binary}"); \
        done; \
        if [[ ",${VEOVEO_CARGO_PACKAGES}," == *,veoveo-recording-mcp,* ]]; then \
            cargo_args+=(--features veoveo-recording-mcp/redap); \
        fi; \
        toolchain="$(rustup default | cut -d" " -f1)"; \
        [[ -n "${toolchain}" ]] || exit 1; \
        rustup run "${toolchain}" rustc --version --verbose > /out/evidence/compiler.txt; \
        sccache --show-stats --stats-format=json > /out/evidence/sccache-before.json; \
        printf "{\"targetInitiallyEmpty\":true,\"incremental\":false,\"compilerCacheInitiallyEmpty\":%s,\"case\":\"%s\"}\n" "${cache_empty}" "${VEOVEO_CACHE_CASE}" > /out/evidence/inputs.json; \
        RUSTUP_TOOLCHAIN="${toolchain}" CARGO_TARGET_DIR=/target cargo "${cargo_args[@]}"; \
        for binary in "${binaries[@]}"; do install -m 0755 "/target/release/${binary}" "/out/bin/${binary}"; done; \
        if [[ ",${VEOVEO_AUXILIARY}," == *,libduckdb,* ]]; then \
            library="$(find /target -name libduckdb.so -type f -print -quit)"; \
            [[ -n "${library}" ]] || { echo "required libduckdb.so was not produced" >&2; exit 1; }; \
            mkdir -p /out/lib; install -m 0755 "${library}" /out/lib/libduckdb.so; \
        fi; \
        sccache --show-stats --stats-format=json > /out/evidence/sccache.json; \
        if [[ "${VEOVEO_CACHE_CASE}" != baseline ]]; then sccache --stop-server; fi; \
        rm -rf /target'

FROM scratch AS artifacts
COPY --from=experiment /out/ /
