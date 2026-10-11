# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
FROM scratch AS entrypoint
COPY --chmod=0555 entrypoint.py /entrypoint.py
COPY --chmod=0555 launch.py /launch.py

FROM uav-sim-dependencies AS runtime

COPY --link --chown=10001:10001 veoveo_uav_sim/ /opt/veoveo/uav-sim/veoveo_uav_sim/
COPY --link --chown=10001:10001 assets/ /opt/veoveo/uav-sim/assets/
COPY --link --from=entrypoint --chown=10001:10001 /entrypoint.py /opt/veoveo/uav-sim/entrypoint.py
COPY --link --from=entrypoint --chown=10001:10001 /launch.py /opt/veoveo/uav-sim/launch.py
COPY --link --chown=10001:10001 uav-overlay.identity.json /opt/veoveo/simulation-overlay/identity.json

ARG SOURCE_REVISION=local
LABEL org.opencontainers.image.revision="${SOURCE_REVISION}" \
      ai.veoveo.simulation.overlay="first_party_uav" \
      ai.veoveo.simulation.base-lock="sha256:0eb702b852276deb0d3c6a2c72c2fe5cc14d8fa32c82a3b76ccbf31e37525d33"

ENTRYPOINT ["/isaac-sim/kit/python/bin/python3", "/opt/veoveo/uav-sim/launch.py"]
