# Recording MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative hosted-server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3. The complete
recording contract is [`docs/RECORDINGS.md`](../../docs/RECORDINGS.md).

## Purpose

This server owns governed recording discovery, immutable layer inspection, sealing,
dataset-scoped virtual Redap catalogs, manifest v9 playback, bounded Arrow projection,
and reactive Rerun live following.

## Invariants

- Keep public recording, playback, catalog grant and projection models in the
  shared [Recording domain contract](../../platform/recordings/contract/DESIGN.md),
  exposed by this library’s isolated `contract` feature. Cross-server consumers import it with
  default features disabled. Runtime admission owns authorization and operational
  bounds; MCP core must not import or define these Recording models.
- The canonical resource is `recording://recordings/{recording_uuidv7}`. Build it
  through the contract's `RecordingUri` and `RecordingId`; resource adapters and
  consumers reuse `RecordingResource` admission. Public cursors carry the owning
  ID and convert to Store types only at query calls.
- Sealing requires the domain-owned `RecordingScope::Seal` (`recording:seal`). Gateway
  policy separately restricts it to the administrator profile and grants.
- Force checked `mcp_setup::SERVER_SETUP` before opening Store, caches or Redap. Discovery
  uses its admitted descriptors and templates; listeners accept only catalog and recording contents.
- One durable recording dataset contains one or more recordings. Dataset UUID is the
  Rerun application ID. Recording UUID is the Rerun recording and segment ID.
- Committed capture, properties, and derived layers are immutable Artifact occurrences.
  SurrealDB manifests and Artifact digests are authoritative. Cache and spool paths are
  never historical playback authority.
- Playback manifest v9 is the only manifest. It returns one stable Redap archive URI,
  one short-lived viewer grant, one optional live receiver, and the governed Blueprint.
- A virtual catalog registers only the exact recording set admitted by its durable grant.
  Direct manifest, asset, query, and chunk requests cannot escape that set.
- Redap is read-only. Entry, dataset, table, registration, task, maintenance, and chunk
  writes remain denied.
- Projection requests state every selector and bound. Arrow scratch, response bytes,
  execution time, and concurrency fail closed before unbounded work or response headers.
- Only the exact Recording Explorer receives the host-mediated projection stream. The
  frame receives no credential, internal URL, object coordinate, path, RRD source, or
  whole-result buffer.
- Live playback uses one recording channel. It preserves static context, skips replayed
  bootstrap rows on reconnect, and advances writing layers reactively.
- Committed Artifact reads require a fresh caller credential. Durable Reason and Stream
  replay are outside this activation and must not persist a submitted bearer.
- `/readyz` checks Store, layer-cache, and projection-scratch readiness. Authenticated
  `/admin/storage` reports their typed bounded counters.

## Module Boundaries

- `contract.rs` exposes the shared Recording domain models; `uris.rs` exposes their
  builders and declarations. Hub and Video import the domain crate directly.
- `service.rs` owns playback plans, sealing, and properties publication.
- `service/views.rs` admits Store references and integrity metadata into the domain
  builders after SQL visibility selection; persisted keys must never enter `expect()`.
- `service/index.rs` assembles SQL-authorized catalog pages, direct reads, and completions;
  the shared contract owns typed catalog cursors. SQL applies tenant and label predicates before limits.
- `bin/server/resources.rs` dispatches the admitted resource variants.
- [`platform/recordings/reader`](../../platform/recordings/reader/DESIGN.md) owns governed Artifact-backed analysis plans.
- `service/grants.rs` maps authenticated caller authority to Store grant admission and reuse.
- `service/projection.rs` owns projection receipts and checked result construction.
- `service/projection/scratch.rs` owns concurrency, Arrow/metadata accounting and restart integrity.
- The shared reader cache owns verified Artifact-to-PVC materialization and eviction;
  `blueprint_cache.rs` supplies Blueprint identity validation.
- `playback.rs` owns durable grants, virtual catalogs, scoped Redap, and manifest assembly.
- `live_playback.rs` and `live_stream.rs` own the Rerun live adapter and framed transport.
- `mcp_setup.rs` owns checked protocol configuration, documents, scope inventory and discovery.
- `bin/server/mcp.rs` owns protocol handlers and subscription admission.
- `bin/server/http.rs` owns authenticated HTTP routes, readiness and diagnostics.
- `bin/server.rs` initializes dependencies and starts the listener.

## Build And Test

- `cargo test -p veoveo-recording-contract`
- `cargo test -p veoveo-recording-mcp --no-default-features --features contract`
- Prove contract dependency isolation in an independent consumer workspace.
- `cargo check -p veoveo-recording-mcp --no-default-features --features runtime --lib`
- `cargo test -p veoveo-recording-mcp --lib`
- `cargo test -p veoveo-recording-mcp --features redap-conformance official_read_profile`
- `cargo test -p veoveo-rrd projection`
- `cargo test -p veoveo-console-bff recording_playback`
- `npm --prefix apps/console/web test`
- `cargo xtask doctor`

Component tests need no GPU. Playback acceptance is different: it requires the typed Rust
browser smoke, a headed browser, and a hardware-backed WebGPU or WebGL context.

## Contract Compliance

Contract revision: 3

- C01: met
- C02: met
- C03: met
- C04: met — catalog reads return 100-item cursor pages; SQL filters visibility and completion matches before limits
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
- C17: met — reference, local and catalog-fixture registrations declare revision 3 and static discovery
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met — authenticated catalog subscriptions and visibility-checked recording/layer subscriptions share the Store LIVE source
- C28: met — discovery contains static roots and templates; mutations invalidate contents, and the workbench reads one catalog page at a time
- C29: met
- C30: met — the endpoint is connection-stateless and derives no durable or domain authority from an MCP transport session
- C31: pending — installed Discover and list readiness qualification is pending
