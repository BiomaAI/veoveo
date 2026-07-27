# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG VLLM_VERSION=v0.25.1

# The vLLM image is used as the build stage so the server binary links against
# the same glibc the runtime stage ships.
FROM vllm/vllm-openai:${VLLM_VERSION}@sha256:e4f88a835143cd22aee2397a26ec6bb80b3a4a6fe0c882bcbc63822904766089 AS builder

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
ARG VEOVEO_TARGET_CACHE_ID
RUN --mount=type=bind,source=.,target=/src,readonly \
    --mount=type=cache,id=veoveo-cargo-registry-v1,target=/usr/local/cargo/registry \
    --mount=type=cache,id=veoveo-cargo-git-v1,target=/usr/local/cargo/git \
    --mount=type=cache,id=${VEOVEO_TARGET_CACHE_ID},target=/target,sharing=locked \
    toolchain="$(rustup default | awk 'NR == 1 { print $1 }')" \
    && test -n "${toolchain}" \
    && RUSTUP_TOOLCHAIN="${toolchain}" CARGO_TARGET_DIR=/target \
        cargo build --release --locked -p veoveo-reason-mcp --bin reason-mcp \
    && mkdir -p /out/bin \
    && install -m 0755 /target/release/reason-mcp /out/bin/reason-mcp

FROM vllm/vllm-openai:${VLLM_VERSION}@sha256:e4f88a835143cd22aee2397a26ec6bb80b3a4a6fe0c882bcbc63822904766089

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
