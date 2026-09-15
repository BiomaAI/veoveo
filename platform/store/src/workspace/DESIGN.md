# Workspace Persistence

## Standards And Protocols

This component uses SurrealDB `3.2.4` transactions over the existing authenticated
WebSocket client. Rust records use the SDK's typed `SurrealValue` representation.
The database schema is private to Veoveo. Gateway JSON projections are a separate
application contract and must not expose database credentials or raw record IDs.

## Ownership

The shared store owns chat records and atomic membership/message transitions.
Gateway handlers own OAuth admission and current Work Context policy evaluation.
The accepted product and remaining delivery gates are in
[`WORKSPACE_PLAN.md`](../../../../docs/WORKSPACE_PLAN.md).

`WorkspaceAuthority` is trusted server input, never a request-body DTO. Its caller
must evaluate the authenticated human against the current Work Context and provide
the matching context digest. Each transaction checks that digest, tenant, enabled
human principal and context again. This preserves group- and role-based admission
without copying the policy evaluator into SurrealQL. Browser-supplied actor IDs,
membership levels and context digests are forbidden.

Chat membership is checked inside every transaction. Mutations serialize through
the chat head, including membership removal. The same transaction allocates the
next event sequence and commits the affected record. A database sequence allocator
is insufficient because allocated order need not equal committed order.

Request IDs are stable UUIDs. Message replay must match chat, author and complete
content, including reply target. Unknown and inaccessible chat references have the
same error. Invitations require explicit acceptance by the named human and fresh
Work Context authority. Accepted invitations cannot restore a removed member.

Queries have bounded pages and mutations have bounded conflict retries. Database
errors may contain submitted text, so public errors and tracing must use the closed
error classification rather than printing the underlying database error.

Record fields are materialized before `IF` conditions. The qualified SurrealDB
release cannot resolve record traversal directly within those conditions; this
constraint is documented in its [transaction reference](https://surrealdb.com/docs/reference/query-language/statements/begin).
Settings use their own revision, so concurrent message traffic does not create a
settings conflict. Membership changes still serialize with messages on the chat head.

Human chat membership, invitations, immutable text messages and event replay are
implemented. Per-chat agent admission and durable execution records are owned by
[`runs/DESIGN.md`](runs/DESIGN.md). Gateway/model execution and installed acceptance
remain part of the Workspace delivery goal.

Private MCP dispatch receipts, Task references and continuation fences are owned
by [`operations/DESIGN.md`](operations/DESIGN.md). They preserve accepted operation
identity across browser and process restarts without replaying tool calls.
