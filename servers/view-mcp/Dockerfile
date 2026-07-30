# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# This runtime provides NVIDIA's CUDA user-space baseline. The host driver and
# Vulkan ICD are injected by NVIDIA Container Toolkit through the graphics
# driver capability; no Mesa Vulkan ICD or CPU rendering fallback is installed.
FROM nvidia/cuda:13.3.0-runtime-ubuntu24.04@sha256:789e629e49401647e22b7054ae9c6c4f6427dba68010ba428deb4cc6b063676e

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libegl1 \
        libvulkan1 \
        libx11-6 \
        libxext6 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /etc/veoveo/view \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=veoveo-rust-artifacts /bin/view-mcp /usr/local/bin/view-mcp
COPY configs/view/layers.json /etc/veoveo/view/layers.json

ENV NVIDIA_DRIVER_CAPABILITIES=graphics,compute,utility \
    WGPU_BACKEND=vulkan \
    WGPU_POWER_PREF=high \
    HOME=/tmp \
    XDG_CACHE_HOME=/tmp/wgpu-cache

USER veoveo
EXPOSE 8801
ENTRYPOINT ["/usr/local/bin/view-mcp"]
