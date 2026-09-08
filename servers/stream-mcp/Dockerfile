# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
ARG DEEPSTREAM_VERSION=9.1

# NVIDIA's Triton image is used only as the DeepStream development stage. The
# deployed image below contains no Triton server process or model repository.
FROM nvcr.io/nvidia/deepstream:${DEEPSTREAM_VERSION}-triton-multiarch@sha256:fd31f5b44ababdbdee8cd397a375e888191b49e402ac237254a4cdc239130f5b AS builder

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates cmake libjson-c-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
RUN --mount=type=bind,source=servers/stream-mcp/gst-runner,target=/src,readonly \
    cmake -S /src \
        -B /tmp/stream-runner-build \
        -DCMAKE_BUILD_TYPE=Release \
    && cmake --build /tmp/stream-runner-build --parallel \
    && mkdir -p /out/bin \
    && install -m 0755 /tmp/stream-runner-build/stream-gst-runner \
        /out/bin/stream-gst-runner

FROM nvcr.io/nvidia/deepstream:${DEEPSTREAM_VERSION}-samples-multiarch@sha256:4f80b374e4a5086552825fe0f5bdd015c8cfd3dbe430cdde5ce9572e80e01583

USER root
# DeepStream 9.1 omits some CPU-side container parsers. MP4 demux and H.264 parse
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

COPY --from=veoveo-rust-artifacts /bin/stream-mcp /usr/local/bin/stream-mcp
COPY --from=builder /out/bin/stream-gst-runner \
    /usr/local/bin/stream-gst-runner

ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,video \
    LD_LIBRARY_PATH=/opt/veoveo/host-libraries:/usr/local/cuda/lib64:/opt/veoveo/host-libraries/deepstream \
    USE_NEW_NVSTREAMMUX=no
COPY --from=veoveo-image-assets --chmod=a=rX servers/stream-mcp/assets/live.html /opt/veoveo/stream/assets/live.html

USER veoveo
RUN test -r /opt/veoveo/stream/assets/live.html \
    && test ! -x /opt/veoveo/stream/assets/live.html
EXPOSE 8797
ENTRYPOINT ["/usr/local/bin/stream-mcp"]
