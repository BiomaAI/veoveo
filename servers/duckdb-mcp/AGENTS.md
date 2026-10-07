# DuckDB MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

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
- Direct calls have no Task identity. Native execution uses `TaskId`; Artifact writers
  check capability, origin and operation agreement before publication. Keep known
  Artifact and usage metadata in the library contract and convert it to JSON at the
  Artifact or Store adapter. File paths and writer-lock keys use `PathBuf`.
- Export requests distinguish tabular data from database snapshots at admission.
  Build query results through their checked constructors; preserve row-width,
  observed-count and inline/Artifact agreement through serialization.
- Source URLs use `HttpsUrl` through materialization, source lists require at least
  one URL, and Artifact inputs use `DuckDbArtifactSourceUri`. Keep address admission
  separate from runtime host/DNS policy and Artifact-plane authorization.
- Query construction uses the checked request builder and positive limit types.
  Reader options use their owning names, values and builders; render admitted options
  without a second validation path. Preserve typed SQL text and table names until the
  engine adapter, and keep table names in result, usage and Artifact metadata.
- Recovery classes are fixed: `query` and `export` resume; `execute` and
  `ingest` are indeterminate after interruption and never gain replay or
  polling fallbacks.
- One replica with a `ReadWriteOnce` workspace is part of correctness; the
  per database write mutex is process local.
- The library's `contract` feature owns database IDs, tool DTOs, tabular source
  types, read SQL fragments and typed usage resources. Cross-server consumers use that feature with defaults
  disabled. Keep MCP and engine dependencies outside it; source policy belongs to the
  materializing server. Database, document, Artifact and usage routes use the contract-owned
  `DuckDbResource` admission and typed builders. Never reconstruct a resource with string prefixes.
- The server is hosted through `veoveo_mcp_contract::hosting`: `DuckdbMcp` implements
  `DomainServer`, and `DurableTasks` adapts its task service. Do not add a
  `ServerHandler`, router, host check or authentication middleware here.
- Force `setup::SERVER_SETUP` before Store or engine initialization. The setup owns
  static discovery; resource reads dispatch admitted variants. Database pagination
  scans only the directory derived from verified identity, with no schema or file-byte reads.
- Usage reads use `DuckDbUsage` and TaskRuntime's SQL owner policy before grouping
  and limits. Catalog responses use checked 100-entry pages. Keep the collection
  cursor and Task IDs typed through query binding. Discovery declares roots and
  templates without enumerating database or Task records.

## Build And Test

- `cargo check -p veoveo-duckdb-mcp`
- `cargo test -p veoveo-duckdb-mcp`
- `cargo test -p veoveo-duckdb-mcp --no-default-features --features contract`
  checks public contracts. Qualify dependency isolation with a separately resolved
  consumer, since workspace feature unification can hide a runtime dependency.
- `cargo clippy -p veoveo-duckdb-mcp --no-default-features --features runtime --all-targets -- -D warnings`
  checks the library independently of the hosted binary.
- The crate links the DuckDB C library through the pinned `duckdb-rs` fork;
  expect a long native first build. The fork links DuckDB 1.5.6 and removes the
  upstream `comfy-table ~7.1` pin so it composes with Rerun 0.38.
- Docker is required for SurrealDB backed integration and smoke tests (root
  README, Develop And Verify). Usage tests own a disposable pinned Store and remove
  it on completion.
- `cargo xtask smoke agent-gateway` downloads the same pinned Spatial archive
  used by the image, verifies its compressed and installed digests,
  and caches it under `target/smoke-assets`.
- The image build pins the DuckDB C API and verifies the Spatial extension
  digest (`servers/duckdb-mcp/Dockerfile`).

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met
- C02: met
- C03: met
- C04: met — usage catalogs filter authority in SQL; owner database catalogs return 100-item cursor pages and retain at most 101 filename candidates
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
- C17: met — local and reference registrations declare revision 3
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met — the shared task-only filter rejects resource observations; accepted task IDs use the durable task source
- C28: met — resource subscription and resource-list change capabilities are absent
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
