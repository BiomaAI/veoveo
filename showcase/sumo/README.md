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

## Local prerequisites

The fake-driver unit test and push smoke need the pinned Rust toolchain and
`just`. They do not need a SUMO installation or Docker.

The live verification also needs Docker Engine with Compose v2. Its host-side
conformance helper builds the bundled PROJ library. On Debian or Ubuntu, install
the native build inputs before the first run:

```bash
sudo apt-get update
sudo apt-get install -y build-essential cmake libsqlite3-dev pkg-config sqlite3
```

Compose validates the complete platform file before selecting the SUMO service.
It therefore requires values for the perception bind mounts even though the live
SUMO verification does not start the perception service. Existing empty
directories are sufficient:

```bash
mkdir -p /tmp/veoveo-perception/config /tmp/veoveo-perception/models
export PERCEPTION_CONFIG_DIR=/tmp/veoveo-perception/config
export PERCEPTION_MODEL_DIR=/tmp/veoveo-perception/models
```

The pinned SUMO runtime is `linux/amd64`. An arm64 Docker host must have
binfmt/QEMU emulation enabled. A cold live run downloads the simulator and
platform images, builds several Rust service images, and needs network access and
several gigabytes of Docker storage.

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
port, so it can run beside an operator's installation. It supplies disposable
platform credentials, while the two perception paths above remain required for
Compose interpolation:

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
