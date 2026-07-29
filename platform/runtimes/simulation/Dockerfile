# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG TARGETPLATFORM=linux/amd64
ARG ISAAC_SIM_IMAGE=nvcr.io/nvidia/isaac-sim:6.0.1@sha256:b1c542b2ecc549b3d1ebb78c25664aa3bacba1709e6ad8e0a68e09426d57dedb

FROM --platform=${TARGETPLATFORM} ${ISAAC_SIM_IMAGE} AS payload

ARG ISAAC_LAB_REVISION=ffff603eafc6b74264a5261cc0183d6a65390d78
ARG ISAAC_LAB_ARCHIVE_URL=https://github.com/isaac-sim/IsaacLab/archive/ffff603eafc6b74264a5261cc0183d6a65390d78.tar.gz
ARG ISAAC_LAB_ARCHIVE_SHA256=fe65b28ad08a943e35005e6c75885929cd6cbd7075c721d8da4d05d97fa392be
ARG WARP_WHEEL_URL=https://files.pythonhosted.org/packages/a8/ba/36023c3161c45f1c509c20c501b17980705fdd3b6c20b7ee9267f50f44a2/warp_lang-1.15.0-py3-none-manylinux_2_28_x86_64.whl
ARG WARP_WHEEL_SHA256=95c169f28bd7d6c78ac4ad62e2df1e61a096033748f757157fa4551aed80d010
ARG NEWTON_WHEEL_URL=https://files.pythonhosted.org/packages/bf/67/28a76c50304d62dc9a9ec9a142e14f1a3156c77c0df778fc3918d0e47d8f/newton-1.4.0-py3-none-any.whl
ARG NEWTON_WHEEL_SHA256=0e11343cc51b86647d9afcd191a21ca4d0d5e410d84072a60ef84af908c72577
ARG MUJOCO_WHEEL_URL=https://files.pythonhosted.org/packages/c3/11/c69199e4123935f98068ab6ab6b35955b4de0f6a91d3f9883805a5789394/mujoco-3.10.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl
ARG MUJOCO_WHEEL_SHA256=966d12f88e77e2b188e7530667b519d6963b9b906cff83bab534e5e4279325a0
ARG MUJOCO_WARP_WHEEL_URL=https://files.pythonhosted.org/packages/9c/a1/a8e616daafb219fccc2d23367894d305e442e2e18bc5e0dd855d9986d979/mujoco_warp-3.10.0.3-py3-none-any.whl
ARG MUJOCO_WARP_WHEEL_SHA256=9435e5d6a7061af32aecc8da38124bfccceea27fc85176b33b66b7c4b76b4c1a
ARG NVIDIA_LIVESTREAM_AOV_VERSION=10.2.0
ARG NVIDIA_LIVESTREAM_AOV_BUILD=110.1.2.lx64.r.cp312
ARG NVIDIA_LIVESTREAM_AOV_URL=https://d4i3qtqj3r0z5.cloudfront.net/omni.kit.livestream.aov-10.2.0%2B110.1.2.lx64.r.cp312.zip
ARG NVIDIA_LIVESTREAM_AOV_SHA256=e842d29982283787b145f3a1f91210b4b16ecc9fbc7038e970ed4dc4bf482c49
ARG NVIDIA_LIVESTREAM_AOV_CONFIG_SHA256=71081c9a5190fc4ed116e31a97fbdf54423e83834c736ce7883343d7676bfad4

USER root

COPY requirements.lock /opt/veoveo/simulation-runtime/requirements.lock
RUN /isaac-sim/kit/python/bin/python3 -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --only-binary=:all: \
      --require-hashes \
      --target /opt/veoveo/python \
      --index-url https://pypi.org/simple \
      --extra-index-url https://download.pytorch.org/whl/cu130 \
      --requirement /opt/veoveo/simulation-runtime/requirements.lock

# Kit prepends its ML archive when Isaac starts. Replace the complete bundled
# Torch graph with the selected stable CUDA graph so Python modules and native
# NCCL/CUDA libraries resolve from one authoritative root. TorchVision and
# TorchAudio are outside the supported runtime subset and have no stable
# Torch 2.12 pair, so their incompatible bundled payloads are removed.
RUN ml_prebundle=/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle \
    && rtx_nvrtc_builtins=/tmp/veoveo-isaac-rtx-nvrtc-builtins \
    && mkdir -p "${rtx_nvrtc_builtins}" \
    && cp -a \
      "${ml_prebundle}"/nvidia/cuda_nvrtc/lib/libnvrtc-builtins*.so* \
      "${rtx_nvrtc_builtins}/" \
    && rm -rf \
      "${ml_prebundle}/torch" \
      "${ml_prebundle}/torchgen" \
      "${ml_prebundle}/functorch" \
      "${ml_prebundle}/triton" \
      "${ml_prebundle}/nvidia" \
      "${ml_prebundle}/torchaudio" \
      "${ml_prebundle}/torchvision" \
      "${ml_prebundle}/torchvision.libs" \
      "${ml_prebundle}"/torch-*.dist-info \
      "${ml_prebundle}"/triton-*.dist-info \
      "${ml_prebundle}"/nvidia_*.dist-info \
      "${ml_prebundle}"/torchaudio-*.dist-info \
      "${ml_prebundle}"/torchvision-*.dist-info \
    && mv \
      /opt/veoveo/python/torch \
      /opt/veoveo/python/torchgen \
      /opt/veoveo/python/functorch \
      /opt/veoveo/python/triton \
      /opt/veoveo/python/nvidia \
      /opt/veoveo/python/torch-*.dist-info \
      /opt/veoveo/python/triton-*.dist-info \
      /opt/veoveo/python/nvidia_*.dist-info \
      "${ml_prebundle}/" \
    && mkdir -p "${ml_prebundle}/nvidia/cuda_nvrtc/lib" \
    && mv \
      "${rtx_nvrtc_builtins}"/* \
      "${ml_prebundle}/nvidia/cuda_nvrtc/lib/" \
    && rmdir "${rtx_nvrtc_builtins}"

# Isaac Sim discovers Warp and Newton through Kit extension roots. Replace
# those payloads atomically so Kit-first and pre-Kit imports resolve the same
# selected packages and native bindings.
RUN curl --fail --location --show-error \
      --output /tmp/warp_lang-1.15.0-py3-none-manylinux_2_28_x86_64.whl \
      "${WARP_WHEEL_URL}" \
    && echo "${WARP_WHEEL_SHA256}  /tmp/warp_lang-1.15.0-py3-none-manylinux_2_28_x86_64.whl" \
      | sha256sum --check --strict \
    && curl --fail --location --show-error \
      --output /tmp/newton-1.4.0-py3-none-any.whl \
      "${NEWTON_WHEEL_URL}" \
    && echo "${NEWTON_WHEEL_SHA256}  /tmp/newton-1.4.0-py3-none-any.whl" \
      | sha256sum --check --strict \
    && curl --fail --location --show-error \
      --output /tmp/mujoco-3.10.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl \
      "${MUJOCO_WHEEL_URL}" \
    && echo "${MUJOCO_WHEEL_SHA256}  /tmp/mujoco-3.10.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl" \
      | sha256sum --check --strict \
    && curl --fail --location --show-error \
      --output /tmp/mujoco_warp-3.10.0.3-py3-none-any.whl \
      "${MUJOCO_WARP_WHEEL_URL}" \
    && echo "${MUJOCO_WARP_WHEEL_SHA256}  /tmp/mujoco_warp-3.10.0.3-py3-none-any.whl" \
      | sha256sum --check --strict \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /tmp/veoveo-warp \
      /tmp/warp_lang-1.15.0-py3-none-manylinux_2_28_x86_64.whl \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /tmp/veoveo-newton \
      /tmp/newton-1.4.0-py3-none-any.whl \
      /tmp/mujoco-3.10.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl \
      /tmp/mujoco_warp-3.10.0.3-py3-none-any.whl \
    && warp_extension=/isaac-sim/extscache/omni.warp.core-1.13.0+lx64 \
    && newton_prebundle=/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle \
    && rm -rf \
      "${warp_extension}/warp" \
      "${warp_extension}"/warp_lang-*.dist-info \
      "${newton_prebundle}/newton" \
      "${newton_prebundle}/mujoco" \
      "${newton_prebundle}/mujoco_warp" \
      "${newton_prebundle}"/newton-*.dist-info \
      "${newton_prebundle}"/mujoco-*.dist-info \
      "${newton_prebundle}"/mujoco_warp-*.dist-info \
    && cp -a /tmp/veoveo-warp/warp "${warp_extension}/warp" \
    && cp -a /tmp/veoveo-warp/warp_lang-*.dist-info "${warp_extension}/" \
    && cp -a /tmp/veoveo-newton/. "${newton_prebundle}/" \
    && sed -i \
      's/^version = "1\.13\.0"$/version = "1.15.0"/' \
      "${warp_extension}/config/extension.toml" \
    && grep -Fx 'version = "1.15.0"' \
      "${warp_extension}/config/extension.toml" \
    && rm -rf \
      /tmp/veoveo-warp \
      /tmp/veoveo-newton \
      /tmp/warp_lang-1.15.0-py3-none-manylinux_2_28_x86_64.whl \
      /tmp/newton-1.4.0-py3-none-any.whl \
      /tmp/mujoco-3.10.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl \
      /tmp/mujoco_warp-3.10.0.3-py3-none-any.whl

# The latest Isaac Lab release supporting Isaac Sim 6.0.1 is a pre-release.
# Its immutable source is retained because the supported core, PhysX, Newton,
# OV, frame-view, and renderer modules are consumed directly by overlays.
RUN curl --fail --location --show-error \
      --output /tmp/isaaclab.tar.gz \
      "${ISAAC_LAB_ARCHIVE_URL}" \
    && echo "${ISAAC_LAB_ARCHIVE_SHA256}  /tmp/isaaclab.tar.gz" \
      | sha256sum --check --strict \
    && mkdir -p /opt/veoveo/isaaclab \
    && tar --extract --gzip \
      --file /tmp/isaaclab.tar.gz \
      --directory /opt/veoveo/isaaclab \
      --strip-components=1 \
    && test "$(cat /opt/veoveo/isaaclab/VERSION)" = "3.0.0" \
    && printf '%s\n' "${ISAAC_LAB_REVISION}" \
      > /opt/veoveo/isaaclab/.veoveo-source-revision \
    && ln -s /isaac-sim /opt/veoveo/isaaclab/_isaac_sim \
    && rm /tmp/isaaclab.tar.gz

RUN curl --fail --location --show-error \
      --output /tmp/omni.kit.livestream.aov.zip \
      "${NVIDIA_LIVESTREAM_AOV_URL}" \
    && echo "${NVIDIA_LIVESTREAM_AOV_SHA256}  /tmp/omni.kit.livestream.aov.zip" \
      | sha256sum --check --strict \
    && mkdir -p \
      "/isaac-sim/extscache/omni.kit.livestream.aov-${NVIDIA_LIVESTREAM_AOV_VERSION}+${NVIDIA_LIVESTREAM_AOV_BUILD}" \
    && unzip -q /tmp/omni.kit.livestream.aov.zip \
      -d "/isaac-sim/extscache/omni.kit.livestream.aov-${NVIDIA_LIVESTREAM_AOV_VERSION}+${NVIDIA_LIVESTREAM_AOV_BUILD}" \
    && config="/isaac-sim/extscache/omni.kit.livestream.aov-${NVIDIA_LIVESTREAM_AOV_VERSION}+${NVIDIA_LIVESTREAM_AOV_BUILD}/config/extension.toml" \
    && echo "${NVIDIA_LIVESTREAM_AOV_CONFIG_SHA256}  ${config}" \
      | sha256sum --check --strict \
    && sed -i \
      '/^# Default values for settings applied to the first spectator stream/,$d' \
      "${config}" \
    && ! grep -q 'spectatorStream' "${config}" \
    && rm /tmp/omni.kit.livestream.aov.zip

RUN groupadd --gid 10001 veoveo \
    && useradd \
      --uid 10001 \
      --gid 10001 \
      --create-home \
      --home-dir /var/lib/veoveo \
      veoveo \
    && mkdir -p \
      /isaac-sim/kit/cache \
      /isaac-sim/kit/data \
      /var/lib/veoveo/.cache/nvidia/GLCache \
      /var/lib/veoveo/.cache/matplotlib \
      /var/lib/veoveo/.cache/ov \
      /var/lib/veoveo/.cache/pip \
      /var/lib/veoveo/.local/share/ov/data \
      /var/lib/veoveo/.nv/ComputeCache \
      /var/lib/veoveo/.nvidia-omniverse/logs \
      /var/lib/veoveo/runtime-cache \
    && chown -R 10001:10001 \
      /isaac-sim/kit/cache \
      /isaac-sim/kit/data \
      /var/lib/veoveo \
    && chmod 0755 /isaac-sim

COPY --chmod=0555 probes/ /opt/veoveo/simulation-runtime/probes/
COPY simulation-runtime.lock.json /opt/veoveo/simulation-base/simulation-runtime.lock.json
COPY veoveo_simulation_base/ /opt/veoveo/simulation-base/veoveo_simulation_base/
RUN echo "73b6d5e8f8d8d263d63c42ba3c708738fcf886d6b398ee9df2c0013733d419c0  /opt/veoveo/simulation-base/simulation-runtime.lock.json" \
      | sha256sum --check --strict \
    && printf '%s\n' \
      "73b6d5e8f8d8d263d63c42ba3c708738fcf886d6b398ee9df2c0013733d419c0" \
      > /opt/veoveo/simulation-base/simulation-runtime.lock.sha256

ENV ACCEPT_EULA=Y \
    HOME=/var/lib/veoveo \
    ISAACLAB_PATH=/opt/veoveo/isaaclab \
    MPLCONFIGDIR=/var/lib/veoveo/.cache/matplotlib \
    PYTHONPATH=/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle:/isaac-sim/extscache/omni.warp.core-1.13.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/simulation-base:/opt/veoveo/python:/opt/veoveo/isaaclab/source/isaaclab:/opt/veoveo/isaaclab/source/isaaclab_newton:/opt/veoveo/isaaclab/source/isaaclab_physx:/opt/veoveo/isaaclab/source/isaaclab_ov:/opt/veoveo/isaaclab/source/isaaclab_ovphysx \
    VEOVEO_SIMULATION_RUNTIME_PROFILE=isaac-6.0.1-lab-3.0.0-beta2.patch1-warp-1.15.0-newton-1.4.0 \
    XDG_CACHE_HOME=/var/lib/veoveo/.cache \
    XDG_DATA_HOME=/var/lib/veoveo/.local/share

RUN /isaac-sim/python.sh /opt/veoveo/simulation-runtime/probes/identity.py

USER 10001:10001
CMD ["/bin/bash"]

FROM payload AS runtime

ARG SOURCE_REVISION=local
ARG COMPATIBILITY_RELEASE=2026.07.0
LABEL org.opencontainers.image.title="Veoveo canonical GPU simulation runtime" \
      org.opencontainers.image.description="Immutable Isaac Sim, Isaac Lab, Warp, Newton, MuJoCo, RTX, and live-stream compatibility lineage" \
      org.opencontainers.image.revision="${SOURCE_REVISION}" \
      org.opencontainers.image.source="https://github.com/BiomaAI/veoveo" \
      veoveo.io/compatibility-release="${COMPATIBILITY_RELEASE}" \
      veoveo.io/isaac-sim-version="6.0.1" \
      veoveo.io/isaac-lab-revision="${ISAAC_LAB_REVISION}" \
      veoveo.io/torch-version="2.12.0+cu130" \
      veoveo.io/warp-version="1.15.0" \
      veoveo.io/newton-version="1.4.0"
