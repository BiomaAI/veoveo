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
[Workspace persistence implementation](src/persistence/DESIGN.md).

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

Workspace wire identities and its private persistence identities are distinct types.
Gateway adapters convert between them at the application service boundary. Workspace
owns both families; shared Store connections do not enumerate Workspace identities.

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
with default features disabled. The `persistence` feature owns private records,
repositories and parameterized queries under `src/persistence`. Its version-zero
lane installs `src/schema/migrations/0000_current.surql`. Shared Store supplies the
authenticated connection; Workspace owns its schema and execution statements.
Gateway projections and colocated fixtures embed SQL from `src/queries/`; repository
transactions stay in `src/persistence/queries/`. Native fixtures use `tests/queries/`.
Rust binds request values and decodes the existing statement result slots.

The composition root supplies a checked execution image and command. The existing
gateway binary hosts `installation-prepare`, `module-migrate` and runtime-authenticated
`control-plane-publish`. Preparation establishes runtime credentials; lane execution
and publication require its completed generation proof. Installed image and Job qualification
is tracked in the active contract plan. This ownership module creates no process or
image requirement.

## Agent Participant Import

`src/persistence/agent_import.rs` owns participant import over the current Workspace
schema lane. Planning validates typed Agent keys and digests through `AgentRepository`
before selecting Workspace participants. The Workspace transaction checks current
Agent authority and revision admission, excludes active run writers, compares the
retained participant with its planned value, and updates participant/chat rows with
Workspace events. Apply and restore use the same owner transaction and preserve the
existing rollback checks. These runtime foreign reads and private Agent function
calls still require the planned kernel API split; the Workspace schema declaration
does not establish that broader runtime contract.

## Gateway Feature

The optional `gateway` feature owns the Workspace HTTP application API in
[`src/gateway/DESIGN.md`](src/gateway/DESIGN.md). Its dependencies are gated separately
from schema declarations. `gateway` activates `app-contract`, the Agents gateway profile,
and RMCP client and HTTP transport features. Workspace consumes reusable gateway HTTP
services and the Agents library. Agents can initialize and serve without Workspace.
