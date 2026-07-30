# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
ARG VEOVEO_SIMULATION_BASE

FROM ${VEOVEO_SIMULATION_BASE}

COPY --chown=10001:10001 identity.json /opt/veoveo/simulation-overlay/identity.json
COPY --chown=10001:10001 anonymous_simulation_overlay.py /opt/veoveo/simulation-overlay/anonymous_simulation_overlay.py

ENV PYTHONPATH=/opt/veoveo/simulation-overlay:${PYTHONPATH}

RUN /isaac-sim/python.sh -c \
      'import anonymous_simulation_overlay, isaaclab, isaaclab_newton, veoveo_simulation_base'

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.overlay="anonymous_external" \
      io.veoveo.simulation.base-lock="sha256:a4d6870c0c748ea1804feb06c24decf11c4d05ff154ae0fb0811e9073816545c"

USER 10001:10001
