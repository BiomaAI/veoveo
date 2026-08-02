# Simulation View MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Operate the provider-neutral, renderer-only simulation visualization control
plane. The crate owns governed scene binding, pose-producer authorization,
durable runtime reconciliation, capacity-admitted logical cameras, live-view
leases, authenticated signaling, and the generic MCP App.

## Invariants

- The slug and URI scheme are `simulation-view`. Isaac is the initial private
  renderer profile and never becomes the public service name.
- Simulation View never integrates dynamics, controls a vehicle, mutates
  extension simulation state, or loads extension Python.
- MCP carries bounded control and resources only. Continuous poses and media
  stay on their private data planes.
- Scene content is immutable, content addressed, artifact governed, bounded,
  and non-executable. Arbitrary network URLs and credentials are rejected.
- Access tokens appear only in open and renew results. State stores hashes,
  signaling compares them in constant time, and resources and logs contain no
  token.
- Pose authorization remains bounded. Renewal advances a durable monotonic
  revision, while an explicit revocation writes a tombstone that automatic
  reconciliation cannot supersede.
- Camera admission returns a typed rejection. It never silently reduces
  quality or switches away from RTX and NVENC.
- Readiness fails closed unless the Isaac renderer, NVIDIA hardware path,
  render product, NVENC, visible frame, and mutually authenticated pose
  ingress all pass.
- The three workload identities remain `simulation-view-mcp`,
  `simulation-view-isaac`, and `simulation-view-pose`.

## Build And Test

- `cargo check -p veoveo-simulation-view-mcp`
- `cargo test -p veoveo-simulation-view-mcp`
- Runtime acceptance must use a headed hardware-backed browser and an
  accessible NVIDIA GPU. Software rendering is not evidence.

## Contract Compliance

Contract revision: 2

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
