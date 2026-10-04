# Gateway Contract

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON and JSON Schema draft 2020-12 | Existing gateway App dependency DTOs and discovery failures; explicit Serde spelling and schemars type names |
| `ai.veoveo/app-resource-dependencies` and `ai.veoveo/app-tool-dependencies` | Gateway-projected metadata for caller-visible App dependencies |
| `ai.veoveo/gateway-discovery-degradation` | Typed discovery failures consumed by the MCP adapter |

## Ownership And Dependencies

This crate owns transport-independent App dependency values, discovery failure
vocabularies and their metadata keys. Typed fields reuse `veoveo-types` admission.
Gateway runtime, MCP adapters and the Console browser contract import these values
directly. MCP owns the MetaObject adapter and sorted/deduplicated degradation wrapper.

A separate crate prevents a dependency cycle: MCP contract already feeds gateway
runtime, while browser schema generation must not enable MCP transports. This library
depends only on foundational types, Serde and schemars. The extraction preserves the
published spellings and schema names. Derived schema collision IDs follow the current
owning Rust module; they do not change serialized JSON or schema definition names.
