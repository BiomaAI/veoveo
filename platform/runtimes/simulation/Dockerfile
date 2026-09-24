# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG TARGETPLATFORM=linux/amd64
ARG ISAAC_SIM_IMAGE=nvcr.io/nvidia/isaac-sim:6.1.0@sha256:af1d2b4e75d553bfa27beb5a401198654aa8d607f3b7a6749196e9ce253def20

FROM --platform=${TARGETPLATFORM} ${ISAAC_SIM_IMAGE} AS payload

ARG ISAAC_LAB_REVISION=ae37b028ea415c91ea2bc32609efcd759ed2b974
ARG ISAAC_LAB_ARCHIVE_URL=https://github.com/isaac-sim/IsaacLab/archive/ae37b028ea415c91ea2bc32609efcd759ed2b974.tar.gz
ARG ISAAC_LAB_ARCHIVE_SHA256=323c0c1d8eb30d31022ef56406eba998bd7760631341191c5d15e8b308a5fa5d
ARG NEWTON_WHEEL_URL=https://files.pythonhosted.org/packages/03/a5/fba1e550b36cae939aaaea40612a6f70a754747b5129ef978add2569da49/newton-1.5.2-py3-none-any.whl
ARG NEWTON_WHEEL_SHA256=b432b0db9ee963c1fe570b88952d913b9eb98c9ca190c58f80882f28dd0dd5d6
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
      --requirement /opt/veoveo/simulation-runtime/requirements.lock

# Isaac Sim 6.1 already bundles the selected Torch, Warp, MuJoCo and MuJoCo
# Warp packages. Isaac Lab 3.0 EA needs Newton 1.5.2; replace only that
# package in Kit's Newton root and verify the independent wheel digest.
RUN curl --fail --location --show-error \
      --output /tmp/newton-1.5.2-py3-none-any.whl \
      "${NEWTON_WHEEL_URL}" \
    && echo "${NEWTON_WHEEL_SHA256}  /tmp/newton-1.5.2-py3-none-any.whl" \
      | sha256sum --check --strict \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /tmp/veoveo-newton \
      /tmp/newton-1.5.2-py3-none-any.whl \
    && newton_prebundle=/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle \
    && rm -rf \
      "${newton_prebundle}/newton" \
      "${newton_prebundle}"/newton-*.dist-info \
    && cp -a /tmp/veoveo-newton/. "${newton_prebundle}/" \
    && rm -rf /tmp/veoveo-newton /tmp/newton-1.5.2-py3-none-any.whl

# Isaac Lab 3.0 EA is the current upstream source for Isaac Sim 6.1.
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

COPY --chmod=0555 patches/apply_isaac_newton_1_5.py /tmp/apply-isaac-newton-1.5.py
RUN simulation_manager=/isaac-sim/exts/isaacsim.core.simulation_manager/isaacsim/core/simulation_manager/impl/simulation_manager.py \
    && /isaac-sim/kit/python/bin/python3 \
      /tmp/apply-isaac-newton-1.5.py \
      "${simulation_manager}" \
    && grep -Fq \
      'isaacsim.physics.newton.tensors.create_simulation_view' \
      "${simulation_manager}" \
    && rm /tmp/apply-isaac-newton-1.5.py

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
RUN echo "3c558c7c20c522263ae799264b9de045943d5efd9b6cd0e4727c89a487a7e180  /opt/veoveo/simulation-base/simulation-runtime.lock.json" \
      | sha256sum --check --strict \
    && printf '%s\n' \
      "3c558c7c20c522263ae799264b9de045943d5efd9b6cd0e4727c89a487a7e180" \
      > /opt/veoveo/simulation-base/simulation-runtime.lock.sha256

ENV ACCEPT_EULA=Y \
    HOME=/var/lib/veoveo \
    ISAACLAB_PATH=/opt/veoveo/isaaclab \
    MPLCONFIGDIR=/var/lib/veoveo/.cache/matplotlib \
    PYTHONPATH=/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle:/isaac-sim/exts/isaacsim.pip.nv/pip_prebundle:/isaac-sim/extscache/omni.warp.core-1.16.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/simulation-base:/opt/veoveo/python:/opt/veoveo/isaaclab/source/isaaclab:/opt/veoveo/isaaclab/source/isaaclab_newton:/opt/veoveo/isaaclab/source/isaaclab_physx:/opt/veoveo/isaaclab/source/isaaclab_ov \
    VEOVEO_SIMULATION_RUNTIME_PROFILE=isaac-6.1.0-lab-3.0.0-EA-warp-1.16.0-newton-1.5.2 \
    XDG_CACHE_HOME=/var/lib/veoveo/.cache \
    XDG_DATA_HOME=/var/lib/veoveo/.local/share

RUN /isaac-sim/python.sh /opt/veoveo/simulation-runtime/probes/identity.py
RUN /isaac-sim/python.sh -c 'from isaaclab.app import AppLauncher; assert AppLauncher'

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
      veoveo.io/isaac-sim-version="6.1.0" \
      veoveo.io/isaac-lab-revision="${ISAAC_LAB_REVISION}" \
      veoveo.io/torch-version="2.11.0+cu128" \
      veoveo.io/warp-version="1.16.0" \
      veoveo.io/newton-version="1.5.2"
