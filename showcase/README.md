# Showcases

Each showcase runs Veoveo against a real external simulator. A showcase keeps
everything it needs in its own subdirectory: images, MCP server, Helm chart,
profile values, gateway configuration, and acceptance tests. Simulators therefore
do not depend on each other, and a new one is added as another subdirectory.

| Showcase | What it runs |
|----------|----------------|
| [`sumo/`](sumo/README.md) | The [SUMO](https://eclipse.dev/sumo/) traffic simulator running the Luxembourg LuST scenario. A Rust MCP server holds the single TraCI connection, pushes `/world/sumo/**` to Recording Hub as typed Rerun streams (map and 3D views), and exposes SUMO control as `sumo__*` tools behind the gateway. |
| [`uav-sim/`](uav-sim/README.md) | A four-vehicle PX4 fleet in Isaac Sim over Google Photorealistic 3D Tiles streamed through Cesium ion. Newton and a batched CUDA Warp plant simulate the vehicles. An MCP server manages sessions and missions. The encoded camera feeds Stream directly, and world state is recorded to Recording Hub on a separate path. |

Component tests are ordinary Cargo tests. Cross-component acceptance tests live
in the Rust smoke harness and run through `cargo xtask smoke`. Showcases deploy
with the same typed profiles as other local installations; SUMO's profile is
`sumo/deploy/deployment.json`.

The UAV showcase is tested by its Rust crate tests and the Python adapter tests
beside them. Running it live on an installation requires NVIDIA registry access
and a `CESIUM_ION_ACCESS_TOKEN`.
