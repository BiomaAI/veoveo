# Agent Instructions

## Repository Map

Read [`docs/CODEMAP.md`](docs/CODEMAP.md) before changing a cross-component contract or
placing a new module. It indexes the normative documents, component designs, code
ownership, and shortest implementation paths. Update the map in the same change when a
document moves, a component is added, or an ownership boundary changes.

An MCP server's design document belongs at `servers/{server}-mcp/DESIGN.md` beside its
crate. Repository-wide architecture stays under `docs/`.

## Worktrees and Commit Discipline

Development normally spans multiple Git worktrees. Treat each worktree as an independent
branch checkout, inspect its branch and status before editing, and preserve changes that
belong to another user or agent. Synchronize worktrees by fast-forwarding when their
histories permit it. Never force histories together merely to make every checkout match.

Commit often as work progresses. Prefer several small, logical commits over one late
catch-all commit because coherent checkpoints make review, recovery, testing, and
cross-worktree synchronization easier. Each commit should capture one completed concern
and leave the repository in a coherent state. Do not mix unrelated changes into a commit,
and do not use commits to conceal incomplete or failing work.

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

## Dependency Currency

Use the latest stable upstream release whenever a dependency, toolchain, image,
or deployment component is introduced or touched. Do not copy stale versions
from examples, blog posts, or upstream guides. Verify the current release from
the authoritative upstream source, pin it exactly for reproducibility, and
update its tests and documentation in the same change.

Pre-release dependencies require an explicit product reason. If the upstream
project has no stable release, use its latest published pre-release and record
that constraint beside the pin.

## GPU Execution Is Mandatory

Veoveo visual, simulation, perception, rendering, and visual-verification workflows
must use an accessible hardware GPU. A software renderer is not a degraded mode and
must never be accepted as evidence that a workflow works.

Before browser automation, an interactive demo, or a screenshot run, prove that the
browser is headed and that both its high-performance WebGPU adapter and WebGL context
are hardware-backed. Reject missing contexts, SwiftShader, llvmpipe, software adapters,
and software rasterizer warnings. If a browser loses WebGL or WebGPU, stop the workflow
immediately. Do not keep using that browser, replace visual verification with an API-only
check, capture an image, or report the visual workflow as verified.

GPU containers must request the required Kubernetes GPU resource and fail closed when
the NVIDIA device, driver capability, or hardware rendering backend is unavailable. Do
not add CPU rendering fallbacks, optional GPU modes, or deployment profiles that remove
a required GPU workload to fit the cluster.

## Provider Completion

Provider job completion is webhook-only. Do not add provider status polling, polling
fallbacks, backup status checks, or timeout recovery paths that query the provider.
Missing webhook delivery is an operational failure.

## MCP Server Contract

Every hosted MCP server and registered extension complies with the normative
server contract in [`mcp/contract/DESIGN.md`](mcp/contract/DESIGN.md): the full
protocol surface for the domain, the canonical schema profile, the shared
runtime boundary, packaging and registration, the well-known docs and contract
resources, and the required crate documents (`DESIGN.md` and `AGENTS.md`).
Work that changes protocol behavior starts from that document, and compliance
gaps are declared in the server's `Contract Compliance` section rather than
left silent.

## Strong Types

Strong types are extremely important in this repository. Prefer typed structs, enums,
and explicit domain types whenever the shape is known or controlled by our contract.
Use raw JSON only at genuinely open-ended boundaries, such as provider-specific model
input schemas or opaque provider payloads that cannot be modeled honestly yet.

## Module Boundaries

Do not create monolithic god files. Rust files should have a focused responsibility and
compose through explicit modules instead of growing into thousands of lines of mixed
types, HTTP routes, state, auth, policy, CLI, tests, and helpers.

Rust is verbose, and many files include colocated tests. A source file around 1,000 lines
is acceptable when it has one clear concern and remains easy to navigate. Do not split
files mechanically by line count alone.

Split a file when responsibilities start to compound: mixed protocol handling, HTTP
routes, persistence, auth, policy, CLI, tests, and helpers in one place; repeated local
helper patterns; hard-to-name sections; or changes that require understanding unrelated
behavior. Files above roughly 1,500 lines require a concrete reason to remain that large.
Generated files, schema snapshots, and intentionally dense test fixtures are the normal
exceptions.

Binary entrypoints should stay thin: parse CLI/config, initialize dependencies, wire
routes/services, and delegate real behavior to modules. New gateway work should be split
into modules such as auth routes, admin routes, OAuth flows, metadata, application state,
HTTP wiring, and command handlers instead of continuing to expand one file.

Gateway code is not exempt from this rule while it is moving fast. When a gateway file
starts mixing unrelated concerns, split the concern into a module in the same change
instead of deferring cleanup until after the feature lands.

The gateway is expected to support many hosted MCP servers, profiles, and auth policies.
That scale alone is not a refactor trigger. Refactor when the code stops composing cleanly
or when server-specific behavior leaks into generic gateway modules.

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

## Documentation Image Generation

Documentation raster images use one canonical generation path: WaveSpeed through
`docs/images/generate.py`. Run it with the repository-managed Python environment:

```sh
uv run --env-file .env --python 3.13 docs/images/generate.py [figure ...]
```

The script's canonical model is `openai/gpt-image-2/text-to-image`. Keep API credentials
in `.env` and use `MEDIA_PROVIDER_API_KEY`; never print or copy the credential into another
file.

Do not use a built-in image-generation tool, another image service, Inkscape, system Python,
or an ad hoc replacement pipeline for these assets. Do not change the established image
style or generation method unless the user explicitly requests that change. Update the
prompts in `docs/images/generate.py`, generate through WaveSpeed, and inspect every output
before accepting it.

## Writing Style

Docs and product copy use classic style with varied sentence rhythm. Write confident
declaratives that each assert one checkable thing, and let causal connectives carry the
argument. Vary sentence length and shape so no template repeats.

Do not scaffold prose with parallel constructions: no semicolon chains, no "X, so that Y"
ladders, no bold-led beat paragraphs, no triads used as structure. Tables and bullet
lists are structure and remain fine; the ban is on list-in-prose.

State requirements and capabilities forward. Do not dramatize or invent failures of the
outside world to make the platform look necessary. Capability copy states outcomes and
the questions a capability answers in the user's domain; mechanisms belong in technical
sections and get at most a closing sentence elsewhere. Keep one hard number where it
earns trust.

The abstract of docs/veoveo-whitepaper-print.html is the register exemplar.

## Design Documentation

Every design document must include a `## Standards And Protocols` section near
its beginning. The section names each external standard, wire protocol, data
format, and repository-owned extension that forms part of the design boundary.
Pin a version when the implementation pins one, state the supported profile or
subset, and distinguish an internal adapter protocol from a public contract.
Do not imply complete conformance when Veoveo implements only selected features.
