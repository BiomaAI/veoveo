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

## Module Boundaries

Do not create monolithic god files. Rust files should have a focused responsibility and
compose through explicit modules instead of growing into thousands of lines of mixed
types, HTTP routes, state, auth, policy, CLI, tests, and helpers.

When a source file is approaching roughly 1,000 lines, prefer extracting a cohesive
module before adding more behavior. Files above roughly 1,500 lines require a concrete
reason to remain that large. Generated files, schema snapshots, and intentionally dense
test fixtures are the only normal exceptions.

Binary entrypoints should stay thin: parse CLI/config, initialize dependencies, wire
routes/services, and delegate real behavior to modules. New gateway work should be split
into modules such as auth routes, admin routes, OAuth flows, metadata, application state,
HTTP wiring, and command handlers instead of continuing to expand one file.

Gateway code is not exempt from this rule while it is moving fast. When a gateway file
starts mixing unrelated concerns, split the concern into a module in the same change
instead of deferring cleanup until after the feature lands.

## Justfile Discipline

Do not abuse the Justfile as a smoke-test framework or scripting language. Keep recipes as
short, memorable dispatch commands for humans. Complex orchestration, process lifecycle,
assertions, retries, JSON parsing, and cleanup belong in Rust smoke harnesses.

All smoke tests for this repository must be implemented in Rust. The Justfile may build
and dispatch those Rust smoke commands, but it must not contain shell-based smoke-test
logic.

Only add smoke-test helper crates when they are current, maintained, and remove concrete
complexity from our actual multi-process smoke tests. Do not add a crate just because it
is popular for CLI tests; if it does not materially improve server lifecycle,
readiness, assertions, cleanup, or diagnostics, keep the in-repo Rust harness.

## Naming

The workspace is `veoveo`. Crates are Veoveo crates. Folder names should stay concise and
should not repeat `veoveo` unless there is a concrete reason.

MCP server crates use `*-mcp`, not `*-mcp-server`.

The media MCP server may use provider-specific implementation internally, but user-facing
names should stay provider-neutral.
