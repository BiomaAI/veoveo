# syntax=docker/dockerfile:1.7
ARG TARGETPLATFORM=linux/amd64
ARG PX4_BUILDER_IMAGE=px4io/px4-dev:v1.17.0@sha256:82e3ddcf4bf3fa4dbc0be6489c6de4158d0efbc8530182c691184302027335ce
ARG ISAAC_SIM_IMAGE=nvcr.io/nvidia/isaac-sim:6.0.1@sha256:b1c542b2ecc549b3d1ebb78c25664aa3bacba1709e6ad8e0a68e09426d57dedb

FROM --platform=${TARGETPLATFORM} ${PX4_BUILDER_IMAGE} AS upstream

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

FROM --platform=${TARGETPLATFORM} ${ISAAC_SIM_IMAGE}

ARG RERUN_SDK_VERSION=0.34.1
ARG AIOHTTP_VERSION=3.14.1
ARG AV_VERSION=18.0.0
ARG PYMAVLINK_VERSION=2.4.49
ARG PYYAML_VERSION=6.0.3
ARG TOML_VERSION=0.10.2
ARG PSUTIL_VERSION=7.2.2
ARG PYARROW_VERSION=25.0.0
ARG LXML_VERSION=6.1.1
ARG FASTCRC_VERSION=0.3.6

USER root
RUN /isaac-sim/python.sh -m pip install --no-cache-dir --no-deps \
      "aiohttp==${AIOHTTP_VERSION}" \
      "av==${AV_VERSION}" \
      "fastcrc==${FASTCRC_VERSION}" \
      "lxml==${LXML_VERSION}" \
      "psutil==${PSUTIL_VERSION}" \
      "pyarrow==${PYARROW_VERSION}" \
      "pymavlink==${PYMAVLINK_VERSION}" \
      "PyYAML==${PYYAML_VERSION}" \
      "rerun-sdk==${RERUN_SDK_VERSION}" \
      "toml==${TOML_VERSION}"

COPY --from=upstream /src/cesium/ /opt/veoveo/extensions/
COPY --from=upstream /src/pegasus/extensions/pegasus.simulator/ /opt/veoveo/extensions/pegasus.simulator/
COPY --from=upstream /src/px4/ /opt/veoveo/px4/
COPY veoveo_uav_sim/ /opt/veoveo/uav-sim/veoveo_uav_sim/
COPY --chmod=0555 entrypoint.py /opt/veoveo/uav-sim/entrypoint.py

RUN groupadd --gid 10001 veoveo \
    && useradd --uid 10001 --gid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/.cache/ov /var/lib/veoveo/.local/share/ov /var/lib/veoveo/.nvidia-omniverse \
    && chown -R 10001:10001 /var/lib/veoveo /opt/veoveo

ENV ACCEPT_EULA=Y \
    HOME=/var/lib/veoveo \
    PYTHONPATH=/opt/veoveo/uav-sim \
    UAV_SIM_EXTENSION_DIRECTORY=/opt/veoveo/extensions \
    UAV_SIM_PX4_DIRECTORY=/opt/veoveo/px4 \
    XDG_CACHE_HOME=/var/lib/veoveo/.cache \
    XDG_DATA_HOME=/var/lib/veoveo/.local/share

EXPOSE 8810
USER 10001:10001
ENTRYPOINT ["/isaac-sim/python.sh", "/opt/veoveo/uav-sim/entrypoint.py"]
