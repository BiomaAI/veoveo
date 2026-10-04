# Shared Macro Instructions

Follow the repository [instructions](../../AGENTS.md) and this crate's
[design](DESIGN.md). Keep expansions focused on declarations and trait delegation.
Runtime semantics belong to ordinary public traits and helpers in `platform/types`.
This proc-macro crate must not depend on `veoveo-types`, MCP, a database SDK or a
server. The `surreal` hook emits the existing SDK derive into a database-owning
consumer; it does not link that SDK here.

Preserve each owner's serialized forms, schema identity, metadata and variant order.
Qualify declaration misuse with compile-fail doctests in `platform/types` and
consumer-owned value/schema comparisons. New macro shapes require an owner-reviewed
reason and a design entry before adoption.
