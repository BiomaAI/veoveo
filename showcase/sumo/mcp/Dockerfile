# syntax=docker/dockerfile:1.7
# The task-native SUMO MCP server: owns the TraCI connection, pushes typed Rerun
# streams into the hub, and serves the governed sumo__* tools over streamable
# HTTP. One process, one sim owner.
FROM python:3.11-slim

RUN pip install --no-cache-dir uv

WORKDIR /app
COPY pyproject.toml ./
COPY src ./src

# The full live surface: MCP + task API, the SUMO client, the rerun push path,
# and the HTTP server.
RUN uv pip install --system --no-cache ".[sumo,rerun,serve]"

RUN useradd --system --uid 10001 --create-home --home-dir /home/sumo sumo
USER sumo

EXPOSE 8795
ENTRYPOINT ["sumo-mcp"]
CMD ["--host", "0.0.0.0", "--port", "8795"]
