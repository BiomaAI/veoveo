# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG TARGETPLATFORM=linux/amd64
ARG ISAAC_SIM_IMAGE=nvcr.io/nvidia/isaac-sim:6.0.1@sha256:b1c542b2ecc549b3d1ebb78c25664aa3bacba1709e6ad8e0a68e09426d57dedb

FROM --platform=${TARGETPLATFORM} ${ISAAC_SIM_IMAGE} AS payload

ARG ISAAC_LAB_REVISION=ffff603eafc6b74264a5261cc0183d6a65390d78
ARG ISAAC_LAB_ARCHIVE_URL=https://github.com/isaac-sim/IsaacLab/archive/ffff603eafc6b74264a5261cc0183d6a65390d78.tar.gz
ARG ISAAC_LAB_ARCHIVE_SHA256=fe65b28ad08a943e35005e6c75885929cd6cbd7075c721d8da4d05d97fa392be
ARG WARP_WHEEL_URL=https://files.pythonhosted.org/packages/1d/3d/47feef2eee4739437e19d6949ea231ca4181c9eacbaa39144761e6130d94/warp_lang-1.16.0-py3-none-manylinux_2_28_x86_64.whl
ARG WARP_WHEEL_SHA256=96449fc1e3b354185e2f09434fb794b5953ab2e8673b104d7e7f48d5d418bb35
ARG NEWTON_WHEEL_URL=https://files.pythonhosted.org/packages/ef/a8/706cd18af6c42a47f750a129c2605e7ed9af5800a37665ad78b870d34383/newton-1.5.0-py3-none-any.whl
ARG NEWTON_WHEEL_SHA256=a9eef789e2e0f857e40df3382f101b5faadff5dd8a61cb86dd5ec5382dd27865
ARG MUJOCO_WHEEL_URL=https://files.pythonhosted.org/packages/41/d6/7b79f5d8fbd019658a3b5feb6ffd09c1e727eaf93f518685d3cc105a28f5/mujoco-3.11.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl
ARG MUJOCO_WHEEL_SHA256=f40214fefc8c2fe0002a3c8abadf30de7a3330634f3c45cc29b407b40a0173fc
ARG MUJOCO_WARP_WHEEL_URL=https://files.pythonhosted.org/packages/e6/03/9bf86946c755a9a020c1bea587841472b0206351f73f67e41050fcb2a20c/mujoco_warp-3.11.0-py3-none-any.whl
ARG MUJOCO_WARP_WHEEL_SHA256=97e77e877c1c2ea064ae68eaace2333dec94f2d8984091ba7b383bc8c34958f6
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
      --output /tmp/warp_lang-1.16.0-py3-none-manylinux_2_28_x86_64.whl \
      "${WARP_WHEEL_URL}" \
    && echo "${WARP_WHEEL_SHA256}  /tmp/warp_lang-1.16.0-py3-none-manylinux_2_28_x86_64.whl" \
      | sha256sum --check --strict \
    && curl --fail --location --show-error \
      --output /tmp/newton-1.5.0-py3-none-any.whl \
      "${NEWTON_WHEEL_URL}" \
    && echo "${NEWTON_WHEEL_SHA256}  /tmp/newton-1.5.0-py3-none-any.whl" \
      | sha256sum --check --strict \
    && curl --fail --location --show-error \
      --output /tmp/mujoco-3.11.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl \
      "${MUJOCO_WHEEL_URL}" \
    && echo "${MUJOCO_WHEEL_SHA256}  /tmp/mujoco-3.11.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl" \
      | sha256sum --check --strict \
    && curl --fail --location --show-error \
      --output /tmp/mujoco_warp-3.11.0-py3-none-any.whl \
      "${MUJOCO_WARP_WHEEL_URL}" \
    && echo "${MUJOCO_WARP_WHEEL_SHA256}  /tmp/mujoco_warp-3.11.0-py3-none-any.whl" \
      | sha256sum --check --strict \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /tmp/veoveo-warp \
      /tmp/warp_lang-1.16.0-py3-none-manylinux_2_28_x86_64.whl \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /tmp/veoveo-newton \
      /tmp/newton-1.5.0-py3-none-any.whl \
      /tmp/mujoco-3.11.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl \
      /tmp/mujoco_warp-3.11.0-py3-none-any.whl \
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
      's/^version = "1\.13\.0"$/version = "1.16.0"/' \
      "${warp_extension}/config/extension.toml" \
    && grep -Fx 'version = "1.16.0"' \
      "${warp_extension}/config/extension.toml" \
    && newton_adapter=/isaac-sim/exts/isaacsim.physics.newton \
    && find "${newton_adapter}" \
      -type f \( -name '*.py' -o -name '*.md' \) \
      -exec sed -i '/ls_parallel/d' {} + \
    && ! grep -Rqs 'ls_parallel' "${newton_adapter}" \
    && rm -rf \
      /tmp/veoveo-warp \
      /tmp/veoveo-newton \
      /tmp/warp_lang-1.16.0-py3-none-manylinux_2_28_x86_64.whl \
      /tmp/newton-1.5.0-py3-none-any.whl \
      /tmp/mujoco-3.11.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl \
      /tmp/mujoco_warp-3.11.0-py3-none-any.whl

# The latest Isaac Lab release supporting Isaac Sim 6.0.1 is a pre-release.
# Its immutable source is retained because the supported core, Newton, OV,
# frame-view, and renderer modules are consumed directly by overlays.
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
    && find \
      /opt/veoveo/isaaclab/source/isaaclab/isaaclab \
      /opt/veoveo/isaaclab/source/isaaclab_newton/isaaclab_newton \
      -type f -name '*.py' -exec sed -i \
      's/from newton\.solvers import SolverNotifyFlags/from newton import ModelFlags as SolverNotifyFlags/g' {} + \
    && sed -i \
      's/from newton\.solvers import SolverBase, SolverKamino, SolverNotifyFlags/from newton import ModelFlags as SolverNotifyFlags\nfrom newton.solvers import SolverBase, SolverKamino/' \
      /opt/veoveo/isaaclab/source/isaaclab_newton/isaaclab_newton/physics/newton_manager.py \
    && ! grep -Rqs 'from newton.solvers import SolverNotifyFlags' \
      /opt/veoveo/isaaclab/source/isaaclab/isaaclab \
      /opt/veoveo/isaaclab/source/isaaclab_newton/isaaclab_newton \
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
RUN echo "7fa0a425e6e5e2a6aa5fb766d60af10b4eb96985005eb0493d4af65909b3d53b  /opt/veoveo/simulation-base/simulation-runtime.lock.json" \
      | sha256sum --check --strict \
    && printf '%s\n' \
      "7fa0a425e6e5e2a6aa5fb766d60af10b4eb96985005eb0493d4af65909b3d53b" \
      > /opt/veoveo/simulation-base/simulation-runtime.lock.sha256

ENV ACCEPT_EULA=Y \
    HOME=/var/lib/veoveo \
    ISAACLAB_PATH=/opt/veoveo/isaaclab \
    MPLCONFIGDIR=/var/lib/veoveo/.cache/matplotlib \
    PYTHONPATH=/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle:/isaac-sim/extscache/omni.warp.core-1.13.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/simulation-base:/opt/veoveo/python:/opt/veoveo/isaaclab/source/isaaclab:/opt/veoveo/isaaclab/source/isaaclab_newton:/opt/veoveo/isaaclab/source/isaaclab_physx:/opt/veoveo/isaaclab/source/isaaclab_ov:/opt/veoveo/isaaclab/source/isaaclab_ovphysx \
    VEOVEO_SIMULATION_RUNTIME_PROFILE=isaac-6.0.1-lab-3.0.0-beta2.patch1-warp-1.16.0-newton-1.5.0 \
    XDG_CACHE_HOME=/var/lib/veoveo/.cache \
    XDG_DATA_HOME=/var/lib/veoveo/.local/share

RUN /isaac-sim/python.sh /opt/veoveo/simulation-runtime/probes/identity.py

USER 10001:10001
CMD ["/bin/bash"]

FROM payload AS runtime

ARG SOURCE_REVISION=local
ARG COMPATIBILITY_RELEASE=2026.08.0
LABEL org.opencontainers.image.title="Veoveo canonical GPU simulation runtime" \
      org.opencontainers.image.description="Immutable Isaac Sim, Isaac Lab, Warp, Newton, MuJoCo, RTX, and live-stream compatibility lineage" \
      org.opencontainers.image.revision="${SOURCE_REVISION}" \
      org.opencontainers.image.source="https://github.com/BiomaAI/veoveo" \
      veoveo.io/compatibility-release="${COMPATIBILITY_RELEASE}" \
      veoveo.io/isaac-sim-version="6.0.1" \
      veoveo.io/isaac-lab-revision="${ISAAC_LAB_REVISION}" \
      veoveo.io/torch-version="2.12.0+cu130" \
      veoveo.io/warp-version="1.16.0" \
      veoveo.io/newton-version="1.5.0"
