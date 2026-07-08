# SUMO Traffic-World Showcase

The platform on a real simulator. [SUMO](https://eclipse.dev/sumo/) runs a live
traffic world; a task-native Python MCP server owns the one TraCI connection,
**pushes** the world into the Recording Hub as typed Rerun streams, and exposes
SUMO control as governed `sumo__*` tools — the long operations as MCP tasks the
agent detaches from and wakes on. SUMO is just another hub producer; the hub
never pulls.

```
┌─ sumo (Docker) ────────────┐      ┌─ sumo-mcp (Python, one process) ─────────┐
│ ghcr.io eclipse-sumo 1.27  │TraCI │ owns TraCI · pushes /world/sumo/** to hub │
│ seeded grid, --remote-port │◄────►│ serves sumo__* over streamable HTTP       │
└────────────────────────────┘      │ sync: query_state, set_signal_phase, …   │
                                     │ tasks: run_batch, generate_network, …    │
   push /world/sumo/**  ─────────────┤ events: sim://congestion (subscribe)     │
             │                       └───────────────────────────────────────────┘
             ▼
      hub-spooler → world dataset → hub-catalog (redap)
```

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
