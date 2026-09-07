# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
FROM uav-sim-dependencies AS runtime

COPY --link --chown=10001:10001 veoveo_uav_sim/ /opt/veoveo/uav-sim/veoveo_uav_sim/
COPY --link --chown=10001:10001 assets/ /opt/veoveo/uav-sim/assets/
COPY --link --chown=10001:10001 --chmod=0555 entrypoint.py /opt/veoveo/uav-sim/entrypoint.py
COPY --link --chown=10001:10001 uav-overlay.identity.json /opt/veoveo/simulation-overlay/identity.json

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.simulation.overlay="first_party_uav" \
      io.veoveo.simulation.base-lock="sha256:7fa0a425e6e5e2a6aa5fb766d60af10b4eb96985005eb0493d4af65909b3d53b"

ENTRYPOINT ["/isaac-sim/python.sh", "/opt/veoveo/uav-sim/entrypoint.py"]
