# Optimization MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Owns compact spatial multi-agent planning. The server expands assignment
candidates from typed agents, groups, tasks, Map identities, capacities,
timing, and policy, then solves the bounded model through `good_lp`. The
canonical governed plan is a mandatory immutable JSON artifact. DuckDB and
Rerun RRD are optional evidence projections.

## Invariants

- Owns the implemented `optimization://` scheme for plans, artifacts, and usage
  records.
- The typed plan JSON and its recorded digest are canonical. DuckDB and RRD
  outputs are derived evidence and never replace the plan.
- Planning is task required on the shared task runtime. No client REST, gRPC,
  or WebSocket job surface, and no provider status polling.
- Frames owns frame conversion. Map owns source features, spatial derivations,
  projected CRS, geodesics, routing, restrictions, terrain, and mobility
  envelopes. The solver retains exact immutable references and performs none
  of that work internally.
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
