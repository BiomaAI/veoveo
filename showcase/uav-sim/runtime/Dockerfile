# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM uav-sim-dependencies AS runtime

COPY --chown=10001:10001 veoveo_uav_sim/ /opt/veoveo/uav-sim/veoveo_uav_sim/
COPY --chown=10001:10001 --chmod=0555 entrypoint.py /opt/veoveo/uav-sim/entrypoint.py
COPY --chown=10001:10001 uav-overlay.identity.json /opt/veoveo/simulation-overlay/identity.json

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.overlay="first_party_uav" \
      io.veoveo.simulation.base-lock="sha256:73b6d5e8f8d8d263d63c42ba3c708738fcf886d6b398ee9df2c0013733d419c0"

ENTRYPOINT ["/isaac-sim/python.sh", "/opt/veoveo/uav-sim/entrypoint.py"]
