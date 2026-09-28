# DuckDB MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

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
- The library's `contract` feature owns database IDs, tool DTOs, tabular source
  types and read SQL fragments. Cross-server consumers use that feature with defaults
  disabled. Keep MCP and engine dependencies outside it; source policy belongs to the
  materializing server. The current URI helpers remain a runtime concern.

## Build And Test

- `cargo check -p veoveo-duckdb-mcp`
- `cargo test -p veoveo-duckdb-mcp`
- `cargo test -p veoveo-duckdb-mcp --no-default-features --features contract`
  checks public contracts. Qualify dependency isolation with a separately resolved
  consumer, since workspace feature unification can hide a runtime dependency.
- `cargo clippy -p veoveo-duckdb-mcp --no-default-features --features runtime --all-targets -- -D warnings`
  checks the library independently of the hosted binary.
- The crate links the DuckDB C library through the pinned `duckdb-rs` fork;
  expect a long native first build. The fork tracks DuckDB 1.5.5 and removes
  the upstream `comfy-table ~7.1` pin so it composes with Rerun 0.38.
- Docker is required for SurrealDB backed integration and smoke tests (root
  README, Develop And Verify).
- `cargo xtask smoke agent-gateway` downloads the same pinned Spatial archive
  used by the image, verifies its compressed and installed digests,
  and caches it under `target/smoke-assets`.
- The image build pins the DuckDB C API and verifies the Spatial extension
  digest (`servers/duckdb-mcp/Dockerfile`).

## Contract Compliance

Contract revision: 2

Contract-only library consumption and the owner-local source vocabulary are implemented.
Typed resource builders and checked MCP setup remain work in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

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
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met — the shared task-only filter rejects resource observations; accepted task IDs use the durable task source
- C28: met — resource subscription and resource-list change capabilities are absent
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: pending — installed Discover and list readiness qualification is pending
