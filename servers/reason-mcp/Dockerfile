# syntax=docker/dockerfile:1.7
ARG VLLM_VERSION=v0.25.1

# The vLLM image is used as the build stage so the server binary links against
# the same glibc the runtime stage ships.
FROM vllm/vllm-openai:${VLLM_VERSION} AS builder

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

WORKDIR /app
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
    --mount=type=cache,id=reason-mcp-target,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-reason-mcp --bin server \
    && cp /app/target/release/server /tmp/reason-mcp

FROM vllm/vllm-openai:${VLLM_VERSION}

USER root
RUN useradd --system --uid 10001 --create-home \
        --home-dir /var/lib/veoveo veoveo

# The runner installs into the image's Python environment beside its vLLM
# runtime; the runner package deliberately declares no vllm dependency.
COPY servers/reason-mcp/runner /opt/veoveo/reason-runner
RUN pip install --no-cache-dir /opt/veoveo/reason-runner \
    && test -x /usr/local/bin/reason-runner

COPY --from=builder /tmp/reason-mcp /usr/local/bin/reason-mcp

ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,video
USER veoveo
EXPOSE 8803
ENTRYPOINT ["/usr/local/bin/reason-mcp"]
