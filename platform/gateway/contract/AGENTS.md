# Gateway Contract

Keep gateway declarations and registration mechanics here: kernel actions, catalog
sections and targets, authorization-resource identities, HTTP/TLS configuration,
App dependencies and discovery values. Reuse foundational admission and preserve
owner wire/schema profiles. Optional modules own their vocabularies and codecs;
the installation recipe chooses which declarations to bind.

MCP adapters belong in `mcp/contract`; hosted behavior belongs in the gateway runtime.
Do not add transport clients, database drivers, HTTP servers or asynchronous runtime
dependencies to this crate. Public registration tests use independent owner types
through the library's public API.
