# Repository Hardening And Verification Plan

The [fork development model](FORK_DEVELOPMENT.md) governs customization. External
source packages, compatibility manifests, gateway fragments and a separately
published Helm library are outside this plan.

Status: active. Completed xtask foundation, image publication, builder graph,
Justfile removal and external-extension delivery sections are retired. The remaining
sections propose enforcement, smoke ownership and governance work; they do not
change the implemented command surface in `AGENTS.md`.

This document consolidates the repository hardening, compiled tooling, contract
enforcement, test ownership, smoke organization, supply-chain policy, and governance
work planned for Veoveo. It describes a sequence of hard cuts. It does not claim that
every target structure already exists. Normative component documents govern the
delivered surfaces.
This plan does not supersede an existing normative contract before the implementing
change and its documentation land.

The plan covers P0, P1, P2 architecture and design enforcement, and P3 governance. The
advanced correctness program originally discussed as a separate P2 track is deferred.
That deferred work includes mutation testing, fuzzing, Miri, Loom, and broad property
test expansion.

## Standards And Protocols

The hardening boundary includes the following standards, formats, and repository-owned
profiles:

| Standard or profile | Boundary use |
|---|---|
| Rust 1.97.1 and Rust Edition 2024 | canonical compiled tooling and workspace implementation, pinned by `rust-toolchain.toml` |
| Cargo metadata format version 1 | workspace, target, dependency, and smoke-package discovery |
| Docker BuildKit and Docker Buildx Bake | internal OCI build graph, builder-family composition, cache mounts, and image publication; the implementation verifies and pins the latest stable compatible releases before making them canonical |
| `veoveo.io/image-build-plan/v1` | internal typed projection of one source-local Bake selection, its Cargo build units, builder families, cache identities, image coordinates, and release evidence; it is not a public extension contract |
| `veoveo.io/image-build-run/v1` | internal immutable record of an image execution, its output mode, elapsed time, result, and Buildx metadata reference |
| Model Context Protocol | public server protocol governed by `mcp/contract/DESIGN.md`; Streamable HTTP verification uses protocol version `2026-07-28` and only claims the repository profile defined there |
| JSON Schema 2020-12 | canonical MCP tool-input and controlled configuration schemas |
| `veoveo.io/deployment/v8` | repository-development profile for independently resolved sources, exact platform targets, installation-owned Helm values, typed registry transport, and a managed GPU allocator closure |
| `veoveo.io/deployment-lock/v8` | immutable installation revision, combined source evidence, and managed GPU allocator artifacts emitted by repository-development publication |
| Offline bundle schema version 1 | repository-owned image and payload integrity contract |
| OCI Distribution Specification, images, and registries | reproducible build, digest pinning, SBOM, provenance, and private release distribution through an installation-configured registry; OCI packaging does not require public availability |
| `veoveo.io/simulation-runtime-build-lock/v1` | exact canonical-base record for Isaac Sim, Isaac Lab, Warp, Newton, MuJoCo, Kit/Python, CUDA, source archives, wheel digests, and NVIDIA runtime requirements |
| `veoveo.io/simulation-conformance-result/v2` | hardware-backed result for one immutable first-party or anonymous external overlay against one base digest, including native Newton motion |
| `veoveo.io/simulation-runtime-release-evidence/v1` | immutable base, runtime tuple, paired result, and private OCI conformance-bundle release record |
| Kubernetes and Helm | deployment rendering and workload security boundary, using the versions pinned by the repository when implemented |
| SPDX license expressions | dependency-license policy input |
| SARIF 2.1.0 | preferred machine-readable exchange for compatible security and static-analysis results |
| Veoveo smoke descriptor version 1 | planned internal typed protocol between component smoke binaries and `xtask`; it is not a public product contract |

Dependencies, tools, actions, images, and deployment components follow AGENTS.md and
[Contract Evolution](CONTRACT_EVOLUTION.md): verify current upstream releases when
selecting a new dependency or upgrade, prefer latest stable, record any supported older
selection's reason, and pin the qualified result exactly. Ordinary consumer edits need
not upgrade unrelated dependencies. The table does not authorize copying example pins.

## Intended Outcome

Veoveo will have one compiled command surface:

```sh
cargo xtask
```

Rust types will own every controlled policy shape that can be expressed honestly in
Rust. Generated human-readable projections will be checked during compilation.
Repository files that remain intentional configuration will be deserialized into shared
types and validated through the required enforcement gate.

The final repository will not use a Justfile or Python and shell programs to define
quality policy or orchestration. Existing Python deliverables remain supported and
tested. The repository-mandated documentation image generator remains
`docs/images/generate.py` until a separate request explicitly changes that policy;
`xtask` will invoke its canonical command rather than reimplementing it.

MCP servers will own their component smoke tests. Platforms, agents, templates,
showcases, examples, and deployment products will own their respective acceptance
flows. Core protocol and smoke infrastructure will not depend on a domain server,
showcase, or example.

Adding a server will not require editing CI, `xtask`, a central smoke enum, the Console,
a conformance registry, dependency-policy configuration, or copied contract checklists.

The hardening implementation will not assume that every compatible extension belongs to
the Veoveo workspace, shares the Veoveo repository revision, uses the core Helm release,
or ships inside the core offline bundle. The external-extension program follows the
immediate repository hardening track and consumes the boundaries established here. It
will not assume a public package index, public registry, Veoveo-operated control plane,
or internet-routable installation. Artifact coordinates and the installation origin
are independent, installation-owned configuration.

Image publication will resolve selected images into a typed, source-local build plan.
Compatible Rust build units share one Cargo invocation for each builder family and
target platform. Runtime image targets consume those artifacts without carrying a
second package list.

The build track extends the existing stack instead of introducing a new build engine.
Cargo remains the Rust compiler and package graph. Bake remains the declarative OCI
graph over BuildKit. Compiled `xtask` code owns repository-specific planning, policy,
publication source state, tool verification, and evidence. Configuration expresses the
graph; custom Rust code validates and coordinates it.

## Baseline Audit

The initial audit on 2026-07-24 found a strong implementation base:

- The audited starting revision contained 35 Rust crates and about 176,000 lines of
  Rust. The build-track implementation adds the deployment contract, `xtask`, and
  owner-local Bioma acceptance packages, bringing the current workspace to 38 crates.
- The Rust toolchain, shared dependencies, and GitHub Actions are pinned.
- GitHub Actions uses read-only repository permissions by default.
- The codebase contains about 660 Rust test functions and no ignored Rust tests.
- Typed MCP contracts, a conformance client, and Rust process smokes already exist.
- Frontend lint and its existing tests passed locally.
- The locked Python SDK, template, reason-runner, and architecture checks passed locally.

The same audit found enforcement drift:

- CI selected Rust 1.96.1 while `rust-toolchain.toml` selected Rust 1.97.1.
- Rust 1.97.1 Clippy reported three blocking findings.
- Two Rust test targets failed because assertions had drifted from their sources.
- Workspace documentation failed because generic `server` binary names collided and one
  intra-doc link was invalid.
- CI did not run the existing frontend tests or Python checks.
- Cargo commands did not consistently require the lockfile.
- The npm dependency-update path did not point at the Console package.
- Repository-level rustfmt, Clippy, dependency, typo, and editor policies were absent.
- Only four of 55 Docker `FROM` directives carried a SHA-256 digest.
- `testing/smoke` mixed 34 commands spanning core, servers, agents, templates,
  deployments, examples, and showcases.
- `testing/mcp-conformance` depended directly on Frames, Map, and Media server crates.
- The MCP checklist existed in the design, a Rust list, and fourteen server manuals.
  The design had reached C30, the Rust list stopped at C29, and a test still expected 24.
- `profile-publish` created and deleted a fresh detached Git worktree for every
  publication. All 1,130 tracked files received checkout-time modification timestamps,
  which made reconstructed Docker copy layers present unchanged path sources as new to
  Cargo.
- `platform-full` selected eighteen Rust image builds. Fifteen held the same anonymous
  Cargo registry and Git cache mounts with `sharing=locked` for their complete Cargo
  commands, while twelve also shared one locked target cache and limited Cargo to four
  jobs.
- Rust image builds used eight target-cache identities. Frames and Stream shared the
  anonymous `/app/target` identity despite different builder environments, while Map,
  Time, and View duplicated registry and Git caches.
- The root Docker context included about 64 MB under `docs/` that no Dockerfile copied.

P0 begins by revalidating these findings against its starting revision. The audit
snapshot informs the work but does not replace a fresh gate result.

## Governing Principles

### One Authoritative Source

Each fact has one owner. Other representations are generated, discovered, or validated
against it.

- Rust contract types own controlled machine-readable policy.
- Design documents own normative prose and the standards boundary.
- Generated sections project typed catalogs into human-readable documents.
- Cargo metadata owns the resolved Rust workspace graph.
- A gateway control plane owns the server set for one installation.
- A component directory owns its smoke scenarios.
- CODEMAP owns the human routing and ownership index.

No new universal server manifest will duplicate Cargo, gateway, Helm, and CODEMAP data.

### Discovery Before Enumeration

Repository enforcement uses filesystem conventions, Cargo metadata, typed control
planes, and rendered deployment profiles. Central arrays of server names, smoke names,
Dockerfiles, or package paths are prohibited when those entries can be discovered.

### Intentional Configuration Is Not Duplication

Some declarations express a real product choice. A deployment profile must say which
servers it deploys. A gateway catalog must say which endpoints it exposes. CODEMAP must
record a new ownership boundary.

Quality configuration, CI registration, smoke dispatch, and copied compliance catalogs
do not express independent product choices. They must not require another edit.

### Fail Closed

A new contract requirement is not implicitly met. A new smoke requirement is not
silently skipped. A missing GPU is a failure for a GPU workflow. A stale lockfile, schema
projection, generated section, image digest, or compliance profile blocks the gate.

### Cache Is Not Evidence

Build caches improve performance and never establish correctness. Every build works from
an empty cache and from a cache containing any state permitted by its declared namespace.
Cold and warm builds of one source revision produce the same runnable artifacts. Each
publication retains its own complete attestation envelope because SLSA provenance
records invocation identity and time.

Compiled artifact caches cannot cross incompatible source, toolchain, target-platform,
native SDK, feature, profile, or compile-environment boundaries. Cargo registry and Git
download caches may span source contexts only under Cargo's integrity and concurrency
model. Source materialization must not hide a changed file from Cargo freshness checks.

### Typed Boundaries

Controlled shapes use structs, enums, newtypes, and exhaustive matching. Raw JSON is
limited to genuinely open provider input and opaque external payloads. Inter-process
JSON used by compiled Veoveo tools has a shared typed schema and version.

### Owner-Local Verification

A component owns the tests that require knowledge of its domain. Cross-component
acceptance belongs to the composition that selects those components. Generic
infrastructure remains unaware of concrete servers.

### Hard Cuts

When an `xtask` command replaces a Just recipe or shell orchestration path, the old
command is removed in that change. Documentation and CI move with it. Temporary
migration work must not leave permanent aliases or compatibility behavior.

## Component Ownership

[Fork development](FORK_DEVELOPMENT.md) owns customization and downstream migrations.
The [deployment contract](../deploy/contract/DESIGN.md) owns component selection,
installation configuration and image closure. The
[simulation runtime design](../platform/runtimes/simulation/DESIGN.md) owns the shared
GPU runtime and overlay qualification. The [image build design](../tools/image-build/DESIGN.md)
owns publication, source materialization, builder families and cache identity.
These delivered mechanisms are inputs to the unfinished enforcement work below.

## Enforcement Layers

Not every property can be proven by the Rust compiler. The plan makes the strongest
appropriate guarantee at each boundary.

| Layer | Guarantee | Examples |
|---|---|---|
| Rust compilation | exhaustive typed policy and API agreement | contract item enums, compliance profiles, architecture layers, smoke requirements, xtask commands |
| Cargo build-time validation | repository-owned static projections match typed sources | generated design sections, contract revisions, schema snapshots, controlled JSON documents |
| `cargo xtask enforce` | resolved metadata and external tools satisfy policy | Clippy, rustfmt, Cargo graph, npm, uv, Docker, Helm, Kubernetes, security scanners |
| Component smoke | a production binary or image satisfies its black-box contract | MCP auth, task flow, artifact publication, GPU rendering |
| Composition acceptance | selected real components work together | Bioma, SUMO, UAV simulation, agent missions |
| Live operational proof | hardware and external services are actually available | NVIDIA, a headed hardware WebGPU-or-WebGL browser path, billed provider calls, cluster deployment |

Build scripts remain deterministic and local. They validate repository-owned static
files and never perform network access, install tools, launch containers, or mutate
source files.

## Target Repository Shape

The hard cuts produce the following ownership structure:

```text
mcp/
├── contract/                   normative types and shared implementation
├── conformance/                executable protocol certification
└── apps-extension/

deploy/
├── contract/                   typed deployment-profile model
├── offline/                    typed bundle contract and implementation
└── helm/

tools/
├── xtask/                      compiled repository command surface
└── smoke-kit/                  domain-neutral smoke infrastructure

platform/
├── gateway/smoke/
├── store/smoke/
└── recordings/smoke/

servers/
└── *-mcp/smoke/

agents/
└── kernel/smoke/

templates/
└── python-mcp/smoke/

showcase/
├── sumo/smoke/
└── uav-sim/smoke/

examples/
└── bioma/smoke/
```

The top-level `testing/` directory has no coherent responsibility in the target
architecture. It is removed after conformance, smoke support, deployment contracts,
offline verification, and component scenarios reach their owners.

## Canonical Rust Policy

### MCP Contract Catalog

`mcp/contract` will own one typed contract catalog. An internal declarative macro will
generate the requirement enum, the complete ordered collection, stable identifiers,
descriptions, serialization, and documentation table projection from one declaration.

Conceptually:

```rust
contract_items! {
    C01 => "Each capability uses the canonical MCP surface",
    C02 => "Every tool declares input and output schemas",
    // ...
    C30 => "Gateway sessions share the scoped connection pool",
}
```

The design document remains the normative explanation. Its checklist table becomes a
verified generated section backed by the typed catalog.

### Compliance Profiles

Shared server compliance will use typed profiles. A profile handles every contract item
through exhaustive matching. Adding an item produces compiler errors until each profile
assigns a fail-closed status.

Hosted Rust servers and external extensions may use different profiles. A server
declares only typed, justified overrides. Repeated lists of every met item disappear
from server manuals. The served contract resource and the manual projection originate
from the same declaration.

### Architecture Layers

Repository layers become an enum used by the dependency policy:

```text
MCP contract and extensions
Platform reusable services
Domain servers
Applications and deployment composition
Testing and tooling
```

Path and Cargo metadata classify workspace packages. Exhaustive rules govern permitted
directions. No crate registry is maintained by hand.

### Deployment Contract

The types currently embedded in the smoke deployment module move to
`deploy/contract`. That crate owns deployment profiles, registry references,
Kubernetes targets, resources, release specifications, secret formats, schema versions,
pure validation, and the resolved distinction between platform components and MCP
servers. It does not expose raw core-chart records as an extension-facing contract.

`xtask` depends on this crate for operational commands. Process execution does not live
in the contract library.

### Offline Contract

`deploy/offline` becomes a Rust package that owns the typed image manifest, bundle
layout, integrity verification, builder, and loader. The current shell builder and
loader are replaced through a hard cut. Source-inspection tests that look for shell
fragments disappear. Core bundle inputs remain core-owned; extension or combined bundles
consume explicit artifact locks under their composition owner.

## Compiled Repository Command Surface

### Xtask Placement

The package lives at `tools/xtask`, is named `veoveo-xtask`, and is never published.
Repository-local Cargo configuration provides:

```toml
[alias]
xtask = "run --quiet --package veoveo-xtask --"
```

The direct equivalent remains available through Cargo:

```sh
cargo run -p veoveo-xtask -- enforce
```

The alias is convenience, not a second implementation.

### Xtask Structure

```text
tools/xtask/src/
├── main.rs
├── context.rs
├── process.rs
├── tools.rs
└── commands/
    ├── enforce.rs
    ├── smoke.rs
    ├── mcp.rs
    ├── test.rs
    ├── deploy.rs
    ├── release.rs
    ├── bundle.rs
    ├── docs.rs
    ├── hooks.rs
    └── doctor.rs
```

`main.rs` parses a typed Clap command and dispatches it. Each command module owns one
workflow. Process invocations use argument arrays and typed paths, never interpolated
`bash -c` programs.

Secrets use a redacting wrapper. Dry-run output for release and deployment commands
shows public arguments while concealing secret values.

### Command Families

The target command surface includes:

```sh
cargo xtask enforce
cargo xtask enforce rust
cargo xtask enforce console
cargo xtask enforce python
cargo xtask enforce repository
cargo xtask enforce supply-chain

cargo xtask smoke list
cargo xtask smoke run map-mcp
cargo xtask smoke run --scope pr
cargo xtask smoke run --requires nvidia-gpu

cargo xtask mcp conformance --endpoint <url>
cargo xtask mcp schemas

cargo xtask deploy validate
cargo xtask deploy cluster up
cargo xtask deploy apply
cargo xtask deploy down

cargo xtask release images --profile <path> --profile-revision <sha>
cargo xtask release charts
cargo xtask bundle validate
cargo xtask bundle create
cargo xtask bundle load

cargo xtask docs architecture
cargo xtask docs pdf
cargo xtask hooks install
cargo xtask doctor
```

One-step native commands remain native. The xtask does not add an alias for every Cargo,
npm, or uv command.

### Xtask Boundaries

The xtask owns ordering, exact flags, prerequisite reporting, process diagnostics,
environment policy, failure summaries, and command discovery. It does not reimplement
Clippy, rustfmt, Ruff, ESLint, Cargo dependency tools, Helm, Docker, protocol
conformance, or smoke lifecycle behavior.

Process and evidence APIs receive an explicit source context. An individual image
command uses one source, while deployment v5 coordinates several such contexts. No
command assumes that repository root, current revision, image tag, and chart root are
universal installation identities.

Image release uses a locked, tool-owned persistent publication worktree keyed by source
identity. Moving that worktree to another exact revision preserves unchanged file
metadata and updates changed paths. A disposable full checkout is not the canonical
publication input.

The release planner models build sources, build units, builder families, target
platforms, produced artifacts, and immutable image coordinates with Rust types. Cargo
metadata and the selected image graph provide membership. The planner does not maintain
a parallel package registry.

`cargo xtask doctor` reports missing or incorrect tools. Enforcement never installs or
updates a developer tool automatically.

### Local Hooks

Local hooks accelerate feedback and do not define policy. `cargo xtask hooks install`
installs a thin launcher for a compiled fast profile. The check selection remains in
Rust. No Python pre-commit framework or copied hook command list is required.

The fast profile covers formatting, conflict markers, accidental large files, secrets,
and applicable changed-file linters. Full Clippy, workspace tests, containers, and
cluster workflows remain pre-push or CI work.

`pre-commit` and `prek` are not initial policy dependencies. They may be reconsidered
only if they can invoke the same compiled fast profile without carrying their own hook
selection or server registry.

## Test Ownership

### Taxonomy

| Category | Owner | Purpose |
|---|---|---|
| Unit test | production crate | pure local behavior |
| Integration test | production crate | multiple modules or real owner-local storage |
| Contract implementation test | owning contract crate | shared constructor, type, schema, and protocol invariant |
| Protocol conformance | `mcp/conformance` | certification of an arbitrary running MCP server |
| Component smoke | component-local `smoke/` crate | production binary or image through a black-box boundary |
| Composition acceptance | example or showcase | selected real components operating together |
| Deployment validation | `deploy/` contracts and xtask | profiles, Helm, images, offline bundles |
| Operational command | xtask | publish, cluster, install, bundle, and documentation workflows |

Static validation, publishing, cluster lifecycle, and schema generation are not smoke
tests.

### MCP Conformance

`testing/mcp-conformance` moves to `mcp/conformance`. It becomes a reusable library and
a thin CLI. The library accepts a typed profile and returns a typed report containing
the contract revision, executed requirement IDs, observed capabilities, results,
evidence, and implementation identity.

Server smoke crates call the library directly. External operators may run the CLI
against a registered extension.

The conformance artifact publishes independently of the Veoveo workspace. Its
hosted-server profile selects applicable checks from a typed declaration and includes
authentication rejection, Host handling, advertised surfaces, schemas, tasks,
subscriptions, and URI ownership where declared. Verifying that a server accepts a
valid gateway assertion and rejects a forged, expired, or misaddressed one belongs to
the extension's own tests against the profile in
[`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md).

Generic conformance depends on MCP contracts and protocol infrastructure only. Direct
dependencies on Frames, Map, Media, another server, a showcase, or an example are
prohibited.

Transport tests that prove the shared canonical server constructor move to
`mcp/contract/tests`. Domain schema checks and provider fakes move to their server smoke
owners. Scripted agent model behavior moves to `agents/kernel/smoke`.

### Repository Contract Enforcement

Typed checklist parsing, revision types, compliance profiles, and document projection
live in `mcp/contract`. Repository discovery and certification live in
`mcp/conformance` and are exposed through `cargo xtask enforce repository`.

Every discovered server must carry the required local design and agent documents.
Checks derive the server set from repository convention and Cargo metadata.

### Shipped Configuration Tests

Gateway configuration validation moves to `platform/gateway`. UAV resource
relationships move to the UAV owner. Bioma identity and deployment overrides move to
the Bioma acceptance package. SUMO catalog policy moves to its showcase.

Large copied JSON objects are not compared for equality. A real derivation relationship
uses a typed base and generated projection. Independent environment configuration is
validated against shared types and owner-specific invariants.

## Component-Owned Smoke

### Smoke Kit

`tools/smoke-kit` is a domain-neutral Rust library. It owns:

- RAII process and container guards.
- Temporary workspaces and unique resource identities.
- Typed command specifications.
- Readiness and bounded timeout handling.
- HTTP and MCP clients.
- Log retention and secret redaction.
- Cleanup evidence.
- NVIDIA and headed-browser preflight.
- Structured smoke descriptors and results.

It does not own a domain fixture, server constructor, deployment name, provider
behavior, DuckDB staging path, or component list.

### Typed Scenario Protocol

Rust smoke packages compile against a shared scenario contract:

```rust
pub trait SmokeScenario {
    const ID: SmokeId;
    const REQUIREMENTS: SmokeRequirements;

    async fn run(context: &mut SmokeContext) -> anyhow::Result<SmokeEvidence>;
}
```

Requirements describe Docker, Kubernetes, NVIDIA GPU, headed browser, external network,
billed service, and required secret needs. A shared macro generates `describe`, `list`,
`run`, and `run-all` commands for each smoke binary.

The generated launcher performs declared preflight before scenario code runs. A GPU
scenario cannot continue without an NVIDIA-backed path. Visual browser acceptance
requires headed Chrome with hardware WebGPU or WebGL. Headless behavioral scenarios
declare a different evidence class and cannot close a visual or GPU gate.

The internal descriptor and result protocol uses a shared versioned schema, with Rust
types and generated bindings for other owning harnesses. Maintained TypeScript browser
and SDK-language harnesses may produce the same bounded lifecycle and evidence contract.
Existing Rust smokes remain valid; migration should remove measured complexity.

### Discovery

A component smoke package follows a local naming and placement convention:

```text
servers/map-mcp/smoke/
package: veoveo-map-mcp-smoke
binary:  map-mcp-smoke
```

Cargo workspace globs include component-local smoke packages where Cargo can express
the pattern safely. `xtask` discovers packages and targets through Cargo metadata.
There is no central smoke enum.

CI scopes are predicates over typed requirements. A new scenario enters the applicable
PR, container, GPU, cluster, or external-service scope without a workflow-list edit.

This discovery governs first-party packages in the current source context. External
repositories retain their own workspace and expose compatible smoke descriptors or
published conformance endpoints.

### Unique Production Binary Names

Generic local binaries named `server` become unique:

```text
map-mcp
media-mcp
frames-mcp
view-mcp
```

Containers may copy a uniquely named build artifact to `/usr/local/bin/server`.
Workspace targets no longer overwrite one another, and Rustdoc output no longer
collides.

### Scenario Ownership Migration

| Current concern | Target owner |
|---|---|
| Media MCP auth and tasks | `servers/media-mcp/smoke` |
| Frames tools, tasks, and artifacts | `servers/frames-mcp/smoke` |
| Map acquisition, activation, and routing | `servers/map-mcp/smoke` |
| Optimization GPU routing, mathematical solving, and verification | `servers/optimization-mcp/smoke` |
| View local and live GPU rendering | `servers/view-mcp/smoke` |
| Perception GPU workflow | `servers/stream-mcp/smoke` |
| Reason GPU workflow | `servers/reason-mcp/smoke` |
| UAV server contract behavior | `servers/uav-sim-mcp/smoke` |
| Full UAV runtime composition | `showcase/uav-sim/smoke` |
| SUMO operations and verification | `showcase/sumo/smoke` |
| Bioma installation acceptance | `examples/bioma/smoke` |
| Datasheet template acceptance | `templates/python-mcp/smoke` |
| Gateway auth, HTTP, session, and projection | `platform/gateway/smoke` |
| Agent lifecycle and scheduling | `agents/kernel/smoke` |
| Agent use of several real servers | the owning example or showcase |
| SurrealDB platform integration | `platform/store/smoke` |
| Recording ingest composition | `platform/recordings/smoke` |
| Helm and profile validation | `deploy/` contract enforcement |
| Offline bundle validation | `deploy/offline` |
| Contract schema generation | `mcp/contract` and xtask |

Gateway smokes use generic fake upstreams. A scenario that requires a real domain server
is composition acceptance and moves to the owner that selects that composition.

### Smoke Hardening

Every smoke run has fixed overall and readiness deadlines, no automatic test retry, a
unique workspace, cleanup guards, captured output, redacted secrets, exact binary and
image identity, deterministic fixtures, and machine-readable evidence.

External network and billed scenarios require explicit selection. GPU and browser work
fails closed. No scenario accepts a software renderer as proof.

## P0: Green And Reproducible

### P0.1 Restore The Canonical Rust Gate

- Re-run the audit on the implementation starting revision.
- Fix all Rust 1.97.1 Clippy findings.
- Resolve stale contract and configuration tests by enforcing invariants.
- Fix Rustdoc links.
- Document workspace libraries while unique binary names are migrated.
- Make `rust-toolchain.toml` the only Rust version source.
- Require `--locked` on dependency-resolving Cargo operations.
- Run tests without hiding later failures after the first failed target.

Acceptance requires formatting, Clippy, tests, and library documentation to pass on the
canonical toolchain.

### P0.3 Complete Existing Language And Configuration Coverage

- Run Console lint, tests, and build.
- Run locked Python SDK, template, reason-runner, and architecture checks.
- Validate controlled gateway and deployment configurations.
- Correct dependency-update paths.
- Remove duplicated Node version declarations.
- Resolve the headless `--disable-gpu` documentation recipe against the mandatory GPU
  policy.

Existing Python products remain under test, but Rust owns orchestration.

## P1: Coding And Supply-Chain Policy

### P1.1 Rust Policy

Add stable Rust 2024 rustfmt policy, workspace lint inheritance, explicit unsafe policy,
`rust-version`, package-audience classification, private publication settings,
repository metadata, and editor defaults.

Selected high-signal Clippy rules are baselined before they become errors. Blanket
pedantic or nursery denial is not used as a substitute for judgment. The single known
unsafe archive operation retains a narrow reviewed exception and safety explanation.

### P1.2 Compiled Local Feedback

Implement `cargo xtask hooks install` and the typed fast enforcement profile. The hook is
a generated launcher with no copied policy. CI remains authoritative.

### P1.3 Rust Dependency Policy

Introduce exact-pinned tooling for licenses, sources, banned dependencies,
vulnerabilities, unused dependencies, and important duplicate versions. The policy
operates on Cargo metadata and `Cargo.lock`.

Current transitive duplication is baselined. New high-risk or major-version duplication
is blocked before the baseline is ratcheted.

Published packages receive an additional dependency-closure check. A public facade may
depend on published implementation packages, but it cannot expose an unpublished crate
or force consumers to reproduce the Veoveo repository layout.

### P1.4 Repository And Artifact Policy

Add repository-wide checks for Docker build policy, image digests, container
vulnerabilities, GitHub Actions, shell formatting, Helm rendering, Kubernetes schemas,
workload security, documentation links, and typos.

File discovery uses repository paths and extensions. Tool configuration does not list
MCP servers. One dependency-update authority covers each ecosystem.

Helm and Kubernetes enforcement accepts arbitrary chart roots and validates a reference
consumer when the extension library is introduced. Render checks cover init containers
and distinguish source-built images from pinned or mirrored upstream images.

Release builds produce SBOM and provenance evidence. Rust binaries intended for
distribution carry auditable dependency information.

### Planned Tool Set

The implementation will verify the current stable release of each selected tool before
pinning it. The intended roles are:

| Tool or native facility | Role | Enforcement location |
|---|---|---|
| rustfmt with `rustfmt.toml` | canonical Rust formatting | fast hook and Rust gate |
| Clippy with workspace lints and `clippy.toml` | Rust correctness and policy | Rust gate |
| Cargo locked mode and future-incompatibility reporting | reproducible resolution and compiler migration warning | Rust gate |
| cargo-deny with `deny.toml` | license, source, ban, and important duplicate policy | supply-chain gate |
| cargo-audit | RustSec advisory evaluation for `Cargo.lock` | supply-chain gate and schedule |
| OSV-Scanner | cross-ecosystem lockfile and artifact vulnerability scan | supply-chain gate and schedule |
| cargo-shear | unused and misplaced Rust dependencies | repository gate |
| cargo-auditable | embedded Rust dependency evidence in release binaries | release build |
| ESLint type-aware configuration and TypeScript compiler | Console static and type checking | Console gate |
| Ruff and one selected Python type checker | Python format, lint, and type policy | Python gate |
| npm and uv locked modes | reproducible non-Rust environments | Console and Python gates |
| Docker BuildKit checks and Hadolint | Dockerfile correctness, resolved build graph, and cache policy | artifact gate |
| Trivy | container and Kubernetes configuration scanning | artifact and deployment gates |
| actionlint and zizmor | GitHub Actions correctness and security | repository gate |
| ShellCheck and shfmt | transitional shell safety before shell removal | repository gate |
| kubeconform | rendered Kubernetes schema validation | deployment gate |
| Helm lint and render | chart contract validation | deployment gate |
| Taplo | TOML formatting and parse validation | fast and repository gates |
| typos and Lychee | prose spelling and link integrity | repository gate |
| Gitleaks | repository secret detection | fast and supply-chain gates |

No tool installs itself during enforcement. The xtask doctor reports the exact pinned
version and installation command. A tool is removed when its entire input language
disappears from the repository.

## P2: Architecture And Design Enforcement

This phase is active. It does not include the deferred advanced correctness program.

### P2.1 Canonical Discovery And Onboarding

Enforce the repository component convention, Cargo membership for Rust components,
required design and agent documents, standards sections, package naming, workspace
policy inheritance, private publishing, and typed contract revision.

Checks discover components. They contain no server names.

Discovery distinguishes first-party workspace components from external artifacts and
endpoints. External compatibility never requires workspace membership.

### P2.2 Dependency Direction

Use Cargo metadata to enforce a small set of boundaries already supported by CODEMAP:

- `mcp/contract` does not depend on domain servers, applications, deployment, or tools.
- Generic gateway code does not depend on a concrete domain server.
- Platform libraries do not depend on server implementations.
- Domain servers consume shared MCP and platform crates, not another server
  implementation.
- `mcp/conformance` does not depend on domain servers, examples, or showcases.
- `tools/smoke-kit` does not depend on Veoveo production implementations.
- A supported SDK facade does not expose private or unpublished implementation
  packages.
- Component smoke packages may depend on conformance and smoke-kit.
- Examples and showcases may compose the components they own.

The implementation first reports the current graph. A rule becomes blocking after its
baseline is clean and its owning design document states the boundary.

### P2.3 Deployment And Control-Plane Relationships

Typed Rust validation enforces uniqueness, URI and route consistency, declared
cross-server schemes, contract revisions, profile references, image identity, singleton
MCP workload semantics, bootstrap validity, artifact audiences, recording dependencies,
and mandatory GPU resources. Universal gateway collision checks live in
`GatewayControlPlane::validate`.

Different environments may select different servers. Enforcement validates
relationships inside the selected profile and does not require every server in every
installation.

Deployment preflight evaluates a resolved graph of platform components and MCP servers.
It does not assume that every artifact shares one repository, revision, tag, chart, or
Helm release.

### P2.4 Contract And Type Boundaries

Public MCP inputs pass the shared schema profile. Controlled wire shapes use typed
models. Cross-server identities use canonical URI and domain types. Shared transport,
task, identity, and document machinery comes from the owning contract crates.

Package audience, compatibility inputs, source identity, artifact coordinates, and
conformance profiles are also typed. These models remain useful when the future SDK,
Helm library, and fragment composer become external release products.

Source-text searches are transitional evidence only. Type construction, schema
inspection, Cargo metadata, and black-box observation carry the long-term gate.

### P2.5 Module Responsibilities

Review files above the repository threshold and split mixed responsibilities. Long
parameter lists become typed commands when the values form one domain operation.
Binary entrypoints remain composition roots.

There is no hard line-count gate or generic design-pattern score. A large cohesive file
may remain with a documented reason.

## P3: Governance

### P3.1 Repository Policy

Add a vulnerability-reporting policy, broad ownership boundaries, contribution routing,
and the selected license and notice policy. CODEOWNERS follows architectural paths and
does not enumerate MCP servers.

### P3.2 Protected Delivery

Protect `main` with stable required checks and code-owner review for contract, auth,
deployment, and GPU-critical paths. Enable secret scanning and push protection. Run
scheduled dependency and vulnerability checks. Attach SBOM and provenance attestations
to release artifacts.

Manual pull-request checklists do not restate automated gates.

## Deferred Correctness Program

The following work is deliberately excluded from P0 through P3:

- cargo-nextest adoption.
- Broad property-test expansion.
- Fuzz targets.
- Mutation testing.
- Miri.
- Loom.
- Global or changed-line coverage thresholds.
- Sanitizer matrices beyond a concrete defect investigation.

These tools may later run as owner-local or scheduled checks. Their deferral does not
weaken the architecture, smoke ownership, contract conformance, or supply-chain work in
this plan.

## Delivery Sequence

The implementation proceeds through coherent hard cuts:

1. Restore the canonical Rust gate and resolve current drift.
2. Add existing Console, Python, documentation, and configuration checks.
3. Add Rust format, lint, metadata, and unsafe policy.
4. Add compiled local hooks.
5. Add dependency and vulnerability policy.
6. Add container, workflow, Kubernetes, and documentation policy.
7. Give production server binaries unique local names.
8. Create `tools/smoke-kit` and the typed smoke descriptor protocol.
9. Add Cargo-discovered xtask smoke dispatch.
10. Move server-owned smoke scenarios one component at a time.
11. Move gateway, platform, agent, template, showcase, example, and deployment
    scenarios to their owners.
12. Promote component-oriented deployment and ownership-aware offline models into their
    contract crates.
13. Remove the central smoke binary and the top-level `testing/` directory.
14. Enable component discovery and dependency-direction enforcement.
15. Enable source-aware deployment, canonical gateway, contract, type-boundary, and
    module-responsibility enforcement.
16. Add repository governance and protected delivery settings.

Each move removes the old owner and command in the same change. A migration commit
leaves the repository coherent and the required gate green.

## New First-Party MCP Server Onboarding Contract

After this plan, a new Rust MCP server requires:

| Intentional change | Requirement |
|---|---|
| Component code, manifest, tests, `DESIGN.md`, and `AGENTS.md` | required |
| Component-local smoke package | required |
| Cargo workspace membership | automatic through a safe glob where possible; otherwise one explicit build declaration |
| Component image definition | required when the component ships an image; discovered into the selected source-local build plan |
| CODEMAP ownership entry | required |
| Gateway entry for an installation that exposes it | required for that installation |
| Deployment entry for a profile that runs it | required for that profile |
| CI workflow edit | prohibited |
| Xtask command or smoke enum edit | prohibited |
| Conformance registry edit | prohibited |
| Console registration edit | prohibited |
| Lint, dependency, or scanner configuration edit | prohibited |
| Copied compliance checklist | prohibited |
| Shared builder package-list edit | prohibited |

The server smoke first runs generic MCP conformance, then its owner-local domain
scenarios. A composition that selects several real servers owns its own acceptance
package.

An external extension remains in its own repository. It consumes published SDK and
conformance artifacts, owns its chart and smoke package, and joins an installation
through explicit gateway, deployment, source, and artifact inputs. It does not require a
Veoveo workspace, CI, xtask, or CODEMAP edit.

## Required CI Shape

GitHub Actions remains a minimal platform adapter. It checks out the repository,
installs exact-pinned prerequisites, and invokes compiled commands.

Required pull-request lanes include:

- `cargo xtask enforce rust`
- `cargo xtask enforce console`
- `cargo xtask enforce python`
- `cargo xtask enforce repository`
- `cargo xtask enforce supply-chain`
- `cargo xtask smoke run --scope pr`

Container, GPU, cluster, external-network, and billed scopes run only on compatible
runners and triggers. Requirement discovery determines scenario membership. Workflow
YAML does not list components.

Container lanes resolve representative image selections through the typed build planner.
They verify builder-family grouping and cache identity before building. Performance
regressions are prevented through graph invariants and cold-cache correctness rather
than a timing threshold on shared runners.

## Completion Criteria

The plan is complete when all of the following statements hold:

- `cargo check --workspace --all-targets` compiles every typed policy and smoke package.
- `cargo xtask enforce` is the canonical local and CI gate.
- The Justfile no longer exists.
- No Python or shell program defines repository quality or orchestration policy.
- `mcp/conformance` is domain-neutral, independently publishable, capability-driven, and
  can certify an arbitrary compatible server.
- `tools/smoke-kit` has no production implementation dependency.
- Every first-party hosted MCP server owns a component-local smoke package.
- Showcases and examples own cross-component acceptance.
- The central `veoveo-smoke` package and top-level `testing/` directory no longer exist.
- Adding a server requires no CI, xtask, conformance-list, scanner, or Console edit.
- Adding a first-party Rust image requires no shared builder package-list edit.
- Supported package policy rejects an unpublished or repository-local dependency from
  an external facade.
- Contract additions fail compilation until shared profiles and conformance coverage are
  exhaustive.
- Static generated projections fail the build when stale.
- Canonical gateway validation rejects route, mount, URI, ownership, and policy identity
  collisions regardless of how the document was authored.
- Deployment and gateway configuration fail typed enforcement when component, source,
  artifact, or workload relationships diverge.
- Deployment validation and publication derive one exact platform target set and reject
  any missing or unnecessary platform or RRD transport image.
- Image publication materializes one exact committed source revision without resetting
  unchanged path metadata on every run.
- A selected image graph produces at most one Cargo build action for each compatible
  source-local builder family, target platform, and profile.
- Rust target caches are explicit, source-aware, platform-aware, and isolated across
  incompatible builder contracts.
- Cold-cache and warm-cache builds produce identical runnable artifacts and independent
  complete release-attestation envelopes.
- Core, extension, and installation-composed offline artifacts retain explicit owners.
- An external extension can consume published verification contracts without joining the
  Veoveo workspace or core image builder.
- An installation can use an organization-owned client origin and authenticated artifact
  registry reachable only through private DNS, an internal network, or a VPN.
- The versioned extension Helm library can be consumed from the installation's private
  registry or verified offline bundle without a public Veoveo service.
- The existing Isaac Sim base is the sole shared simulation lineage, and Veoveo UAV and
  independently owned external simulator overlays pass GPU acceptance against the same
  base digest.
- GPU and browser evidence always proves hardware-backed execution.
- Release artifacts carry exact dependency, SBOM, and provenance evidence.
- Protected delivery prevents merging when any required enforcement layer fails.

The plan hardens Veoveo by reducing duplicated knowledge. Stronger enforcement is
valuable only when the repository has fewer sources of truth after the change than it
had before.
