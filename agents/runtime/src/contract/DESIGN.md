# Agents Contract

## Standards And Protocols

The OAuth JWT extension `managed_agent` is a public JSON object with `instance`,
`generation` and `epoch`. JSON Schema draft 2020-12 preserves the published
`ManagedAgentToken` schema name and module-qualified schema identity. The feature
uses Serde admission and depends on foundational types without MCP, async, Store
or GPU runtime dependencies.

Gateway policy JSON uses the fourteen spellings declared by `AgentAction`. The
gateway catalog registry binds that closed vocabulary through the lower gateway
contract. Registration preserves the public action names and declares each action's
selector requirements, supported target and audit access.

## Admission And Authority

The owner codec rejects unknown fields and invalid instance identifiers. Public
counter fields retain their signed 64-bit wire profile. Syntax admission does not
establish current registration or grant authorization. The runtime checks current
registration, installed templates and registration-source collisions before using
the claim. Internal execution attribution converts both counters to positive unsigned
64-bit values without casts; invalid counters refuse forwarding.

Composition reserves the owner claim name even when the adapter is disabled. A
bound typed key belongs to one immutable registry and cannot read or contribute
values through another registry. Internal assertions carry checked audit attribution
under their separately versioned protocol. Public claim wire behavior is unchanged.

## Policy Actions

Agent control, definition management and instance lifecycle actions target the
gateway. Policy rules may select gateway profiles; MCP server, tool, resource scheme,
prompt and protected-resource selectors are forbidden for these actions. Rust
callers obtain an action handle from the registry's typed `AgentAction` key before
requesting authorization. Shared policy code evaluates the rule and principal.

`src/gateway/tests/policy_actions.rs` qualifies the selector restrictions through the
full catalog validator. The vocabulary's own tests check its wire spellings. These
tests do not establish installed instance lifecycle behavior.

## Authoring Contract

The [authoring contract](authoring/DESIGN.md) owns definitions, revisions, model choices,
runtime templates and instance-control DTOs. Its installation facts and borrowed
caller facts keep validation independent of gateway catalog and principal types.
The `contract` feature gates every dependency it uses and excludes runtime services;
the optional `catalog` feature provides the MCP-model projection adapter.

## Operator Control

`control.rs` owns authenticated operator messages, input-request decisions, wake
receipts and conversation views. Closed JSON request shapes reject supplied authority.
Conversation entries expose runtime wakes and episodes without server-domain fields.
The browser schema bundle keeps the `agent-control` filename and existing DTO shapes.
