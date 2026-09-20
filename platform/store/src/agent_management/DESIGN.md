# Agent Catalog Persistence

Status: registry, gateway authoring and revision-aware Workspace execution are deployed.
The [managed lifecycle module](instances/DESIGN.md) adds transactional instance
admission and controller fencing; its external provisioning integration is in progress.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| SurrealDB `3.2.4` | Migration 0088, explicit transactions, typed bindings, current context digests and the existing transactional outbox |
| Veoveo identity and Work Context | Caller-authenticated user or service principal, exact tenant/context and current action-policy admission |
| UUID | Client UUIDv7 request identity; deterministic UUIDv5 internal definition/receipt identity |
| JSON and SHA-256 | Canonical typed Rust content serialization binds a published revision; this is an internal digest, not a provider protocol |

## Authority

`AgentCatalogAuthority` is an authenticated server decision and has no deserializer.
The gateway must evaluate the exact requested action before calling this module.
It supplies the current Work Context digest and membership, along with an explicit
decision about editing other owners' definitions. Store transactions recheck enabled
tenant/principal state, tenant equality and the context digest. Both human and service
principals are supported. Viewer membership never authorizes mutation.

Private authoring reads and mutations require the home context and either ownership
or context-management admission. Published catalog reads expose a separate struct
without instructions or template parameters. Revision execution requires current
audience membership and an enabled definition. Disabling an archived definition also
closes execution of its retained revisions.

The gateway resolves publication authority for every audience context and passes its
digest. The store rechecks every context in the publication transaction. The gateway
also admits the selected model/template and exact capabilities, and validates ownership
transfer eligibility. These policy responsibilities are not inferred from a string
identifier or from definition ownership alone.

## Transactions And Revisions

One mutation commits the definition, its idempotency receipt and an outbox event.
Create reserves one retained-definition slot in the context's shared counter. Concurrent
creators contend on that counter, including when they choose different keys. Archive
retains the record and its capacity reservation. The caller supplies an installation
limit of at most 10,000 retained definitions per context.

Each update names the expected management revision. Draft content receives a digest
computed by Rust. Publication checks that exact digest while creating an immutable
revision and switching the head in the same transaction. Republishing identical
content reuses its retained revision. Name/description changes do not change executable
content. Draft edits do not alter published content.

Request identity binds tenant, actor and payload. A repeated request returns its
original response snapshot, even after a subsequent edit. A changed payload conflicts.
Replay still requires current identity, context and ownership. Receipts contain private
authoring data and have no public collection endpoint. Outbox payloads contain IDs and
revisions only; no prompt or provider credential enters event payloads.

## Validation And Evidence

Closed typed content admits model references, bounded instructions, exact tool names,
execution budgets and either chat execution or managed-template references with scalar
parameters. It cannot express a provider URL, raw secret, container command or memory
migration. Publication and runtime callers must apply the installation's narrower
ceilings in addition to these structural bounds.

`platform/store/tests/agent_management.rs` uses two connections to an isolated pinned
database. It qualifies concurrent creation/editing, replay, catalog confidentiality,
revision retention, emergency disable, context/identity fencing and capacity rollback.
It does not establish gateway authorization, browser behavior or deployment acceptance.

The upstream prerelease-only formatter `@surrealdb/surql-fmt@0.1.0-beta.2` was evaluated
and rejected here: it rewrites typed function signatures and nested mutation expressions
into invalid syntax. Keep the reviewed SQL formatting until that incompatibility is
resolved. Validation and execution use the repository-pinned SurrealDB image,
independent of the host CLI version.

## Chat Resolution And Installation Import

`agent_executable` resolves an immutable publication under current context authority.
A new admission selects the enabled published head. An existing pinned binding may
retain an archived definition, while disable and audience removal deny both paths.
Publisher display attribution is projected separately from private instructions.

The focused `import` module supports an installation-authorized, offline source-digest
conversion of retained chat bindings. It exports the exact typed records and fences
apply/restore against any concurrent edit or active model run. Its authority input is
trusted server state; no HTTP author can invoke this migration boundary. Normal
startup does not import or reconcile definitions.
