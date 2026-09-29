# View MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Captures reproducible points of view over governed static scene compositions.
Runs Bevy without a window, keeps bounded tile and GPU residency across
captures, and returns images with resolved pose, exact composition provenance,
and attribution.

## Invariants

- Public camera, capture, composition and resource types, `ViewScope` and
  `ViewTaskKind` belong in the library's `contract` feature. Keep that profile free of MCP transport,
  async, database and GPU dependencies. Import resolved authority from `veoveo-types`;
  authentication and current authorization stay in the runtime and adapter.
- Require `ViewScope` in permission checks. Requests and capture Tasks share
  `server/auth.rs`; preserve unrelated names in the authenticated grant set.
- Construct and parse View addresses through `ViewResource` and its domain-specific
  URI types. Keep tile keys typed through the registry. Scene URI construction checks
  numeric admission; runtime limits still apply at read time.
- `server/setup.rs` owns fixed discovery declarations. Force checked setup before
  opening dependencies; scene changes invalidate resource contents.
- Owns the `view://` URI scheme plus the `ui://view/preview.html` app view.
  Identity: slug `view`, MCP `/view/mcp`. Map owns geographic source truth;
  View derives no routing or search products. It renders exact governed inputs
  through bounded declarative overlays and preserves their identities.
- Build immutable composition and view records through their checked constructors.
  Decoding checks repeated identities, camera resolution and composition digests.
  Capture snapshots validate resolved geometry, artifact bytes and request revisions
  before Task claiming; keep snapshot fields private.
- Construct capture outputs through `CapturedFrame::builder` and preview manifests
  through `PreviewSceneRecord::new`. Keep record fields and image bytes immutable;
  derive repeated identities, byte metadata, local transforms and oversize status.
- Public Task delivery validates completed capture payloads in `server/tasks/results.rs`
  after SQL selection and before projection. Use the same capture builder and formatter
  as the writer; preserve governed attribution and keep diagnostics free of payload data.
- Every view binds one immutable composition. Compositions, views, frames, and
  capture tasks are scoped by principal, tenant and Work Context. Local metre
  positions require one exact Frames revision and operation input.
  Public Task reads, mutations and subscriptions share the checked context query
  in `server/tasks.rs`; apply its selection in SQL before decoding.
- Overlay geometry accepts only bounded typed primitives. Large geometry and
  oriented meshes resolve through the shared artifact plane under the
  forwarded caller token. Executable content, arbitrary URLs, credentials, and
  ungoverned mesh bytes are invalid.
- The canonical camera state is the exact geodetic pose; target rigs resolve
  to it before selection or capture. Geodetic and ECEF math stays `f64` until
  local transforms cast to Bevy `f32`.
- `capture_frame` is task only on the shared task runtime. A capture snapshots
  one camera revision, composition, resolved artifacts, and explicit scene
  time. Camera replacement uses an expected revision.
- API keys never enter MCP requests or resource identities; credentials,
  redirects, and request caps live in the server side layer catalog, and
  cache keys are credential free.
- Compositions, views, frames, and tile keys are in-process state. The shared
  task database persists recoverable capture snapshots, but no View catalog or
  disk cache survives restart. Raw, decoded, and GPU caches keep independent
  byte budgets.
- Production readiness requires a hardware Vulkan adapter (NVIDIA in the
  production profile); CPU and fallback adapters fail readiness. The preview
  app stays self contained (vendored three.js and draco, at most 2 MiB) and
  drives the real tool lifecycle; never add parallel convenience tools.

## Build And Test

- `cargo check -p veoveo-view-mcp`
- `cargo test -p veoveo-view-mcp --no-default-features --features contract --lib`
- Prove contract dependency isolation with a separate consumer workspace.
- `cargo test -p veoveo-view-mcp` (camera, traversal, cache, and decode tests
  run without a GPU)
- `cargo xtask image build --target view-mcp` followed by `cargo xtask smoke
  view-mcp` runs the renderer smoke:
  requires Docker and an NVIDIA GPU with the container toolkit, verifies a
  hardware Vulkan adapter, and captures a deterministic local tileset plus
  governed overlays through the production task boundary.
- `cargo xtask smoke view-google-live --output <output>` is the billed live
  acceptance against Google Photorealistic 3D Tiles: requires
  `GOOGLE_MAPS_API_KEY` (passed by name) and an NVIDIA adapter.

## Contract Compliance

Contract revision: 3

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
- C17: met — both gateway registrations declare revision 3
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: pending — public Task subscriptions apply Work Context SQL selection;
  installed cross-context delivery qualification is pending
- C28: met
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: pending — installed Discover and list readiness qualification is pending
