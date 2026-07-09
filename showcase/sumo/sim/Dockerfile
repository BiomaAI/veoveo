# syntax=docker/dockerfile:1.7
# SUMO serving TraCI on a real, geo-referenced city: the LuST scenario
# (Luxembourg SUMO Traffic, lcodeca/LuSTScenario, MIT) — a validated OpenStreetMap
# network with a full day of realistic demand and actuated signals. Because the
# network carries a real projection, vehicle positions convert to true lat/lon
# and land on the actual streets in any map view. Internal-network only — TraCI
# has no auth, so the port is never published beyond the compose network.
FROM ghcr.io/eclipse-sumo/sumo:v1_27_1

ENV SUMO_HOME=/usr/share/sumo

# The published LuST scenario, pinned to a commit for reproducibility. Shallow
# clone keeps the layer to the scenario itself.
ARG LUST_REF=master
RUN git clone --depth 1 --branch "$LUST_REF" \
        https://github.com/lcodeca/LuSTScenario.git /lust \
    && rm -rf /lust/.git \
    && chmod -R a+w /lust/scenario

WORKDIR /lust/scenario

# dua.actuated: dynamic user assignment (realistic routes) + actuated traffic
# lights — the richest of the four bundled configs. Start at the morning ramp
# (07:00 = 25200 s) so there is dense traffic the moment a client connects.
EXPOSE 8813
CMD ["sumo", "-c", "dua.actuated.sumocfg", \
     "--begin", "25200", \
     "--remote-port", "8813", \
     "--step-length", "1", \
     "--no-step-log", "true", \
     "--no-warnings", "true", \
     "--time-to-teleport", "300"]
