# syntax=docker/dockerfile:1.7
# SUMO serving TraCI on a real, geo-referenced city: the LuST scenario
# (Luxembourg SUMO Traffic, lcodeca/LuSTScenario, MIT) — a validated OpenStreetMap
# network with a full day of realistic demand and actuated signals. Because the
# network carries a real projection, vehicle positions convert to true lat/lon
# and land on the actual streets in any map view. Internal-network only — TraCI
# has no auth, so the port is never published beyond the compose network.
ARG SUMO_PLATFORM=linux/amd64
FROM --platform=${SUMO_PLATFORM} ghcr.io/eclipse-sumo/sumo:v1_27_1@sha256:87623396d3501ca8d0ac25154e202bc38baa55a31c724e4dfd6ae60f297c6bf2

ENV SUMO_HOME=/usr/share/sumo

# The published LuST scenario, pinned to a commit for reproducibility.
ARG LUST_REF=5edb7ecb9ad196c39172b6eb95d19ed789f4b1a6
RUN git init /lust \
    && git -C /lust remote add origin https://github.com/lcodeca/LuSTScenario.git \
    && git -C /lust fetch --depth 1 origin "$LUST_REF" \
    && git -C /lust checkout --detach FETCH_HEAD \
    && rm -rf /lust/.git \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && chown -R veoveo:veoveo /lust/scenario

WORKDIR /lust/scenario
COPY --chmod=0555 run-sumo.sh /usr/local/bin/run-sumo

# dua.actuated: dynamic user assignment (realistic routes) + actuated traffic
# lights — the richest of the four bundled configs. Start at the morning ramp
# (07:00 = 25200 s) so there is dense traffic the moment a client connects.
EXPOSE 8813
USER veoveo
ENTRYPOINT ["/usr/local/bin/run-sumo"]
CMD ["-c", "dua.actuated.sumocfg", \
     "--begin", "25200", \
     "--remote-port", "8813", \
     "--step-length", "1", \
     "--summary-output", "/tmp/dua.actuated.summary.xml", \
     "--tripinfo-output", "/tmp/dua.actuated.tripinfo.xml", \
     "--log", "/tmp/dua.actuated.log", \
     "--no-step-log", "true", \
     "--no-warnings", "true", \
     "--time-to-teleport", "300"]
