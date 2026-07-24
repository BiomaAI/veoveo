# Artifact MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Owns the MCP surface for artifact discovery, authorization, and sharing over
the shared artifact plane: metadata reads, grants, release state, and share
links. It fronts `artifact-service` and holds no bytes of its own.

## Invariants

- Canonical URI scheme is `artifact://`: `artifact://index`,
  `artifact://{artifact_id}`, `artifact://metadata/{artifact_id}`, and
  `artifact://grants/{artifact_id}`.
- Every request presents the gateway signed internal identity
  (`GatewayInternalTokenVerifier`); direct unsigned access is rejected.
- Byte and grant authority stays with `artifact-service` and the platform
  store. Subscription state is session local and in memory.
- Controlled shapes are typed structs from `veoveo_mcp_contract`; tool schemas
  come from the shared `tool` macro, with declared output schemas.
- All six tools are quick metadata actions. A durable operation would require
  the shared task runtime, never a private queue.
- The crate has no `DESIGN.md` yet; the typed contract in `src/lib.rs` is the
  current authority for shapes and URIs. Widening the surface starts by
  writing that document.

## Build And Test

- `cargo check -p veoveo-artifact-mcp`
- `cargo test -p veoveo-artifact-mcp`
- Docker is required for SurrealDB backed tests and smoke work (root README,
  Develop And Verify).
- The container image builds from `servers/artifact-mcp/Dockerfile`.

## Contract Compliance

Contract revision: 2

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: pending — unverified
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
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C24: met
