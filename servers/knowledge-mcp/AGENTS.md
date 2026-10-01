# Knowledge Service Instructions

## Purpose

Implement the catalog and retrieval contract in [DESIGN.md](DESIGN.md). Keep domain
records and source authorization with their owning servers.

## Invariants

- Source reads go through the gateway with the Knowledge extension declared.
- Caller collection exposure and access predicates run in SQL before ranking limits
  and decoding. Final access checks reuse the shared evaluator.
- Fence member reads before source I/O. Coordinator leases and source/member epochs
  reject late writes; collection completion requires its Store-issued sync ticket.
  Failures leave cached chunks stale.
- Metadata indexing admits only the closed metadata document; source bodies and
  external navigation URLs cannot enter its embeddings.
- Use the shared GPU embedding client. Contract-only builds exclude service runtime
  dependencies, and source fixtures never count as GPU inference qualification.
- Start source listeners before building a generation. Activate after complete
  traversal and reconciliation. Preserve the prior generation pointer on failure;
  source loss and expired freshness must still hide its unsafe cached results in SQL.

## Build And Test

Run `cargo test -p veoveo-knowledge-mcp --tests` for contract and isolated pipeline
checks. The coordination suite adds source-loss restart, conditional reuse and lease
renewal during slow reads. The source-gateway suite checks native discovery events and
listener readiness with in-process MCP transports. The HTTP suite adds signed gateway requests, current authority
changes and cross-replica reads. The indexing-host suite adds signed machine authentication,
credential rotation, source-loss recovery, catalog replacement, readiness and shutdown.
The fixtures need Docker and the pinned SurrealDB image; each owns its
containers and has a 180-second timeout per case. Run
`cargo check -p veoveo-knowledge-mcp --no-default-features --features contract` to
qualify the public library dependency boundary. Hosted and GPU acceptance follow the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-8-knowledge-service).

## Contract Compliance

Target contract revision: `veoveo.ai/hosted-mcp/v3`.

| Items | Status | Scope |
|---|---|---|
| C02, C04–C06, C08–C10, C12–C14, C18–C26, C28–C30 | met | typed direct tools, SQL catalog pages, shared stateless transport, signed request authority and embedded documents; no durable tools or private byte routes |
| C03, C11 | met | no domain Tasks, Artifact operations or recording operations |
| C01, C07, C15–C17, C27, C31, C32 | pending | schema conformance, completion, catalog subscriptions, packaging, registration and installed conformance |
