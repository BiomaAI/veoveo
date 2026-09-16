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
content, including reply target and normalized explicit agent destinations. Unknown and inaccessible chat references have the
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

## Human Turns And Participation

`participation.rs` owns atomic human turns. A single transaction commits the message
and each resolved response run. Owner settings select explicit requests, one default
assistant for unaddressed messages, or automatic responses from one to four agents.
Explicit destinations replace the default assistant. Automatic destinations combine
with explicit requests and deduplicate by agent identity. Every target must remain an
active agent in this chat, and a turn can address at most four agents.

All runs admitted with a message freeze the message's sequence as their context
boundary. Request replay compares the original text and explicit destinations. It
returns the original resolved destinations even after owner settings change. Reads,
agent output and reconnects never resolve participation again. Removing a configured
agent updates participation and the owner settings revision in the same transaction.

Room capacity does not reject human text. A response that cannot fit the four-run
limit commits as failed with the `capacity` reason. The gateway may also reject an
unclaimed run when its process is full or its configured definition changed. A
claimed or terminal run cannot be overwritten by that rejection.

Migration `0083` adds optional policy and message-address fields. An existing chat
without policy uses explicit requests. Existing messages without address metadata
retain empty destinations and immutable text. This declared persisted-data transition
requires no history rewrite. The new HTTP request requires `addressedAgents`, and
full owner settings require `participation`. Gateway and browser edge ship together
as a coordinated contract cut. Stale clients fail validation and must reload.

## Reply Context

A human turn may refer to a human message or a terminal agent response in the same
chat. `WorkspaceReplyTarget` distinguishes their identities before database binding.
The transaction rejects foreign, missing and still-running targets before committing
any message. Exact retry retains the original target and quote. A reply never grants
access to a private Task result or to another chat.

Admission captures up to 160 Unicode characters of the shared author's name and the first 500 Unicode
characters of the shared text. This bounded quote remains available when the original
is outside the latest history page. It is server-authored; the browser sends only the
typed reference. Both the quote and reference enter the bounded model prompt as
untrusted chat data. Existing human replies retain their original record references;
records predating this change may have no captured quote.

Migration `0084` widens the existing reference's allowed record tables and adds
optional quote fields. It does not rewrite message text or remove any record. The
HTTP surface makes a coordinated cut to `{kind: "message" | "response", id}`.
Old cached clients must reload, and mixed gateway/browser versions reject unsupported
shapes during rollout. Qualify both deployments before admitting new replies. Once
response references exist, recovery uses the new reader; rollback to a reader that
assumes every reference is a human message is unsupported. No old wire alias remains.

The database tests qualify Unicode bounds, foreign-chat and removed-member denial,
ongoing-response rejection, concurrent retry and paged recovery after agent removal.
Keep the established SQL formatting: `@surrealdb/surql-fmt` `0.1.0-beta.2`, the latest
published formatter during this change, failed qualification by moving a leading
negation into an IF body and stripping dots from nested field definitions. Its output
must not be used for these queries or migrations.
