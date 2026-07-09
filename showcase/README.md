# Showcases

Each showcase proves the platform end to end on a real external system. Every
showcase is self-contained in its own subdirectory here — its containers, its
MCP server, its compose overlay, and its scripts — so they stay independent and
new ones drop in as siblings.

| Showcase | What it proves |
|----------|----------------|
| [`sumo/`](sumo/README.md) | The [SUMO](https://eclipse.dev/sumo/) traffic simulator as a live world: a task-native Python MCP server owns the one TraCI connection, pushes `/world/sumo/**` into the Recording Hub as typed Rerun streams (map + 3D views of real Luxembourg), and exposes SUMO control as governed `sumo__*` tools. |

Task entry points for every showcase live in the root `Justfile`, namespaced
`showcase-<name>-*` so each showcase's recipes group together and a new one drops
in as a sibling set. For SUMO: `just showcase-sumo-test`, `showcase-sumo-smoke`,
`showcase-sumo-up`, `showcase-sumo-verify`.
