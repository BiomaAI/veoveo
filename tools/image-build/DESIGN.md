# Image Compilation Inputs

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Cargo metadata v1 | locked Linux amd64 graph, all features for conservative input discovery, normal and build edges |
| Docker Buildx Bake | typed target selection and generated context overrides |
| BuildKit source mounts | read-only compilation inputs, persistent locked Cargo caches |
| `veoveo.io/rust-source-context/v1` | repository-owned SHA-256 source identity, separate from an OCI artifact digest |
| Git | exact committed publication source; local builds also admit non-ignored working-tree files |

## Ownership

`tools/xtask/src/commands/image/source_context.rs` derives shared Rust source contexts.
The checked-in Bake catalog owns compiler images and runtime assembly. Managed worker
identity, resource limits, and cache-preserving maintenance belong to `control/`.
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

Standalone NVIDIA and SUMO compiler families retain their complete source mounts.
Moving a binary between ABI families requires independent compatibility and runtime
evidence before changing that boundary.
