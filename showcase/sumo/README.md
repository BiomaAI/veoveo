# SUMO traffic-world showcase

This showcase connects a real [SUMO](https://eclipse.dev/sumo/) traffic world to
Veoveo. A Rust MCP server owns the single serialized TraCI connection, publishes
typed Rerun frames to the Recording Hub, and exposes governed traffic reads,
actuation, durable tasks, resources, and subscriptions.

The bundled simulation is the MIT-licensed LuST Luxembourg scenario at a pinned
source revision. TraCI stays inside the Kubernetes cluster. The loopback MCP
projection requires the same gateway-signed Ed25519 identity assertion as every
other hosted server.

The upstream SUMO 1.27.1 image is published for `linux/amd64`. The showcase
images declare that architecture explicitly.

## Capabilities

- `query_state` and `describe_scenario` return typed live-world data.
- `set_signal_phase`, `reroute_vehicle`, `set_edge_speed`, `close_lane`,
  and `open_lane` mutate the serialized simulation.
- `run_batch` is a durable task. Its recovery class is
  `interrupted_indeterminate` because a live simulation advance cannot be
  replayed safely after an uncertain interruption.
- `generate_network`, `compute_routes`, and `optimize_signals` run SUMO
  programs as resumable durable tasks. Outputs enter the shared artifact plane
  through task-bound write capabilities.
- `sumo://state` and `sumo://scenario` are typed resources.
- `sumo://congestion` supports subscriptions and resource-update notifications.
- `/world/sumo/**` is pushed continuously to Recording Hub.

Task state lives in the required SurrealDB 3.2.1 platform store. The server uses
Veoveo's final task extension and shared task runtime.

## Tests

The deterministic driver unit tests need only the pinned Rust toolchain:

```bash
just showcase-sumo-test
```

The in-process push smoke writes fake-driver frames through the real Recording Hub
durability boundary and queries the resulting RRD segments:

```bash
just showcase-sumo-smoke
```

The live verification targets the active k3d profile. It checks the unauthenticated
boundary, reads the live world, changes an edge speed, advances a durable batch,
and proves that Recording Hub retained the world:

```bash
just showcase-sumo-verify
```

## Run in k3d

Use the latest versions pinned in `deploy/local/k3d/versions.env`. The cluster
profile requires a working NVIDIA container runtime even though SUMO itself does
not request a GPU. This proves that later GPU simulators and renderers can use the
same local cluster.

```bash
just k3d-node-build
just sumo-k3d-create
just showcase-sumo-build
just showcase-sumo-import
just showcase-sumo-resources
just showcase-sumo-platform-up
just showcase-sumo-up
```

Normal clients use the `operator` gateway profile at
`http://localhost:8780/mcp/operator`. Run `just info` to mint the local service
token and inspect the namespaced SUMO surface through that gateway. The direct
authenticated verification endpoint is `http://127.0.0.1:8895/sumo/mcp`; it
exists for the Rust acceptance harness.

SUMO's TraCI server accepts one client. The chart deliberately has no TCP
readiness probe on port 8813 because a probe would consume that connection and
terminate the simulation. `sumo-mcp` owns connection readiness and retries while
the LuST network loads.

The showcase chart leaves telemetry disabled because the minimal local platform
does not install a collector. Set `telemetry.enabled=true` and configure its
endpoint when a profile installs the collector.

## Visualize with Rerun

Local Helm values project Recording Hub to `127.0.0.1:9877`. Put the canonical
Mapbox token in the repository root `.env`:

```dotenv
MAPBOX_ACCESS_TOKEN=pk.example
```

Launch the viewer:

```bash
just showcase-sumo-view
```

The recipe maps the canonical value to Rerun's
`RERUN_MAPBOX_ACCESS_TOKEN` variable and connects to the hub. Add
`/world/sumo/vehicles` to a Map view. The `/world/sumo/network` entity uses
SUMO's local Cartesian frame and belongs in a 3D view.

## Layout

```text
showcase/sumo/
  deploy/
    gateway.json             # SUMO-owned gateway profile
    platform-values.yaml     # minimal platform selection
    helm/                    # SUMO and sumo-mcp release
  sim/
    Dockerfile               # pinned SUMO and LuST world
    run-sumo.sh
  sumo-mcp/
    Cargo.toml
    Dockerfile
    src/contract.rs          # typed domain and task contracts
    src/driver.rs            # deterministic and TraCI drivers
    src/recording.rs         # typed Recording Hub publisher
    src/server/              # auth, MCP, tasks, artifacts, and HTTP
```

Remove only this profile with `just showcase-sumo-down`. The platform remains
available for another simulator profile.
