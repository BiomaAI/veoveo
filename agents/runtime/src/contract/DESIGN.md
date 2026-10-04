# Agents Public Claim Contract

## Standards And Protocols

The OAuth JWT extension `managed_agent` is a public JSON object with `instance`,
`generation` and `epoch`. JSON Schema draft 2020-12 preserves the published
`ManagedAgentToken` schema name and module-qualified schema identity. The feature
uses Serde admission and depends on foundational types without MCP, async, Store
or GPU runtime dependencies.

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
