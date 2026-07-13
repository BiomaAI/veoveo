# syntax=docker/dockerfile:1.7
ARG DEEPSTREAM_VERSION=9.0

# NVIDIA's Triton image is used only as the DeepStream development stage. The
# deployed image below contains no Triton server process or model repository.
FROM nvcr.io/nvidia/deepstream:${DEEPSTREAM_VERSION}-triton-multiarch AS builder

USER root
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH}
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates cmake curl libjson-c-dev pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain 1.96.1

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
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --jobs 4 -p veoveo-perception-mcp --bin server \
    && cp /app/target/release/server /tmp/perception-mcp
RUN cmake -S servers/perception-mcp/deepstream-runner \
        -B /tmp/perception-runner-build \
        -DCMAKE_BUILD_TYPE=Release \
    && cmake --build /tmp/perception-runner-build --parallel 4 \
    && cp /tmp/perception-runner-build/perception-deepstream-runner /tmp/

FROM nvcr.io/nvidia/deepstream:${DEEPSTREAM_VERSION}-samples-multiarch

USER root
# DeepStream 9 omits some CPU-side container parsers. MP4 demux and H.264 parse
# are installed explicitly; decode and inference remain NVIDIA GPU elements.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl gstreamer1.0-plugins-bad gstreamer1.0-plugins-good \
        libjson-c5 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home \
        --home-dir /var/lib/veoveo veoveo

# CUDA's compatibility package can precede the native libraries injected by
# the NVIDIA Container Toolkit. GeForce GPUs do not support that forward-
# compatibility path, while the host driver supports CUDA 13.x minor
# compatibility. Give the architecture's native library directory priority so
# every NVIDIA plugin resolves one coherent set of driver libraries.
ARG TARGETARCH
RUN mkdir -p /opt/veoveo \
    && case "${TARGETARCH}" in \
        amd64) driver_lib_dir=/usr/lib/x86_64-linux-gnu ;; \
        arm64) driver_lib_dir=/usr/lib/aarch64-linux-gnu ;; \
        *) echo "unsupported target architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && ln -s "${driver_lib_dir}" /opt/veoveo/host-libraries

COPY --from=builder /tmp/perception-mcp /usr/local/bin/perception-mcp
COPY --from=builder /tmp/perception-deepstream-runner \
    /usr/local/bin/perception-deepstream-runner

ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,video \
    LD_LIBRARY_PATH=/opt/veoveo/host-libraries:/usr/local/cuda/lib64:/opt/veoveo/host-libraries/deepstream \
    USE_NEW_NVSTREAMMUX=no
USER veoveo
EXPOSE 8797
ENTRYPOINT ["/usr/local/bin/perception-mcp"]
