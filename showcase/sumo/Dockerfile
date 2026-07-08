# syntax=docker/dockerfile:1.7
# Headless SUMO serving TraCI, with a seeded grid scenario baked in at build so
# a run is deterministic and reproducible. Internal-network only — TraCI has no
# auth, so the port is never published beyond the compose network.
FROM ghcr.io/eclipse-sumo/sumo:v1_27_1

ENV SUMO_HOME=/usr/share/sumo
WORKDIR /scenario

# Generate a deterministic grid network + seeded random trips at build time.
# A geo-projection is applied so /world/sumo/** GeoPoints are real lon/lat.
RUN netgenerate --grid --grid.number 4 --grid.length 200 \
        --tls.guess true \
        --proj "+proj=utm +zone=32 +datum=WGS84" \
        --output-file grid.net.xml \
    && python3 "$SUMO_HOME/tools/randomTrips.py" \
        -n grid.net.xml -o trips.xml -r routes.rou.xml \
        --seed 42 --begin 0 --end 600 --period 2 --validate \
    && printf '%s\n' \
        '<configuration>' \
        '  <input>' \
        '    <net-file value="grid.net.xml"/>' \
        '    <route-files value="routes.rou.xml"/>' \
        '  </input>' \
        '  <time><begin value="0"/><end value="3600"/></time>' \
        '</configuration>' > grid.sumocfg

EXPOSE 8813
# Block waiting for the TraCI client (sumo-mcp) to connect and drive stepping.
CMD ["sumo", "-c", "/scenario/grid.sumocfg", "--remote-port", "8813", \
     "--step-length", "1", "--no-step-log", "true", "--start"]
