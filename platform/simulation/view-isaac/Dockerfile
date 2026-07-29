# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG TARGETPLATFORM=linux/amd64
ARG SIMULATION_RUNTIME_IMAGE=veoveo/simulation-runtime:2026.07.0

FROM --platform=${TARGETPLATFORM} ${SIMULATION_RUNTIME_IMAGE} AS runtime

USER root
RUN mkdir -p \
      /opt/veoveo/simulation-view \
      /var/lib/veoveo/simulation-view/artifacts/sha256 \
      /var/lib/veoveo/runtime-cache/simulation-view \
    && chown -R 10001:10001 \
      /opt/veoveo/simulation-view \
      /var/lib/veoveo/simulation-view \
      /var/lib/veoveo/runtime-cache/simulation-view \
    && chmod 0700 \
      /var/lib/veoveo/simulation-view \
      /var/lib/veoveo/simulation-view/artifacts \
      /var/lib/veoveo/simulation-view/artifacts/sha256 \
      /var/lib/veoveo/runtime-cache/simulation-view

COPY --chown=10001:10001 veoveo_simulation_view/ /opt/veoveo/simulation-view/veoveo_simulation_view/
COPY --chown=10001:10001 --chmod=0555 entrypoint.py /opt/veoveo/simulation-view/entrypoint.py

ENV PYTHONPATH=/opt/veoveo/simulation-view:/isaac-sim/extscache/omni.warp.core-1.13.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/python:/opt/veoveo/isaaclab/source/isaaclab:/opt/veoveo/isaaclab/source/isaaclab_newton:/opt/veoveo/isaaclab/source/isaaclab_physx:/opt/veoveo/isaaclab/source/isaaclab_ov:/opt/veoveo/isaaclab/source/isaaclab_ovphysx

ARG SOURCE_REVISION=local
ARG COMPATIBILITY_RELEASE=2026.07.0
LABEL org.opencontainers.image.title="Veoveo Simulation View Isaac renderer" \
      org.opencontainers.image.description="Renderer-only OpenUSD, RTX HydraTexture, NVIDIA NVENC, and WebRTC implementation" \
      org.opencontainers.image.revision="${SOURCE_REVISION}" \
      org.opencontainers.image.source="https://github.com/BiomaAI/veoveo" \
      veoveo.io/compatibility-release="${COMPATIBILITY_RELEASE}" \
      veoveo.io/simulation-runtime-profile="isaac-6.0.1-lab-3.0.0-beta2.patch1-warp-1.15.0-newton-1.4.0" \
      veoveo.io/simulation-view-renderer-profile="veoveo.io/simulation-view-renderer/isaac-rtx/v1"

EXPOSE 8810/tcp 49100-49131/tcp 47998-48029/udp

USER 10001:10001
ENTRYPOINT ["/isaac-sim/python.sh", "/opt/veoveo/simulation-view/entrypoint.py"]
