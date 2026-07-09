# SUMO Traffic-World Showcase

The platform on a real simulator. [SUMO](https://eclipse.dev/sumo/) runs a live
traffic world; a task-native Python MCP server owns the one TraCI connection,
**pushes** the world into the Recording Hub as typed Rerun streams, and exposes
SUMO control as governed `sumo__*` tools — the long operations as MCP tasks the
agent detaches from and wakes on. SUMO is just another hub producer; the hub
never pulls.

```
┌─ sumo (Docker) ────────────┐      ┌─ sumo-mcp (Python, one process) ─────────┐
│ eclipse-sumo 1.27 + LuST   │TraCI │ owns TraCI · pushes /world/sumo/** to hub │
│ real Luxembourg, geo-ref'd │◄────►│ serves sumo__* over streamable HTTP       │
│ --remote-port 8813         │      │ read:  query_state, describe_scenario     │
└────────────────────────────┘      │ act:   set_signal_phase, reroute_vehicle, │
                                     │        set_edge_speed, close/open_lane    │
   push /world/sumo/vehicles  ───────┤ tasks: run_batch, generate_network,       │
   (GeoPoints, real lat/lon)  │      │        compute_routes, optimize_signals   │
             │                       │ events: sim://congestion (subscribe)      │
             ▼                       └───────────────────────────────────────────┘
      hub-spooler → world dataset → hub-catalog (redap)   → Rerun viewer (live map)
```

## The scenario: LuST (real Luxembourg)

The SUMO container runs [**LuST — Luxembourg SUMO Traffic**](https://github.com/lcodeca/LuSTScenario)
(MIT): a validated OpenStreetMap network of Luxembourg City with a full day of
realistic demand and actuated signals, started at the morning ramp (07:00). Because
the network is geo-referenced, vehicle positions convert to true lat/lon and land on
the actual streets in the Rerun map view — `sumo-mcp` calibrates the cartesian→lon/lat
map once from the network's own projection, then reads all vehicles per frame in a
single TraCI subscription round-trip and publishes them as one coloured GeoPoints layer.

## What the agent controls

- **Read** — `query_state` (every vehicle's geo position + speed, signals, mean speed), `describe_scenario`
- **Act** — `set_signal_phase`, `reroute_vehicle`, `set_edge_speed` (variable-speed sign), `close_lane` / `open_lane` (model an incident)
- **Time** — `run_batch` (fast-forward, as a detachable MCP task) — the sleep/wake op
- **Wake** — subscribe `sim://congestion`; a jam pushes `resources/updated`
- **Offline** — `generate_network` / `compute_routes` / `optimize_signals` shell out to the real SUMO CLIs (netgenerate / duarouter / tlsCoordinator)

## Visualize it live

```bash
docker compose -f compose.yaml -f showcase/compose.showcase.yaml \
    --profile hub --profile showcase up -d --build
# then attach a native Rerun viewer to the hub's live proxy on your machine:
rerun --port auto "rerun+http://127.0.0.1:9877/proxy"
```

Cars appear moving on the Luxembourg map, coloured red (congested) → green (free-flowing).

## Why Python, and why our own server

The overwhelming majority of MCP servers are Python, so the showcase
demonstrates a *proper* one: task-native, streamable-HTTP, strongly typed with
pydantic. The public SUMO MCP servers are inspiration only — we rebuild the
taxonomy on the SDK's lowlevel server to get the **task API** (the long ops
return `CreateTaskResult`; the client detaches, polls, and reads the terminal
result — the exact sleep/wake path the agent kernel drives), gateway
projection/auth, and resource subscriptions for wakes.

> The task API is pinned to `mcp==1.28.x` (the SEP-1686 line the workspace's
> Rust gateway/kernel speak) and isolated in `sumo_mcp/tasks_compat.py`, so the
> future migration to the SEP-2663 tasks extension is a localized change.

> ⚠️ Never use `HypaSMarty/SUMO-MCP-Server` — it is a malware lure. The
> functional inspiration is `XRDS76354/SUMO-MCP-Server` (arXiv 2506.03548).

## Layout

```
showcase/
  compose.showcase.yaml     # sumo + sumo-mcp, layered on the hub profile
  sumo/Dockerfile           # headless SUMO + seeded grid scenario, TraCI :8813
  sumo-mcp/                 # the Python server (package veoveo-sumo-mcp)
    src/sumo_mcp/
      sim_driver.py         # SimDriver protocol; Fake (tests) + Traci (live)
      tools.py              # pydantic-typed tools; single-owner serialization
      tasks_compat.py       # the SEP-1686 task seam
      server.py             # lowlevel MCP server: sync + task tools + resources
      streams.py            # push path: /world/sumo/** → hub
      push.py               # sumo-sim: standalone push loop
      runtime.py            # container entry: own TraCI + push + serve
      resources.py          # congestion watch condition
    tests/                  # 16 tests, fake driver, no SUMO needed
```

## Run it

Tests (no SUMO needed — fake driver):

```bash
just test-sumo-mcp
```

Push spine against the real Rust hub (no SUMO needed — fake driver pushes,
the hub captures and QueryEngine reads back):

```bash
just smoke-sumo-push
```

Full live stack (SUMO + sumo-mcp + hub, Docker):

```bash
docker compose -f compose.yaml -f showcase/compose.showcase.yaml \
    --profile hub --profile showcase up --build
# SUMO drives traffic; sumo-mcp pushes /world/sumo/** into the hub and serves
# sumo__* on 127.0.0.1:8795. Query the world via the hub catalog.
```

## The loop it demonstrates

SUMO streams the world into the hub → the agent reads its world model → calls a
task tool (`run_batch`, `optimize_signals`), **detaches and sleeps** → wakes on
the task result → acts (`set_signal_phase`, `reroute_vehicle`) → a
`sim://congestion` resource update wakes the next episode. Every arrow is
machinery that now exists.
