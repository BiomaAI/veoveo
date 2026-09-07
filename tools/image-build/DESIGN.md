# Image Compilation Inputs

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Cargo metadata v1 | locked Linux amd64 graph, all features for conservative input discovery, normal and build edges |
| Docker Buildx Bake | typed target selection and generated context overrides |
| BuildKit source mounts | read-only compilation inputs, persistent locked Cargo caches |
| Docker BuildKit Syft scanner 1.12.0 | digest-pinned release generator; Syft 1.51.0 emits SPDX SBOM attestations |
| `veoveo.io/rust-source-context/v1` | repository-owned SHA-256 source identity, separate from an OCI artifact digest |
| `veoveo.io/normalized-parent/v1` | immutable dependency publication receipt, recipe identity and OCI runtime digest |
| `veoveo.io/compiler-cpu-comparison/v1` | compiler-only quota experiment, warmup, source variants, observed Cargo packages, binary digests and cgroup deltas |
| Git | exact committed publication source; local builds also admit non-ignored working-tree files |

## Ownership

`tools/xtask/src/commands/image/source_context.rs` derives source contexts for every Rust compiler family.
The checked-in Bake catalog owns compiler images and runtime assembly. Managed worker
identity, resource limits, and cache-preserving maintenance belong to `control/`.
The xtask image benchmark owns controlled source-edit comparisons. It operates on
temporary Cargo-derived contexts and holds a control-library quota lease through
restoration. Its local artifacts grant no compiler-family or image-release admission.
The [image-build runbook](../../docs/IMAGE_BUILDS.md) defines commands and receipts.

## Context Construction

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

The generated directory preserves file permissions and modification times. Preserving
times matters because Cargo uses them for freshness even though BuildKit excludes
mtime from source cache keys. The planner runs locked, offline Cargo metadata against
the generated workspace before handing it to BuildKit. Temporary contexts remain alive
through the solve and are removed with the prepared plan.

## Reuse And Evidence

`package.metadata.veoveo.image-asset-inputs` declares repository-relative runtime
asset paths within the declaring package. The planner removes those files from the
compiler context and binds a separate `veoveo-image-assets` context to the selected
runtime images. It records both identities in the family plan. Cargo manifests,
target entrypoints, Rust source and explicit compiler inputs cannot be declared as
assets. An accidental production `include_str!` of an excluded asset fails compilation.
Map's App generator sources and generated HTML use this boundary; Stream declares its
live App HTML. The runtime Dockerfiles consume the named context supplied by
`cargo xtask image`.

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

Standalone NVIDIA and SUMO compiler families receive the same Cargo-derived source
boundary. Their selected packages include native runner sources, Python runners, image
recipes and runtime assets. They still use their existing compiler and runtime images.
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
