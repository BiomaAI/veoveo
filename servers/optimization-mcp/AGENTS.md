# Optimization MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 1.

## Purpose

Owns high level optimization planning: one or many agents selecting options to
complete tasks under typed constraints, solved through `good_lp`. Rerun RRD
recordings are the canonical mission worldline; DuckDB holds indexes and
summaries over that worldline and never replaces it.

## Invariants

- Owns the `optimization://` scheme: sessions, recordings, segments, contexts,
  snapshots, plans, scenarios, solves, artifacts, and usage records.
- The RRD worldline is canonical and append only. DuckDB rows and stored JSON
  summaries are materialized views; when they conflict with segment data the
  segment wins and the view is rebuilt.
- Planning is task required on the shared task runtime. No client REST, gRPC,
  or WebSocket job surface, and no provider status polling.
- Spatial types come from `veoveo_mcp_contract::coordinates`. Frames owns
  frame conversion; Map owns projected CRS, geodesics, routing, and geofences.
  The solver performs no CRS, datum, or routing work internally.
- Artifact bytes go through the shared artifact plane with the caller's
  `PlaneCaller`; the server has no byte route. Durable ownership state lives
  in the installation SurrealDB.
- Planning output is advisory. There is no autonomous execution path.

## Build And Test

- `cargo check -p veoveo-optimization-mcp`
- `cargo test -p veoveo-optimization-mcp`
- The solver uses `good_lp` with the pure Rust `microlp` backend, so local
  checks need no native solver libraries.

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
- C17: pending — gateway registration does not state the contract revision
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C24: met
