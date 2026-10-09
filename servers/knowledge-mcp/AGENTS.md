# Knowledge Service Instructions

## Purpose

Implement the catalog and retrieval contract in [DESIGN.md](DESIGN.md). Keep domain
records and source authorization with their owning servers.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `KnowledgeMcp`
  implements `DomainServer`, filters its lists by caller policy as `no_store`
  listings, and authorizes its documents through `authorize_documents`.
  `KnowledgeListener` serves subscriptions through `ListenOnly`; `/readyz` reports
  indexing readiness. Do not add a `ServerHandler`, router, host check or
  authentication middleware here.
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
changes and cross-replica reads. It runs the shared hosted conformance checker against
the server's HTTP surface and immutable documentation collection. The indexing-host suite adds signed machine authentication,
credential rotation, source-loss recovery, catalog replacement, readiness and shutdown.
The fixtures need Docker and the pinned SurrealDB image; each owns its
containers and has a 180-second timeout per case. Run
`cargo check -p veoveo-knowledge-mcp --no-default-features --features contract` to
qualify the public library dependency boundary. Hosted and GPU acceptance follow the
[consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#knowledge-service).
Packaging checks use `cargo test -p veoveo-deployment-smoke --test knowledge_helm` and
`cargo xtask image plan --target knowledge-mcp`. The chart's single indexing replica
uses Recreate, distinct liveness/readiness probes and installation-owned credentials.
The ignored installed check in `tests/support/installed.rs` exercises the deployed
catalog, caller-visible statistics, completion, initial subscription notifications, document retrieval, source revisions and embedding tool through HTTPS. Use
the installation's ordinary caller token and public control plane as described in
the [reference runbook](../../examples/bioma/README.md#acceptance). It requires a
running hardware embedding runtime and leaves source records unchanged.

The separate ignored policy case is
`installed::policy::restricted_caller_lists_catalog_and_document_denials_agree_through_installed_gateway`
in the existing `http` target. Supply a closed JSON fixture through
`VEOVEO_KNOWLEDGE_POLICY_INPUT` as described in [DESIGN.md](DESIGN.md).
It uses an existing restricted caller and never publishes policy or widens access.
Run `cargo test -p veoveo-knowledge-mcp --test http installed::` without
`--ignored` for the fixture admission, protocol-denial and actual registered SDK
cleanup controls. Both installed cases create private outcome files before network
access and retain original close futures through owner cancellation. These native
controls do not qualify unattended installed startup or hardware embedding.

`tests/evaluation.rs` qualifies corpus completeness, source revision checks, recall
scoring and generation-linked report persistence with synthetic vectors. The ignored
`tests/gpu_retrieval.rs` measures actual-model retrieval and rebuild throughput under
concurrent search using a separately qualified CUDA runtime. Its source captures and
judgments follow the [evaluation runbook](evaluation/README.md); they do not replace
installed source conformance.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met — the shared hosted checker qualifies the native HTTP surface; domain HTTP checks cover tools, resources, completion and subscriptions. C31 tracks installed qualification.
- C02: met — direct tools declare generated input and output schemas.
- C03: met — the domain has no task-augmented operations.
- C04: met — typed exact addresses and SQL-admitted catalog pages.
- C05: met — resources and templates accompany tools.
- C06: met — no compatibility helpers.
- C07: met — the shared checker validates generated schemas, including the object root of the tagged embedding request.
- C08: met — rmcp and Schemars generate schemas.
- C09: met — server-owned scopes, resource builders and checked domain models.
- C10: met — shared HTTP construction and serialized response enforcement.
- C11: met — no Artifact or Recording operations.
- C12: met — authenticated documents use the mounted admin routes.
- C13: met — persistence uses the platform Store.
- C14: met — no byte routes.
- C15: met — OCI and Helm publications exist.
- C16: met — registration and caller policy expose search, embedding, resources, catalog completion and subscriptions; installed acceptance is tracked by C31.
- C17: met — the typed registration and crate documents declare revision 4.
- C18: met — shared document resources.
- C19: met — the shared declaration includes every checklist status.
- C20: met — authenticated document index and bodies.
- C21: met — documents and digests are embedded at build time.
- C22: met — the owning design declares its standards and supported profile.
- C23: met — this manual supplies the required sections.
- C24: met — the crate uses the MCP server naming convention.
- C25: met — Streamable HTTP is the hosted transport.
- C26: met — shared stateless final-profile transport.
- C27: pending — native catalog observation qualifies admission, two replicas, expiry, revocation and cancellation; installed recovery qualification remains open.
- C28: met — resource-list changes are independent of content notifications; tool and prompt list changes are absent.
- C29: met — read replicas share Store; the single indexing worker uses a Store lease and Recreate updates.
- C30: met — source consumers reuse the gateway's shared upstream transport.
- C31: pending — installed discovery and readiness qualification for the completed catalog surface.
- C32: pending — installed K01–K10 qualification for `knowledge.docs`.
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
