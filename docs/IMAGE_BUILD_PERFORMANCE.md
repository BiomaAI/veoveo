# Image Build Performance

## Standards And Protocols

| Boundary | Measurement profile |
|---|---|
| `veoveo.io/image-build-plan/v1` | resolved targets, Cargo units, builder family, and cache identity |
| `veoveo.io/image-build-run/v1` | operation, source state, elapsed time, result, and metadata reference |
| Docker Buildx 0.35.0 | Bake client and local Docker exporter |
| Docker BuildKit 0.31.2 | digest-pinned OCI worker with checked-in garbage-collection policy |
| OCI image manifest digest | artifact-identity comparison |
| `SOURCE_DATE_EPOCH=0` | reproducible output timestamp input |

## Result

The consolidated `platform-core` graph meets its structural and incremental-build
acceptance criteria. Six runtime images now consume one `rust-trixie-v1` artifact
build. A warm build reused the Cargo action completely and produced the same six image
digests as the cold build. A source-only gateway edit compiled only
`veoveo-mcp-gateway`; the other five runtime images kept their digests.

The measurements are local engineering evidence, not a shared-runner service-level
objective. The graph invariants and digest comparisons are the durable acceptance
conditions.

## Measurement Environment

The measurement host ran Linux 6.8 on x86-64 with an AMD Ryzen 9 5950X, 16 physical
cores, 32 hardware threads, and 62 GiB of memory. Docker client and server were 29.2.1.
Cargo and rustc were 1.97.1. The selected platform was `linux/amd64`.

Buildx used the repository-managed 0.35.0 binary. The named `veoveo` builder ran the
digest-pinned BuildKit 0.31.2 image. The shared family target cache was:

```text
veoveo-target-v1-42641a0fbe67-0f4aa446a10e-linux-amd64-release
```

The measured source content was the implementation tree based on
`e5633f163ac6bd6fe713c29d4f00cd66cd1c630b`. The first three records report a dirty
checkout because the build implementation had not yet been committed. The cold and
warm inputs were byte-identical. The gateway experiment added one source comment and
removed it after the run. A clean committed-source confirmation is recorded below once
the implementation checkpoint exists.

## Structural Baseline

The replaced graph was audited before modification. It did not have comparable retained
elapsed-time evidence, so this report does not reconstruct or estimate an old duration.

The old `platform-core` path launched six independent Cargo commands. Twelve trixie
images shared a locked target mount across the wider graph, and each command imposed
`--jobs 4`. A fresh publication worktree reset source modification times on every run.
Other Rust families used private or anonymous target caches; Frames and Perception
shared one anonymous cache despite incompatible builder environments.

The new planner proves the selected package set from Cargo metadata and Bake labels.
For `platform-core`, it resolves six packages, eight production binaries, and one Cargo
action. No central builder package list repeats the selected image membership.

## Measurements

| Case | Buildx-recorded time | Wall time | Cargo result | Acceptance |
|---|---:|---:|---|---|
| Cold managed builder | 374.735 s | 384.71 s | one family action; release compile finished in 4m44s | passed |
| Warm identical input | 34.106 s | 44.00 s | family Cargo layer was fully cached | passed |
| Gateway-only source edit | 62.121 s | 72.10 s | only `veoveo-mcp-gateway`; 25.36 s Cargo compile | passed |

The warm Buildx duration was 90.9% lower than the cold Buildx duration. Image export,
especially the Recording Hub runtime, now dominates the warm path. Export optimization
can proceed independently because the Cargo invalidation objective is satisfied.

The retained local evidence directories are:

```text
target/veoveo-xtask/evidence/e5633f163ac6bd6fe713c29d4f00cd66cd1c630b/
  build-group-platform-core-1785004591050019659-185380/
  build-group-platform-core-1785004992553373683-228534/
  build-group-platform-core-1785005046502456612-231171/
```

Each directory contains `plan.json`, `run.json`, and `buildx-metadata.json`.

## Artifact Identity

Cold and warm builds produced these identical runtime image digests:

| Target | Digest |
|---|---|
| `artifact-service` | `sha256:93322fda9ef1c534dd7895cfa10a0f10982dd00710c1063a2bb6b97b83e985e9` |
| `console-bff` | `sha256:6f592586b3b58eee3fa3181eea7ce90e5e3ad5f6b480b680297b0e65d7d958e5` |
| `mcp-gateway` | `sha256:1a72c5ea6f98b713ad169feb08ef7d77b572a7c14a312f5adfe0eddf2826f11b` |
| `recording-forwarder` | `sha256:927ae5efdcb3f314b618408607861f96b4f7abba56aeb5875a455a6a69a7e2a1` |
| `recording-hub` | `sha256:1003cabe89652ea1a0f1e70fdf9250b16315fd914da927b1b54ddbdb02f64755` |
| `recording-mcp` | `sha256:5e808ce467476ae522a79c001bba1d7fbace81fc5cda205e19db8c48001297a8` |

The gateway-only edit changed only `mcp-gateway`, to
`sha256:253fb0db04fb593e76f6e3a9b7aa2f481fee93ddac2389970fbcc0b5bc556aef`.
Every other digest remained identical.

## Cache Retention Finding

The first incremental trial exposed a second-order failure in BuildKit's default
garbage-collection policy. The policy placed a 488 MiB limit on source-local and
execution-cache mounts, while the Rust target mount occupied about 2.93 GiB. BuildKit
reclaimed that mount, and the next gateway-only build recompiled the family.

The checked-in daemon policy now keeps source-local, Git checkout, and execution cache
mounts for seven days, reserves 20 GB, and applies explicit maximum-used and
minimum-free-space limits. The builder was deliberately recreated after that change,
which removed the previous builder cache. The accepted runs above used the corrected
policy, and BuildKit retained the 2.93 GiB target cache across the warm and gateway
experiments.

## Acceptance Matrix

| Requirement | Evidence | Result |
|---|---|---|
| One Cargo action per compatible selected family | one `rust-trixie-v1` plan for six packages and eight binaries | pass |
| No fixed low parallelism throttle | shared builder invokes Cargo without `--jobs` | pass |
| Explicit compatible cache identity | source-, family-, platform-, and profile-derived target cache | pass |
| Cold and warm output equality | all six runtime image digests match | pass |
| Gateway edit avoids unrelated compilation | Cargo output names only `veoveo-mcp-gateway` | pass |
| Unchanged runtime images remain identical | five non-gateway digests match | pass |
| Execution evidence is immutable | unique create-only evidence directory for every run | pass |
| Output timestamps are reproducible | epoch input and timestamp-rewriting exporters | pass |
