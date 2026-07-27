# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG DEEPSTREAM_VERSION=9.0

# NVIDIA's Triton image is used only as the DeepStream development stage. The
# deployed image below contains no Triton server process or model repository.
FROM nvcr.io/nvidia/deepstream:${DEEPSTREAM_VERSION}-triton-multiarch@sha256:2e45070ad134b9ab2caa4a97ba4d52fa8744a4f0db30900bd92828d51425a69a AS builder

USER root
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH}
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates cmake curl libjson-c-dev pkg-config \
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
        cargo build --release --locked -p veoveo-perception-mcp --bin perception-mcp \
    && mkdir -p /out/bin \
    && install -m 0755 /target/release/perception-mcp /out/bin/perception-mcp
RUN --mount=type=bind,source=.,target=/src,readonly \
    cmake -S /src/servers/perception-mcp/deepstream-runner \
        -B /tmp/perception-runner-build \
        -DCMAKE_BUILD_TYPE=Release \
    && cmake --build /tmp/perception-runner-build --parallel \
    && install -m 0755 /tmp/perception-runner-build/perception-deepstream-runner \
        /out/bin/perception-deepstream-runner

FROM nvcr.io/nvidia/deepstream:${DEEPSTREAM_VERSION}-samples-multiarch@sha256:f28592681ec863f9bd90c39a1759b93ec34eb00e2348d70e659fd98aaa1eb87b

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

COPY --from=builder /out/bin/perception-mcp /usr/local/bin/perception-mcp
COPY --from=builder /out/bin/perception-deepstream-runner \
    /usr/local/bin/perception-deepstream-runner

ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,video \
    LD_LIBRARY_PATH=/opt/veoveo/host-libraries:/usr/local/cuda/lib64:/opt/veoveo/host-libraries/deepstream \
    USE_NEW_NVSTREAMMUX=no
USER veoveo
EXPOSE 8797
ENTRYPOINT ["/usr/local/bin/perception-mcp"]
