# Frames MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

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
- `batch_transform` requires official Tasks; direct calls are
  rejected. Direct conversions write their operation record before returning.
- Frames starts empty. Helm and installation bootstrap never create a world,
  frame, origin, or revision. Clients author worlds with `create_world` and
  atomically publish complete trees with `publish_world`.
- Revision-scoped `frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}`
  identities are the only local-frame identities. Sessions pin one immutable
  revision and never follow a mutable world head implicitly.
- Keep Frames read queries in the owning runtime with typed IDs through database
  bindings. SQL applies tenant, current labels, and linked-parent checks. World reads
  are shared within the tenant under label clearance; publication requires ownership.
- Approximation permission is explicit per request, and every result carries
  a `CoordinateOperationProvenance` record. Results also carry canonical
  revision references and typed SHA-256 digests for only the frame-world
  revisions traversed by the conversion.

## Build And Test

- `cargo check -p veoveo-frames-mcp`
- `cargo test -p veoveo-frames-mcp`
- `cargo test -p veoveo-frames-mcp --lib state::read_tests` runs isolated SurrealDB
  visibility, parent-integrity, head-consistency, and direct-frame selection cases.
- Cross-server consumers enable only `contract` with default features disabled.
- `cargo test -p veoveo-frames-mcp --no-default-features --features contract --test coordinate_contract`
  checks schema compatibility, typed world addresses, and malformed identity admission.
- Docker is required for SurrealDB backed smoke tests (root README, Develop
  And Verify).
- A plain workspace Rust build; no extra native toolchain beyond the
  repository defaults.

## Contract Compliance

Contract revision: 3

Operation-resource owner/profile enforcement is incomplete: records have no profile
identity and current reads apply only tenant and labels. The foundations plan tracks
the persisted authority contract and SQL migration before installed acceptance.

- C01: met
- C02: met
- C03: met
- C04: pending — direct world/revision/frame reads apply SQL visibility and parent checks; world and usage catalogs still need bounded SQL pages and static discovery
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
- C25: met
- C26: met
- C27: pending — world and usage subscriptions use Store LIVE observations; replica and reconnect qualification is pending
- C28: met — the Store observer feeds resource contents and discovery-list invalidation into the shared hub
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: pending — installed Discover and list readiness qualification is pending
