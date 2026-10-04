# Workspace Contracts And Services

## Standards And Protocols

Workspace publishes an HTTP JSON application contract with camelCase object fields,
closed enum spellings and UUID identities. Its operation views project MCP Tasks and
tool continuations into browser records. The `app-contract` feature preserves native
MCP `2026-07-28` App results and input responses through the repository's pinned
RMCP 3.5.0 adapter. These envelopes form the browser edge of an App session; Workspace
HTTP routes do not implement an MCP server.

The `schema` feature exposes the internal Rust module declaration contract from
[`veoveo-modules`](../modules/DESIGN.md). Current Workspace persistence executes the
qualified SurrealDB 3.3.0 profile through the
[Store Workspace implementation](../store/src/workspace/DESIGN.md).

## Application Contracts

The [`contract`](src/contract/mod.rs) module owns chat, invitation, participation,
agent-run and operation DTOs. Its identities admit the UUID parser's complete version
and alias profile and preserve the existing string schemas. Agents supplies model
references and budgets through its contract-only library. Gateway supplies the typed
hosted tool name. Artifact references use the Artifact contract.

`contract` activates model, serialization and schema dependencies, including the
shared Chrono clock profile. It does not activate MCP integration, asynchronous
execution, Store or runtime services. `app-contract` adds
[`apps.rs`](src/contract/apps.rs) and the pinned RMCP SDK's default features
(base64, macros and server) with schema support. This SDK profile includes its server
transport dependencies. Workspace enables the SDK's client and HTTP client transport
features through `gateway`.
`AppToolResult` preserves the SDK's typed Task, input-required and completed result
variants. `schema_bundle()` is available with `app-contract` and includes the complete
browser contract, including native App envelopes.

Workspace wire identities and Store persistence identities are distinct types. Gateway
adapters convert between them at the application service boundary. Store and Workspace
do not depend on each other's packages for these identities.

## Schema Ownership

The `schema` feature exports `schema::module_setup(execution)` for the optional
`workspace` module. It claims `workspace_*` tables and the explicit functions
`fn::workspace_authority`, `fn::workspace_member`, and `fn::workspace_operation_owner`.
The module requires Agents because participant admission calls
`fn::agent_chat_revision`; transitive requirements include the required kernel lanes.
The browser application in `apps/workspace` consumes Workspace behavior and does not
own the Rust schema declaration.

## Execution And Placement

The default feature set is empty. Schema-only builds activate only `veoveo-modules`
with default features disabled. There is no persistence runtime in this crate yet.
Its migration lane is empty, and the existing Store implementation and mixed catalog
continue to execute production Workspace queries and migrations. Owner migrations
and queries will live together here when the persistence moves.

The composition root supplies a checked execution image and command. The existing
gateway binary hosts `installation-prepare`, `module-migrate` and runtime-authenticated
`control-plane-publish`. Preparation applies the mixed catalog; lane execution and
publication require its completed generation proof. Installed image and Job qualification
is tracked in the active contract plan. This ownership module creates no process or
image requirement.

## Agent Participant Import

The current Store `agent_management/import.surql` transaction reads Workspace runs,
updates Workspace participants and chats, and creates Workspace events. The target
dependency points from Workspace to Agents, so participant import belongs in a
Workspace API. Composition must coordinate that operation with Agent catalog
operations while preserving writer exclusion, transactional checks, explicit rollback
and audit. The empty declaration does not certify the current reverse writes.

## Gateway Feature

The optional `gateway` feature owns the Workspace HTTP application API in
[`src/gateway/DESIGN.md`](src/gateway/DESIGN.md). Its dependencies are gated separately
from schema declarations. `gateway` activates `app-contract`, the Agents gateway profile,
and RMCP client and HTTP transport features. Workspace consumes reusable gateway HTTP
services and the Agents library. Agents can initialize and serve without Workspace.
