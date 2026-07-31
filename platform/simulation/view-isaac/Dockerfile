# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG TARGETPLATFORM=linux/amd64
ARG SIMULATION_RUNTIME_IMAGE=veoveo/simulation-runtime:2026.07.0

FROM --platform=${TARGETPLATFORM} ${SIMULATION_RUNTIME_IMAGE} AS runtime

USER root
ADD --checksum=sha256:fe38d6195974620e87d7cdad32bb4b38d5933389523d56e75bbae23bcf4da55f \
    https://github.com/CesiumGS/cesium-omniverse/releases/download/v0.29.0/CesiumGS-cesium-omniverse-linux-x86_64-v0.29.0.zip \
    /tmp/cesium.zip
COPY cesium_vendor_install.py /tmp/cesium_vendor_install.py
RUN mkdir -p /opt/veoveo/extensions /opt/veoveo/simulation-view-python \
    && /isaac-sim/python.sh -m zipfile -e /tmp/cesium.zip /opt/veoveo/extensions \
    && install -m 0644 /tmp/cesium_vendor_install.py \
      /opt/veoveo/extensions/cesium.omniverse/cesium/omniverse/install/vendor_install.py \
    && sed -i 's/if server_prim_path != "":/if server_prim_path:/' \
      /opt/veoveo/extensions/cesium.omniverse/cesium/omniverse/usdUtils/usdUtils.py \
    && /isaac-sim/python.sh -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /opt/veoveo/simulation-view-python \
      /opt/veoveo/extensions/cesium.omniverse/vendor/lxml-6.0.2-cp312-cp312-manylinux_2_26_x86_64.manylinux_2_28_x86_64.whl \
    && rm -f /tmp/cesium.zip /tmp/cesium_vendor_install.py
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

ENV PYTHONPATH=/opt/veoveo/simulation-view:/opt/veoveo/simulation-view-python:${PYTHONPATH}

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
