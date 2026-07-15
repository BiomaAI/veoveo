# Showcases

Each showcase proves the platform end to end on a real external system. Every
showcase is self-contained in its own subdirectory here: its images, MCP server,
Helm chart, profile values, gateway configuration, and verification contract.
That boundary keeps simulators independent and lets new ones arrive as siblings.

| Showcase | What it proves |
|----------|----------------|
| [`sumo/`](sumo/README.md) | The [SUMO](https://eclipse.dev/sumo/) traffic simulator as a live world: a task-native Rust MCP server owns the one TraCI connection, pushes `/world/sumo/**` into the Recording Hub as typed Rerun streams (map + 3D views of real Luxembourg), and exposes SUMO control as governed `sumo__*` tools. |

Task entry points for every showcase live in the root `Justfile`, namespaced
`showcase-<name>-*` so each showcase's recipes group together. For SUMO:
`just showcase-sumo-test`, `showcase-sumo-smoke`, `showcase-sumo-up`, and
`showcase-sumo-verify`.
