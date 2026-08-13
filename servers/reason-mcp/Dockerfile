# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG VLLM_IMAGE=vllm/vllm-openai:muse-glimmer@sha256:413c8fbecb1204a218117c77a4ea4b3a211d5686ff99c31d82ba6dd0cec8c5a6

# The vLLM image is used as the build stage so the server binary links against
# the same glibc the runtime stage ships.
FROM ${VLLM_IMAGE} AS builder

USER root
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH}
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain 1.97.1

WORKDIR /src
ARG VEOVEO_CARGO_CACHE_ID
ARG VEOVEO_TARGET_CACHE_ID
ARG VEOVEO_CARGO_BUILD_JOBS=8
RUN --mount=type=bind,source=.,target=/src,readonly \
    --mount=type=cache,id=${VEOVEO_CARGO_CACHE_ID}-registry-v1,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=${VEOVEO_CARGO_CACHE_ID}-git-v1,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=${VEOVEO_TARGET_CACHE_ID},target=/target,sharing=locked \
    toolchain="$(rustup default | awk 'NR == 1 { print $1 }')" \
    && test -n "${toolchain}" \
    && RUSTUP_TOOLCHAIN="${toolchain}" CARGO_TARGET_DIR=/target \
        cargo build --release --locked --jobs "${VEOVEO_CARGO_BUILD_JOBS}" \
        -p veoveo-reason-mcp --bin reason-mcp \
    && mkdir -p /out/bin \
    && install -m 0755 /target/release/reason-mcp /out/bin/reason-mcp

FROM ${VLLM_IMAGE}

USER root
RUN useradd --system --uid 10001 --create-home \
        --home-dir /var/lib/veoveo veoveo

# The runner installs into the image's Python environment beside its vLLM
# runtime; the runner package deliberately declares no vllm dependency.
COPY servers/reason-mcp/runner /opt/veoveo/reason-runner
RUN pip install --no-cache-dir /opt/veoveo/reason-runner \
    && test -x /usr/local/bin/reason-runner

COPY --from=builder /out/bin/reason-mcp /usr/local/bin/reason-mcp

ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,video
USER veoveo
EXPOSE 8803
ENTRYPOINT ["/usr/local/bin/reason-mcp"]
