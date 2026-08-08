# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG TARGETPLATFORM=linux/amd64
ARG PX4_BUILDER_IMAGE=px4io/px4-dev:v1.17.0@sha256:82e3ddcf4bf3fa4dbc0be6489c6de4158d0efbc8530182c691184302027335ce
ARG SIMULATION_RUNTIME_IMAGE=veoveo/simulation-runtime:2026.07.0

FROM --platform=${TARGETPLATFORM} ${PX4_BUILDER_IMAGE} AS uav-upstream

ARG PX4_COMMIT=d6f12ad1c4f70ad3230afd7d86e971421e02fef4
ARG PEGASUS_COMMIT=644da37e9d5268e5f9a34e78bdcfd57a8bab82b4
ARG CESIUM_VERSION=0.29.0
ARG CESIUM_SHA256=fe38d6195974620e87d7cdad32bb4b38d5933389523d56e75bbae23bcf4da55f

WORKDIR /src
RUN git clone https://github.com/PX4/PX4-Autopilot.git px4 \
    && git -C px4 checkout --detach "${PX4_COMMIT}" \
    && git -C px4 submodule update --init --recursive
COPY patches/px4-1.17.0-multi-instance-gcs.patch /tmp/px4.patch
RUN git -C px4 apply --check /tmp/px4.patch \
    && git -C px4 apply /tmp/px4.patch \
    && make -C px4 px4_sitl_default \
    && rm -rf px4/.git px4/build/px4_sitl_default/build_gtest

RUN git clone https://github.com/PegasusSimulator/PegasusSimulator.git pegasus \
    && git -C pegasus checkout --detach "${PEGASUS_COMMIT}" \
    && find pegasus/extensions/pegasus.simulator -type f \
      \( -name '*.py' -o -name '*.toml' \) \
      -exec sed -i 's/[[:space:]]\+$//' {} +
COPY patches/pegasus-5.1.0-isaac-6.0.1.patch /tmp/pegasus.patch
COPY patches/pegasus-5.1.0-hot-path.patch /tmp/pegasus-hot-path.patch
RUN git -C pegasus apply --unidiff-zero --check /tmp/pegasus.patch \
    && git -C pegasus apply --unidiff-zero /tmp/pegasus.patch \
    && git -C pegasus apply --check /tmp/pegasus-hot-path.patch \
    && git -C pegasus apply /tmp/pegasus-hot-path.patch \
    && rm -rf pegasus/.git

RUN curl --fail --location --show-error \
      --output /tmp/cesium.zip \
      "https://github.com/CesiumGS/cesium-omniverse/releases/download/v${CESIUM_VERSION}/CesiumGS-cesium-omniverse-linux-x86_64-v${CESIUM_VERSION}.zip" \
    && echo "${CESIUM_SHA256}  /tmp/cesium.zip" | sha256sum --check --strict \
    && mkdir -p /src/cesium \
    && unzip -q /tmp/cesium.zip -d /src/cesium
COPY patches/cesium-0.29.0-preinstalled-vendor.patch /tmp/cesium.patch
COPY patches/cesium-0.29.0-external-viewports.patch /tmp/cesium-viewports.patch
RUN patch --directory=/src/cesium --strip=1 < /tmp/cesium.patch
RUN patch --directory=/src/cesium --strip=1 < /tmp/cesium-viewports.patch

FROM --platform=${TARGETPLATFORM} ${SIMULATION_RUNTIME_IMAGE} AS uav-overlay

ARG RERUN_SDK_VERSION=0.35.0
ARG AIOHAPPYEYEBALLS_VERSION=2.7.1
ARG AIOHTTP_VERSION=3.14.1
ARG AIOSIGNAL_VERSION=1.4.0
ARG ATTRS_VERSION=26.1.0
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

USER root
COPY --from=uav-upstream --chown=10001:10001 /src/cesium/ /opt/veoveo/extensions/
RUN /isaac-sim/python.sh -m pip install \
      --disable-pip-version-check \
      --no-cache-dir \
      --no-deps \
      --target /opt/veoveo/uav-python \
      "aiohappyeyeballs==${AIOHAPPYEYEBALLS_VERSION}" \
      "aiohttp==${AIOHTTP_VERSION}" \
      "aiosignal==${AIOSIGNAL_VERSION}" \
      "attrs==${ATTRS_VERSION}" \
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
      /opt/veoveo/extensions/cesium.omniverse/vendor/lxml-6.0.2-cp312-cp312-manylinux_2_26_x86_64.manylinux_2_28_x86_64.whl \
    && chown -R 10001:10001 /opt/veoveo/uav-python

COPY --from=uav-upstream --chown=10001:10001 /src/pegasus/extensions/pegasus.simulator/ /opt/veoveo/extensions/pegasus.simulator/
COPY --from=uav-upstream --chown=10001:10001 /src/px4/ /opt/veoveo/px4/
RUN PYTHONPATH=/opt/veoveo/uav-python/rerun_sdk:/opt/veoveo/uav-python \
    /isaac-sim/python.sh -c \
      'import rerun; assert rerun.__version__ == "0.35.0"'

ENV MAVLINK20=1 \
    PYTHONPATH=/opt/veoveo/uav-sim:/opt/veoveo/uav-python/rerun_sdk:/opt/veoveo/uav-python:/opt/veoveo/simulation-overlay:${PYTHONPATH} \
    ROS_LOG_DIR=/tmp/ros/log \
    UAV_SIM_EXTENSION_DIRECTORY=/opt/veoveo/extensions \
    UAV_SIM_PX4_DIRECTORY=/opt/veoveo/px4

EXPOSE 8810
USER 10001:10001

FROM uav-overlay AS runtime

COPY --chown=10001:10001 veoveo_uav_sim/ /opt/veoveo/uav-sim/veoveo_uav_sim/
COPY --chown=10001:10001 --chmod=0555 entrypoint.py /opt/veoveo/uav-sim/entrypoint.py
COPY --chown=10001:10001 uav-overlay.identity.json /opt/veoveo/simulation-overlay/identity.json

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.overlay="first_party_uav" \
      io.veoveo.simulation.base-lock="sha256:73b6d5e8f8d8d263d63c42ba3c708738fcf886d6b398ee9df2c0013733d419c0"

ENTRYPOINT ["/isaac-sim/python.sh", "/opt/veoveo/uav-sim/entrypoint.py"]
