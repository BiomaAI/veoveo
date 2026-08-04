# Image Build Performance

## Standards And Protocols

| Boundary | Measurement profile |
|---|---|
| `veoveo.io/image-build-plan/v2` | resolved targets, Cargo units, builder family, cache identity, and source epoch |
| `veoveo.io/image-build-run/v2` | operation, elapsed time, result, raw BuildKit events, and phase timings |
| Docker Buildx 0.35.0 | Bake client and local Docker exporter |
| Docker BuildKit 0.31.2 | digest-pinned OCI worker with checked-in garbage-collection policy |
| OCI image manifest digest | artifact-identity comparison |
| Git commit timestamp | reproducible output timestamp input that preserves older inherited layers |

## Result

The consolidated `platform-core` graph meets its structural and incremental-build
acceptance criteria. Six runtime images now consume one `rust-trixie-v1` artifact
build. A warm build reused the Cargo action completely and produced the same six image
digests as the cold build. A source-only gateway edit compiled only
`veoveo-mcp-gateway`; the other five runtime images kept their digests.

The measurements are local engineering evidence, not a shared-runner service-level
objective. The graph invariants and digest comparisons are the durable acceptance
conditions.

The current development path separates runtime staging from release qualification.
One warm `chart-mcp` stage completed in 8.846 seconds. A subsequent qualified build
completed in 9.376 seconds and retained the same runnable digest. In both runs,
timestamp-normalized export accounted for about 6.8 seconds; compilation was fully
cached. The v2 evidence preserves the raw BuildKit event stream behind those totals.

## Clean Reproducibility Experiment

Two independent BuildKit 0.31.2 daemons built the same clean source archive from
separate directories with deliberately different checkout modification times. Both
used the source commit timestamp `1785884846`, timestamp rewriting, and the same
`linux/amd64` target. The exported OCI archives had the identical SHA-256 digest:

```text
793edd1cf7b4f0712e91cfe15ae0d93bbc8faad222c4c04e0052bca4064dc1fd
```

Both exports selected runtime manifest
`sha256:0a560a48b44275da4ddd08ad4c2c59d4ef4c4c2f9dcce109ec16dc13f1bb4e0a`.
This proves reproducibility without rewriting inherited base layers. An earlier
experiment with an epoch older than checkout files correctly failed identity equality,
because BuildKit clamps only timestamps newer than the epoch. The planner now derives
the epoch from the selected commit and rejects zero.

## Measurement Environment

The measurement host ran Linux 6.8 on x86-64 with an AMD Ryzen 9 5950X, 16 physical
cores, 32 hardware threads, and 62 GiB of memory. Docker client and server were 29.2.1.
Cargo and rustc were 1.97.1. The selected platform was `linux/amd64`.

Buildx used the repository-managed 0.35.0 binary. The named `veoveo` builder ran the
digest-pinned BuildKit 0.31.2 image. The committed-source confirmation used this shared
family target cache:

```text
veoveo-target-v1-42641a0fbe67-77b1d106e3f4-linux-amd64-release
```

The authoritative measurements use clean source at
`7d7693aa0c97a1685729126020820c0243ccdd36`. An earlier implementation experiment used
a dirty tree based on `e5633f163ac6bd6fe713c29d4f00cd66cd1c630b`. That experiment remains
useful because it isolates a gateway-only source edit; it is not the committed-source
performance baseline.

## Structural Baseline

The replaced graph was audited before modification. It did not have comparable retained
elapsed-time evidence, so this report does not reconstruct or estimate an old duration.

The old `platform-core` path launched six independent Cargo commands. Twelve trixie
images shared a locked target mount across the wider graph, and each command imposed
`--jobs 4`. A fresh publication worktree reset source modification times on every run.
Other Rust families used private or anonymous target caches; Frames and Stream
shared one anonymous cache despite incompatible builder environments.

The new planner proves the selected package set from Cargo metadata and Bake labels.
For `platform-core`, it resolves six packages, eight production binaries, and one Cargo
action. No central builder package list repeats the selected image membership.

## Committed-Source Measurements

| Case | Buildx-recorded time | Cargo result | Acceptance |
|---|---:|---|---|
| Cold cache namespace | 495.720 s | one family action; release compile finished in 5m39s | passed |
| Settled warm identical input | 26.822 s | family Cargo layer was fully cached | passed |

The settled warm duration was 94.6% lower than the cold duration. The builds loaded the
runtime images locally and did not push them.

One intervening warm run took 476.104 seconds while the host was writing the cold cache,
reclaiming 191.2 GiB from unrelated worktrees, and running unrelated container
workloads. BuildKit still reported a fully cached Cargo layer, and its six image digests
matched the other committed-source runs. The record is retained as host-contention
evidence and excluded from the cold-to-warm comparison.

The committed-source evidence directories are:

```text
target/veoveo-xtask/evidence/7d7693aa0c97a1685729126020820c0243ccdd36/
  build-group-platform-core-1785010290585910300-123271/
  build-group-platform-core-1785010818386977821-157280/
  build-group-platform-core-1785011379025669628-191532/
```

Each directory contains `plan.json`, `run.json`, and `buildx-metadata.json`.

## Incremental-Edit Experiment

| Case | Buildx-recorded time | Wall time | Cargo result | Acceptance |
|---|---:|---:|---|---|
| Cold managed builder | 374.735 s | 384.71 s | one family action; release compile finished in 4m44s | passed |
| Warm identical input | 34.106 s | 44.00 s | family Cargo layer was fully cached | passed |
| Gateway-only source edit | 62.121 s | 72.10 s | only `veoveo-mcp-gateway`; 25.36 s Cargo compile | passed |

The warm Buildx duration was 90.9% lower than the cold Buildx duration. Image export,
especially the Recording Hub runtime, now dominates the warm path. Export optimization
can proceed independently because the Cargo invalidation objective is satisfied.

These preliminary runs used the earlier cache identity
`veoveo-target-v1-42641a0fbe67-0f4aa446a10e-linux-amd64-release`. Their evidence
directories are:

```text
target/veoveo-xtask/evidence/e5633f163ac6bd6fe713c29d4f00cd66cd1c630b/
  build-group-platform-core-1785004591050019659-185380/
  build-group-platform-core-1785004992553373683-228534/
  build-group-platform-core-1785005046502456612-231171/
```

## Artifact Identity

All three clean committed-source builds produced these identical runtime image digests:

| Target | Digest |
|---|---|
| `artifact-service` | `sha256:e5890343b229fa67c977a1c36ab9c57dac0ebecccb1d86539e6fb1f35ab64aa8` |
| `console-bff` | `sha256:35799ef140d0ba70c30bd1f9a0240962616e90a63c4af8d54da8c1d9e4303b99` |
| `mcp-gateway` | `sha256:a39f70333714e61fda8d0b18dc3f65e532ca754776925a15abafbe52b249c003` |
| `recording-forwarder` | `sha256:b7a1f90e963152afa93c832eff433abab554fdefc3ba4944e69b140ca87791be` |
| `recording-hub` | `sha256:c64b129d20efbab71d3e1e5a4054e8c777134197595c55aa545c511c11302f12` |
| `recording-mcp` | `sha256:79910a56e5358a4e146003e24f8462225884a8f361a9fceab2ed9f2164f358cd` |

In the preliminary incremental-edit experiment, the gateway-only edit changed only
`mcp-gateway`, to
`sha256:253fb0db04fb593e76f6e3a9b7aa2f481fee93ddac2389970fbcc0b5bc556aef`.
Every other digest remained identical.

## Cache Retention Finding

The first incremental trial exposed a second-order failure in BuildKit's default
garbage-collection policy. The policy placed a 488 MiB limit on source-local and
execution-cache mounts, while the Rust target mount occupied about 2.93 GiB. BuildKit
reclaimed that mount, and the next gateway-only build recompiled the family.

That first daemon-policy correction kept source-local, Git checkout, and execution
cache mounts for seven days, reserved 20 GB, and applied explicit maximum-used and
minimum-free-space limits. The builder was deliberately recreated after the change,
which removed the previous builder cache. The accepted runs above used that policy,
and BuildKit retained the 2.93 GiB target cache across the warm and gateway
experiments.

A later composed GPU acceptance cycle exposed a wider-lineage limit. The managed cache
held 170.13 GB while the general policy allowed only 160 GB, even though the host had
593 GB free. Consecutive Stream releases therefore discarded and downloaded the
DeepStream Triton builder lineage again; a two-line MCP App edit paid more than five
minutes to extract that unchanged base before compiling the crate in 17 seconds.

Both policies now retain 240 GB and begin collection above 320 GB while preserving
80 GB of host free space. BuildKit compares a filtered policy's threshold with total
worker usage, so the source and execution-cache rule must use the same envelope; a
lower filtered threshold purges Cargo mounts even when those mounts are individually
small. `cargo xtask image builder reconfigure --confirm veoveo` applies a policy
revision with Buildx `--keep-state`, which avoids deleting warm lineages merely to
correct their retention policy.

The corrected-policy Stream release confirmed both cache classes:

| Case | Wall time | Cargo result | DeepStream bases |
|---|---:|---|---|
| Refill after the erroneous purge | 707.47 s | 5m23s full dependency compile | 9.66 GB runtime lineage downloaded and extracted |
| Corrected-policy warm release | 24.63 s | freshness check finished in 1.55 s | builder and runtime lineages fully cached |

The warm release was 96.5% faster. It changed only the committed cache-policy
documentation between source revisions, which forced the source-bound Cargo action to
run while leaving the Stream dependency closure unchanged. No Rust crate compiled, no
base blob transferred, the apt and toolchain layers remained cached, and every registry
layer was reused. The release evidence is under:

```text
target/veoveo-xtask/evidence/
  a6ea5ea0b01660845b1959eafed828b72e650430/
    release-target-stream-mcp-1785264085414316426-4147999/
  7556ad2bd3f0dc90e8875f6f7ac683e8fd21ddc2/
    release-target-stream-mcp-1785264826837254158-21199/
```

## Host Smoke Build Retention

`cargo xtask smoke` originally launched nested Cargo with the package-scoped
environment inherited from `cargo xtask`. In particular, the child saw
`CARGO_MANIFEST_DIR=tools/xtask`, while an ordinary outer Cargo invocation saw that
variable as absent. Build scripts that declared it through `rerun-if-env-changed`
alternated fingerprints on every dispatch. Ring rebuilt first, then invalidated the
TLS, MCP, DuckDB, SurrealDB, Rerun, and smoke graph.

The command runner now removes the parent package's Cargo build environment before
launching nested Cargo while preserving user configuration such as `CARGO_HOME`,
`CARGO_TARGET_DIR`, `CARGO_NET_OFFLINE`, and `RUSTFLAGS`. Every real smoke scenario
also keeps `veoveo-smoke` and `veoveo-mcp-conformance` in one stable base build unit;
scenario-specific executable requirements remain additive.

| Case | Cargo result | Wall time |
|---|---|---:|
| Defective immediate identical `helm-config` dispatch | repeated dependency compile, 1m57s Cargo | 146.19 s |
| Corrected settled identical dispatch | freshness check only, 0.57 s | 4.88 s |

The corrected command was 96.7% faster and still validated both deployment fixtures.
The same sanitized nested-Cargo path is used by `cargo xtask enforce rust`, which
prevents its format, Clippy, test, and documentation phases from alternating package
fingerprints.

## Revision Metadata Cache Boundary

A full publication exposed an independent invalidation path in the simulation images.
Their `SOURCE_REVISION` argument was declared before the payload was assembled, although
the value was consumed only by the final OCI revision label. BuildKit includes an in-scope
argument in subsequent cache keys. A new Git revision therefore reinstalled the complete
Isaac Python dependency graph even when every runtime input and source file was unchanged.

The canonical simulation runtime, Isaac renderer, first-party UAV overlay, and external
overlay fixture now declare revision-only arguments after their final `RUN` or `COPY`
instruction. Compatibility inputs that determine payload bytes remain above the build.
The repository smoke rejects any future placement that lets a revision label invalidate
payload work. Publication evidence must show the expensive dependency layers cached when
only the source revision changes; an identical revision is not sufficient proof of this
boundary.

The first corrected build necessarily migrated the layer chain because the Dockerfile
instruction history changed:

| Case | Buildx-recorded time | Simulation payload |
|---|---:|---|
| Defective revision-only full publication `b4e4a1d` | 940.493 s | Python graph rebuilt; new payload uploaded |
| Corrected-layout migration `c3d4ff0` | 754.609 s | new cache boundary populated once |
| Cross-revision full publication `062fb7b` | 156.068 s | every build and SBOM action cached; payload upload took 0.3 s |
| Stable-payload full publication `1def437` | 156.198 s | base and Isaac renderer actions cached |

The cross-revision publication was 83.4% faster than the defective run. Registry
inspection found the same 31 Simulation Runtime payload layers and the same 34 Isaac
renderer payload layers on both sides of the revision change.

Internal overlays consume the cache-stable `payload` stage rather than the published
`runtime` stage. The latter adds source revision and compatibility labels without
changing the filesystem. This distinction prevents publication metadata from changing
the parent image config seen by the Isaac renderer, the first-party UAV overlay, or the
external overlay fixture. A subsequent UAV publication kept PX4, Pegasus, Cesium, its
Python and Rerun graph, the canonical simulation payload, and all runtime source layers
cached. Its 255.647-second Buildx duration was dominated by the 225.9-second reproducible
export of the inherited image.

The registry exporter still traverses large cached images when rewriting timestamps.
The full platform run spent about 148 seconds there despite uploading payload layers in
0.3 seconds. This cost is now isolated from dependency compilation and belongs to a
future exporter optimization.

## Acceptance Matrix

| Requirement | Evidence | Result |
|---|---|---|
| One Cargo action per compatible selected family | one `rust-trixie-v1` plan for six packages and eight binaries | pass |
| No fixed low parallelism throttle | shared builder invokes Cargo without `--jobs` | pass |
| Explicit compatible cache identity | source-, family-, platform-, and profile-derived target cache | pass |
| Cold and warm output equality | all six runtime image digests match across three clean committed-source runs | pass |
| Gateway edit avoids unrelated compilation | Cargo output names only `veoveo-mcp-gateway` | pass |
| Unchanged runtime images remain identical | five non-gateway digests match | pass |
| Execution evidence is immutable | unique create-only evidence directory for every run | pass |
| Release output timestamps are reproducible | epoch input and timestamp-rewriting registry exporter | pass |
| Revision-only metadata preserves simulation payload cache | trailing build arguments, identical payload layers, and cross-revision publications | pass |

## Extension Platform Closure

The `external-extension-platform` plan resolves nine runtime targets. Eight use one
`rust-trixie-v1` Cargo action; Map uses the distinct `rust-bookworm-v1` family. The
typed deployment resolver requires the same target set when gateway composition selects
Artifact, Frames, Map, Media, Recording, and RRD:

```text
artifact-mcp
artifact-service
frames-mcp
map-mcp
mcp-gateway
media-mcp
recording-forwarder
recording-hub
recording-mcp
```

This is a structural acceptance result. The services keep separate runtime images and
can change independently, while one release plan prevents an installation from
silently omitting a selected dependency.

## Simulation Overlay Cold Stage

The first complete `showcase-uav-sim-overlay-acceptance` build at implementation
checkpoint `675d118` completed in about eight minutes with the large Isaac lineage
already present locally. About 155 seconds were spent recursively initializing PX4
submodules, including FlightGear and NuttX sources that `px4_sitl_default` does not
execute. Importing the inherited 22 GB image lineage into the local Docker store cost
roughly another two minutes.

The GPU acceptance result is valid; these costs concern source preparation and local
export. The next PX4 image change must replace recursive initialization with the
hash-pinned SITL dependency closure and prove the resulting binary in the same hardware
smoke. Registry release publication avoids the local Docker import, while the canonical
base remains a shared digest rather than being flattened into each overlay.
