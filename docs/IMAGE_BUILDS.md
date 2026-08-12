# Image Builds

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Docker Buildx 0.35.0 | canonical Bake execution client |
| Docker BuildKit 0.31.2 | digest-pinned `docker-container` worker |
| Dockerfile frontend 1.25.0 | digest-pinned Dockerfile parser for touched Rust images |
| Docker Buildx Bake | checked-in image catalog and named-context graph |
| OCI images | `linux/amd64` release output with immutable Git revision tags |
| `veoveo.io/image-build-plan/v2` | repository-owned resolved build-plan evidence with the source commit timestamp |
| `veoveo.io/image-build-run/v2` | repository-owned immutable execution record with BuildKit phase timings |
| `veoveo.io/image-affected-plan/v1` | changed-path to image-consumer closure |
| `veoveo.io/image-stage-evidence/v2` | non-release runnable identity from a staged registry publication with explicit host-push and cluster-pull endpoints |
| `veoveo.io/development-image-lock/v1` | complete development-only image closure derived from a qualified lock |
| Cargo metadata version 1 | package and production-binary discovery |

## Build-System Boundary

Veoveo keeps the build engines that already own their domains. Cargo compiles, tests,
lints, and documents Rust. Bake declares the OCI target graph and delegates execution
to BuildKit. `xtask` resolves repository policy, validates Cargo and Bake agreement,
selects compatible builder families, manages source materialization, and records
evidence. `cargo xtask smoke` dispatches the typed Rust acceptance harness; one-step
native tool commands remain native.

`xtask` is not a replacement compiler or a second image graph. It contains no fixed
package list and does not interpret Dockerfiles. The planner discovers the complete
builder family from labels on every Bake target, while the command selection controls
which runtime images BuildKit exports. Bazel, Buck2, Pants, Earthly, and a custom
remote-execution service are outside this design. They would add another graph and cache
authority without addressing a requirement that Cargo, Bake, and the typed planner
leave unmet.

The existing managed-builder control lives in the internal
`tools/image-build/control` crate. `xtask` owns repository policy and the smoke harness
owns certification assertions, but both use this one implementation for the pinned
Buildx binary, BuildKit configuration, registry transport, and shared lease. The crate
does not plan targets or define another build graph.

The Python Datasheet example has two deliberate package boundaries. Its template
Dockerfile consumes an extension-owned lock and a released SDK from the configured
private index. Veoveo's own `datasheet-mcp` Bake target instead uses the checked-in
environment under `tools/image-build/datasheet/`. That environment locks the same
template and SDK as non-editable path distributions from the exact source revision.
First-party publication therefore remains reproducible without weakening the external
template or requiring an operator's package-index configuration.

## Command Surface

`docker-bake.hcl` is the image catalog. Rust image builds go through `xtask`, which
resolves the selected Bake graph, verifies its Cargo declarations, derives cache
identities, and supplies the generated family arguments.

```bash
cargo xtask doctor
cargo xtask image builder status
cargo xtask image builder ensure

cargo xtask image plan --target mcp-gateway
cargo xtask image plan --group platform-full --format json
cargo xtask image affected --since origin/main --format json

cargo xtask image build --target mcp-gateway
cargo xtask image build --group showcase-sumo

cargo xtask image stage \
  --target mcp-gateway \
  --push-registry registry.example.com \
  --pull-registry registry.example.com \
  --registry-transport tls \
  --revision "$(git rev-parse HEAD)" \
  --evidence-output output/stage/mcp-gateway.json
```

Local builds load selected images into Docker. Raw `docker buildx bake` remains useful
for graph inspection, but it does not receive the typed artifact-family override and is
not a supported Rust build command.

The managed builder is named `veoveo`. Commands always pass that name explicitly and do
not change Docker's globally selected builder. `ensure` creates a missing builder and
fails when an existing one has a different driver, BuildKit image, or daemon version.
Profile publication generates the exact registry stanza from the profile's host-push
endpoint and transport. The publisher probes `/v2/` before acquiring the builder.
Ordinary local builds retain an already configured registry-capable worker. A genuinely
different insecure endpoint recreates only the builder definition with `--keep-state`,
preserving the worker cache. The explicit maintenance command restores the checked-in
base configuration:

```bash
cargo xtask image builder reconfigure --confirm veoveo
```

Cache deletion requires the separate destructive command:

```bash
cargo xtask image builder recreate --confirm veoveo
```

Buildx 0.35.0 is exact. An exact host plugin is accepted. On Linux amd64 and arm64,
`ensure` can instead download the matching official binary into the main Git
worktree's ignored `target` directory and verifies its checked-in SHA-256 before use.
All linked worktrees, including the publication worktree, resolve that same binary and
Buildx state through Git's common directory. Docker credentials remain in the
operator's ordinary Docker configuration.

The builder runs the digest-pinned BuildKit 0.31.2 image. Its checked-in base daemon
configuration contains no registry hostname. An `insecure-http` profile adds only its
selected registry stanza in a content-addressed generated file; a `tls` profile uses
the base configuration and host trust roots. The configuration preserves source-local
and Cargo cache mounts for seven days. The
filtered and general policies both retain at least 240 GB, begin collection above
320 GB, and protect 80 GB of host free space. A filtered policy's space trigger applies
to total worker usage, not merely the records selected by its filter; using a lower
trigger there would evict Cargo cache mounts before the general image-lineage limit.
These bounds accommodate the simultaneous Isaac, DeepStream, and ordinary Rust image
lineages used by the acceptance suite. `status` verifies the driver, daemon version,
image digest, and reports the active configuration digest. Image operations hold one
shared builder lease across configuration and execution, preventing linked worktrees
from changing the daemon underneath another build.

Simulation certification holds the same lease. A deployment lock may authorize one
`insecure-http` registry at its exact host and port; otherwise TLS applies. Image
configuration, attestation inspection, and digest-addressed materialization all select
the managed builder explicitly. Docker Engine never resolves the remote certification
image and runs the local materialization with pulls disabled.
Materializations are tagged in Docker under `veoveo-simulation-certify-cache` by a
SHA-256 of the complete remote coordinate. Their source label must match before reuse.
This avoids repeatedly serializing and importing the large Isaac filesystem. Cache
removal is explicit:

```bash
cargo xtask image certification-cache-prune \
  --confirm veoveo-simulation-certify-cache
```

## Development Staging

Staging publishes the runnable image without SBOM or provenance attestations. It is the
fast path for a development cluster and can never become release evidence. The evidence
document records `releaseEligible: false`, the source revision, the staging index, and
the exact runnable platform-manifest digest.

`image affected` computes the consumer closure before staging. It includes committed
changes since the selected baseline and current working-tree changes. Cargo reverse
dependencies, Dockerfile `COPY` and `ADD` inputs, Bake named contexts, and target
consumers participate in the result. Explicit contract-consumer edges cover surfaces
that do not appear in either graph; an MCP App presentation or host-contract change,
for example, selects Console with the serving MCP image. The plan reports Helm, SDK,
generated-contract, and lock-input changes separately. A graph-wide input broadens the
result and records the reason.

A target build compiles only the Rust packages and binaries declared by its selected
image targets. A group or exact build still consolidates every selected member of a
compatible family into one Cargo invocation. The UAV runtime follows the same boundary:
its dependency payload contains pinned Isaac, Cesium, PX4, Pegasus, and Python wheels,
while the runnable target adds only repository-owned runtime source and identity.

A development image lock starts from one validated qualified deployment lock. Each
staged image replaces the matching source/target/repository tuple, while every
unchanged image retains its qualified runnable digest. The command emits the typed lock
and Helm-compatible values:

```bash
cargo xtask image development-lock \
  --base-lock deploy/deployment.lock.json \
  --stage-evidence output/stage/mcp-gateway.json \
  --output output/development/image-lock.json \
  --values-output output/development/images.values.json
```

The resulting values are an immutable GitOps input. They select
`global.veoveoRegistry` and the complete `global.imageDigests` map, so a controller
changes only workloads whose runnable digest changed. The development lock is
intentionally not accepted as a release `DeploymentLock`; release rollout continues to
require the attested qualified closure.

## Release Qualification

A direct qualification resolves one source commit. A profile release resolves the profile's
installation-repository commit and every independently selected source commit. It holds
each tool-owned publication worktree lock and the shared builder lease while planning
and pushing the result. The profile path may be in another Git worktree.

```bash
cargo xtask release images \
  --profile showcase/sumo/deploy/deployment.json \
  --profile-revision "$(git rev-parse HEAD)"

cargo xtask release images \
  --group platform-full \
  --push-registry registry.example.com \
  --pull-registry registry.example.com \
  --registry-transport tls \
  --revision "$(git rev-parse HEAD)"

cargo xtask release images \
  --target mcp-gateway \
  --push-registry registry.example.com \
  --pull-registry registry.example.com \
  --registry-transport tls \
  --revision "$(git rev-parse HEAD)" \
  --stage-evidence output/stage/mcp-gateway.json
```

The persistent source lives under:

```text
target/veoveo-xtask/publication/<source-id>/source
```

An unchanged file keeps its metadata when Git moves the worktree to another revision.
The release command rejects dirty, missing, unregistered, or otherwise inconsistent
publication state. It never cleans or silently recreates that state.

Every local build and release creates a unique evidence directory:

```text
target/veoveo-xtask/evidence/<revision>/
  <operation>-<selection-kind>-<selection>-<run-id>/
    plan.json
    run.json
    buildx-metadata.json
```

`plan.json` records the resolved source, dirty state, source commit timestamp, targets,
packages, binaries, families, cache identities, tags, and platform. `run.json` records
the operation, output mode, start time, duration, exit status, raw BuildKit trace, and
phase windows for compilation, SBOM, provenance, timestamp normalization, export, and
push. The Buildx file
contains the exporter result and attested publication-index digests reported by
BuildKit. A failed execution also retains its plan and terminal record. Publication
inspects the immutable `repository@publicationDigest` coordinate through ordinary
Docker registry authentication. A profile may explicitly admit `insecure-http` for a
private development registry at its configured address. Direct publication admits that
transport only for a loopback registry. Private production registries remain
TLS-verified.

Image evidence records the selected Git commit timestamp as `sourceDateEpoch`.
Execution sets `SOURCE_DATE_EPOCH` to the repository's separate `buildDateEpoch`, which
is a pinned cache ABI rather than revision metadata. BuildKit folds this predefined
argument into every stage key, so changing it for each commit would invalidate otherwise
identical dependency work. The build epoch advances only when a newly pinned parent image
contains newer metadata. BuildKit clamps newer filesystem metadata to that stable
boundary without rewriting older inherited base layers. Registry publication rewrites
output timestamps at export. A local build uses the
Docker exporter and is a disposable developer artifact; its image identity is not
release evidence. Cold and warm registry builds of one source state must produce the
same runnable platform-manifest digest. Build cache remains an optimization and never
supplies the source identity or release tag.

Every qualified registry release attaches BuildKit SBOM and maximum-mode provenance
attestations. Qualification supplied with stage evidence must produce the same runnable
digest; a mismatch fails before evidence is accepted. Staging never attaches these
release attestations.
Those attestations contain run identity and build timestamps, so their enclosing OCI
index is publication evidence rather than a reproducible artifact identity. Each locked
image records `digest` for the single runnable `linux/amd64` manifest consumed by Helm
and `publicationDigest` for the attested OCI index emitted by that release. Publication
rejects an index without both SPDX SBOM and SLSA provenance statements.

## Rust Builder Families

The trixie and bookworm families each execute one Cargo action for their complete
discovered production-binary catalog. This keeps Cargo's unified feature graph stable
when a developer moves between a direct target, `platform-core`, `platform-full`, and a
showcase. Runtime Dockerfiles consume the resulting scratch artifact target through the
`veoveo-rust-artifacts` named context, while Bake exports only the selected runtime
images.

| Family | Contract |
|---|---|
| `rust-trixie-v1` | shared Rust 1.97.1 trixie builder |
| `rust-bookworm-v1` | shared Rust 1.97.1 bookworm builder |
| `rust-deepstream-v1` | standalone NVIDIA DeepStream SDK |
| `rust-vllm-v1` | standalone vLLM runtime ABI |
| `rust-sumo-bullseye-v1` | standalone SUMO-compatible bullseye ABI |

Cargo registry and Git caches use builder-family identities:

```text
veoveo-cargo-<family>-registry-v1
veoveo-cargo-<family>-git-v1
```

Their mounts are locked within one family. Different libc and SDK families retain
parallelism, while two builds of the same family cannot race while Cargo unpacks a
crate or checks out a Git dependency. Target caches derive from the source identity,
an explicit builder-family compatibility epoch, platform, and Cargo profile:

```text
veoveo-target-v1-<source-hash>-<family-epoch>-linux-amd64-release
```

The source hash excludes the revision. The epoch changes only when the toolchain, ABI,
target, Cargo profile, or native SDK becomes incompatible. Dockerfile text is not a
cache key because Cargo already fingerprints source, features, flags, and dependencies.
Separate clones and incompatible SDK or libc families cannot mix target output. The
target mount remains `sharing=locked` because a family now has one Cargo writer. There
is no cross-image lock convoy and no fixed `--jobs` throttle.

The planner applies the resolved source revision to the complete Bake target-context
closure. A runtime built directly and the same runtime consumed as `target:<name>` by
an overlay therefore have identical build arguments and share BuildKit layers.

Heavy server-only dependency graphs do not become the default library graph. The
Recording MCP binary requires its package-qualified `redap` feature, and the trixie
image family consistently enables it because Recording MCP belongs to that family.
Stream, Reason, video, and smoke builds outside the trixie image family still avoid the
DataFusion-backed Redap server dependencies. The all-feature Rust gate compiles and
tests the production surface.

Every Rust family, including the isolated DeepStream, vLLM, and SUMO families, reads the
complete workspace through the canonical read-only BuildKit source mount. UAV MCP uses
the shared trixie family.
Standalone Dockerfiles do not copy a handwritten subset of workspace members. The
planner rejects a standalone builder that omits the source mount or introduces a
builder-stage `COPY`, which prevents a new workspace crate from breaking an otherwise
unrelated image late in a release.

## Adding An Image

A Rust image target declares these labels in `docker-bake.hcl`:

```text
io.veoveo.build.mode
io.veoveo.build.package
io.veoveo.build.binaries
io.veoveo.build.family
io.veoveo.build.auxiliary
```

The package and binary must exist in Cargo metadata. Shared-family runtime Dockerfiles
copy named artifacts and contain no Cargo build stage. A genuinely different libc,
toolchain, SDK, target triple, feature set, rustflag, or compile-time environment
requires a distinct typed family.

A target with none of these labels is a native non-Rust image target. Image planning
does not require that source to contain `Cargo.toml` or `Cargo.lock`; Docker Bake owns
its build graph. A target that declares any Rust label must declare the complete label
set and pass locked Cargo package and binary validation.

## Platform Image Closure

The deployment contract resolves an exact Veoveo image set from typed platform
components, selected MCP servers, and composed gateway requirements. Profile validation
and profile publication compare that set with the resolved Bake targets before any
build or push. The platform source declares no image group. Publication passes all
required targets to one Bake invocation, rejects missing and unnecessary platform
targets, and lets Bake share dependency and Rust-family work across the selection.

The `external-extension-platform` group contains the platform-side Artifact, Frames,
Map, Media, Recording, and RRD transport images:

```bash
cargo xtask image plan --group external-extension-platform
cargo xtask image build --group external-extension-platform
```

RRD is a data format and transport capability, not a fourth recording service image.
Its producer-side runtime is `recording-forwarder`; the installation side is
`recording-hub` plus `recording-mcp`. These remain separate images and release
identities.

The simulation ABI probe is intentionally a different group:

```bash
cargo xtask image build --group showcase-uav-sim-overlay-acceptance
```

It builds the canonical base, the first-party UAV overlay, and the anonymous overlay.
It does not claim platform integration. A deployment profile that selects the
simulation extension and Frames, Map, Media, Optimization, or RRD derives those
platform targets from the typed selection. Extension and workload image groups cannot
satisfy or enlarge platform closure.

## External Repositories

The repository boundary is intentional. A Veoveo-compatible extension keeps its
language toolchain, workspace, image graph, tests, and release command in its own
repository. It may adopt its own `xtask` when that repository has orchestration worth
compiling, but it does not invoke `veoveo-xtask` or copy Veoveo's builder families.

| Consumer activity | External repository owner | Veoveo integration boundary |
|---|---|---|
| Build | native Cargo, npm, uv, or other language build; repository-local OCI graph | published SDK and contract dependencies |
| Test | unit, integration, schema, and policy tests in the extension repository | pinned compatibility manifest and schema revision |
| Smoke | black-box lifecycle and domain scenarios owned beside the extension | standalone Veoveo conformance artifact and smoke descriptor |
| Package | extension OCI image and consumer-owned chart | extension manifest, immutable image digest, chart API, and provenance |
| Integrate | installation-owned binding, selected extension release, and digest-pinned values | validated gateway requirements and ordinary Helm or GitOps composition |

Published describes an immutable distribution state, not public visibility. SDKs,
images, charts, conformance artifacts, and provenance may resolve from an authenticated
customer-operated registry, a Veoveo-operated private registry, another
installation-configured package source, or a verified offline bundle. The
installation's client-facing origin is separate configuration and may be reachable only
through private DNS, an internal network, or a VPN.

The current delivery includes the source-local planner, private Python SDK, standalone
conformance distribution, gateway composer, Helm library, compatibility release
generator, named-source deployment v5 coordination, exact platform-image closure
enforcement, and paired hardware simulation-overlay certification.

Multi-source composition passes an explicit source context and immutable artifact
identity into the same typed planner model. It coordinates independently published
graphs; it does not merge external packages into the core Cargo workspace or one
universal builder command.
