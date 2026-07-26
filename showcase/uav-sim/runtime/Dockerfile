# syntax=docker/dockerfile:1.7
ARG TARGETPLATFORM=linux/amd64
ARG PX4_BUILDER_IMAGE=px4io/px4-dev:v1.17.0@sha256:82e3ddcf4bf3fa4dbc0be6489c6de4158d0efbc8530182c691184302027335ce
ARG ISAAC_SIM_IMAGE=nvcr.io/nvidia/isaac-sim:6.0.1@sha256:b1c542b2ecc549b3d1ebb78c25664aa3bacba1709e6ad8e0a68e09426d57dedb
ARG UAV_SIM_BASE_IMAGE=veoveo/uav-sim-base:isaac-6.0.1-isaaclab-3.0.0-beta2.patch1-warp-1.15.0-newton-1.4.0

FROM --platform=${TARGETPLATFORM} ${ISAAC_SIM_IMAGE} AS isaac-lab-source

ARG ISAAC_LAB_TAG=v3.0.0-beta2.patch1
ARG ISAAC_LAB_COMMIT=ffff603eafc6b74264a5261cc0183d6a65390d78

USER root
WORKDIR /src/isaaclab
ADD --checksum=sha256:5c1a0dc5e3a69f10ade3c9d8f6a5e702e609d1dc896c9c9f011c667b6ea38eb0 \
  https://github.com/isaac-sim/IsaacLab/archive/refs/tags/v3.0.0-beta2.patch1.tar.gz \
  /tmp/isaaclab.tar.gz
RUN tar --extract --gzip --file /tmp/isaaclab.tar.gz --strip-components=1 \
    && rm /tmp/isaaclab.tar.gz
COPY patches/apply_isaaclab_runtime_tuple.py /tmp/apply_isaaclab_runtime_tuple.py
RUN /isaac-sim/python.sh /tmp/apply_isaaclab_runtime_tuple.py /src/isaaclab \
    && printf '%s\n' "${ISAAC_LAB_COMMIT}" > .veoveo-source-revision

FROM --platform=${TARGETPLATFORM} ${PX4_BUILDER_IMAGE} AS uav-upstream

ARG PX4_COMMIT=d6f12ad1c4f70ad3230afd7d86e971421e02fef4
ARG PEGASUS_COMMIT=644da37e9d5268e5f9a34e78bdcfd57a8bab82b4
ARG CESIUM_VERSION=0.29.0
ARG CESIUM_SHA256=fe38d6195974620e87d7cdad32bb4b38d5933389523d56e75bbae23bcf4da55f

WORKDIR /src
RUN git clone https://github.com/PX4/PX4-Autopilot.git px4 \
    && git -C px4 checkout --detach "${PX4_COMMIT}" \
    && git -C px4 submodule update --init --recursive \
    && make -C px4 px4_sitl_default \
    && rm -rf px4/.git px4/build/px4_sitl_default/build_gtest

RUN git clone https://github.com/PegasusSimulator/PegasusSimulator.git pegasus \
    && git -C pegasus checkout --detach "${PEGASUS_COMMIT}" \
    && find pegasus/extensions/pegasus.simulator -type f \
      \( -name '*.py' -o -name '*.toml' \) \
      -exec sed -i 's/[[:space:]]\+$//' {} +
COPY patches/pegasus-5.1.0-isaac-6.0.1.patch /tmp/pegasus.patch
RUN git -C pegasus apply --unidiff-zero --check /tmp/pegasus.patch \
    && git -C pegasus apply --unidiff-zero /tmp/pegasus.patch \
    && rm -rf pegasus/.git

RUN curl --fail --location --show-error \
      --output /tmp/cesium.zip \
      "https://github.com/CesiumGS/cesium-omniverse/releases/download/v${CESIUM_VERSION}/CesiumGS-cesium-omniverse-linux-x86_64-v${CESIUM_VERSION}.zip" \
    && echo "${CESIUM_SHA256}  /tmp/cesium.zip" | sha256sum --check --strict \
    && mkdir -p /src/cesium \
    && unzip -q /tmp/cesium.zip -d /src/cesium
COPY patches/cesium-0.29.0-preinstalled-vendor.patch /tmp/cesium.patch
RUN patch --directory=/src/cesium --strip=1 < /tmp/cesium.patch

FROM --platform=${TARGETPLATFORM} ${ISAAC_SIM_IMAGE} AS runtime-base

ARG SIMULATION_RUNTIME_LOCK_SHA256=0a27bf50bc5334dcbefe8488a74dc57c4514a0a13921f037e1b5e03b4702b5d8
ARG ISAAC_LAB_COMMIT=ffff603eafc6b74264a5261cc0183d6a65390d78
ARG WARP_VERSION=1.15.0
ARG NEWTON_VERSION=1.4.0
ARG MUJOCO_VERSION=3.10.0
ARG MUJOCO_WARP_VERSION=3.10.0.3
ARG SOURCE_REVISION=local

USER root
COPY --from=isaac-lab-source /src/isaaclab/ /opt/veoveo/isaaclab/
COPY isaaclab-build-requirements.txt /tmp/isaaclab-build-requirements.txt
RUN /isaac-sim/kit/python/bin/python3 -m pip install \
      --no-cache-dir \
      --no-deps \
      --require-hashes \
      --target /tmp/isaaclab-build-backend \
      --requirement /tmp/isaaclab-build-requirements.txt \
    && test "$(cat /opt/veoveo/isaaclab/.veoveo-source-revision)" = "${ISAAC_LAB_COMMIT}" \
    && for package in /opt/veoveo/isaaclab/source/*; do \
         if [ -f "${package}/setup.py" ]; then \
           PYTHONPATH=/tmp/isaaclab-build-backend \
             /isaac-sim/kit/python/bin/python3 -m pip install \
             --no-cache-dir \
             --no-deps \
             --no-build-isolation \
             "${package}" || exit 1; \
         fi; \
       done \
    && site_packages=/isaac-sim/kit/python/lib/python3.12/site-packages \
    && find "${site_packages}" \
      -mindepth 1 \
      -maxdepth 1 \
      -type d \
      -name 'isaaclab*' \
      ! -name '*.dist-info' \
      -exec rm -rf {} + \
    && find /opt/veoveo/isaaclab/source \
      -mindepth 1 \
      -maxdepth 1 \
      -type d \
      -printf '%p\n' \
      | sort > "${site_packages}/veoveo-isaaclab-source.pth" \
    && test "$(/isaac-sim/kit/python/bin/python3 -c 'import importlib.metadata; print(importlib.metadata.version("isaaclab"))')" = "6.1.14" \
    && case "$(/isaac-sim/kit/python/bin/python3 -c 'import isaaclab; print(isaaclab.__file__)')" in \
         /opt/veoveo/isaaclab/source/*) ;; \
         *) exit 1 ;; \
       esac \
    && rm -rf /tmp/isaaclab-build-backend /tmp/isaaclab-build-requirements.txt

COPY canonical-requirements.txt /tmp/canonical-requirements.txt
RUN /isaac-sim/kit/python/bin/python3 -m pip download \
      --dest /tmp/canonical-wheels \
      --no-deps \
      --only-binary=:all: \
      --require-hashes \
      --requirement /tmp/canonical-requirements.txt \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --no-index \
      --no-deps \
      --target /tmp/warp-payload \
      "/tmp/canonical-wheels/warp_lang-${WARP_VERSION}-py3-none-manylinux_2_28_x86_64.whl" \
    && /isaac-sim/kit/python/bin/python3 -m pip install \
      --no-index \
      --no-deps \
      --target /tmp/newton-payload \
      "/tmp/canonical-wheels/newton-${NEWTON_VERSION}-py3-none-any.whl" \
      "/tmp/canonical-wheels/mujoco-${MUJOCO_VERSION}-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl" \
      "/tmp/canonical-wheels/mujoco_warp-${MUJOCO_WARP_VERSION}-py3-none-any.whl" \
    && warp_extension=/isaac-sim/extscache/omni.warp.core-1.13.0+lx64 \
    && newton_prebundle=/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle \
    && test -d "${warp_extension}/warp" \
    && test -d "${newton_prebundle}/newton" \
    && rm -rf \
      "${warp_extension}/warp" \
      "${warp_extension}"/warp_lang-*.dist-info \
      "${newton_prebundle}/newton" \
      "${newton_prebundle}/mujoco" \
      "${newton_prebundle}/mujoco_warp" \
      "${newton_prebundle}"/newton-*.dist-info \
      "${newton_prebundle}"/mujoco-*.dist-info \
      "${newton_prebundle}"/mujoco_warp-*.dist-info \
    && cp -a /tmp/warp-payload/warp "${warp_extension}/warp" \
    && cp -a /tmp/warp-payload/warp_lang-*.dist-info "${warp_extension}/" \
    && cp -a /tmp/newton-payload/. "${newton_prebundle}/" \
    && sed -i \
      "s/^version = \"1\\.13\\.0\"$/version = \"${WARP_VERSION}\"/" \
      "${warp_extension}/config/extension.toml" \
    && grep -Fx "version = \"${WARP_VERSION}\"" "${warp_extension}/config/extension.toml" \
    && canonical_pythonpath="${warp_extension}:${newton_prebundle}" \
    && test "$(PYTHONPATH="${canonical_pythonpath}" /isaac-sim/kit/python/bin/python3 -c 'import importlib.metadata; print(importlib.metadata.version("warp-lang"))')" = "${WARP_VERSION}" \
    && test "$(PYTHONPATH="${canonical_pythonpath}" /isaac-sim/kit/python/bin/python3 -c 'import importlib.metadata; print(importlib.metadata.version("newton"))')" = "${NEWTON_VERSION}" \
    && test "$(PYTHONPATH="${canonical_pythonpath}" /isaac-sim/kit/python/bin/python3 -c 'import importlib.metadata; print(importlib.metadata.version("mujoco"))')" = "${MUJOCO_VERSION}" \
    && test "$(PYTHONPATH="${canonical_pythonpath}" /isaac-sim/kit/python/bin/python3 -c 'import importlib.metadata; print(importlib.metadata.version("mujoco-warp"))')" = "${MUJOCO_WARP_VERSION}" \
    && rm -rf \
      /tmp/canonical-requirements.txt \
      /tmp/canonical-wheels \
      /tmp/warp-payload \
      /tmp/newton-payload

COPY simulation-runtime.lock.json /opt/veoveo/simulation-base/simulation-runtime.lock.json
COPY veoveo_simulation_base/ /opt/veoveo/simulation-base/veoveo_simulation_base/
RUN echo "${SIMULATION_RUNTIME_LOCK_SHA256}  /opt/veoveo/simulation-base/simulation-runtime.lock.json" \
      | sha256sum --check --strict \
    && printf '%s\n' "${SIMULATION_RUNTIME_LOCK_SHA256}" \
      > /opt/veoveo/simulation-base/simulation-runtime.lock.sha256 \
    && groupadd --gid 10001 veoveo \
    && useradd --uid 10001 --gid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p \
      /isaac-sim/kit/cache \
      /isaac-sim/kit/data \
      /var/lib/veoveo/.cache/ov \
      /var/lib/veoveo/.cache/warp \
      /var/lib/veoveo/.local/share/ov \
      /var/lib/veoveo/.nvidia-omniverse \
    && chown -R 10001:10001 \
      /isaac-sim/kit/cache \
      /isaac-sim/kit/data \
      /opt/veoveo \
      /var/lib/veoveo \
    && chmod 0755 /isaac-sim

LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.profile="isaac-sim-6" \
      io.veoveo.simulation.base-lock="sha256:${SIMULATION_RUNTIME_LOCK_SHA256}" \
      io.veoveo.simulation.isaac-sim="6.0.1" \
      io.veoveo.simulation.isaac-lab="3.0.0-beta2.patch1@${ISAAC_LAB_COMMIT}" \
      io.veoveo.simulation.warp="${WARP_VERSION}" \
      io.veoveo.simulation.newton="${NEWTON_VERSION}" \
      io.veoveo.simulation.mujoco="${MUJOCO_VERSION}" \
      io.veoveo.simulation.mujoco-warp="${MUJOCO_WARP_VERSION}"

ENV ACCEPT_EULA=Y \
    HOME=/var/lib/veoveo \
    NVIDIA_DRIVER_CAPABILITIES=compute,graphics,utility,video \
    PYTHONPATH=/isaac-sim/extscache/omni.warp.core-1.13.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/simulation-base \
    XDG_CACHE_HOME=/var/lib/veoveo/.cache \
    XDG_DATA_HOME=/var/lib/veoveo/.local/share

USER 10001:10001

FROM ${UAV_SIM_BASE_IMAGE} AS uav-overlay

ARG RERUN_SDK_VERSION=0.35.0
ARG AIOHAPPYEYEBALLS_VERSION=2.7.1
ARG AIOHTTP_VERSION=3.14.1
ARG AIOSIGNAL_VERSION=1.4.0
ARG ATTRS_VERSION=26.1.0
ARG AV_VERSION=18.0.0
ARG FROZENLIST_VERSION=1.8.0
ARG IDNA_VERSION=3.18
ARG PYMAVLINK_VERSION=2.4.49
ARG PYYAML_VERSION=6.0.3
ARG TOML_VERSION=0.10.2
ARG TYPING_EXTENSIONS_VERSION=4.16.0
ARG YARL_VERSION=1.24.2
ARG MULTIDICT_VERSION=6.7.1
ARG PROPCACHE_VERSION=0.5.2
ARG PSUTIL_VERSION=7.2.2
ARG PYARROW_VERSION=25.0.0
ARG FASTCRC_VERSION=0.3.6
ARG NVIDIA_LIVESTREAM_AOV_VERSION=10.2.0
ARG NVIDIA_LIVESTREAM_AOV_BUILD=110.1.2.lx64.r.cp312
ARG NVIDIA_LIVESTREAM_AOV_URL=https://d4i3qtqj3r0z5.cloudfront.net/omni.kit.livestream.aov-10.2.0%2B110.1.2.lx64.r.cp312.zip
ARG NVIDIA_LIVESTREAM_AOV_SHA256=e842d29982283787b145f3a1f91210b4b16ecc9fbc7038e970ed4dc4bf482c49
ARG NVIDIA_LIVESTREAM_AOV_CONFIG_SHA256=71081c9a5190fc4ed116e31a97fbdf54423e83834c736ce7883343d7676bfad4

USER root
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
COPY --from=uav-upstream --chown=10001:10001 /src/cesium/ /opt/veoveo/extensions/
RUN /isaac-sim/python.sh -m pip install --no-cache-dir --no-deps \
      "aiohappyeyeballs==${AIOHAPPYEYEBALLS_VERSION}" \
      "aiohttp==${AIOHTTP_VERSION}" \
      "aiosignal==${AIOSIGNAL_VERSION}" \
      "attrs==${ATTRS_VERSION}" \
      "av==${AV_VERSION}" \
      "fastcrc==${FASTCRC_VERSION}" \
      "frozenlist==${FROZENLIST_VERSION}" \
      "idna==${IDNA_VERSION}" \
      "multidict==${MULTIDICT_VERSION}" \
      "propcache==${PROPCACHE_VERSION}" \
      "psutil==${PSUTIL_VERSION}" \
      "pyarrow==${PYARROW_VERSION}" \
      "pymavlink==${PYMAVLINK_VERSION}" \
      "PyYAML==${PYYAML_VERSION}" \
      "rerun-sdk==${RERUN_SDK_VERSION}" \
      "toml==${TOML_VERSION}" \
      "typing-extensions==${TYPING_EXTENSIONS_VERSION}" \
      "yarl==${YARL_VERSION}" \
      /opt/veoveo/extensions/cesium.omniverse/vendor/lxml-6.0.2-cp312-cp312-manylinux_2_26_x86_64.manylinux_2_28_x86_64.whl

COPY --from=uav-upstream --chown=10001:10001 /src/pegasus/extensions/pegasus.simulator/ /opt/veoveo/extensions/pegasus.simulator/
COPY --from=uav-upstream --chown=10001:10001 /src/px4/ /opt/veoveo/px4/

ENV MAVLINK20=1 \
    PYTHONPATH=/isaac-sim/extscache/omni.warp.core-1.13.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/uav-sim:/opt/veoveo/simulation-base \
    UAV_SIM_EXTENSION_DIRECTORY=/opt/veoveo/extensions \
    UAV_SIM_PX4_DIRECTORY=/opt/veoveo/px4

EXPOSE 8810
USER 10001:10001

FROM uav-overlay AS runtime

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.overlay="first_party_uav" \
      io.veoveo.simulation.base-lock="sha256:0a27bf50bc5334dcbefe8488a74dc57c4514a0a13921f037e1b5e03b4702b5d8"

COPY --chown=10001:10001 veoveo_uav_sim/ /opt/veoveo/uav-sim/veoveo_uav_sim/
COPY --chown=10001:10001 --chmod=0555 entrypoint.py /opt/veoveo/uav-sim/entrypoint.py
COPY --chown=10001:10001 uav-overlay.identity.json /opt/veoveo/simulation-overlay/identity.json

ENTRYPOINT ["/isaac-sim/python.sh", "/opt/veoveo/uav-sim/entrypoint.py"]
