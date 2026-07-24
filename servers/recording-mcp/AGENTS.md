# Recording MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

The governed catalog and read boundary for Recording Hub data: recording
discovery, bounded queries, subscriptions, artifact publication, and range
capable RRD playback for archive and live shards. The repository ingest,
storage, and playback contract is normative in
[`docs/RECORDINGS.md`](../../docs/RECORDINGS.md).

## Invariants

- Owns the `recording://` scheme. Gateway policy and audit target the
  canonical `recording://recordings/{id}` resource before either the MCP
  endpoint or a byte route reaches this server.
- Frozen and sealed segments are immutable. The data route never decodes,
  merges, or rewrites an archive during a request, and it reauthorizes every
  shard read.
- The HTTP routes beside the MCP endpoint exist because the embedded Rerun
  viewer consumes byte streams; they implement byte ranges and immutable
  validators, and the manifest binds each shard to catalog UUIDv7, ordinal,
  bounds, length, and digest. Do not add routes outside this governed set.
- Live playback is bound to one writing segment identity and ends at
  rollover; the follow projection keeps a bounded row ID history window.
- Module boundaries are pinned by DESIGN.md: `contract.rs` owns the typed
  manifest, `service/read.rs` resolves authorized filesystem plans from
  durable identities, `live_playback.rs` owns the follow projection,
  `bin/server.rs` owns HTTP framing, and authorization stays in `service.rs`.
- Durable catalog state lives in the installation SurrealDB; artifact
  operations go through the shared artifact plane with the forwarded
  internal identity.

## Build And Test

- `cargo check -p veoveo-recording-mcp`
- `cargo test -p veoveo-recording-mcp`
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
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C24: met
