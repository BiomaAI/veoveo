# Image Builds

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Docker Buildx 0.35.0 | canonical Bake execution client |
| Docker BuildKit 0.31.2 | digest-pinned `docker-container` worker |
| Dockerfile frontend 1.25.0 | digest-pinned Dockerfile parser for touched Rust images |
| Docker Buildx Bake | checked-in image catalog and named-context graph |
| OCI images | `linux/amd64` release output with immutable Git revision tags |
| `veoveo.io/image-build-plan/v1` | repository-owned resolved build-plan evidence |
| `veoveo.io/image-build-run/v1` | repository-owned immutable execution record |
| Cargo metadata version 1 | package and production-binary discovery |

## Build-System Boundary

Veoveo keeps the build engines that already own their domains. Cargo compiles, tests,
lints, and documents Rust. Bake declares the OCI target graph and delegates execution
to BuildKit. `xtask` resolves repository policy, validates Cargo and Bake agreement,
selects compatible builder families, manages source materialization, and records
evidence. The Justfile remains a short human-facing dispatcher.

`xtask` is not a replacement compiler or a second image graph. It contains no fixed
package list and does not interpret Dockerfiles. Bazel, Buck2, Pants, Earthly, and a
custom remote-execution service are outside this design. They would add another graph
and cache authority without addressing a requirement that Cargo, Bake, and the typed
planner leave unmet.

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

cargo xtask image build --target mcp-gateway
cargo xtask image build --group showcase-sumo
```

Local builds load selected images into Docker. Raw `docker buildx bake` remains useful
for graph inspection, but it does not receive the typed artifact-family override and is
not a supported Rust build command.

The managed builder is named `veoveo`. Commands always pass that name explicitly and do
not change Docker's globally selected builder. `ensure` creates a missing builder and
fails when an existing one has a different driver, BuildKit image, daemon version, or
configuration. Cache deletion requires the explicit destructive command:

```bash
cargo xtask image builder recreate --confirm veoveo
```

Buildx 0.35.0 is exact. An exact host plugin is accepted. On Linux amd64 and arm64,
`ensure` can instead download the matching official binary into the main Git
worktree's ignored `target` directory and verifies its checked-in SHA-256 before use.
All linked worktrees, including the publication worktree, resolve that same binary and
Buildx state through Git's common directory. Docker credentials remain in the
operator's ordinary Docker configuration.

The builder runs the digest-pinned BuildKit 0.31.2 image. Its checked-in daemon
configuration preserves source-local and Cargo cache mounts for seven days, reserves
20 GB for cache, and applies explicit space limits. `status` verifies the driver,
daemon version, image digest, and configuration digest. A configuration change fails
closed until the operator invokes the explicit recreation command.

## Release Publication

A release resolves one full commit and holds an exclusive lock while it moves the
tool-owned publication worktree, plans every phase, and pushes the result.

```bash
cargo xtask release images \
  --profile showcase/sumo/deploy/deployment.json \
  --profile-revision "$(git rev-parse HEAD)"

cargo xtask release images \
  --group platform-full \
  --registry registry.example.com \
  --revision "$(git rev-parse HEAD)"
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

`plan.json` records the resolved source, dirty state, targets, packages, binaries,
families, cache identities, tags, and platform. `run.json` records the operation,
output mode, start time, duration, exit status, and metadata filename. The Buildx file
contains the exporter result and image digests reported by BuildKit. A failed execution
also retains its plan and terminal record. Release locks consume the exporter digest
directly and verify its image reference. They do not rediscover the digest through a
second registry request, which keeps the evidence path independent of private-registry
TLS transport.

Image execution sets `SOURCE_DATE_EPOCH=0`. Registry publication rewrites timestamps,
which removes wall-clock creation time from the release image. A local build uses the
Docker exporter and is a disposable developer artifact; its image identity is not
release evidence. Cold and warm registry builds of one source state must produce
identical image digests. Build cache remains an optimization and never supplies the
source identity or release tag.

Every registry release attaches BuildKit SBOM and maximum-mode provenance attestations.
The release lock records the resulting manifest-list digest, not an attestation-free
local image identity.

## Rust Builder Families

The trixie and bookworm families each execute one Cargo action for all selected
production binaries. Runtime Dockerfiles consume the resulting scratch artifact target
through the `veoveo-rust-artifacts` named context.

| Family | Contract |
|---|---|
| `rust-trixie-v1` | shared Rust 1.97.1 trixie builder |
| `rust-bookworm-v1` | shared Rust 1.97.1 bookworm builder |
| `rust-uav-bookworm-v1` | standalone compile-time WebRTC bundle |
| `rust-deepstream-v1` | standalone NVIDIA DeepStream SDK |
| `rust-vllm-v1` | standalone vLLM runtime ABI |
| `rust-sumo-bullseye-v1` | standalone SUMO-compatible bullseye ABI |

Cargo registry and Git caches use the fixed identities
`veoveo-cargo-registry-v1` and `veoveo-cargo-git-v1`. Target caches derive from the
source identity, complete builder-family contract, platform, and Cargo profile:

```text
veoveo-target-v1-<source-hash>-<family-hash>-linux-amd64-release
```

The source hash excludes the revision. A new commit can reuse compatible artifacts,
while separate clones and incompatible SDK or libc families cannot mix target output.
The target mount remains `sharing=locked` because a family now has one Cargo writer.
There is no cross-image lock convoy and no fixed `--jobs` throttle.

Every Rust family, including the isolated DeepStream, vLLM, UAV, and SUMO families,
reads the complete workspace through the canonical read-only BuildKit source mount.
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

## Platform Image Closure

The deployment contract resolves an exact Veoveo image set from typed platform
components, selected MCP servers, and composed gateway requirements. Profile validation
and profile publication compare that set with the resolved Bake targets before any
build or push.

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
simulation extension and Frames, Map, Media, or RRD must also select Bake groups whose
resolved targets satisfy the platform image closure.

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
generator, named-source deployment v2 coordination, platform-image closure
enforcement, and paired hardware simulation-overlay certification.

Multi-source composition passes an explicit source context and immutable artifact
identity into the same typed planner model. It coordinates independently published
graphs; it does not merge external packages into the core Cargo workspace or one
universal builder command.
