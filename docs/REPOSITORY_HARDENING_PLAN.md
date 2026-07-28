# Repository Hardening And Verification Plan

Status: approved implementation direction.

This document consolidates the repository hardening, compiled tooling, contract
enforcement, test ownership, smoke organization, supply-chain policy, and governance
work planned for Veoveo. It describes a sequence of hard cuts. It does not claim that
every target structure already exists. Delivery-state tables identify the hard cuts
that have landed, and normative component documents govern those delivered surfaces.
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
| Model Context Protocol | public server protocol governed by `mcp/contract/DESIGN.md`; the current Streamable HTTP verification uses protocol version `2025-11-25` and only claims the repository profile defined there |
| JSON Schema 2020-12 | canonical MCP tool-input and controlled configuration schemas |
| `veoveo.io/deployment/v2` | repository-development profile for independently resolved source revisions, source-owned images and charts, gateway requirements, and typed platform selection |
| `veoveo.io/deployment-lock/v2` | immutable combined evidence emitted by repository-development source publication |
| `veoveo.io/gateway-server-fragment/v1` | extension-owned declaration of one hosted server's protocol surface and platform requirements |
| `veoveo.io/gateway-binding/v1` | installation-owned declaration of exposure, authorization, tenant, policy, and producer bindings |
| Offline bundle schema version 1 | repository-owned image and payload integrity contract |
| `veoveo.io/compatibility-manifest/v1` | generated supported release contract relating SDK artifacts, contract and schema revisions, standalone tools, Helm API, and optional simulation runtimes |
| Veoveo extension Helm library API | delivered versioned chart-helper contract, packaged for authenticated OCI registry or offline-bundle distribution; consumer charts remain responsible for their values shape and installation policy |
| OCI Distribution Specification, images, and registries | reproducible build, digest pinning, SBOM, provenance, and private release distribution through an installation-configured registry; OCI packaging does not require public availability |
| `veoveo.io/simulation-runtime-build-lock/v1` | exact canonical-base record for Isaac Sim, Isaac Lab, Warp, Newton, MuJoCo, Kit/Python, CUDA, source archives, wheel digests, and NVIDIA runtime requirements |
| `veoveo.io/simulation-conformance-result/v1` | hardware-backed result for one immutable first-party or anonymous external overlay against one base digest |
| `veoveo.io/simulation-runtime-release-evidence/v1` | immutable base, runtime tuple, paired result, and private OCI conformance-bundle release record |
| Kubernetes and Helm | deployment rendering and workload security boundary, using the versions pinned by the repository when implemented |
| SPDX license expressions | dependency-license policy input |
| SARIF 2.1.0 | preferred machine-readable exchange for compatible security and static-analysis results |
| Veoveo smoke descriptor version 1 | planned internal typed protocol between component smoke binaries and `xtask`; it is not a public product contract |

Every dependency, tool, action, image, or deployment component introduced while
executing this plan must use the latest stable upstream release verified from its
authoritative source, then be pinned exactly. The table records protocol boundaries and
does not authorize copying example versions into the implementation.

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

## Build Track Delivery State

The repository-wide plan remains active. The following build concerns have crossed
their hard-cut boundary:

| Plan concern | Current state |
|---|---|
| P0.2 xtask foundation | delivered for `doctor`, canonical Rust enforcement, typed Rust smoke dispatch, image planning, builder management, and image release; later deployment, bundle, documentation, and hook commands remain planned |
| Justfile hard cut | delivered; documented workflows use `cargo xtask`, the typed Rust smoke harness, or a clear one-step native command, with no compatibility aliases |
| P0.4 publication inputs | delivered through a locked persistent worktree, exact commit resolution, source-local profile loading, metadata-preservation tests, and the `docs/` context exclusion |
| P1.5 internal image graph | delivered for the initial `linux/amd64` families, including consolidated trixie and bookworm Cargo actions, typed cache identities, managed Buildx and BuildKit, reproducible output timestamps, and immutable execution evidence |
| External repository flow | Python SDK distributions, domain-neutral native/OCI conformance and gateway composition, typed artifact contracts, the private extension Helm library, compatibility bundle generation, an agent-run external integration procedure, typed platform selection, source-role-qualified local publication, anonymous multi-repository contract acceptance, and canonical simulation-overlay certification are delivered |

The normative operating contract is
[`IMAGE_BUILDS.md`](IMAGE_BUILDS.md). Measured acceptance belongs in
[`IMAGE_BUILD_PERFORMANCE.md`](IMAGE_BUILD_PERFORMANCE.md), including clean
committed-source evidence for implementation checkpoint `7d7693a`.

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
Cold and warm builds of one source revision produce equivalent artifacts and evidence.

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

## External Extension Compatibility

The extension program publishes SDK artifacts and conformance tooling, provides a
reusable Helm library, composes gateway fragments, resolves a typed minimal platform,
and coordinates independently versioned repositories. The delivered schemas now govern
those seams.

### Supported External Artifact Boundary

Repository policy distinguishes private packages, published implementation packages,
supported external facades, and developer tools. A supported package may not expose an
unpublished dependency or a repository-layout type. Published artifacts carry complete
coordinates, checksums, license metadata, toolchain requirements, and provenance.
Publication means that an immutable version is available through a configured artifact
source. It does not require anonymous access, public discovery, or an internet-facing
registry.

The compatibility manifest is generated from typed release inputs, Cargo metadata, and
the resolved artifact set. It is not a second hand-maintained version registry.

The curated MCP SDK controls the supported external surface. Hardening does not publish
the internal crate graph as an external API.

### Installation-Owned Addressing And Distribution

An installation owns one canonical external origin, such as
`https://veoveo.customer.example` or `https://veoveo.bioma.ai`. External means that
authorized clients can address the installation. It does not mean that the origin is
reachable from the public internet. Private DNS, split-horizon DNS, internal load
balancers, VPN routing, and air-gapped networks are valid deployment forms.

The artifact registry is separate configuration. A customer-operated registry, a
Veoveo-operated private registry, or an offline bundle may supply the same immutable
artifacts. Registry endpoints, repository prefixes, trust roots, and credential Secret
references belong to the installation. No extension contract embeds a Veoveo registry
hostname or assumes that image and chart artifacts cross the public internet.

The extension Helm library is a package rather than an installed service. Consumer
charts may resolve its versioned package from the configured authenticated OCI registry
or carry the exact package in an offline bundle. Production composition records its
digest with the consumer chart and rejects an unresolved mutable dependency.

### Standalone Conformance

`mcp/conformance` must build and publish without depending on a domain server or
requiring an external consumer to compile the Veoveo workspace. A typed hosted-server
profile selects checks from the server or extension declaration.

Certification covers the applicable authentication boundary, Host validation,
capability discovery, schemas, tools, resources, prompts, tasks, subscriptions,
notifications, and URI ownership. The report records the exact compatibility inputs.

### Canonical Gateway Safety

Safety that applies to every complete gateway control plane belongs in
`GatewayControlPlane::validate`. Route, mount, MCP path, URI-scheme, resource ownership,
and policy identity collisions cannot be composer-only checks.

The fragment composer produces an ordinary validated `GatewayControlPlane`. The
extension describes its surface while the installation continues to own exposure and
authorization.

`veoveo.io/gateway-server-fragment/v1` contains the extension-owned server identity,
routes, URI schemes, capabilities, upstream identity, and declared platform
requirements. `veoveo.io/gateway-binding/v1` contains installation-owned profile,
tenant, authorization, secret, audience, and policy decisions. The composer orders
inputs deterministically and records schema versions, source identities, input hashes,
contributed object identities, and the final control-plane digest.

### Source-Aware Evidence

Xtask workflows carry a named source and independent revision. Their internal command and
evidence types still carry an explicit source root, resolved revision, artifact
coordinate, image digest, and chart identity. They do not rely on one global `HEAD`, one
image tag, or one chart root.

Deployment v2 applies that constraint to named sources. Each source resolves its own
revision, chart artifact, image lock, gateway fragment, compatibility manifest, and
release evidence. Installation status reports the source revision, Helm release, and
image digest independently. Local development may materialize one detached worktree per
source; production composition consumes immutable published artifacts and does not
require source checkouts.

### Source-Local Build Graphs

Each source context resolves and builds its own image units. An external extension keeps
its Cargo workspace, builder families, cache namespace, and release evidence in its
repository. Installation composition consumes immutable image coordinates and digests;
it does not combine external packages into the Veoveo workspace builder.

The deployment v2 release resolver coordinates several explicit source contexts. It
treats each source revision as an independent graph and never creates one universal
package list.

### Component-Oriented Deployment

Deployment validation distinguishes MCP servers from platform components and evaluates
a resolved component graph. The core chart owns first-party workload definitions and
accepts typed server identities; internal Deployment records are not a public extension
API.

Semantic preflight verifies gateway and workload agreement, bootstrap targets, artifact
audiences, recording dependencies, image selection, and mandatory GPU resources. Helm
schema validates values shape; Rust owns semantic relationships.

### Canonical Isaac Simulation Base And External Overlays

Veoveo owns one provider-neutral simulation foundation at
`platform/runtimes/simulation`. Bake target `simulation-runtime` publishes that
foundation, and `simulation-runtime.lock.json` records its exact build contract. The
UAV runtime and external simulator images are overlays; neither owns or replaces the
canonical tuple.

| Input | Canonical value |
|---|---|
| Isaac Sim | `6.0.1-rc.7+release.42383.32955d8d.gl` from the digest-pinned Isaac Sim 6.0.1 image |
| Isaac Lab | `v3.0.0-beta2.patch1` at `ffff603eafc6b74264a5261cc0183d6a65390d78` |
| Python | `3.12.13` |
| Warp | `1.15.0` |
| Newton | `1.4.0` |
| MuJoCo | `3.10.0` |
| MuJoCo Warp | `3.10.0.3` |
| CUDA toolkit | `12.9` |
| Kit | `110.1.2` |

Isaac Lab is pinned to a pre-release because it has no stable Isaac Sim 6.0-compatible
release. The lock records that reason, the full source revision and archive digest, the
digest of every replacement wheel, the upstream image digest, and the minimum NVIDIA
runtime contract.

Isaac Sim registers bundled Warp and Newton payloads through Kit. Installing newer
wheels beside them would create a mixed module graph. The base replaces the controlled
payload coherently and establishes one authoritative Python path for Warp and Newton.
The hardware probe rejects a loaded module outside those roots or a runtime component
that differs from the embedded lock.

The existing UAV runtime and `testing/fixtures/simulation-overlay` derive from the same
canonical base target. Hardware acceptance passed on an NVIDIA RTX 4090 with driver `580.173.02`.
Each overlay proved CUDA, Vulkan, RaytracedLighting, the exact runtime tuple, 20 distinct
Newton tiled-camera outputs, and 20 distinct RTX render products. The anonymous overlay
also executes overlay-owned CUDA code. Host networking, host IPC, software rendering,
and CPU simulation are not accepted substitutes.

The pod contract includes an NVIDIA GPU request, a compatible runtime class, writable
Kit cache and data paths, and a private memory-backed `/dev/shm`. The 20-camera probe
passed with a 2 GiB shared-memory limit. Shader-cache persistence remains configurable
because cold RTX startup performs material compilation work.

The shared base does not own one simulator domain. Cesium, Pegasus, PX4, UAV code,
scenarios, and UAV environment conventions remain in the existing UAV overlay, where
they can evolve without changing the shared runtime contract.

An external repository may build its own simulator overlay from the exact base digest.
The overlay owns its code, assets, scenarios, chart, dependency lock, smoke evidence,
and image in its configured private registry. It may add compatible Kit extensions and
Python packages, but it may not silently replace the base's Isaac Sim, Isaac Lab, Warp,
Newton, MuJoCo, CUDA, or core Kit/Python versions. A conflicting dependency set fails
the compatibility profile until Veoveo deliberately advances the canonical base.

`simulation-certify` accepts only digest-addressed base and overlay images. It inspects
their SBOM and provenance attestations and writes
`veoveo.io/simulation-conformance-result/v1`. `cargo xtask release
simulation-runtime` accepts one first-party and one anonymous result from the same
source revision and base digest, publishes their private OCI evidence bundle, and writes
`veoveo.io/simulation-runtime-release-evidence/v1`. Compatibility publication consumes
that release evidence rather than copying the runtime tuple by hand.

### Bundle And Composition Ownership

Core, extension, and installation-composed bundles have separate ownership. The core
offline builder never discovers and silently absorbs an external repository's
artifacts. A combined bundle consumes explicit source and artifact locks and belongs to
the installation or extension composition that selected them.

### External Smoke Ownership

First-party servers in this repository own workspace smoke packages. An external
extension owns its smoke package in its repository and consumes the published
conformance and smoke contracts. It does not join the Veoveo Cargo workspace to become
verifiable.

Static UAV registration checks now live with `servers/uav-sim-mcp`. SUMO deployment
checks live with the SUMO crate, and Bioma control-plane and cross-surface checks live
under `examples/bioma/acceptance`. UAV domain acceptance and generic Simulation View
acceptance remain independent. A showcase-owned composed command consumes both and
captures the real Console follow camera at takeoff, mission, and landing plus the
governed Rerun recording. Its revision-qualified evidence is a migration input for the
smoke-kit sequence below; it does not justify adding example-specific assertions to
generic gateway tests or MCP conformance.

### Consumer-Generic Helm Enforcement

Chart hardening accepts a chart root and rendered installation as inputs. Security,
digest, identity, NetworkPolicy, secret mount, GPU, container, and init-container checks
do not assume the core chart is the only consumer. The library chart is verified
through a reference consumer as well as Veoveo's own charts.

The extension Helm library, fragment schemas, component selection schema, and
multi-source profile have crossed their hard-cut boundaries. Compatibility generation,
the simulation build lock, hardware result schema, paired-overlay publisher, and
platform-image closure enforcement have crossed the same boundary.

### Delivered External Extension Slices

The supported external workflow was implemented through coherent vertical slices:

1. Defined the normative external-extension contract and created an anonymous,
   independently owned acceptance consumer.
2. Published the curated Python SDK, compatibility manifest, and standalone conformance
   binary and image through configurable private artifact sources and offline bundles.
3. Delivered the extension Helm library, stable installation labels, private registry
   configuration, and a reference consumer chart.
4. Added typed server fragments and installation bindings, canonical path collision
   validation, deterministic composition, and composition provenance.
5. Hard-cut repository-development profiles to v2 with named source roles, independent
   revisions, collision-free image and chart locks, structured rendered-image
   inspection, and source-specific status. Fielded installations consume published
   extension releases through ordinary digest-pinned Helm or GitOps inputs.
6. Resolved the typed minimal platform graph, beginning with the platform foundation,
   Artifact MCP, Frames MCP, and Recording MCP. Disabled components disappear from
   workloads, services, storage, policy, bootstrap, gateway inventory, and digest
   requirements together.
7. Implemented the coherent Kit payload mechanism in the existing Isaac Sim base, added
   the pinned Isaac Lab input, and certified both the Veoveo UAV overlay and an anonymous
   external overlay against the resulting base digest.

The delivered workflow keeps a clean external checkout outside the Veoveo workspace.
It consumes released artifacts, builds and publishes its own image and chart, runs
standalone conformance, contributes a server fragment, and joins an installation
without editing Veoveo or hand-authoring a complete gateway document. A coding-agent
runbook coordinates those existing artifacts and standard tools without adding a
Veoveo deployment wrapper. Repository-development validation and image publication
reject a selected platform whose Bake groups omit Artifact, Frames, Map, Media,
Recording, or RRD transport images.

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
├── apps-extension/
└── task-extension/

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
command uses one source, while deployment v2 coordinates several such contexts. No
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

### Justfile Removal

The Justfile is removed. Its supported commands have typed replacements or clear native
commands. Existing Rust smoke binaries continue to own service lifecycle while
`cargo xtask smoke` dispatches them.

CODEMAP, README, CI, and component documentation move to the new command in the same
hard-cut change.

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
authentication rejection, valid internal assertions, Host handling, advertised
surfaces, schemas, tasks, subscriptions, and URI ownership where declared.

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

Every smoke package compiles against a shared scenario contract:

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
scenario cannot continue without an NVIDIA-backed path. A browser scenario cannot
continue unless headed Chrome exposes hardware-backed WebGPU or WebGL.

The internal descriptor and result protocol uses shared versioned Rust types serialized
as JSON across the process boundary.

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

### P0.2 Introduce Xtask

- Create the modular `veoveo-xtask` package.
- Add the repository-local Cargo alias.
- Implement `doctor` and `enforce rust`.
- Route Rust CI through the same command.
- Remove the corresponding Just recipes when replaced.

The first xtask change coordinates existing green commands. It does not absorb smoke or
deployment behavior.

### P0.3 Complete Existing Language And Configuration Coverage

- Run Console lint, tests, and build.
- Run locked Python SDK, template, reason-runner, and architecture checks.
- Validate controlled gateway and deployment configurations.
- Correct dependency-update paths.
- Remove duplicated Node version declarations.
- Resolve the headless `--disable-gpu` documentation recipe against the mandatory GPU
  policy.

Existing Python products remain under test, but Rust owns orchestration.

### P0.4 Stabilize Image Publication Inputs

- Exclude repository roots such as `docs/` from the Docker context when no Dockerfile
  consumes them.
- Replace the disposable publication checkout with a locked, tool-owned persistent
  worktree keyed by source identity.
- Resolve and verify one exact committed revision before changing the publication
  worktree.
- Preserve unchanged path metadata across revisions while allowing Git to update every
  changed path.
- Test source materialization with fixture revisions that distinguish changed and
  unchanged files.
- Prove that an empty BuildKit and Cargo cache remains a supported input.

This hard cut removes the incremental rebuild amplification before the image graph is
consolidated. It adds no compatibility alias for `profile-publish`. The later xtask hard
cut removes that command when image release assumes deployment publishing.

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

### P1.5 Image Build Graph And Cache Policy

The image release planner resolves selected Rust image units and groups them by a typed
builder-family identity. Compatibility includes:

- Source identity.
- Rust toolchain, builder image, libc, native SDK, and system dependency contract.
- Target platform, architecture, and Rust target triple.
- Cargo profile, feature resolution, rustflags, and compile-time environment.

Each selected compatible family uses one Cargo invocation at Cargo's available
parallelism. Unique production binary names land before this consolidation. Runtime
targets consume the family artifact target through the Bake graph and keep their
runtime-specific bases, files, users, and configuration.

Build-unit membership comes from Cargo metadata and the intentional selected image
definitions. A fixed shared-builder command that lists every package is prohibited.
Adding a discovered first-party image must not require editing a second builder package
list.

Target-cache identities derive from source identity, builder family, target platform,
and Cargo profile. They are never anonymous and are not fragmented by output image.
Package registry and Git caches use a concurrency policy that does not hold one
BuildKit-exclusive lock throughout independent compiles when Cargo's cache locking
provides the required safety.

Enforcement validates the resolved graph rather than imposing an elapsed-time threshold.
It rejects incompatible cache sharing, anonymous Rust target caches, fixed low job
limits on consolidated builders, and more than one Cargo build action for a selected
family. Cold-cache and warm-cache acceptance compare produced artifact identities and
release evidence.

Deployment profiles add a second graph invariant. The typed platform selection and
gateway requirements resolve the exact Veoveo-owned OCI image closure. Validation and
profile publication compare that closure with the selected Bake targets before building
or pushing. `external-extension-platform` is the canonical group for Artifact, Frames,
Map, Media, Recording, and producer-side RRD transport. These images remain separate
services; the group coordinates their publication and never copies them into an
extension or simulation image.

#### Canonical Build Inputs

`docker-bake.hcl` remains the image catalog. A Rust image target declares its Cargo
package, production binaries, builder family, build mode, and optional auxiliary
artifacts through repository-owned OCI labels:

```text
io.veoveo.build.mode
io.veoveo.build.package
io.veoveo.build.binaries
io.veoveo.build.family
io.veoveo.build.auxiliary
```

Build mode is `rust-shared` or `rust-standalone`. Binary and auxiliary collections use
comma-separated identifiers from closed Rust enums and validated Cargo target names.
The family name selects a typed builder contract; it does not authorize two
incompatible environments to share compiled artifacts.

Cargo metadata proves that each declared package and binary exists. Bake selects the
intentional image targets. No checked-in shared-builder command, central image manifest,
or `xtask` package array repeats that membership.

Shared runtime targets consume a family artifact target through the named context
`veoveo-rust-artifacts`. The artifact target receives the exact selected package and
binary arguments through an ephemeral JSON Bake override. `xtask` first resolves the
checked-in HCL with `docker buildx bake --print`, constructs the typed plan, merges the
override, resolves the result again, and executes only that verified graph.

#### Internal Image Commands

The repository image surface is:

```sh
cargo xtask image builder status
cargo xtask image builder ensure
cargo xtask image builder recreate --confirm veoveo

cargo xtask image plan --target <target>
cargo xtask image plan --group <group>
cargo xtask image build --target <target>
cargo xtask image build --group <group>

cargo xtask release images --profile <path> --profile-revision <ref>
cargo xtask release images --target <target> --registry <registry> --revision <ref>
cargo xtask release images --group <group> --registry <registry> --revision <ref>
```

`image plan` and `image build` use the current checkout and record its full revision and
dirty state. A local build loads the selected images into Docker. A release resolves one
full commit, moves the persistent publication worktree to that commit, applies immutable
revision tags, pushes images, and records Buildx metadata and resulting digests.

Raw Docker and Bake commands remain diagnostic implementation surfaces. They are not a
second supported route for Rust images because they cannot construct the typed family
overlay.

#### Managed Builder

Image commands use a named `docker-container` Buildx builder called `veoveo`; they never
change the operator's global builder selection. The builder uses a digest-pinned stable
BuildKit image and a checked-in daemon configuration for the canonical loopback
development registry. `xtask` passes `--builder veoveo` explicitly.

Buildx is exact. The command accepts an exact host plugin. On supported Linux hosts it
can download the official release into the main worktree's ignored `target` directory
and verify its architecture-specific SHA-256. Git's common directory makes that binary
and its isolated Buildx state identical from every linked worktree. Docker credentials
remain in the operator's normal configuration.

The command creates and bootstraps a missing builder. An existing builder with the
wrong driver, image, daemon version, or configuration fails validation. The checked-in
BuildKit garbage-collection policy retains source-local and Cargo cache mounts long
enough for the incremental workflow and applies explicit reserved, maximum, and
free-space bounds. Only `image builder recreate --confirm veoveo` may remove an
incompatible builder and its cache.

The implementation verified Buildx 0.35.0 and BuildKit 0.31.2 on 2026-07-25.
Execution rechecks their authoritative release pages before pinning. A newer stable
release replaces this baseline; a pre-release does not.

#### Publication Source Lifecycle

The publication source lives under the main worktree:

```text
target/veoveo-xtask/publication/<source-id>/source
```

The source identity covers the normalized Git origin, canonical common Git directory,
and object format. An adjacent file carries an exclusive advisory lock from source
preparation through the final Bake phase.

The first release creates a detached worktree. Later releases require that worktree to
remain registered and clean, then use an ordinary detached checkout to move it to the
resolved commit. Git updates changed paths and preserves unchanged path metadata. The
tool never removes a healthy publication worktree after a build and never silently
resets, cleans, or recreates corrupt state.

An in-repository deployment profile is loaded from the selected revision, not from a
different checkout. Every Bake path and Docker context resolves inside the locked
publication source.

#### Initial Builder Families

The first delivery supports one explicit target platform, `linux/amd64`.

| Family | Initial Rust image units |
|---|---|
| `rust-trixie-v1` | gateway, artifact service, recording forwarder, recording hub, recording MCP, Console BFF, artifact MCP, media MCP, timeseries MCP, DuckDB MCP, optimization MCP, frames MCP, stdio bridge, and agent kernel when selected |
| `rust-bookworm-v1` | map MCP, time MCP, and view MCP |
| `rust-uav-bookworm-v1` | UAV MCP; standalone because the embedded WebRTC bundle changes its compile environment |
| `rust-deepstream-v1` | Stream MCP |
| `rust-vllm-v1` | reason MCP |
| `rust-sumo-bullseye-v1` | SUMO MCP |

The trixie and bookworm families use one shared workspace-artifact Dockerfile. It
bind-mounts the source read-only, invokes Cargo once for the selected packages and
unique binaries, and exports those binaries through a scratch artifact stage. Runtime
Dockerfiles keep their runtime bases, users, files, configuration, native downloads,
entrypoints, and ports.

Frames moves to the slim trixie contract. Its native build dependencies become part of
that family. The UAV, DeepStream, vLLM, and SUMO families remain standalone in the first
delivery, although their cache identities become explicit and source-aware. Each
standalone family reads the complete workspace through the same read-only source-mount
boundary. The typed planner rejects handwritten builder-stage workspace `COPY` lists,
so adding a workspace member cannot leave one isolated image with an incomplete Cargo
graph.

#### Cache Contract

Cargo download caches use the fixed identities:

```text
veoveo-cargo-registry-v1
veoveo-cargo-git-v1
```

They use shared mounts under Cargo's cache locking. Target caches use locked mounts and
the following derived identity:

```text
veoveo-target-v1-<source-hash>-<family-hash>-linux-amd64-release
```

The source hash excludes the revision. The family hash covers the resolved builder
image, builder Dockerfile, Rust toolchain, target triple, platform, Cargo profile,
features, rustflags, compile-time environment, libc, SDK, and native dependency
contract.

Registry-backed cache export and import are deferred. Stable source and family
identities allow that backend to arrive later without changing package discovery or
mixing incompatible compiled artifacts.

#### Build Performance Evidence

The initial audit records the old graph, cache identities, lock policy, job limit, and
source-materialization behavior. It does not invent elapsed times after that graph has
been replaced. The optimized measurements use the named managed builder and retain
each unique local evidence directory. Builder recreation is explicit because it
removes the builder's cache.

Acceptance requires one Cargo action for each selected compatible family, identical
image digests for cold and warm builds of the same inputs, and no unrelated workspace
compilation after a gateway-only change. The measurement report records the available
elapsed time, Cargo actions, crates compiled, BuildKit cache hits, image digests, source
state, and Buildx metadata. It identifies a structural baseline when a comparable old
timing does not exist.

Shared CI runners do not enforce a percentage or elapsed-time threshold. Graph
invariants, cold-cache correctness, reproducible image identity, and single-crate
invalidation remain the durable gate.

The canonical simulation lineage has a separate cold-stage optimization. The first
measured overlay build spent about 155 seconds recursively initializing PX4 submodules,
including FlightGear and NuttX trees that are outside `px4_sitl_default`. The next
specialized-image change narrows checkout to the exact SITL dependency closure, proves
the resulting PX4 binary in the UAV hardware smoke, and records cold timing and source
digests. It may not substitute an unverified shallow or partial tree merely to improve
elapsed time.

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
2. Add the xtask foundation and route Rust enforcement through it.
3. Add existing Console, Python, documentation, and configuration checks.
4. Stabilize image source materialization and exclude unused Docker context roots.
5. Add Rust format, lint, metadata, and unsafe policy.
6. Add compiled local hooks.
7. Add dependency and vulnerability policy.
8. Add container, workflow, Kubernetes, and documentation policy.
9. Give production server binaries unique local names.
10. Add typed image release planning, enforce cache identities, and consolidate the
    trixie and bookworm builder families.
11. Create `tools/smoke-kit` and the typed smoke descriptor protocol.
12. Add Cargo-discovered xtask smoke dispatch.
13. Move server-owned smoke scenarios one component at a time.
14. Move gateway, platform, agent, template, showcase, example, and deployment
    scenarios to their owners.
15. Move `testing/mcp-conformance` to `mcp/conformance`, remove domain dependencies, and
    establish its standalone artifact boundary.
16. Promote component-oriented deployment and ownership-aware offline models into their
    contract crates.
17. Remove the central smoke binary and the top-level `testing/` directory.
18. Complete the Justfile hard cut.
19. Enable component discovery and dependency-direction enforcement.
20. Enable source-aware deployment, canonical gateway, contract, type-boundary, and
    module-responsibility enforcement.
21. Add repository governance and protected delivery settings.

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
- Deployment validation and publication reject a selected platform when its Bake groups
  omit any required platform or RRD transport image.
- Image publication materializes one exact committed source revision without resetting
  unchanged path metadata on every run.
- A selected image graph produces at most one Cargo build action for each compatible
  source-local builder family, target platform, and profile.
- Rust target caches are explicit, source-aware, platform-aware, and isolated across
  incompatible builder contracts.
- Cold-cache and warm-cache builds produce equivalent artifacts and release evidence.
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
