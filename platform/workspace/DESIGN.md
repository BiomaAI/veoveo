# Workspace Persistence Module

## Standards And Protocols

This crate exposes the internal Rust module declaration contract from
[`veoveo-modules`](../modules/DESIGN.md). It has no browser, protocol or database
runtime dependency. Current Workspace persistence executes the qualified SurrealDB
3.3.0 profile through the [Store Workspace implementation](../store/src/workspace/DESIGN.md).

## Ownership

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
gateway image is the initial host candidate; its `installation-bootstrap` command
applies the mixed catalog and publishes the control plane. A named-lane command and
migration Job need their own implementation and qualification. This ownership module
creates no process or image requirement.

## Agent Participant Import

The current Store `agent_management/import.surql` transaction reads Workspace runs,
updates Workspace participants and chats, and creates Workspace events. The target
dependency points from Workspace to Agents, so participant import belongs in a
Workspace API. Composition must coordinate that operation with Agent catalog
operations while preserving writer exclusion, transactional checks, explicit rollback
and audit. The empty declaration does not certify the current reverse writes.
