# SUMO Traffic-World Showcase

This showcase connects a real [SUMO](https://eclipse.dev/sumo/) traffic world to
Veoveo. A Rust MCP server owns the single serialized TraCI connection, publishes
typed Rerun frames to the Recording Hub, and exposes governed traffic reads,
actuation, durable tasks, resources, and subscriptions.

The bundled simulation is the MIT-licensed LuST Luxembourg scenario, pinned to a
specific source revision. TraCI is never published to the host. The loopback MCP
port requires the same gateway-signed Ed25519 identity assertion as every other
hosted server.

The pinned upstream SUMO 1.27.1 container is published for `linux/amd64`. Both
showcase images therefore declare that platform explicitly; arm64 Docker hosts
need binfmt/QEMU emulation enabled.

## Capabilities

- `query_state` and `describe_scenario` return typed live-world data.
- `set_signal_phase`, `reroute_vehicle`, `set_edge_speed`, `close_lane`, and
  `open_lane` mutate the serialized live simulation.
- `run_batch` is a durable, task-required operation. Because advancing a live
  simulation cannot be replayed safely after an uncertain interruption, recovery
  classifies it as `interrupted_indeterminate`.
- `generate_network`, `compute_routes`, and `optimize_signals` invoke the real
  SUMO command-line programs as resumable durable tasks. Successful outputs are
  uploaded through a task-bound write capability to the shared artifact plane;
  container paths are never returned.
- `sumo://state` and `sumo://scenario` are typed resources.
- `sumo://congestion` is subscribable and emits resource-update notifications.
- `/world/sumo/**` is continuously pushed to the Recording Hub. The hub never
  polls SUMO.

Task state lives in the required SurrealDB 3.2.0 platform store. The server uses
Veoveo's final task extension and shared task runtime; no deprecated MCP task API,
provider polling, in-memory task registry, or compatibility path is present.

## Run

Unit tests use the deterministic Rust fake driver and do not require SUMO:

```bash
just showcase-sumo-test
```

The push smoke runs the real Recording Hub durability boundary in-process, writes
typed fake-driver frames, and queries the resulting RRD segments:

```bash
just showcase-sumo-smoke
```

Bring up the full self-hosted stack plus the real LuST simulation:

```bash
just showcase-sumo-up
```

The authenticated endpoint is `http://127.0.0.1:8895/sumo/mcp`. Port `8795`
remains the canonical chart MCP port. Normal clients should access SUMO through a
gateway profile that includes the `sumo` server; the loopback port exists for
operator verification and still rejects requests without an internal assertion.

The live verification builds an isolated Compose project, waits for LuST/TraCI,
asserts the unauthenticated boundary, drives a read, actuation, and durable task,
queries the recorded world, and tears the project down. Its smoke-only Compose
projection removes dependency host ports and reserves an available loopback MCP
port, so it can run beside an operator's installation:

```bash
just showcase-sumo-verify
```

Required self-hosted secrets are the same ones documented by the root
`.env.example`. The Rust smoke harness supplies isolated test credentials for its
own disposable project.

## Layout

```text
showcase/sumo/
  compose.showcase.yaml   # overlay for SUMO and sumo-mcp
  compose.smoke.yaml      # host-port isolation for the Rust live smoke
  sim/Dockerfile          # pinned SUMO/LuST TraCI world
  mcp/Cargo.toml          # veoveo-sumo-mcp crate
  mcp/Dockerfile          # pinned Rust build and SUMO runtime
  mcp/src/contract.rs     # typed domain and durable-operation contracts
  mcp/src/driver.rs       # FakeSimDriver and serialized TraciSimDriver
  mcp/src/recording.rs    # typed Recording Hub publisher
  mcp/src/server/         # auth, MCP, task runtime, artifact, and HTTP modules
```
