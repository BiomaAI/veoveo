# Image Compilation Inputs

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Cargo metadata v1 | locked Linux amd64 graph, all features for conservative input discovery, normal and build edges |
| Rust 1.98.1 | Bookworm control compiler, Linux amd64 GNU ABI; separate Cargo selection from analytics consumers |
| Docker Buildx Bake | typed target selection and generated context overrides |
| BuildKit source mounts | disposable writable compiler inputs for freshness synchronization, read-only native inputs, persistent locked Cargo caches |
| Docker BuildKit Syft scanner 1.12.0 | digest-pinned release generator; Syft 1.51.0 emits SPDX SBOM attestations |
| `veoveo.io/rust-source-context/v1` | repository-owned SHA-256 source identity, separate from an OCI artifact digest |
| `veoveo.io/normalized-parent/v1` | immutable dependency publication receipt, recipe identity and OCI runtime digest |
| `veoveo.io/compiler-cpu-comparison/v1` | compiler-only quota experiment, warmup, source variants, observed Cargo packages, binary digests and cgroup deltas |
| sccache 0.17.0 | SHA-256-pinned Linux amd64 experiment tool; local disk cache and client-side compilation, incremental Rust disabled |
| `veoveo.io/compiler-cache-comparison/v1` | fresh Cargo target comparison, exact compiler and binary identity, typed sccache counters, no release eligibility |
| BuildKit local cache export | OCI cache index pinned to one manifest digest, `mode=max` export, and import into an initially empty worker |
| `veoveo.io/compiler-worker-comparison/v1` | same-host worker isolation, unchanged result reuse, matched source-edit inputs and artifacts, CPU/phase timing, and verified cleanup |
| Git | exact committed publication source; local builds also admit non-ignored working-tree files |

## Ownership

`tools/xtask/src/commands/image/source_context.rs` derives source contexts for every Rust compiler family.
The checked-in Bake catalog owns compiler images and runtime assembly. Managed worker
identity, resource limits, and cache-preserving maintenance belong to `control/`.
The xtask image benchmark owns controlled source-edit comparisons. It operates on
temporary Cargo-derived contexts and holds a control-library quota lease through
restoration. Its local artifacts grant no compiler-family or image-release admission.
The [image-build runbook](../../docs/IMAGE_BUILDS.md) defines commands and receipts.

The compiler-cache experiment consumes the existing family's compile stage as a named
BuildKit context. It introduces no alternative Rust toolchain or production wrapper.
Its recipe installs the pinned sccache release and builds each case into an empty
filesystem-backed `/target` directory, removed before that step commits. Only the
experiment's bounded compiler-cache mount carries compiled results between cases.
The Rust harness requires matching ordinary-artifact and experiment binary digests,
records cache state before and after each case, and rejects failed Cargo actions or
cache errors. Compiler-probe failures remain visible as diagnostics: native build
scripts can deliberately test unsupported compiler inputs inside a successful build.

The worker comparison uses the admitted shared compiler recipe without a compiler
wrapper. It exports the primary worker's complete result cache, creates an isolated
BuildKit worker with the declared CPU and memory limits, and proves that worker's cache
starts empty. The control library owns that temporary worker and its state volume.
Both workers remain under the common builder lease, and solves run sequentially.
Buildx 0.37.0 and BuildKit 0.33.0 were reconfirmed as the latest stable upstream releases
on September 8, 2026; the experiment reuses their existing exact pins.

The comparison imports one exact OCI cache manifest, verifies an unchanged build with
zero compilation and no Cargo cache mounts, then applies the same disposable Rust
source edits on both workers. Each matched pair must produce identical compiled files.
Logs record the actual compiled package sets. An unchanged BuildKit result can be
portable even when the next edit recompiles dependencies because execution cache mounts
were not transported. The measurements distinguish these outcomes. They do not claim
network performance across hosts, a cold primary baseline, or GPU runtime acceptance.

The command records cache export bytes, worker identity, CPU consumption, BuildKit phase
windows, and command-entry timing. Storage preflight retains the existing 20% reserve.
The temporary worker, volume, and exported cache are removed on completion; ordinary
builder caches and local binary evidence remain. Cleanup failure makes the comparison
fail. Normal scope exit also attempts worker cleanup after an error. An externally
terminated process can leave its uniquely named worker for explicit recovery.

## Context Construction

Image selection expands every Bake `target:` context before collecting Rust build
units. Each compiler family records the runtime targets that consume it, including
transitive dependencies, and exports their complete binary set. Asset contexts and
standalone compiler inputs follow that same dependency set. Only directly selected
image targets enter publication; an internal runtime dependency does not become an
extra published output. Selecting `computer-host`, for example, builds both the host
launcher and storage executable while reusing the independent provider build.

Cargo parses every workspace member before building a selected package. The context
therefore preserves the root manifest and lockfile, toolchain selection, `.cargo`
configuration, every local package manifest, and every real target entrypoint reported
by Cargo. It contains no handwritten workspace member list or synthetic Rust stubs.

The planner follows normal and build dependencies from the selected packages. It reads
an all-feature graph to conservatively include optional production inputs, including
the package-qualified Recording Redap feature. Dev-only edges do not expand the source
closure. Selected local packages contribute all versioned and non-ignored package
files, including build scripts, native code, and embedded assets. Unselected packages
contribute only the metadata files and target entrypoints Cargo needs for discovery.

The shared task runtime consumes the workspace's feature-neutral MCP contract dependency.
Analytics is enabled by its actual consumers. A native Cargo feature-graph regression
requires Stream and Reason to retain the task runtime without DuckDB or its native
build script. This production graph check is separate from conservative file discovery.

A package that reads compilation inputs outside its own directory declares them as
repository-relative files or directories:

```toml
[package.metadata.veoveo]
image-build-inputs = ["configs/shared-runtime.json"]
```

Inputs must exist in the admitted Git file set and remain available under the root
`.dockerignore`, which is preserved in the generated context. Absolute paths and parent traversal
are rejected. A symlink must be relative and resolve to a file already in the declared
closure. This prevents a local file outside the repository from silently influencing a
published binary. Deleted working-tree files are omitted unless Cargo requires them
as metadata inputs, in which case planning fails.

The generated directory preserves source permissions and modification times. Before
Cargo executes, `source-freshness.rs` compares the admitted bytes, modes and links with
an input mirror inside the locked target cache. It gives changed files a fresh timestamp
and retains the mirror's timestamp for unchanged files. Removal or changed symlink
inputs conservatively refresh the local source tree. The source mount is writable for
this timestamp adjustment; BuildKit discards its writes after the action. Original
worktree files remain untouched. Registry and Git dependencies retain their Cargo cache.

This bridge is necessary because Cargo's stable freshness checks use timestamps while
BuildKit's source identity uses content. A checkout with older timestamps can otherwise
reuse a binary compiled from newer, different bytes in the shared target cache. The
mirror follows executed compilation, including failed attempts, rather than planner
visits or BuildKit cache hits. The helper rejects a backwards freshness clock. Every
Rust compiler family uses the same helper, compiled by its existing pinned compiler.

The planner runs locked, offline Cargo metadata against the generated workspace before
handing it to BuildKit. Temporary contexts remain alive through the solve and are removed
with the prepared plan.

## Reuse And Evidence

`package.metadata.veoveo.image-asset-inputs` declares repository-relative runtime
asset paths within the declaring package. The planner removes those files from the
compiler context and binds a separate `veoveo-image-assets` context to the selected
runtime images. It records both identities in the family plan. Cargo manifests,
target entrypoints, Rust source and explicit compiler inputs cannot be declared as
assets. An accidental production `include_str!` of an excluded asset fails compilation.
Map's App generator sources and generated HTML use this boundary; Stream declares its
live App HTML, and Reason declares its Python runner. The runtime Dockerfiles consume the named context supplied by
`cargo xtask image`.

Runtime HTML copies use `--chmod=a=rX`: files remain read-only, while directories
created by `COPY` retain traversal permission. An octal `0444` also applies to new
parent directories and makes them inaccessible to the runtime user. Each consumer
checks HTML readability as its declared non-root user during image assembly.

The source digest covers ordered paths, modes, file/link kinds, and bytes with a schema
domain separator. The plan also records file count and the complete-source package
set. This digest identifies source inputs; it is not a substitute for the compiled
artifact’s digest. BuildKit additionally keys compilation on the Dockerfile, pinned
compiler image, selected packages and binaries, features, arguments, and cache ABI.

Shared scratch artifact targets keep their own cacheable compilation action. Runtime
assembly consumes them through the `veoveo-rust-artifacts` named context. A web-only
Console edit changes frontend assembly while preserving the BFF compilation inputs.
Changing an embedded asset in a shared Rust crate invalidates its consumers.

Exact source revision and dirty status remain in the plan and command receipt.
Different source revisions may share compilation output when their admitted compiler
inputs match. Staging and qualification continue to inspect immutable runnable OCI
digests; this context boundary grants no release eligibility by itself.

Stream's Rust executable uses `rust-bookworm-control-artifacts`, built by the common
artifact recipe. Its control family keeps Cargo feature unification separate from
Map's analytics family. The DeepStream development stage compiles only the C++ runner
and mounts only `servers/stream-mcp/gst-runner`; Rust edits preserve that native action.
Runtime assembly combines the two binaries with the digest-pinned DeepStream runtime.
Reason consumes the same control compiler and assembles its executable with the
digest-pinned vLLM 0.28.0 runtime. Runner dependency manifests have their own build
mounts, while Python source becomes an executable archive in a later layer. Rust and
Python source edits reuse the large GPU runtime and dependency installation.
Reason disables install-time Python bytecode and fixes archive timestamps, member
order, permissions, and the system-account date to the admitted source epoch.
These internal packaging bytes must survive eviction and rebuilding of the runtime
assembly cache. Qualification compares the resulting runnable manifest with its
staged digest after that eviction.

`rust-control.Dockerfile` installs stable Rust 1.98.1 over the latest published
official Bookworm Rust image, 1.98.0, then removes the bootstrap toolchain before any
compilation. The [official image catalog](https://github.com/docker-library/official-images/blob/master/library/rust)
has not published a 1.98.1 tag as of September 8, 2026, although the
[Rust release](https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/) and rustup distribution
are available. Bookworm preserves a glibc baseline below DeepStream's Ubuntu 24.04
runtime. The compiler image and target-cache epoch change together.

The standalone SUMO compiler family receives the same Cargo-derived source boundary.
Its selected package includes its native inputs, image recipe and runtime assets.
Moving a binary between ABI families requires independent compatibility and runtime
evidence before changing that boundary.

The Recording reader extraction removes Hub, forwarder and Recording MCP production
sources from Stream and Reason contexts. All workspace manifests and real Cargo target
entrypoints remain present for discovery. A service implementation edit outside those
metadata entrypoints cannot invalidate their compiler action.

## Normalized Dependency Publication

A Bake consumer declares `io.veoveo.build.normalized-parent` with the target name of
its dependency image. Exactly one named context must refer to that target. Every
local target in the parent's dependency graph declares `io.veoveo.build.input-paths`
as comma-separated context-relative files or directories. The planner materializes
only these inputs, its declared Dockerfile, and any applicable Docker ignore file.
An undeclared application file is unavailable to the dependency build.

The recipe identity hashes the complete resolved Bake target graph, ordered source
identities, fixed build epoch, exporter options, and the pinned BuildKit image. Tags,
cache import/export hints, and output/attestation settings are excluded because the
normalization publisher supplies its own output policy. Other Bake fields, including
unknown upstream fields, remain part of the hash. Dependency Dockerfiles contain
upstream version and digest pins. The recipe identifies admitted build inputs; its
published OCI digest identifies the resulting bytes.

Staging and qualification first resolve the registry's recipe tag. An explicit
missing manifest permits one dependency build with timestamp rewriting and no release
attestations. Authentication, transport, and malformed-manifest errors stop the
command. A local admission receipt pins the resolved digest; later runs validate that
immutable manifest and fail if it disappeared. Another build host can resolve the
same registry recipe tag without rebuilding. These internal parent images carry no
release eligibility. The final image keeps the normal qualification requirements.

The runtime solve replaces the dependency target context with a digest-pinned
`docker-image://` input. It therefore consumes the already normalized filesystem.
The UAV overlay uses Dockerfile frontend 1.27.0 and independent `COPY --link` layers.
Its numeric ownership and regular destination directories permit assembly without
reading or unpacking the parent filesystem. The entrypoint receives its mode in a
scratch stage, since combining `--chmod` with `--link` disables BuildKit’s merge
optimization. The runtime copies that prepared file without a chmod flag. Application
source revisions keep their exact OCI revision labels and separate image receipts. Local Docker-load builds retain the direct dependency graph because that
exporter does not rewrite inherited timestamps or require a publication registry.

Plans retain parent recipes and input counts. Each publication run writes
`parents/<target>/receipt.json`, plus BuildKit metadata, raw events, and phase timings
when it publishes a missing parent. Command evidence includes parent resolution and
publication time. Local admission receipts live below
`target/veoveo-xtask/normalized/<registry-hash>/<recipe-hash>/`.


## Shared-Host Cache Capacity

The managed worker retains an 80 GiB cache floor and targets at least 20% free space
on its filesystem. Its configured collection trigger is 22%. BuildKit 0.33.0 divides by
binary GiB and multiplies by decimal GB when resolving percentages, so an unadjusted
20% setting undershoots an exact 20% reserve. The 22% trigger covers that reserve with
additional headroom; a future upstream rounding correction only increases the headroom. Collection also begins above 320 GiB of worker cache. The aged-input and
broader pressure policies share these thresholds. The first policy gives old source
and compiler-cache mounts a seven-day retention window. The broader policy can reclaim
other unused state when the host needs space, subject to the cache floor.

The effective threshold must cover release preflight's default 20% reserve. Each build must still
budget its peak additional storage. Garbage collection cannot guarantee that reserve
when other host owners consume more space than the reclaimable cache can cover.
Registry artifacts and Kubernetes persistent data belong to their own storage owners.
