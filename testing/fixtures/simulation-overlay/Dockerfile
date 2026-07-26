# syntax=docker/dockerfile:1.7
ARG VEOVEO_SIMULATION_BASE

FROM ${VEOVEO_SIMULATION_BASE}

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.overlay="anonymous_external" \
      io.veoveo.simulation.base-lock="sha256:0a27bf50bc5334dcbefe8488a74dc57c4514a0a13921f037e1b5e03b4702b5d8"

COPY --chown=10001:10001 identity.json /opt/veoveo/simulation-overlay/identity.json
COPY --chown=10001:10001 anonymous_simulation_overlay.py /opt/veoveo/simulation-overlay/anonymous_simulation_overlay.py

ENV PYTHONPATH=/isaac-sim/extscache/omni.warp.core-1.13.0+lx64:/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle:/opt/veoveo/simulation-overlay:/opt/veoveo/simulation-base
USER 10001:10001
