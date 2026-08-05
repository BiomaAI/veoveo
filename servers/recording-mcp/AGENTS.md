# Recording MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

The governed catalog and read boundary for Recording Hub data: recording
discovery, bounded queries, subscriptions, artifact publication, lazy Redap
archive playback, and bounded Rerun-channel live following. The repository ingest,
storage, and playback contract is normative in
[`docs/RECORDINGS.md`](../../docs/RECORDINGS.md).

## Invariants

- Owns the `recording://` scheme. Gateway policy and audit target the
  canonical `recording://recordings/{id}` resource before either the MCP
  endpoint, playback manifest, or live route reaches this server.
- Frozen and sealed shards are immutable layers of one recording-scoped Redap
  dataset segment. The durable catalog and shards are authoritative; the
  in-memory Rerun catalog is derived, bounded, and reconstructible.
- Playback manifest v8 returns one stable Redap archive URI, its deterministic
  layer revision, one optional live source, and recording-scoped access
  material. Do not return archive shard URLs or add a whole-recording RRD
  route.
- The public Rerun Data Protocol surface is read-only and recording-scoped.
  Its isolated derived catalog may enumerate the one authorized recording
  because native Rerun source navigation requires `FindEntries`. Reject
  cross-recording entry access, writes, registration, tables, tasks, and
  maintenance.
- Live playback is bound to one recording identity. Its one Rerun WebViewer
  channel advances reactively across writing-segment rollovers without
  replaying prior row identities. Each framed transport item is one complete
  RRD. The follow projection keeps a bounded row ID history window and rewrites
  every outgoing message to the same dataset identity as the archive.
- Governed queries and analysis snapshots may include complete acknowledged
  ingest parts from a writing segment. A task-local copy binds the exact part
  sequence, byte length, and SHA-256 before Hub rollover can replace it.
- Module boundaries are pinned by DESIGN.md: `contract.rs` owns the typed
  manifest, `service.rs` resolves authorized playback plans,
  `service/read.rs` resolves governed analysis plans, `playback.rs` owns
  sessions and Redap, `live_playback.rs` owns the follow projection,
  `live_stream.rs` owns the recording-scoped framed RRD transport, and
  `bin/server.rs` owns transport composition.
- Durable catalog state lives in the installation SurrealDB; artifact
  operations go through the shared artifact plane with the forwarded
  internal identity.

## Build And Test

- `cargo check -p veoveo-recording-mcp`
- `cargo test -p veoveo-recording-mcp --features redap`
- Tests are colocated in `src/` and use filesystem fixtures; no external
  services, GPU, or Docker are required.

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
- C17: pending — gateway registration does not state the contract revision
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met — the gateway owns pooled transport while this server retains MCP session state
- C24: met
