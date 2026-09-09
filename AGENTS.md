# Agent Instructions

## Repository Map

Read [`docs/CODEMAP.md`](docs/CODEMAP.md) before changing a cross-component contract or
placing a new module. It indexes the normative documents, component designs, code
ownership, and shortest implementation paths. Update the map in the same change when a
document moves, a component is added, or an ownership boundary changes.

A design document belongs beside the code it governs: an MCP server's at
`servers/{server}-mcp/DESIGN.md`, and other contract-bearing components (the
gateway composer, conformance, extensions, deploy contract, simulation runtime,
templates) keep a `DESIGN.md` in their own directory the same way.
Repository-wide architecture stays under `docs/`.

The active tree is wider than `servers/` and `docs/`: `platform/` holds the
gateway and runtimes, `agents/` the kernel and durable runtime, `mcp/` shared
contracts and bridges, `apps/console` the Console, `extensions/` external
extension contracts, `examples/bioma` the enterprise GitOps reference,
`showcase/` the simulator workloads, `testing/` conformance and smoke,
`sdk/` and `templates/` the Python surface, `tools/` xtask and screenshots,
and `deploy/` installation material. `docs/CODEMAP.md` indexes all of it.

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

## Contract Evolution

Internal refactors use a hard cut: replace obsolete names and behavior, update callers,
and keep one canonical internal model. Do not introduce hidden aliases or fallbacks.

Public protocols, installed clients, persisted data, and deployment transitions may use
explicit versioned adapters or migrations. Declare the boundary, owner, supported
versions, qualification cases, and retirement criteria in the owning DESIGN.md. Keep
authorization and domain behavior in the canonical implementation. An adapter cannot
invent guarantees its upstream does not provide or silently weaken a security policy.
Unsupported combinations fail with an actionable upgrade or configuration diagnostic.

Published removals require a documented migration and support window. A breaking cut
is appropriate for an unreleased surface or an explicitly coordinated installation
upgrade. Prove mixed-version safety when a rolling update permits overlap; otherwise
declare and test the required drain. Persistent formats need recovery and rollback
semantics before destructive conversion. `mcp/bridges/legacy` remains an explicitly
declared boundary under its own AGENTS.md.

The accepted decisions and remaining implementation work are recorded in
[`docs/CONTRACT_EVOLUTION.md`](docs/CONTRACT_EVOLUTION.md). Contract permission does not
establish that a new runtime profile is implemented or qualified.

## Dependency Currency

Use exact, qualified dependency, toolchain, image, and deployment pins. For a new
dependency or a planned upgrade, verify the latest stable release from its authoritative
upstream source and prefer it. Record a concrete compatibility or qualification reason
when selecting an older supported release. Do not copy versions from examples or guides.

Editing a consumer does not require an unrelated dependency upgrade. Review upstream
security and support status regularly, prioritize applicable security fixes, and qualify
upgrades independently of feature work when possible. Unsupported or vulnerable pins
require a documented mitigation, owner, and replacement deadline; a pin is not a reason
to ignore a security defect. Update affected tests and documentation with a pin change.

Pre-release dependencies and maintained provider patches require an explicit product
reason, exact provenance, a supported qualification matrix, and an upstream/removal
plan. Record the constraint beside the pin.

## GPU Execution Is Mandatory

Veoveo visual, simulation, perception, rendering, and visual-verification workflows
must use an accessible hardware GPU. A software renderer is not a degraded mode and
must never be accepted as evidence that a workflow works.

Always choose the fastest compatible NVIDIA hardware-accelerated path for rendering,
simulation, video encode and decode, perception, and other GPU-suitable work. A
provider-neutral public contract does not justify replacing a faster NVIDIA data plane
with a slower CPU implementation. Avoid GPU-to-CPU readback, duplicate encoding, and
per-consumer rendering when a CUDA buffer, NVENC bitstream, or shared GPU render product
can carry the same result.

When existing code performs GPU-suitable work on the CPU and a compatible accelerated
implementation is available, mark the exact path with `TODO(GPU)` and state the intended
replacement. A TODO records migration debt; it does not authorize a new CPU fallback or
allow the unaccelerated path to become acceptance evidence.

Before visual acceptance, an interactive demo, or screenshot/publication capture used
as visual evidence, prove that the browser is headed and that at least one of its
high-performance WebGPU adapter or WebGL context is hardware-backed. Probe both APIs
when the browser exposes them. Reject the
browser when neither API reaches hardware, and reject SwiftShader, llvmpipe, software
adapters, and software rasterizer warnings as hardware evidence. If a browser loses its
last hardware-backed graphics API, stop the workflow immediately. Do not keep using that
browser, replace visual verification with an API-only check, capture an image, or report
the visual workflow as verified.

Behavioral browser tests for forms, authorization, navigation, keyboard handling, and
other non-rendering contracts may run headless with maintained browser tooling. Declare
them as behavioral evidence. They cannot establish visual fidelity, graphics throughput,
video quality, or GPU execution. A behavioral pass never substitutes for required headed
hardware acceptance, and GPU workload tests retain their hardware requirement.
Headless diagnostic screenshots may accompany a behavioral test failure when labeled
as diagnostic evidence. They cannot be used for product publication or visual acceptance.

Browser-side H.264 playback is the only software exception. A client may decode a
stream in software when Media Capabilities reports the exact configuration as
`supported` and `smooth` but not `powerEfficient`. The UI must identify that path as
software H.264 decode and may claim hardware decode only when `powerEfficient` is true.
This exception does not relax the headed hardware WebGPU-or-WebGL requirement,
server-side NVENC, GPU rendering, simulation, perception, or any other accelerated
workload.

GPU containers must request the required Kubernetes GPU resource and fail closed when
the NVIDIA device, driver capability, or hardware rendering backend is unavailable. Do
not add CPU rendering fallbacks, optional GPU modes, or deployment profiles that remove
a required GPU workload to fit the cluster.

## Provider Completion And Recovery

Each provider adapter declares and qualifies its completion profile. Prefer the native
authenticated event path: webhooks, resumable watches, or event streams. A definitive
synchronous response may settle an operation. Bounded status reconciliation is permitted
when the provider exposes authoritative, operation-correlated outcomes; a provider with
only a status API may use a declared bounded polling profile. Generic background polling
and undocumented transport fallbacks are prohibited.

Persist operation identity and dispatch intent before side effects. Authenticate and
correlate observations, persist settlement before acknowledging recoverable delivery,
deduplicate repeats, and reject stale operation/process epochs. Declare event retention,
gap recovery, deadlines, request/rate budgets, cancellation, and consistency semantics.
An expired wait, missing event, disconnected watch, or unqualified not-found response
does not prove failure or authorize another mutation. Retain unresolved outcomes and
resource fencing until authoritative evidence settles them. Existing provider profiles
keep their documented mechanism until a replacement is implemented and qualified.

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

A module boundary does not require another process, image, or service. Add a deployment
boundary for a concrete privilege, fault, scaling, or release-isolation requirement and
record the operational cost. Core capability status does not prescribe a hosting topology.

## Repository Command Discipline

The repository has no Justfile. One-step Cargo, Helm, uv, Docker, and Kubernetes
commands remain native. Repository-specific policy and multi-step coordination belong
in `cargo xtask`.

The xtask surface is `doctor`, `enforce rust|python`, `image`, `release`, `smoke`,
and `test-report`. Before committing a build-input change, run the checks it touches
through the evidence recorder
(`cargo xtask test-report run --name <check> -- <command>`, then
`cargo xtask test-report show`) and commit the updated
`testing/local-test-report.json` with the change, since the GitHub workflow only
displays that committed evidence. Documentation-only changes do not invalidate build
evidence. Do not commit red or stale report entries.

Tests use the maintained tooling appropriate to their boundary: Rust for service and
process invariants, TypeScript/browser tooling for Console behavior, and the SDK's
language for consumer acceptance. Keep domain assertions in one owning harness.
`cargo xtask smoke` dispatches scenarios and their required build prerequisites; it
does not reimplement their lifecycle, assertions, retries, or cleanup. Until a new
dispatcher is implemented, native framework commands run through `test-report`.

Every harness must provide bounded timeouts, isolated fixtures, owned cleanup, secret
redaction, useful failure diagnostics, and machine-readable results. Declare hardware,
network, identity, and service prerequisites explicitly. Prefer existing framework
behavior over custom browser synchronization or protocol clients. Add dependencies
only when they remove concrete complexity from our tests.

Evidence reuse must follow each check's actual inputs and execution environment. Shared
contract or toolchain changes broaden that dependency closure. Unknown dependencies
require conservative rechecking. The current local report still uses a repository-wide
digest; continue its documented workflow until scoped evidence is implemented under
[`docs/CONTINUOUS_INTEGRATION.md`](docs/CONTINUOUS_INTEGRATION.md).

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
