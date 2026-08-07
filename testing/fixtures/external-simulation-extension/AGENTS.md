# Anonymous Simulation MCP Server — Agent Manual

## Purpose

Certify that an independently packaged simulation MCP server can own the
`veoveo.io/live-view/v2` camera, product, viewer-lease, signaling, and App surface
without importing platform source or a shared renderer.

## Invariants

- The fixture owns one stable authoritative camera/product identity and ephemeral
  actor-and-browser viewer leases.
- This is a protocol and packaging fixture. It never serves as GPU rendering,
  NVENC, WebRTC media, or visual acceptance evidence.
- It imports the selected Python SDK release from its locked package index rather
  than a Veoveo checkout.
- Viewer tokens appear only in open and renew results. Resources and logs stay
  redacted.
- The MCP endpoint and administrative docs use the same gateway internal-auth
  middleware.
- No scene mirror, visualization pose protocol, generic renderer, or compatibility
  alias may return.

## Build And Test

- `uv sync --locked --all-extras`
- `uv run --locked --all-extras pytest`
- `uv build`
- `helm lint deploy/helm`
- `docker buildx bake anonymous-simulation-extension --print`

Real visual certification belongs to each implementation and requires an accessible
NVIDIA GPU plus a headed hardware-backed browser.

## Contract Compliance

Contract revision: 2. C01 through C30 are met by the fixture's declared surface.
