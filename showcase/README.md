# Showcases

Each showcase proves the platform end to end on a real external system. Every
showcase is self-contained in its own subdirectory here: its images, MCP server,
Helm chart, profile values, gateway configuration, and verification contract.
That boundary keeps simulators independent and lets new ones arrive as siblings.

| Showcase | What it proves |
|----------|----------------|
| [`sumo/`](sumo/README.md) | The [SUMO](https://eclipse.dev/sumo/) traffic simulator as a live world: a task-native Rust MCP server owns the one TraCI connection, pushes `/world/sumo/**` into the Recording Hub as typed Rerun streams (map + 3D views of real Luxembourg), and exposes SUMO control as governed `sumo__*` tools. |
| [`uav-sim/`](uav-sim/README.md) | Isaac Sim renders Google Photorealistic 3D Tiles through Cesium ion while Newton and a batched CUDA Warp plant simulate PX4-controlled UAVs; a provider-neutral MCP server governs sessions and missions, the encoded camera feeds Stream directly, and typed world state enters Recording Hub independently. |

Component tests remain native Cargo commands. Cross-component acceptance lives
in the typed Rust smoke harness and is dispatched through `cargo xtask smoke`.
Deployment uses the same typed profile scenarios as every installation. SUMO's
composition lives in `sumo/deploy/deployment.json`.

The UAV runtime uses its Rust crate tests and the colocated Python adapter
tests. Its installation-owned live proof requires NVIDIA registry access plus
`CESIUM_ION_ACCESS_TOKEN`.
