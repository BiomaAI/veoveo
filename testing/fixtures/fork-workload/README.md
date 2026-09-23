# Fork Workload Fixture

This Python MCP server uses the SDK in the fork checkout. It declares a logical
camera, shared product, viewer authorization and an MCP App. Its synthetic product
qualifies protocol behavior only; it cannot establish GPU rendering or video playback.

From this directory:

```sh
uv sync --locked --all-extras
uv run --locked --all-extras pytest -q
```

The root Bake target `anonymous-simulation-mcp` builds the local SDK and fixture from
one source snapshot. The fixture chart bundles the internal shared Helm library.
The adjacent `fork-installation` owns complete gateway configuration, public endpoints,
trust and platform selection.
