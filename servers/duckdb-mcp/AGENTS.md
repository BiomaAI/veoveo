# DuckDB MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 1.

## Purpose

Veoveo's hosted analytical SQL domain: mutable DuckDB databases scoped to
their owner, unrestricted analytical SQL inside them, governed data ingress,
immutable exports, and DuckDB Spatial. Isolation is enforced around the
engine rather than by narrowing SQL.

## Invariants

- Canonical identity: slug `duckdb`, URI scheme `duckdb://`, endpoint
  `/duckdb/mcp`. Resource identities keep the scheme when the gateway mounts
  tools under the `duckdb__` namespace.
- Arbitrary SQL is the capability, and file, network, extension, and
  configuration authority stay outside caller SQL. Only the pinned Spatial
  extension is loaded, from its local path.
- Storage authority stays split: DuckDB files hold mutable analytical data,
  SurrealDB holds tasks, owners, leases, and usage, and the artifact plane
  holds immutable bytes. The server owns no bucket, artifact index, or byte
  route.
- Database file paths derive from the verified owner identity and are never
  accepted from a client.
- Recovery classes are fixed: `query` and `export` resume; `execute` and
  `ingest` are indeterminate after interruption and never gain replay or
  polling fallbacks.
- One replica with a `ReadWriteOnce` workspace is part of correctness; the
  per database write mutex is process local.

## Build And Test

- `cargo check -p veoveo-duckdb-mcp`
- `cargo test -p veoveo-duckdb-mcp`
- The crate links the DuckDB C library through the pinned `duckdb-rs` fork;
  expect a long native first build.
- Docker is required for SurrealDB backed integration and smoke tests (root
  README, Develop And Verify).
- The image build pins the DuckDB C API and verifies the Spatial extension
  digest (`servers/duckdb-mcp/Dockerfile`).

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
