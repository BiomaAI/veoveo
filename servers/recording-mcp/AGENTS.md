# Recording MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative hosted-server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2. The complete
recording contract is [`docs/RECORDINGS.md`](../../docs/RECORDINGS.md).

## Purpose

This server owns governed recording discovery, immutable layer inspection, sealing,
dataset-scoped virtual Redap catalogs, manifest v9 playback, bounded Arrow projection,
and reactive Rerun live following.

## Invariants

- The canonical resource is `recording://recordings/{recording_uuidv7}`.
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

- `contract.rs` owns recording, layer, seal, and playback-manifest types.
- `service.rs` owns visibility, playback plans, sealing, and properties publication.
- `service/read.rs` owns governed Artifact-backed analysis plans.
- `service/projection.rs` owns projection receipts and bounded scratch.
- `layer_cache.rs` owns verified Artifact-to-PVC materialization and eviction.
- `playback.rs` owns durable grants, virtual catalogs, scoped Redap, and manifest assembly.
- `live_playback.rs` and `live_stream.rs` own the Rerun live adapter and framed transport.
- `bin/server.rs` owns transport composition, readiness, and diagnostics.

## Build And Test

- `cargo test -p veoveo-recording-mcp --lib`
- `cargo test -p veoveo-recording-mcp --features redap-conformance official_read_profile`
- `cargo test -p veoveo-rrd projection`
- `cargo test -p veoveo-console-bff recording_playback`
- `npm --prefix apps/console/web test`
- `cargo xtask doctor`

Component tests need no GPU. Playback acceptance is different: it requires the typed Rust
browser smoke, a headed browser, and a hardware-backed WebGPU or WebGL context.

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
- C17: pending — Gateway registration does not state the hosted-server contract revision
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
- C30: met — the endpoint is connection-stateless and derives no durable or domain authority from an MCP transport session
