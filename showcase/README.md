# Reference Integrations

Each directory here is a reference integration: a worked example of connecting an
external system to Veoveo, here a real simulator. A showcase keeps
everything it needs in its own subdirectory: images, MCP server, Helm chart,
profile values, gateway configuration, and acceptance tests. Simulators therefore
do not depend on each other, and a new integration is added as another subdirectory.

| Showcase | What it runs |
|----------|----------------|
| [`sumo/`](sumo/README.md) | The [SUMO](https://eclipse.dev/sumo/) traffic simulator running the Luxembourg LuST scenario. A Rust MCP server holds the single TraCI connection, pushes `/world/sumo/**` to Recording Hub as typed Rerun streams (map and 3D views), and exposes SUMO control as `sumo__*` tools behind the gateway. |
| [`uav-sim/`](uav-sim/README.md) | A four-vehicle PX4 fleet in Isaac Sim over Google Photorealistic 3D Tiles streamed through Cesium ion. Newton and a batched CUDA Warp plant simulate the vehicles. An MCP server manages sessions and missions. The encoded camera feeds Stream directly, and world state is recorded to Recording Hub on a separate path. |

## Adding An Integration

A robot, a fleet, or a simulator connects through the same pattern both reference
integrations use. Its code goes in its own subdirectory here, or in its own server
directory in an installation's fork.

1. Write an MCP server that owns the system's control connection. SUMO's
   `sumo-mcp` is a Rust example. For Python, start from
   [`templates/python-mcp`](../templates/python-mcp) and the SDK in
   [`sdk/python`](../sdk/python/README.md).
2. Expose reads and commands as tools, long runs as tasks with a declared recovery
   class, and watched conditions as subscribable resources.
3. Publish state and sensor data to Recording Hub as Rerun streams through the
   recording forwarder.
4. Publish cameras through the `veoveo.io/live-view/v4` contract.
   [`testing/fixtures/fork-workload`](../testing/fixtures/fork-workload/DESIGN.md)
   is a minimal Python server that implements it.
5. Package an image and Helm chart, requesting a GPU for any rendering or
   simulation, register the server in the gateway control plane, and add
   acceptance tests to the smoke harness.

Add domain authority the system needs, such as the UAV integration's per-vehicle
grants and command leases, inside its server. The installation's identity, policy,
task, artifact, and audit checks apply to it from its first request.

[Fork development](../docs/FORK_DEVELOPMENT.md) covers code placement and
registration.

Component tests are ordinary Cargo tests. Cross-component acceptance tests live
in the Rust smoke harness and run through `cargo xtask smoke`. Showcases deploy
with the same typed profiles as other local installations; SUMO's profile is
`sumo/deploy/deployment.json`.

The UAV showcase is tested by its Rust crate tests and the Python adapter tests
beside them. Running it live on an installation requires NVIDIA registry access
and a `CESIUM_ION_ACCESS_TOKEN`.
