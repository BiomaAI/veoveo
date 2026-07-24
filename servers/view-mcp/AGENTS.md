# View MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 1.

## Purpose

Captures reproducible points of view over georeferenced 3D Tiles. Runs Bevy
without a window, keeps bounded tile and GPU residency across captures, and
returns images with resolved pose, layer identity, and attribution. Several
callers own independent views while the service shares immutable source
content.

## Invariants

- Owns the `view://` URI scheme plus the `ui://view/preview.html` app view.
  Identity: slug `view`, MCP `/view/mcp`. Map owns geographic source truth;
  View adds no routing, search, overlays, or feature identities.
- The canonical camera state is the exact geodetic pose; target rigs resolve
  to it before selection or capture. Geodetic and ECEF math stays `f64` until
  local transforms cast to Bevy `f32`.
- `capture_frame` is task only on the shared task runtime; a capture
  snapshots one camera revision and ignores later updates. Camera replacement
  uses an expected revision.
- API keys never enter MCP requests or resource identities; credentials,
  redirects, and request caps live in the server side layer catalog, and
  cache keys are credential free.
- Views, frames, and tile keys are in process state that does not survive
  restart; there is no persistent disk cache and no database. Raw, decoded,
  and GPU caches keep independent byte budgets.
- Production readiness requires a hardware Vulkan adapter (NVIDIA in the
  production profile); CPU and fallback adapters fail readiness. The preview
  app stays self contained (vendored three.js and draco, at most 2 MiB) and
  drives the real tool lifecycle; never add parallel convenience tools.

## Build And Test

- `cargo check -p veoveo-view-mcp`
- `cargo test -p veoveo-view-mcp` (camera, traversal, cache, and decode tests
  run without a GPU)
- `just smoke-view-mcp` builds the NVIDIA image and runs the renderer smoke:
  requires Docker and an NVIDIA GPU with the container toolkit, verifies a
  hardware Vulkan adapter, and captures a deterministic local tileset through
  the production task boundary.
- `just smoke-view-google <output>` is the billed live acceptance against
  Google Photorealistic 3D Tiles: requires `GOOGLE_MAPS_API_KEY` (passed by
  name) and an NVIDIA adapter.

## Contract Compliance

Contract revision: 1

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: pending — gateway registration does not state the contract revision
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C24: met
