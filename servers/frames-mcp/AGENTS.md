# Frames MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 1.

## Purpose

Owns complete rooted spatial-frame worlds and bounded coordinate conversion for
robots, sensors, vehicles, simulations, and mission workspaces. A world revision
contains its ECEF root, geodetic tangent anchors, static rigid transforms, and
recording- or stream-backed dynamic transforms. Earth geography, projected CRS
work, geodesics, geofences, and routing belong to `map-mcp`.

## Invariants

- Canonical identity: slug `frames`, URI scheme `frames://`, endpoint
  `/frames/mcp`, health `/frames/healthz`. Resource identities keep the
  scheme under the gateway `frames__` projection.
- Frame math is local and deterministic; the engine never calls Map MCP or
  another hosted server. The server never guesses an origin or axis
  convention, never treats degrees as radians, and never copies a coordinate
  it could not transform.
- Durable state (worlds, immutable revisions, operations, tasks, usage, ownership) lives in
  SurrealDB through the shared `TaskRuntime`. Large batch results go through
  the artifact plane, never object store paths or content URLs.
- `batch_transform` requires the final task extension; direct calls are
  rejected. Direct conversions write their operation record before returning.
- Frames starts empty. Helm and installation bootstrap never create a world,
  frame, origin, or revision. Clients author worlds with `create_world` and
  atomically publish complete trees with `publish_world`.
- Revision-scoped `frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}`
  identities are the only local-frame identities. Sessions pin one immutable
  revision and never follow a mutable world head implicitly.
- Approximation permission is explicit per request, and every result carries
  a `CoordinateOperationProvenance` record.

## Build And Test

- `cargo check -p veoveo-frames-mcp`
- `cargo test -p veoveo-frames-mcp`
- Docker is required for SurrealDB backed smoke tests (root README, Develop
  And Verify).
- A plain workspace Rust build; no extra native toolchain beyond the
  repository defaults.

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
- C17: pending — registration does not state the contract revision
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C24: met
