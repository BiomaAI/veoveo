# Anonymous Simulation MCP Server — Agent Manual

## Purpose

Certify that an independently packaged simulation MCP server can own the
`veoveo.io/live-view/v4` camera, shared-product, stream-authorization, and App surface
without importing platform source or a shared renderer.

## Invariants

- The fixture owns one stable authoritative camera, one continuous stream product,
  and independent actor-and-browser authorizations without a viewer quota.
- This is a protocol and packaging fixture. It never serves as GPU rendering,
  NVENC, advancing H.264 media, or visual acceptance evidence.
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

Contract revision: 3.

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
- C17: met
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met
