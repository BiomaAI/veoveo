# Artifact MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Owns the MCP surface for artifact discovery, authorization, and sharing over
the shared artifact plane: metadata reads, grants, release state, and share
links. It fronts `artifact-service` and holds no bytes of its own.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `ArtifactMcp`
  implements `DomainServer`, pages `resources/list` with the Artifact plane cursor
  through `Listing::page`, and reads metadata members as `DomainRead::no_store`.
  `ArtifactListener` serves subscriptions through `ListenOnly`. Do not add a
  `ServerHandler`, router, host check or authentication middleware here.
- Canonical URI scheme is `artifact://`: `artifact://index`,
  `artifact://{artifact_id}`, `artifact://metadata/{artifact_id}`, and
  `artifact://grants/{artifact_id}`.
- Every request presents the gateway signed internal identity
  (`GatewayInternalTokenVerifier`); direct unsigned access is rejected.
- Byte and grant authority stays with `artifact-service` and the platform
  store. Subscription state is session local and in memory.
- Occurrence identity and metadata come from `veoveo_artifact_contract`.
  Capability levels and resolved invocation values come from `veoveo_types`.
  Grant and share-link values come from the Artifact plane contract.
  Access decisions and service request shapes come from `veoveo_mcp_contract`;
  tool schemas use the shared `tool` macro with declared output schemas.
- All six tools are quick metadata actions. A durable operation would require
  the shared task runtime, never a private queue.
- `DESIGN.md` is the domain contract; the typed contract in `src/contract.rs`
  carries its shapes and URIs. Both documents are embedded at build time and
  served under the well-known surface.
- Hosted declarations belong to `src/bin/server/setup.rs`; initialization and
  discovery consume the shared checked setup. Domain authorization stays in the service.
- `artifact.metadata` observations come from Artifact service snapshots through
  `src/knowledge.rs`. Keep owner and grant deadlines, selected-context access and
  labels in the descriptor. Metadata resource bodies use neutral Artifact URIs;
  authorize before evaluating a conditional read. Never infer the latest modifier.
- Index resources use `ArtifactIndexCursor` and `ArtifactIndexPage`; return metadata
  links and the optional continuation cursor. Selection belongs to Artifact service's
  SQL admission, before paging and decoding. Keep transfer locations out of the index.
- Subscriptions invalidate admitted member and index addresses before closing for
  access loss. Reuse Store's read predicate when selecting grant and retention
  deadlines; expiry needs a scheduled reconciliation even without a database write.

## Public Library

Consumers select `default-features = false, features = ["contract"]`. Keep the
contract feature free of MCP integration, service, provider and asynchronous runtime
dependencies. The optional `knowledge` feature exposes Artifact-owned access mapping
without service or runtime dependencies. Execution uses `runtime`; hosted endpoints and binaries require `mcp`.
Qualify the isolated consumer and both runtime feature configurations when changing
these gates. Public types keep their existing domain owners.

## Build And Test

- `cargo check -p veoveo-artifact-mcp`
- `cargo test -p veoveo-artifact-mcp`
- `tests/gateway_source_conformance.rs` runs public-gateway source conformance with a
  disposable Artifact, private caller/admin tokens and a previously absent grantee.
  It restarts the selected Artifact MCP Deployment and always reconciles its grant.
  Follow [the installed harness contract](../../testing/installed/DESIGN.md).
  Keep the Artifact grant driver here; derived-source consumers can reuse it.
- `tests/gateway_sharing.rs` owns installed anonymous sharing assertions. Select a
  disposable private Artifact with known bytes and an administrator credential.
  Keep link URLs out of reports and errors; restore private state and revoke returned
  links even after a failure. Follow the input contract in `DESIGN.md`.
- `node --test tests/workbench-pagination.test.mjs` in `apps/console/web` verifies
  headless page navigation behavior; it is not visual or GPU acceptance.
- Docker is required for SurrealDB backed tests and smoke work (root README,
  Develop And Verify).
- The container image builds from `servers/artifact-mcp/Dockerfile`.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

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
- C17: met — local and reference registrations and crate documents declare revision 3
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
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
