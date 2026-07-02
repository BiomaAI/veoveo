# Agent Instructions

## Hard Cut Policy

Default behavior in this repository is a hard cut.

Do not preserve old names, old environment variables, old commands, old protocol paths,
old package names, old resource URIs, old behavior, or compatibility shims unless the
user explicitly asks for compatibility in the current request.

This means:

- Rename by replacing the old surface, not by adding aliases.
- Remove obsolete code paths instead of keeping alternate paths.
- Use one canonical configuration name.
- Use one canonical protocol/resource shape.
- Do not add hidden compatibility behavior.
- Do not describe unsupported legacy behavior in user-facing docs.

If a change would break existing callers, that is acceptable by default. Document the new
canonical path, update tests and examples to it, and delete the old path.

## Provider Completion

Provider job completion is webhook-only. Do not add provider status polling, polling
fallbacks, backup status checks, or timeout recovery paths that query the provider.
Missing webhook delivery is an operational failure.

## Strong Types

Strong types are extremely important in this repository. Prefer typed structs, enums,
and explicit domain types whenever the shape is known or controlled by our contract.
Use raw JSON only at genuinely open-ended boundaries, such as provider-specific model
input schemas or opaque provider payloads that cannot be modeled honestly yet.

## Naming

The workspace is `veoveo`. Crates are Veoveo crates. Folder names should stay concise and
should not repeat `veoveo` unless there is a concrete reason.

MCP server crates use `*-mcp`, not `*-mcp-server`.

The media MCP server may use provider-specific implementation internally, but user-facing
names should stay provider-neutral.
