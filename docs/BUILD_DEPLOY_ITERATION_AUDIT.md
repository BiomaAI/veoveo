# Build And Deployment Iteration Audit

Status: completed and accepted on September 8, 2026, at `https://veoveo.bioma.ai`.
Core changes are committed and the shared charts are active with the Bioma reference configuration. Local compiler
ABI and throughput experiments have recorded results. Live metadata-only publication
preserves every running Pod. Disposable component-selected execution now passes the independent native Git/OCI
fixture. Cluster coordination now serializes cooperating disposable installers;
Stream and Reason use the shared Rust 1.98.1 control compiler, pass hardware replay,
and are active with the Bioma configuration. Reason's Python input path stays on
NVDEC and CUDA. The local
build-engine cache trial has recorded results. The user deferred independent build
storage and host comparisons on September 8; they are future performance tests and
do not block completion. Public endpoint and headed Console acceptance pass against
the deployed Veoveo installation.
The findings below retain the pre-change evidence. The delivery record identifies
implemented changes and their verification.

## Delivery Record

| Concern | Implementation | Verification |
|---|---|---|
| Rollout triggers | Removed platform, UAV, and SUMO chart-version Pod annotations; Stream hashes runtime files; UAV bootstrap requires its content digest; Flux values ConfigMaps carry the watch label | Rendered chart tests cover metadata-only publication, scoped catalog changes, image-only changes, and generated Flux watch labels |
| Builder resources | Declared 12 CPUs and 36 GiB without swap, rejected resource drift, exposed cgroup CPU snapshots, and upgraded the managed Buildx/BuildKit pins; added a controlled source-edit benchmark | Identical BFF/gateway binaries compile in 84.9 s at four CPUs versus 37.0 s at twelve; a live failure-path test verifies quota restoration; strict Clippy and 99 unit/worker tests pass |
| Host cache retention | Added a reviewable Cargo cache plan for old executable copies and redundant incremental variants; hourly retention can reclaim superseded outputs within one day | Candidate and Cargo-lock interoperability tests pass; the broader pass recovered 18.4 GiB of Cargo outputs and about 61 GiB of image cache; the 20 GiB experiment now passes the reserve gate |
| Automatic cache capacity | Reduced the protected BuildKit cache floor to 80 GiB and set a 22% collection trigger that covers the 20% host reserve after the pinned daemon’s percentage rounding | Native worker policy reports 404,000,000,000 bytes of free-space protection; all 14 Cargo mounts survive reconfiguration; UAV staging takes 5.6 s including reconfiguration with the same runnable digest; the 20 GiB growth preflight passes |
| Component-selected installation | Explicit owner selection with dependency expansion before resolution; every installation unit uses the checked plan; receipts report actual applied/reused targets | Independent native Git/OCI platform and extension updates replace only the selected Pod; the unselected repository is unavailable, its Deployment, Pods, and Helm storage remain identical, and an ownership overlap issues no API writes |
| Concurrent profile execution | Cluster Lease serializes cooperating disposable installers across repositories; ownership planning repeats under the lock, and conditional release checks its exact identity | Native contention admits one winner, stale deletion fails, incomplete execution retains the lock, and the selected-update fixture accounts for control writes while preserving unselected application state |
| Local Console loop | Corrected the BFF proxy to 8786, added gateway OAuth/discovery routes, fixed the Vite port, and documented one local authentication origin with a private MCP transport | Production TypeScript/Vite build passes; authenticated headed hardware-WebGL refresh preserves the document and signed-in session |
| Packaged MCP Apps | Map and Stream load bounded immutable HTML snapshots from image assets; declared presentation inputs have their own assembly context outside Rust compiler mounts | 99 helper/planner tests, both server target checks and strict Clippy pass; a real presentation-only revision stages both images in 12.1 s with zero Cargo execution and unchanged binary layers |
| Normalized GPU dependencies | Declared exact dependency input contexts and recipe-keyed OCI parent publication, reused by digest in staging and qualification | Two source-only revisions staged in 14.6 s and 15.6 s; optimized tooling stages in 2.8 s, or 3.1 s after old dependency cache eviction; warm qualification takes 10.3 s and preserves the runnable digest |
| Reusable Rust inputs | Shared compiler targets receive Cargo-derived contexts with complete workspace manifests, production dependency sources, and embedded assets | Real frontend-only revision staged in 26.0 s with the Rust action cached and identical binary layer; qualification preserved its runnable digest |
| Cargo source freshness | Added a content-aware input mirror under each locked target cache, with disposable source timestamp synchronization for every Rust family | An old-timestamp edit/revert regression passes; the corrected ordinary BFF/gateway artifacts equal clean-target outputs; 40 focused tests, strict Clippy, formatting and standalone-family planning pass |
| Compiler-cache experiment | Added an existing-family sccache 0.17.0 comparison with empty Cargo targets, bounded cache storage, CPU timing and full compiled-artifact identity | All 955 cacheable operations hit on recovery, but the solve takes 333.7 s versus 328.4 s without the wrapper; the wrapper remains outside normal builds; all experiment cache mounts were removed |
| Second-worker reuse | Added an isolated worker comparison using the existing compiler recipe and an exact exported OCI cache manifest | Unchanged import takes 4.6 s with zero compilation; matched source edits take 39.5 s on the existing worker and 369.5 s on the fresh worker; binaries and native library match within each pair; temporary worker, volume, and cache export are removed |
| Bazel integration and cache trial | Builds Stream Rust, the native CMake runner, and OCI assembly in the existing SDK environment | A restored workspace reuses all 780 Rust compilation actions; both edited builds execute the same six actions and produce identical artifacts; inherited-image materialization remains costly; Cargo/BuildKit stays in production |
| Shared task runtime feature closure | Uses the workspace MCP contract dependency without implicitly enabling analytics | Stream's Linux normal/build graph falls from 629 to 608 package/version pairs with no added packages; Stream and Reason both exclude DuckDB; their Rust targets and the task runtime pass tests and strict Clippy |
| Shared recording APIs | Moved encoded RRD operations, governed analysis plans, visibility rules, and bounded cache mechanics into libraries; Stream and Reason no longer import Hub or Recording MCP | 30 reader/video/Recording MCP tests pass; all consuming targets compile with Redap enabled; all Rust families now receive Cargo-derived contexts, with real graph tests excluding the service implementations |
| Common GPU control compiler experiment | Built Stream and Reason together through the existing Bookworm artifact recipe, with explicit package, binary and cache overrides | Both candidates pass RTX 4090 recording tasks with published artifacts; both separated compiler images are now active and pass installed hardware acceptance |
| Stream compiler separation | Shared Bookworm control artifact recipe with Rust 1.98.1; DeepStream compiles only the C++ runner from its own directory | Warm Rust edits take 16.7 s and 15.9 s; a committed edit stages in 26.2 s with the native action cached. The qualified image is active and passes RTX 4090 replay; all 28 other running Pods preserve identity and restart count |
| Reason compiler separation | Shared Bookworm control compiler; hash-locked Python dependencies and runner source use separate image mounts | Rust edits stage in 32.6 s; Python edits stage in 11.1 s without Cargo. Only the corresponding executable layer changes. The qualified vLLM 0.28.0 image is active and passes RTX 4090 reasoning; the other 28 running Pods remain unchanged |
| Compiler runtime probes | Added a Rust candidate probe with explicit App or runner input, private listener ownership, exact executable digest, and verified cleanup; video smoke builds omit the unused conformance CLI | Native Stream and Reason hardware replay preserve the installed Deployment, Pod identity, and restart count; the v3 receipt binds each payload and verifies temporary cache removal |
| Focused flight acceptance | Routes composed-flight scenarios through `veoveo-flight-smoke`, using shared browser source and Stream-owned wire types | Resolved client/helper graphs exclude service implementations, SurrealDB, DuckDB and Rerun; original flight assertions remain in the focused harness |
| Focused configuration checks | Routed `helm-config` through the existing deployment harness and moved its assertions beside that harness | Dispatcher coverage rejects the broad smoke/conformance build unit; the same assertions remain part of the full gateway suite |
| Release inputs | Split Bioma image locks by rendered release closure; selected Helm publication accepts repeated `--chart` with isolated receipts | Render tests compare generated locks with consumed images; packaging tests require only the selected chart artifact |
| Complete command timing | Added command-entry records with preparation, lock waits, solve references, manifest inspection, terminal failures, per-solve CPU deltas, and filesystem extraction windows | Failure-path tests retain errors before BuildKit starts; cached compiler actions report no execution; extraction remains visible when a scanner materializes cached layers |
| Exact image selection | Repeated `--target` flags select one sorted Bake solve for planning, local builds, staging, and qualification; affected planning excludes dev-only Cargo edges | CLI selection tests and dependency-closure fixtures cover staging/qualification parity and retained build dependencies |
| Passive convergence observation | GitOps verification defaults to observation; explicit requested reconciliation remains a declared mode in v3 evidence with the verification start time | Six process-level Rust fixtures pass; an already-active revision is observed in 1.232 s; the live metadata-only publication reaches verified readiness in 40.5 s from push start without reconciliation requests |
| Immutable GitOps inputs | OCI source names bind complete chart manifest digests; generated immutable Helm values ConfigMaps have content suffixes; one HelmRelease update selects both references | Rust configuration checks exercise updates in both release directions; live activation takes 116.1 s from push start with identical Pod identities, restart counts, Deployment revisions, and Helm histories |
| Live chart publication | Activated the shared platform and UAV chart changes, then published a platform chart with only its version changed | The version-only update takes 40.5 s from push start; the platform Helm revision advances once while the UAV release and all 29 running Pods remain unchanged |
| Recording recovery | Resolve accepted batch identity before checking whether a stream permits new appends | A regression reproduces the live finished-stream error; 57 store tests pass; the corrected Hub reconciles its journal and becomes Ready |
| Map projection recovery | Read indexed canonical Map changesets through a transactional committed head; backfill populated installations while Map writers are stopped | All 164 Map/store tests pass; both migrations are verified live; the corrected image becomes Ready five seconds after container start with its existing checkpoint |
| Runtime App permissions | Preserve traversal on asset directories and check readability as the runtime user during image assembly | Map and Stream image builds pass; the corrected Map image is active; the earlier 12.1 s presentation-only assembly measurement did not detect this runtime defect |
| Obsolete Flux health checks | Enable cancellation on both controllers; update Flux to 2.9.5 with exact controller image digests and preserve bounded rollback remediation | The isolated Rust regression holds the updated Pod unready for five seconds, then the corrected OCI revision converges in 21.6 s against five-minute controller timeouts; all 29 application Pods retain identity and restart count |

## Live Activation Evidence

The September 8, 2026 UTC activation deploys Veoveo with the Bioma reference
configuration. Flux remains the application owner. All three observers started before
their Git pushes and used passive reconciliation mode.

| Git revision | Change | Push start to verified readiness | Observed result |
|---|---|---:|---|
| `ad36d941545e24806390717f5cdcbb753e6e892d` | Shared chart activation and corrected Recording Hub image | 1,229.0 s | Both Helm releases converge and all 25 Deployments become Ready |
| `146fdcb21c8c8a8440fe594059cc57506619f9e1` | Immutable chart source and values references | 116.1 s | All 29 running Pod identities and restart counts, all 25 Deployment generations and revisions, and both Helm histories remain unchanged |
| `837b7f857a6a501be8772a506976733a1e41b44c` | Platform chart version changes with identical runtime inputs | 40.5 s | Platform Helm revision advances from 114 to 115; UAV stays at 99; all Pod identities, restart counts, and Deployment generations and revisions remain unchanged |

The first activation includes recovery from existing operational failures. Recording
Hub had already materialized its final batch but rejected the leftover journal because
its stream was finished. The corrected image resolves that idempotent replay. Reason's
existing database connection was stuck even though a fresh connection could query the
database; restarting its one unhealthy Pod restored readiness. This intervention is
part of the first activation timing.

That activation exposed a chart/value ordering race. The generated values ConfigMaps
changed before Flux fetched the new chart artifacts. The platform performed an extra
upgrade with its old chart, while the old UAV chart rejected the new `contentSha256`
field. Immutable chart sources and values references now move together on each
HelmRelease. The Rust configuration checks enforce complete source digest names,
rewritten values references, immutable ConfigMaps, and unchanged unselected inputs.

Map also delayed the first rollout by replaying the shared outbox before opening its
HTTP listener. The outbox contained about 16.8 million events. Its checkpoint advanced
through the backlog, but startup exceeded the five-minute probe budget and the
Deployment progress deadline, which triggered container restarts and Helm rollback.
The persisted checkpoint eventually caught up and the release recovered. Domain-scoped
projection recovery and bounded database health checks remain deployment work; the
metadata-only result does not measure those service startup paths.

The Map recovery implementation now pages the canonical changeset log through a
fixed committed Map head. A live read found two Map changesets while the shared
outbox had 16,805,799 events. Migration `0047` adds the changeset sequence index;
SurrealDB 3.2.4 `EXPLAIN` reports a forward `IndexScan` with the requested range and
limit. The integration test exercises sparse sequence gaps, a commit beyond the
captured bound, reopening a partially projected DuckDB database, idempotent recovery,
and failure without checkpoint advancement when a feature revision is missing.
All 164 Map and store tests pass with real SurrealDB and the pinned Spatial extension,
including both million-feature R-tree gates.
SurrealDB allocates sequence values separately from the enclosing data transaction.
A native 3.2.4 probe committed sequence 3 while sequence 2 remained in an open
transaction, then committed sequence 2. The shared outbox maximum therefore cannot
serve as Map's recovery boundary. Migration `0047` also adds a transactional Map
head: concurrent changesets contend on that record and a lower late sequence is
rejected atomically. Activation requires stopping Map writers while the bootstrap
seeds that head, then resuming the new image with its persisted projection.

The activation exposed two additional defects. Migration `0047` seeded zero on the
populated database despite two canonical changesets. Forward migration `0048` uses
an explicit unindexed scan for this one-time backfill and preserves any higher head.
A regression recreates the pre-migration schema around a real authored changeset,
applies both migrations, and verifies the populated head and repeat application.
The same 164 tests pass. Live bootstrap applied `0048` at 03:19:01 UTC and verified
head 3,963,196 while preserving the existing projection checkpoint 16,803,961.

The new image then failed before serving because `COPY --chmod=0444` also made its
new asset directories non-traversable. Map and Stream now use `a=rX` and test asset
readability as the declared runtime user during assembly. Both image builds pass;
the corrected Map publication takes 27.6 s with all compilation cached. This image
check does not qualify Stream's GPU runtime.

Recovery also exposed a controller queue delay. The active Helm action waits up to
20 minutes for the failed image, while the root Kustomization waits up to 30 minutes
for release health. A later Git commit could not interrupt those checks with the
settings used during Map activation. Both installed Flux controllers supported the opt-in
`CancelHealthCheckOnNewRevision` gate. The installed defaults left it disabled.
The [Helm controller's pinned implementation](https://github.com/fluxcd/helm-controller/blob/v1.6.3/internal/features/features.go)
and [Kustomize controller's pinned implementation](https://github.com/fluxcd/kustomize-controller/blob/v1.9.4/internal/features/features.go)
define the cancellation boundary.

Revision `52c5afb3fff3d60937677d6c040b965bd942ac19` activates the corrected Map
image, runnable manifest
`sha256:e87b697b112a88b775b103289cbfed019f7110309691eddf2a0a2b660954eb4f`.
Flux finished the obsolete revision after 20 minutes 51 seconds, then reconciled the
corrected commit in 15.7 s. Map's Pod started at 03:41:37 UTC, its container started
at 03:41:39, and it became Ready at 03:41:44 with zero restarts. The maintenance
interruption, including the migration and failed package recovery, lasted about
32 minutes. The requested observer retained the queue delay in its apply phase.

All 25 Deployments are Ready. Only Map and the two Gateway Pods changed identity;
the other 26 running Pods retained their identities and restart counts. The UAV
Helm history stayed at 99. The canonical Map head and old projection checkpoint
remain 3,963,196 and 16,803,961 respectively. This five-second startup uses an
already caught-up persisted projection; it is not a like-for-like benchmark against
the earlier 16.8-million-event backlog. The real database regression proves bounded
recovery in the presence of unrelated events. Evidence is retained under
`output/development/map-recovery-activation-20260908/`.

The Flux reference now enables that gate in both controllers and pins the stable
2.9.5 distribution by release and controller OCI index digest. The native platform
apply uses its existing `veoveo-flux-platform` field manager. Application release
ownership stays with Flux. The controller update and isolated regression leave all
29 application Pod identities and restart counts unchanged; platform and UAV Helm
histories stay at 121 and 99.

`cargo xtask smoke gitops-cancel-verify` creates a disposable namespace and publishes
one immutable Helm chart with three distinct OCI configuration artifacts. It first
establishes a healthy release. The broken revision must reach both controllers'
health checks with an updated, unready Pod for at least five seconds before the fix
is submitted. The corrected source, root, Helm release, and Deployment converge in
21.6 s despite five-minute controller timeouts. The namespace is then removed.
This is cancellation evidence for a controlled workload, not a repeat of Map's
startup workload or a measurement of Git server delivery. The checked-in Rust harness
owns lifecycle, assertions, timing, and cleanup. Evidence is retained in
`output/development/flux-cancellation-20260908/stable-cancellation.json`.

The corrected Hub publication took 118.8 s end to end, including 34.9 s of Cargo
compilation. Its image is pinned to runnable manifest
`sha256:5316ed89fc9806f7e2951fd40134889a764976130b3785a7abf53a61068c33a2`.
The version-only chart publication compared all 35 archive files; only `Chart.yaml`
changed. No image build was needed for either subsequent GitOps update.

An additional scoped Cargo cleanup removed a plan containing 10.72 GiB of superseded
executables and incremental variants. Dependency libraries and current executable
links were retained. The subsequent resource preflight reported 391 GiB free and
accepted a 10 GiB build-growth allowance above the retained filesystem reserve.

Local evidence is retained under `output/development/chart-activation-20260908/`,
`output/development/immutable-gitops-inputs-20260908/`, and
`output/development/chart-metadata-20260908/`. Each directory contains the push
timestamps and typed convergence report. The latter two also retain before/after Pod,
Deployment, and Helm history observations.

## Standards And Protocols

| Boundary | Audited profile |
|---|---|
| Cargo and Rust | repository toolchain 1.97.1, Cargo metadata v1, optimized image builds |
| Docker Buildx and BuildKit | managed Buildx 0.35.0 and BuildKit 0.31.2, Bake, persistent execution caches, registry exporter |
| OCI images | digest-pinned `linux/amd64` runtime manifests, separate staging and qualification evidence |
| Helm and Kubernetes | Helm 4.2.3 rendering, Deployment Pod templates, atomic Helm releases |
| Flux | installation-owned Git source, OCIRepository, HelmRelease, values ConfigMap watches |
| Repository evidence | image-build-run v2 and gitops-convergence-evidence v2 |

## Finding

Iteration still pays for work outside the changed component. Whole-workspace inputs
invalidate container compilation actions. Separate GPU builder families repeat common
Rust compilation. GPU overlay publication revisits large inherited layers, and chart
metadata restarts workloads whose runtime inputs did not change.

The strongest architectural improvement is to make compiled artifacts independently
reusable, assemble images from those artifacts and immutable runtime dependencies, and
promote only the installation components whose runtime inputs changed. Cargo and
BuildKit can implement the first experiments. A replacement build engine needs a
measured advantage over that corrected baseline.

## Scope And Evidence

The audit inspected clean revision `fd87d2197bbfcd52fa36b397a1666481a74b74e9`,
44 retained image-run records, 65 retained convergence records, the managed builder,
and the running Bioma installation. The records are a retained sample, not a benchmark
distribution. Their timings mix cold dependencies, warm builds, staging, and qualification.

Fresh checks measured planning and help dispatch, rendered charts in temporary
directories, and read cluster and container metadata. No images were rebuilt, no
workloads were changed, and no cache or persistent data was deleted. GPU runtime and
browser acceptance were not exercised.

| Observation | Evidence | Interpretation |
|---|---|---|
| Builder limited to four CPUs on a host reporting 32 logical CPUs | Docker `NanoCpus=4000000000`; cgroup `cpu.max` is `400000 100000`; memory limit is 36 GiB | parallel compilation and export share a small CPU budget |
| Builder has accumulated throttling | `cpu.stat`: 136,998 throttled periods out of 334,615 periods | lifetime evidence of quota pressure; this does not measure the slowdown of any individual recorded build |
| Host lacks its configured disk reserve | zero-growth release preflight reports 278 GiB available against a 366 GiB reserve and exits 1 | even a zero-growth run fails the existing 20 percent reserve policy |
| Retained build state is material | managed BuildKit reports 215.68 GB; the main worktree `target` directory occupies 227 GiB | storage capacity and cache retention need coordinated ownership |
| Control commands remain relatively small | Console plan 7.252 s end to end, including 3.173 s internal planning; unchanged affected plan 5.264 s; deployment help dispatch 1.173 s | optimizing dispatch alone cannot recover the minutes lost downstream |
| UAV source overlay takes 201.406 s | revision `5916bb33`, staged runtime: compilation 0 s, timestamp normalization 180.392 s, push 0.075 s | inherited-layer processing dominates this run |
| Console presentation edit invokes Rust | revision `9a32be3c`, staged BFF: total 53.377 s; Rust action 46.735 s; web build 8.577 s | the edit changed frontend and verification files; the existing cache did not avoid recompiling the unchanged BFF and shared Rust sources |
| Stream reconstructs a large Rust graph inside its SDK image | revision `785b49a5`, stage 1,827.397 s, Rust action 901.389 s, 527 Cargo compilation log entries | narrow the dependency graph and the compiler/runtime-image boundary |
| Reason repeats related work separately | same revision `785b49a5`, stage 1,139.000 s, Rust action 626.032 s | sharing compatible Rust artifacts is a substantial candidate improvement |
| Recent explicit GitOps convergence can be quick | retained runs for `22005604`, `e01c1a0f`, and `8e3c2975f`: 4.009 s, 18.717 s, and 28.669 s | these commands request reconciliation and sometimes observe work already underway; they do not bound passive commit-to-ready latency |

Image records are under
`target/veoveo-xtask/evidence/<full-source-revision>/<operation>-target-<target>-*/run.json`.
Each directory retains `plan.json` and `buildkit-events.jsonl`. Convergence records
are under `output/development/gitops-convergence-<revision>.json`. The table preserves
the findings if those disposable files disappear.

## Immediate Corrections

### Restore event-driven values updates

`examples/bioma/kustomization.yaml` generates both Helm values ConfigMaps with
`reconcile.fluxcd.io/watch: Enabled` under annotations. The live ConfigMaps have that
annotation and no corresponding label. The live helm-controller uses the default
watch selector, which selects labels. Both HelmReleases have five-minute intervals.
Consequently, a values-only edit can wait for the next interval unless another event
or an explicit reconciliation request wakes the release. This interval-sized delay is
an inference from the live configuration, not a timed mutation experiment.
[Flux documents the required label](https://fluxcd.io/flux/guides/helmreleases/#reacting-immediately-to-changes-in-referenced-secrets-and-configmaps).

Move the marker to generated labels. Verify a Git-authored values update without the
convergence harness requesting Helm reconciliation. Keep exact-revision observation.

### Stop chart metadata from restarting unchanged workloads

Two temporary copies of each chart were rendered with identical Bioma values and
image locks, changing only `Chart.yaml` version from `0.1.0-audit1` to
`0.1.0-audit2`. The platform render changed 16 of 18 Deployment Pod templates.
The UAV render changed all six: four pilots, the simulator, and its MCP server.
Every affected `.spec.template.spec` remained identical. The changing field was
`veoveo.ai/chart-revision` in Pod annotations.

The owning platform templates are `domain-services.yaml`, `gateway.yaml`,
`recording.yaml`, `stream.yaml`, and `reason.yaml` under `deploy/helm/veoveo/templates`.
The UAV templates are `deployment.yaml` and `agent-deployments.yaml` under
`showcase/uav-sim/deploy/helm/templates`.
A Pod-template metadata change triggers a Deployment rollout.
[Kubernetes defines this rollout boundary](https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#updating-a-deployment).

Keep chart identity on release or object metadata. Derive Pod rollout annotations
from the actual configuration and mounted content consumed by that workload. Preserve
required configuration reloads when removing the coarse chart-version trigger.
Acceptance must show zero changed Pod templates after a metadata-only publication and
the correct changed templates after a runtime-config edit.

### Make builder capacity an explicit contract

`tools/image-build/control/src/lib.rs::create` sets the worker image and network but
does not declare the observed CPU or memory limits. Builder validation therefore
does not explain the four-CPU quota. The audit did not establish who applied it or
when it began affecting builds.

Declare the resource budget, report effective cgroup limits and throttling deltas,
and benchmark controlled CPU allocations with the simulator running. Additional
compiler jobs cannot escape the container CPU quota. BuildKit's Docker driver already
supports CPU and memory controls.
[Docker documents the available resource controls](https://docs.docker.com/build/builders/drivers/docker-container/).

The zero-growth preflight command was:

```sh
cargo xtask release preflight --expected-growth-gib 0 \
  --kubernetes-node k3d-veoveo-bioma-server-0 --namespace veoveo
```

Filesystem availability changed during the audit: an initial read showed 90 percent
usage and the later preflight showed 85 percent. No reclamation was performed by the
audit. The node reported Ready with no DiskPressure at the later check; 27 retained
Evicted pod objects were historical. Separate build storage or a dedicated build host
would give the compiler cache a budget independent of recordings and the live cluster.

## Core Build And Deployment Changes

### Reuse compiled artifacts independently of image assembly

Extend the existing scratch Rust artifact targets into reusable publication boundaries.
An artifact's identity must include its complete source inputs, lockfile, toolchain,
target ABI, feature set, flags, and build-script inputs. Keep exact Git provenance in
the build receipt. Multiple source revisions can reuse an artifact only when those
inputs are identical.

Today `tools/image-build/rust-workspace.Dockerfile` binds the complete repository into
one Cargo action. A Console frontend edit invalidates that action even when the BFF
source is unchanged. Cargo may finish with a freshness check, but changed feature
combinations or missing cache artifacts can turn that check into compilation, as the
retained Console trace demonstrates. Docker checks bind-mounted inputs when deciding
whether a build action is reusable.
[Docker describes this invalidation rule](https://docs.docker.com/build/cache/invalidation/).

Give frontend assembly a stable compiled-BFF input. Preserve complete Cargo workspace
metadata when deriving a narrower Rust source context; the current image contract
deliberately rejects handwritten subsets. Include embedded assets, native sources, and
generated inputs in the declared closure. A web-only stage must execute zero Rust
compiler actions and preserve the BFF artifact digest.

### Separate Rust compilation from NVIDIA runtime packaging

Stream and Reason declare almost identical Rust dependencies but compile them in
different SDK images with different target caches. Stream's Linux normal/build
dependency tree has no CUDA or GStreamer Rust binding. Its native GStreamer runner is
a separate CMake binary. Reason starts its Python runner through a process boundary.
The current Reason Dockerfile explicitly uses the vLLM builder to match runtime glibc.

A common compiler environment with a proven compatible ABI could build these Rust
control binaries together, then copy them into their existing NVIDIA runtime images.
Keep SDK-dependent native compilation in the matching NVIDIA development image. Verify
ELF interpreter and symbol requirements, shared libraries, runtime startup, and the
hardware GPU workload before admitting a common family. Compatibility is a hypothesis
to test, not an assumption that all Linux binaries are interchangeable.

### Extract reusable recording APIs from service implementations

`platform/recordings/video` depends on both Recording Hub and Recording MCP. Stream
and Reason also depend directly on Recording MCP. This pulls service implementation
changes into consumers that need governed recording access. The cold Stream trace
compiled the forwarder, Hub, Recording MCP, and video library before Stream itself.

Move shared recording types and the authorized read-plan/client boundary into focused
libraries. Keep service lifecycle, spool management, and server-only query engines in
their owners. Preserve capability checks and the existing fail-closed read behavior.
A Hub-only implementation edit should leave Stream and Reason artifact inputs unchanged
unless it changes their actual shared contract.

### Publish normalized GPU dependencies once

The UAV dependency stage is already separated from application source, but it is still
a live `target:` dependency of every runtime solve. The 201-second trace includes a
180-second timestamp rewrite and separate dependency/runtime cache exports. Merely
adding another stage has not established a reusable normalized publication boundary.

Publish the expensive dependency payload as an immutable image and use its exact
normalized digest as the overlay parent. Experiment with independent `COPY --link`
layers where their destination semantics permit it. Docker supports layer reuse and
rebasing through this mechanism; its effect on this graph still needs measurement.
[Docker documents the supported behavior](https://docs.docker.com/reference/dockerfile/#copy---link).

Keep deterministic timestamps and the same runnable digest through staging and
qualification. Acceptance must inspect inherited layer digests and transferred bytes,
then time two distinct source-only revisions. A warm result should meet the existing
30-second overlay budget without rewriting dependency layers. The trace's phase
windows overlap and must not be added together to estimate savings.

### Execute one exact staging selection

`ImageSelectionArgs` exposes one target or a predefined group. `Selection::exact`
already exists internally for profile publication. Expose an exact affected selection
to staging, resolve it once, and run one Bake solve with one digest receipt per target.

Launching separate stage commands cannot provide the intended parallelism today.
`PublicationSource` retains an exclusive source lock throughout publication, and
`BuilderLease` retains an exclusive builder lock. Both waits precede creation of
`run.json`. The current evidence omits queue time, source preparation, worker setup,
planning, and post-build manifest inspection. Measure command-entry-to-ready time as
well as the individual phases. Preserve cache locking when introducing concurrency.

### Give installation components independent release inputs

Both Bioma Helm values ConfigMaps include the same complete `images.lock.yaml`, so an
image-lock edit changes inputs presented to both releases. The platform HelmRelease
owns most services; UAV depends on its readiness. Chart publication also iterates a
fixed list of all three charts in `tools/xtask/src/commands/helm.rs`.

Project each component's values from the complete typed lock. Publish only charts
whose chart inputs changed. Split atomic releases where independent rollout and
failure recovery are required, following `DEPLOY-SCOPE-023` in
[the platform improvement plan](PLATFORM_IMPROVEMENTS_PLAN.md#phase-7-component-scoped-deployment-hard-cut).
Keep installation mutation under the existing GitOps owner. Helm release splitting
must include explicit object ownership and migration of shared hooks and resources.

### Provide a working local UI loop

The Console already has Vite, but its development proxy targets port 8796 while the
BFF defaults to 8786. Port 8796 is the chart's Recording MCP port. A canonical local
runbook should align proxy destinations and preserve the expected authentication
origin. Vite's backend integration supports serving development assets alongside the
real backend.
[Vite documents this integration](https://vite.dev/guide/backend-integration).

Package MCP App HTML as immutable image assets that the owning server serves, where
that removes an unnecessary Rust link step. Stream and Map currently use `include_str!`
for their App HTML. Authenticated resource behavior and image provenance stay intact.
Validate UI iterations in a headed hardware-backed browser. A local UI preview is a
separate checkpoint from immutable deployment and release acceptance.

## Tooling Choices And Implementation Order

| Order | Change | Acceptance |
|---:|---|---|
| 1 | Fix Flux watch labels and chart-driven Pod churn | values changes wake reconciliation; metadata-only chart updates change zero Pod templates |
| 2 | Declare builder resources and isolate durable build storage | explain every CPU quota; meet disk reserve; improve build time without degrading the live GPU workload |
| 3 | Make Console artifacts and normalized UAV parents independently reusable | web-only build performs no Rust compilation; two source-only UAV revisions each stage within 30 s |
| 4 | Stage an exact target set and record the whole elapsed path | one solve and per-target receipts; queue and preparation delays remain visible |
| 5 | Share ABI-compatible Rust compilation and extract recording client contracts | common dependency actions execute once; service-internal edits do not rebuild unrelated consumers |
| 6 | Split release ownership and provide the local UI loop | unselected owners receive no writes; presentation edits use local feedback before image publication |

A durable BuildKit worker on separate build storage is the first infrastructure
experiment. If host contention remains material, a dedicated native Linux builder can
use Buildx's existing remote support. The managed-builder contract currently requires
the Docker-container driver, so this needs an explicit implementation change.
[Buildx supports externally managed BuildKit workers](https://docs.docker.com/build/builders/drivers/remote/).

Benchmark a compiler cache such as sccache after narrowing artifact inputs. Its Rust
cache requires incremental compilation to be disabled and does not cache crates that
invoke the system linker. It can help recover dependency builds across workers, but
it cannot remove the measured GPU exporter tail or guarantee a fast changed-binary
link. Keep local incremental compilation as a separately measured option.
[sccache documents these Rust limitations](https://github.com/mozilla/sccache/blob/main/docs/Rust.md).

Bazel is a candidate if per-action reuse across hosts remains unmet after these
changes. It provides shared action results and a content-addressed artifact store.
[Bazel describes that cache model](https://bazel.build/remote/caching).
A representative trial should include Stream's Rust, CMake, and container assembly,
plus cold, unchanged, source-edit, and second-worker runs. Account for rule maintenance
and duplicated dependency declarations. The measured exporter and rollout defects
remain necessary fixes with any build engine.

Lower optimization in a disposable development profile is another experiment. It
would change the binary and therefore cannot use today's staged-to-qualified digest
promotion contract. Favor artifact reuse first, preserving runtime performance and
qualification identity.

The affected planner also needs dependency-kind precision: it discards Cargo
`dep_kinds`. Optimization is a dev-dependency of Map, which makes Optimization source
changes reach Map and UAV through the planner's closure. Exclude dev-only edges from
runtime-image invalidation while retaining their test acceptance closure.

## Implementation Measurements

The September 7 Console benchmark used baseline `40a34ecb` and isolated frontend-only
revision `78b065d6`. The latter changes the page title and its recorded build evidence.
Both selected exactly `console-bff` with the same Rust compiler inputs.

| Measurement | Baseline | Frontend-only revision |
|---|---:|---:|
| Command entry through staged receipt | 29.348 s | 25.996 s |
| Builder setup | 16.758 s, including registry reconfiguration | included in the command record |
| BuildKit solve evidence duration | 6.564 s command span | 11.637 s run window |
| Rust compilation | Cargo freshness check, 2.09 s | cached BuildKit action; zero Cargo execution |

The first three runtime layers are identical. The BFF binary layer is
`sha256:fbe5c7854e76a01e30ebca9295100d9c0fc4bb5432d0227c000dda8e71864dd1`.
Only the frontend layer changes. Qualification took 21.214 s and retained runnable
digest `sha256:d287b9f7ff9f5b62caa595fc3787d6c059f148df9251112847bcf57ddee61afd`.
These are single-run measurements, not latency distributions. No benchmark image was
installed into the running cluster. Receipts are retained under
`output/development/bff-input-cache-{baseline,web-only,qualified}.json`.

The first normalized UAV parent published successfully as
`sha256:b9cb0b3be283b5593da465653f7f9f99aa57a7f314e10ec3b85c6fc2a5737120`.
The subsequent ordinary-COPY overlay started unpacking that parent, adding latency
and temporary disk use. The assembly was canceled with a recorded failed outcome.
The corrected overlay uses independent COPY layers. A read-only inspection of the
running UAV container confirmed `/opt`, `/opt/veoveo`, the application directory, and
the overlay identity directory are regular directories. No workload was restarted.

A second experiment identified an upstream condition: `COPY --link --chmod=0555`
uses the ordinary file action in Dockerfile frontend 1.27.0, forcing parent extraction.
The corrected recipe applies the entrypoint mode in a scratch stage and links its
result without `--chmod`. The two canceled assembly runs are retained as failed
evidence rather than presented as successful overlay measurements.

The corrected UAV baseline `c179bb8e` staged in 23.064 s. Two isolated source-only
revisions, `2081d3e8` and `ad605a21`, staged in 14.565 s and 15.648 s. All 37 inherited
layer blobs remained identical (17,571,096,830 compressed bytes). Only layer 39, the
entrypoint layer, changed. Its file bytes matched each source revision, with mode
0555 and owner/group 10001. Timestamp normalization took 21 ms and 45 ms. These runs
retain about 77 KiB of application overlay data and execute no compiler actions.

The remaining command floor is mostly control work: the first source-only command
spent 9.235 s in builder setup and 3.088 s in planning. Both repeatedly hashed the same
63 MiB managed Buildx executable with unoptimized SHA-256. The development profile
now optimizes that hash implementation while retaining every full checksum check.
A timestamp-keyed verification cache was rejected after a same-timestamp mutation
test showed that filesystem metadata could not reliably identify changed bytes.

With optimized SHA-256, repeating `ad605a21` staged in 2.789 s and retained runtime
digest `sha256:2beef03e6ed1b4e811f6f82c61d3ef9c89f5692cf970cd1bab6dd91657d45a46`.
This warm repeat measures the tooling improvement; the two distinct source revisions
above establish application-edit reuse. GPU runtime and qualification acceptance are
separate and have not been claimed from these assembly measurements.

The recording extraction keeps service lifecycle and Blueprint interpretation in their
owners. The new reader crate has no Hub, forwarder, or Recording MCP dependency. Its
cache still verifies downloads and cache hits and retains leases until readers release
them. The existing missing-Artifact-credential rejection remains in place.

Thirty reader, video and Recording MCP library tests passed, including the moved
Blueprint identity checks and new cache-hit, lease and unverified-source checks. The
consuming binaries compile with Recording Redap enabled. The first focused test attempt
was canceled when disk preflight showed insufficient reserve. Removing exactly 43 old
worker-only vLLM snapshot records recovered 29.83 GB; Cargo cache mounts, registry images
and deployed workloads were retained. A 20 GiB growth preflight then passed. The next
Reason runtime assembly may fetch its base again.

Read-only probes found glibc 2.39 in both deployed Stream and Reason images. Their Rust
executables resolve libc, libm, libgcc_s and the amd64 ELF loader; Stream also inherits
its runtime's preloaded driver-support libraries. This supports investigating a common
older-glibc compiler, but it does not establish candidate-binary or GPU compatibility.

The resolved standalone plans now contain 316 files for Stream, 321 for Reason, and
281 for SUMO. Both recording consumers contain nine complete local production packages,
including the new reader, while excluding Hub, forwarder and Recording MCP implementation
sources. Their native and Python runner files remain available. The three-target plan
completed in 2.1 seconds, and 83 xtask tests plus strict Clippy passed. Compiler ABI
families remain separate pending candidate runtime acceptance.

An isolated Hub implementation edit at `3e2905cc` leaves Stream and Reason's source
identities unchanged from `1548fa57`. Stream retains
`sha256:4fb70582913e456bc4074f66cacd77044eb3710ed217cfaa929127948eef5ac0`;
Reason retains `sha256:a1c44e2559b7c1522d6cfdc7c285d91938d15563d1812b891aeb4719bae81521`.
This verifies the planned input isolation with a real source revision. It does not
claim a measured compiler or linker duration.

The normalized parent's manifest and all 37 layer blobs were verified in the local
registry before retiring 61 exact worker cache records rooted in the original UAV
dependency build. That maintenance recovered 52.1 GB and retained registry artifacts
and Cargo execution caches. Staging `ad605a21` after eviction took 3.100 s and preserved
its runtime digest. The published dependency boundary therefore works without the
original dependency snapshots.

Cold qualification after that eviction took 885.574 s. The raw trace records a
434.704 s extraction window, 55.015 s of SBOM scanning, 59 ms of timestamp normalization,
and a 153.038 s export window. These windows are not a partition of total duration.
The run retained the staged runnable digest and published SPDX and SLSA attestations
in index `sha256:9851a29a5e38ce63d9b8297b8061f92517de86ffdac9f88b7901d574842b02d8`.
The host remained above its 366 GiB disk reserve.

The subsequent warm qualification took 10.342 s with the scanner explicitly pinned
to Docker BuildKit Syft scanner 1.12.0 and its OCI digest. Its SBOM action was cached,
and it again preserved the staged runnable digest. Receipts are
`output/development/uav-normalized-qualified.json` and
`output/development/uav-pinned-qualified.json`. These establish artifact promotion;
they do not establish GPU workload or headed-browser acceptance. Image evidence now
records extraction separately, and progress reports aggregate phase windows only after
the solve ends instead of calling a phase complete when its first layer finishes.

Map and Stream now package App HTML separately from their Rust executables. The
shared loader rejects missing, empty, non-UTF-8 and oversized assets before service
startup. Resource reads retain their existing authorization and return a fixed startup
snapshot. The Cargo-derived planner gives runtime assembly a separate asset context;
Map's context contains seven presentation files and Stream's contains one. All Cargo
metadata and native runner inputs remain in the compiler boundary.

Fourteen App-helper tests and 85 xtask tests passed. Both servers passed all-target
compilation checks, and strict Clippy passed across the helper, planner and consumers.
The real two-image plan resolved in 2.5 s. A fixture presentation edit changes only the
asset digest.

The first packaged-image solve at `aca3cb3c` took 640.795 s while establishing the
worker's Bookworm compiler cache; its Map Cargo action took 8 min 44 s. This is a cold
baseline, not an estimate of the Rust link cost for every presentation edit.

Runtime recipes now copy App assets after native setup. Baseline `e733162b` staged both
images in 14.415 s. Presentation-only revision `ea8aa2fe` changes the two App titles and
its test receipt; it staged both images in 12.084 s in one solve. Both Rust actions,
Stream's CMake action and Map's runtime setup were cached. No Cargo command executed.

Only Map layer 33 and Stream layer 22 changed. The images retain 33 and 22 preceding
layer blobs respectively. Their unchanged binary layers are
`sha256:18f7a1544fe208f0bb777aeb90bb99ac6fb61555916a15bb5cf922e2159a09cf`
for Map and `sha256:b1c278dcbe04270100686751bf976694f4064fbdb9cbd14abdda3c490308080f`
for Stream. Archive inspection verified exact committed App bytes, mode 0444, and
root ownership. The changed compressed asset layers contain 416,204 and 5,764 bytes.

Receipts are `output/development/packaged-apps-final-{baseline,edited}.json`; the
manifest and archive inspection is in
`output/development/packaged-apps-layer-inspection.json`. These are packaging and
input-reuse measurements. The benchmark images were not deployed, and headed visual
acceptance remains pending.

A local Vite process became ready in 255 ms. Headed Chrome 151 exposed hardware WebGL
through ANGLE on the RTX 4090. Its exposed WebGPU adapter reported SwiftShader and was
rejected as hardware evidence; WebGL remained hardware-backed. A temporary React
heading edit appeared without a document reload, and restoring the source produced a
second update in the same document. The source file returned to its exact Git blob.

This browser check exercised the error screen because the local BFF and gateway ports
were unbound. It does not verify authenticated local login or application data flows.
The temporary tab and Vite process were closed. Evidence is retained in
`output/development/console-vite-{gpu-probe,hmr-inspection}.json`.

The subsequent authenticated check used fresh native gateway, BFF, and conformance
binaries, an isolated in-memory SurrealDB 3.2.4 container, and the repository's fake
OIDC provider. Vite became ready in 146 ms. The browser followed both authorization
callbacks through port 4173 and reached the Overview as the fixture principal.
The BFF established its MCP session directly on port 8788. Snapshot, App catalog,
App events, and snapshot events returned HTTP 200. The fixture's absent media upstream
remained visible as unavailable.

Changing and restoring an Overview heading preserved the document and signed-in
session. Hardware WebGL remained available on the RTX 4090; SwiftShader WebGPU was
again excluded from hardware evidence. The source returned to its committed bytes.
The temporary browser tab, local services, database container, and private key files
were removed. No production authentication state changed. The record is
`output/development/console-vite-authenticated-hmr.json`. This verifies the local
authenticated UI loop; it does not accept a hosted GPU workload or the staged images.

The passive GitOps verifier observed Bioma revision
`fd87d2197bbfcd52fa36b397a1666481a74b74e9` in 1.232 s. Both Helm inventories were
Ready, and the selected gateway, Console BFF, and UAV simulator Deployments passed
rollout and availability checks. The source, root, and release generations and explicit
reconciliation annotations stayed identical. The sampled gateway, Console, simulator,
and UAV MCP Pod identities also stayed identical. This is verification overhead for
an already-active revision, not publication latency or GPU runtime acceptance.
The record is `output/development/gitops-passive-existing-revision.json`; before/after
state is in `output/development/gitops-passive-{before,after}.json` and
`output/development/gitops-passive-pods-{before,after}.json`.

## Remaining Experiment Boundary

The initial 20 GiB growth preflight found 368 GiB available against 386 GiB required,
including the 366 GiB reserve. The Kubernetes node remained Ready with no disk pressure.
The retained BuildKit cache measured 191.92 GB. The available host has one NVMe shared
with the live cluster; no separate build disk or second builder was available for this
implementation cycle. That check retained expanded GPU qualification snapshots and
used a Cargo retention policy that excluded outputs produced within the same day.

This constraint recurred across three consecutive goal turns. A fresh scoped Cargo
plan identified one superseded gateway executable copy and 16 older incremental
variants. Applying it reclaimed 1.76 GiB while retaining dependency libraries, current
executables, and the newest incremental variants. The post-maintenance preflight still
reported 368 GiB available against 386 GiB required. No additional build volume is
mounted. The maintenance record is
`output/development/cargo-cache-final-maintenance.json`. Treating additional storage
as a prerequisite for all further local experiments was too conservative.

The subsequent user-directed cleanup removed an unused BuildKit 0.31.2 image, 139 old
BuildKit records outside the recent build graph, and 27 expanded normalized-UAV image
records. Those BuildKit removals reclaimed approximately 12.6 and 48.5 GiB. All 14
execution-cache mounts were retained. Registry verification found the normalized
parent manifest and all 37 compressed layer blobs intact. No Kubernetes application
image was eligible after accounting for workloads, retained ReplicaSets, and containers.

The Cargo command now accepts `--older-than-hours`, with the same seven-day default
expressed as 168 hours. A six-hour pass removed 16 executable copies and 240 older
incremental variants, reclaiming 18.4 GiB while retaining dependency libraries and
current executable links. Tests cover same-day age boundaries, current links, redirected
directories, newest variants, and Cargo lock interoperability.

Free space increased from approximately 347 to 426 GiB. The 20 GiB growth preflight
passed with 38 GiB beyond its retained reserve. Restaging the previously measured UAV
revision took 4.433 s, executed zero compilation and filesystem extraction, and retained
runnable digest `sha256:2beef03e6ed1b4e811f6f82c61d3ef9c89f5692cf970cd1bab6dd91657d45a46`.
The next uncached SBOM qualification must materialize the expanded filesystem again.
Separate build storage remains useful for that cost and for isolation; it is no longer
a capacity prerequisite for the next 20 GiB local experiment. Second-worker reuse still
requires another worker.

The current evidence is `output/development/cargo-cache-hourly-maintenance.json`,
`output/development/cleanup-normalized-parent-preserved.json`, and
`output/development/uav-after-cache-cleanup.stage.json`. Image command
`1788802461571092011-stage-14415` records the complete staging duration.

The common Stream/Reason compiler family remains unadmitted. Its candidate executable
must pass service startup and hardware workload checks in both runtime images. The
controlled CPU comparison below measures one warm source-edit workload. The completed
compiler-cache recovery comparison below provides no elapsed-time gain. The same-host
second-worker comparison below now distinguishes completed-result reuse from Cargo
execution-cache reuse. Network transfer between hosts and independent build storage
remain unmeasured. A Bazel migration has no measured advantage from this work and has
not been introduced.

The changed Flux configuration and shared charts are active with the Bioma reference
configuration. The live activation records above include passive commit-to-ready timing
and unchanged workload identities. Selected chart publication and separate release image closures are delivered;
splitting atomic Helm ownership still belongs to the complete `DEPLOY-SCOPE-023` migration.
These boundaries remain explicit in the
[iteration register](DEVELOPMENT_ITERATION.md#active-follow-ups-worth-fixing-next).

The component migration now has a pure ownership planner in
`deploy/contract/src/components/`. It expands exact component IDs, compares complete
rendered units with locked inputs and object inventories, and rejects cross-owner
overlap before producing any allowed mutation. It also checks the previous Helm
inventory because an upgrade can delete objects removed from a chart. Matching installed
inputs and objects produce an unchanged action for a dependency. These tests establish
the preflight contract; the profile schema migration, installer integration, and actual
zero-write deployment acceptance remain open.

The planner now distinguishes immutable provenance from deployable content. Components
from the same repository can retain different source revisions, and a newer component
revision can consume an unchanged image from an earlier commit. Exact provenance still
has to match the desired lock. A separate content digest decides whether the installed
inputs and object inventory require an upgrade. Independent temporary Git histories
exercise a platform documentation commit and an extension values update: the platform
receives an unchanged action while the extension receives the upgrade. These are pure
planning tests; they do not establish live Helm or Kubernetes zero-write evidence.

Tracing those inputs into the publisher found another false invalidation. Revisions
`f973a98a` and `3712cbca` share Veoveo chart tree
`545f89c2e6a246735f1b359a1afd88043dc82e0b`, but the existing `git archive` command produced
different archive hashes. Git places commit identity and timestamps in that archive,
as described in its [archive documentation](https://git-scm.com/docs/git-archive).
Publication and installation now share `source_chart_content_digest`, which hashes the
actual chart files in the verified source checkout. It covers file paths, executable
modes, and exact bytes, including files excluded by Git export attributes. Commit
metadata and checkout timestamps no longer change chart identity. Existing source-chart
lock entries require regeneration with this encoding; OCI artifact digests are unchanged.

Deployment execution now lives in `deploy/runtime`, a shared Veoveo library consumed by
the release publisher and focused deployment smoke binary. The general smoke binary no
longer compiles a second copy of the installer. Source resolution, Helm inputs, public
configuration, cluster lifecycle, and GPU allocation have explicit modules. The publisher
and installer use one source-chart lock constructor and content check. This establishes
the shared execution boundary for component compilation. The selected execution and
native scope acceptance built on it are recorded below.

The shared chart constructor now verifies actual input bytes and executable modes
against the Git tree before publication builds begin. Installation preflight uses the
same verifier for profile inputs. Tests reproduced two ways an ordinary Git difference
check can hide edited values: index freshness hints and a clean filter that rewrites
content. Both fail the new check. An ignored file added inside a chart also fails its
complete inventory check, while unrelated working files remain outside the deployment
input boundary. This establishes the file provenance needed by component compilation;
it does not activate component-selected deployment or change artifact digest encodings.

Installation preparation now honors the chart revision recorded by each component.
Previously, a source-name lookup forced every component from one repository onto the
top-level publication revision. The resolver now shares checkouts only when source name,
repository, and revision all match. A regression uses real Git commits and Helm renders
to advance one chart while retaining another from the same repository at an older commit.
Both prepared inventories match their locks, and substituting the newer checkout for
the retained revision fails. Image digests in this regression are synthetic compiler
fixtures. Selected execution and live zero-write evidence remain unfinished.

The image catalog now retains qualified builds of a shared target at different source
revisions. Installation passes each Helm release only its recorded image selection.
Regression cases update one consumer's image while preserving the other consumer's
complete inventory. A separate case records the same synthetic runnable digest for
two real source commits and verifies that preparation preserves both build provenances.
The complete catalog still rejects duplicate build identities, changed repository owners,
and unqualified artifacts. Publication narrows unused image candidates before sealing
the render, which keeps image-map-dependent configuration identical during installation.
These checks cover Git and Helm preparation. Component publication now includes the
chart and configuration path below; development-lock promotion and actual zero-write
execution remain open.

Components now retain their installation document and configuration commit explicitly.
Preparation restores older installation snapshots and checks their files against Git.
A regression replaces and deletes a values file in the current installation, then
prepares the updated component with its new values and the retained component with its
original values. Exact unit identity includes configuration provenance; the content
identity excludes revision-only changes. This completes another input prerequisite for
scoped publication. It does not expose a partial installation selector or establish live
zero-write evidence.

Component publication now has a command:
`cargo xtask release components --profile ... --base-lock ... --component ... --image-evidence SOURCE=PATH --lock-output ...`.
It imports qualified image evidence, verifies the OCI publication and runnable digests,
and renders only the requested components with their recorded charts and configuration.
Every unrequested inventory, including dependency components, stays unchanged. The
output lock retains prior image versions and records each updated consumer's new build
provenance. Duplicate updates, unused evidence, repository ownership changes, and
conflicting qualifications for an existing build revision fail before output publication.

The same command accepts `--source-revision SOURCE=COMMIT` for chart inputs and
`--refresh-configuration` for installation inputs. Either can run with no image evidence;
all existing image identities remain fixed. Inputs can also be combined for one exact
component set. The compiler restores the base installation commit and retains every
unrequested component while the output records the current installation commit. Native
Git and Helm tests cover both directions for chart-only, configuration-only, and combined
updates with the other repository unavailable. They also reproduce retained values after
the current checkout deletes them and reject a chart that overlaps an unselected owner.
This closes scoped chart/configuration lock composition. Live selected execution and
development-lock promotion remain open.

Regression coverage uses independent installation, platform, and extension Git histories
with real Helm renders. Each source update succeeds while the other source repository
is unavailable. The result can be prepared again from its lock with identical inventories.
Shared-target cases preserve another consumer's image version, including builds with
identical runnable bytes. These compiler fixtures use synthetic image digests; they do
not qualify a new live OCI rollout. The command adds a publication receipt and reuses
command-level timing. Selective cluster execution remains unfinished.

The installation path now checks Helm's historical deletion and recovery boundary
before its first write. Stored manifests and hooks are bound to exact release revisions,
including a different deployed revision or successful rollback candidate. Historical
objects cannot transfer into another component or release. Live reads batch exact
object names by kind and namespace, then check Helm ownership and known GitOps markers.
Release metadata is checked again after those reads. Helm hooks receive explicit owner
metadata during compilation, and installation now uses the canonical Helm 4 rollback
flag. Execution fencing, general raw adoption, and selected execution remain open.

A live Rust ownership test passed against the reference cluster in its own temporary
namespace. Two ConfigMap-only releases installed successfully through the corrected
Helm invocation. The preflight then rejected cross-component transfer of a historical
manifest and hook, and raw application over a Helm-owned ConfigMap. ConfigMap UIDs and
resource versions and both Helm revisions remained unchanged after rejection. Namespace
removal was verified. This establishes the ownership preflight's live behavior; full
selected execution and GPU workload acceptance remain separate requirements.

The full-profile installer now verifies local installed-unit receipts before reusing
unchanged Helm releases or prepared manifest sets. Receipts retain actual installation
provenance, a cluster UID, stored Helm revision and manifest identity, and observed
object fingerprints. Reuse adds no Kubernetes bookkeeping object. The initial apply
captures actual API contents, including defaults and status. Subsequent runs recheck
those contents, object UIDs, and Helm metadata. Completed TTL Jobs and successfully
deleted Helm hooks have explicit observation rules.

Baseline capture now asks Kubernetes for a server dry-run projection when literal
comparison fails after installation. A native regression reproduced a missed receipt
because the API converted `1024Mi` to `1Gi` and `0.1` CPU to `100m`, and omitted empty
Pod fields. The corrected path records the normalized object and reuses the release
without another Helm revision or resource version. A changed-memory dry-run also
preserves the live resource version. The fixture runs zero replicas with a synthetic
image and verifies namespace cleanup; it establishes API behavior only. Unchanged
reuse does not issue a dry-run request.

An isolated live Rust regression verifies unchanged Helm revisions and ConfigMap
resource versions, source-revision-only reuse, an update confined to one release,
successful hook deletion, raw ConfigMap reuse, and rejection of drift and replacements.
Failed application invalidates the local receipt. Helm's field-manager conflict
protection remains active. The native fixture removes and verifies its temporary
namespace. This is unit-reuse acceptance. The selected CLI fixture below adds native scope
evidence; cross-host fencing and GPU acceptance remain open.

GPU device-plugin migration now prepares its direct removal and workload-quiesce
inventory before profile writes. A workload outside the selected component set fails
preflight. Retiring objects cannot overlap current owners, and Helm retirement rejects
deletion hooks and cascading Namespace, Node, or CRD removal. Execution rechecks object
versions and Helm revision, then scales only the captured Deployments with Kubernetes
resource-version preconditions. The live Rust API fixture rejects an unselected workload
and post-preflight drift before scaling, completes the checked retirement, and preserves
the other Deployment. It uses zero replicas and verifies namespace cleanup. This closes
the direct GPU transition inventory gap; cross-host fencing, complete mutation receipts,
selected execution, and GPU workload acceptance remain open.

The full-profile installer now consumes the pure mutation planner. Checked Helm history
and live object inventories distinguish absence, reusable installed provenance, and an
apply requirement. Missing receipts and drift never borrow the desired lock's digest
as evidence of a prior installation. Every prepared namespace, bootstrap, allocator,
claim creation, public configuration, gateway activation, and source release operation
checks its exact planned unit before execution. A stale unchanged decision fails and
requires a new plan. This integrates full-profile planning; it does not expose a
component selector or establish complete mutation receipts.
All six native deployment regressions pass together, including installed reuse,
normalization, ownership rejection, planned updates, and GPU transition lifecycle.
Every fixture verifies removal of its temporary namespace.

Installation helpers now consume operational inputs from each component's immutable
configuration snapshot. Replacing the current gateway file or changing its required
Secret keys cannot change a retained gateway's activation or preflight requirements.
Rollout waits belong to the compiled Deployment owner. Selected GPU consumers and their
allocator must agree on retained settings before installation writes. The native Git/Helm
regression replaces the gateway file and refreshes only extension configuration, then
verifies the retained bundle, Secret key names, and waits. GPU policy disagreement has a
focused preflight regression. These checks establish input retention, not selected live
deployment acceptance or GPU qualification.

Planned GPU quiesce invalidates affected release reuse, which allows Helm to restore
desired replicas after the migration. The native restoration test exposed an additional
lifecycle gap: rollout completion at zero replicas can leave terminating Pods alive.
The migration now retains child UIDs across ReplicaSet revisions and waits for every
owned Pod to disappear before retiring the device plugin. An isolated digest-pinned
CPU sleep fixture verifies Pod removal and subsequent ready-replica restoration.
The fixture establishes Kubernetes lifecycle behavior, not GPU qualification.

The next experiment at `4d3e30dd` built both control binaries in the existing
`rust-bookworm-artifacts` target. It reused that target's pinned Rust 1.97.1 image and
cache family, explicitly selected both packages and binaries, and exported a local
scratch artifact. The Cargo action took 351.3 s; the recorded command took 354.5 s.
This established the combined recording dependency graph in a cache previously used
by Map. It is not a matched comparison with the earlier standalone builds.

Both ELF files use `/lib64/ld-linux-x86-64.so.2` and require glibc symbols through
2.34. Their dynamic dependencies are libc, libm, libgcc_s and the loader. Temporary
copies passed loader verification, resolved libraries, and exited successfully from
`--help` in their respective deployed runtime containers. Stream retained its
runtime's driver-support preloads. The temporary copies were removed. No executable
in an installed image was replaced.

| Candidate | Bytes | SHA-256 |
|---|---:|---|
| Stream | 43,454,984 | `f73e340483b46a0f4926d2f9230e001cef717bd7e66964b3dd3608034c46cad5` |
| Reason | 41,423,504 | `5d4f1a6b934e4d2e6b21c2c2ba9f6f653558ffa8e99c2e50af41360bca5cc4c9` |

The Reason model mount contains 5.7 GiB of checkpoint files, and its existing Python
environment reports CUDA on the RTX 4090. Its readiness endpoint nevertheless timed
out after eight seconds. More importantly for acceptance, the current Reason runner
decodes video with PyAV and resizes frames with Pillow on the CPU. That path requires
an NVDEC/CUDA migration under the repository GPU rule. The current runner cannot
provide hardware video-processing acceptance for the compiler-family change.

The experiment record is `output/development/common-rust-abi-experiment.json`, with
BuildKit metadata in `output/development/common-rust-artifacts.buildkit.json`.
CLI execution proves that the candidate loaders and initial executable paths work;
it does not prove initialized services or completed GPU tasks. The production Bake
catalog and runtime recipes retain their existing families.

The controlled quota experiment selects Console BFF and gateway from their admitted
Trixie compiler family. `cargo xtask image builder benchmark` warms that exact Cargo
selection, then varies trailing comments in temporary source files while retaining
the dependency cache. The worker lease covers quota changes and restoration. Each
measured sample must compile the same package set and produce the same binary digests.

The first fixture used comments of different lengths and produced different binary
digests between quota groups. Its comparison was rejected and retained under
`output/development/compiler-cpu-comparison/review.json`. The accepted fixture uses
fixed-length comments and records the SHA-256 of each source variant.

| Quota, in execution order | Compiler action | Complete artifact solve | Worker CPU time |
|---|---:|---:|---:|
| Four CPUs, first sample | 82.025 s | 84.462 s | 307.901 s |
| Twelve CPUs, first sample | 36.638 s | 40.283 s | 304.508 s |
| Twelve CPUs, second sample | 37.416 s | 39.642 s | 307.101 s |
| Four CPUs, second sample | 87.675 s | 90.107 s | 330.594 s |

Mean compiler time falls from 84.850 s to 37.027 s, a 2.29× speedup and 56.4 percent
reduction for these source edits. Every measured run compiles only `veoveo-console-bff`
and `veoveo-mcp-gateway`. BFF retains binary digest
`sha256:4464f1e295ad622f7aa5d26ea436bad77e86fb4cf1305955cc5613100eb2c70f`,
and gateway retains
`sha256:1a628ac39acafcf7248af5d240bdfc3cd6cfff47469e47ac6dd05b454300d9fd`.
The unmodified-source warmup is separately recorded and excluded from this comparison.
These local compiler artifacts have no image release eligibility.

The worker returned to its declared 1,200,000/100,000 µs CPU quota and 36 GiB memory
limit. A separate live test deliberately edits an unrelated workspace entrypoint,
requires the benchmark to reject the resulting Cargo freshness check, and verifies
the complete worker contract after automatic quota restoration. Ninety-eight unit
tests, that worker test, and strict Clippy pass.

The running UAV simulator reported simulation and visual readiness during the samples.
Its reported simulation time advanced 353.033 s during a 352.998 s observation window,
an aggregate real-time factor of 1.0001. The running Pod retained its five existing
simulator restarts. These operational observations do not establish headed browser
acceptance or GPU image qualification. Two samples per quota do not define a latency
distribution or predict every crate's speedup.

Evidence is under `output/development/compiler-cpu-identical-artifacts/`; its
`comparison.json` contains source identities, compiled packages, binary digests,
phase windows and cgroup deltas. Before/after simulator observations are
`output/development/compiler-cpu-fixed-uav-{before,after}.json`. The worktree's BFF and
gateway source files remained byte-identical throughout both experiments.


### Content-Aware Cargo Freshness

The compiler-cache comparison exposed a correctness defect in the shared target
cache. An ordinary artifact solve returned the previous CPU benchmark's binaries
while Cargo reported freshness in 0.54 s. The current source files had older
timestamps than those cached outputs. A build with an empty target directory returned
the original BFF and gateway bytes instead. The native DuckDB library matched.

`tools/image-build/source-freshness.rs` now compares the admitted source tree with an
input mirror inside the locked Cargo target cache. Changed bytes receive a fresh
source timestamp. Unchanged inputs retain their previous compilation timestamp.
The helper also tracks modes, deletions and symlinks. It adjusts only the disposable
BuildKit source mount and leaves the worktree unchanged. Each Rust builder family
compiles and executes this helper using its existing pinned compiler.

The first corrected ordinary solve rebuilt nine local packages in a 65.3 s BuildKit
compiler window. Both binaries then matched the clean-target build. A real Cargo
regression changes and reverts equal-length source with deliberately old timestamps,
executes the resulting binary each time, and verifies unchanged input timestamps.
Cargo's upstream documentation describes the
[timestamp freshness mechanism](https://doc.rust-lang.org/stable/nightly-rustc/cargo/core/compiler/fingerprint/index.html).

### Compiler-Cache Recovery Result

The completed experiment uses the existing Rust 1.97.1 Trixie compiler environment
with exactly `console-bff` and `mcp-gateway`. Every measured case starts with an empty
Cargo target directory and disables incremental compilation. The wrapper is pinned
to sccache 0.17.0 with the upstream archive checksum. Its client-side mode is enabled,
and its private cache has an 8 GiB limit.

| Case | Solve elapsed | Compiler action | Builder CPU time |
|---|---:|---:|---:|
| No compiler wrapper | 328.4 s | 325.8 s | 2,652.9 s |
| Populate compiler cache | 446.8 s | 443.4 s | 2,882.1 s |
| Recover into an empty target | 333.7 s | 331.3 s | 1,718.4 s |

All cases compile the same 476 package names. Both server binaries and `libduckdb.so`
match each other and the ordinary artifact target byte for byte. Recovery records
522 Rust hits, 290 C/C++ hits and 143 assembler hits, with zero misses or cache
errors. The native build scripts still perform five unsuccessful configuration
probes inside successful Cargo actions; their failure counter remains in evidence.
The cache occupies 341 MiB after recovery.

Recovery reduces builder CPU consumption by 35.2 percent but provides no elapsed-time
improvement in this sample. A Rust compiler process for `surrealdb_core` remained
active for more than two minutes during recovery. This observation warrants a
representative action-cache experiment that measures work before cache lookup, in
addition to Rust linking and procedural macros. It does not establish second-host
performance. The normal build path retains its persistent Cargo targets without a
compiler wrapper.

The accepted comparison is
`output/development/compiler-cache-recovery/comparison.json`; adjacent case directories
retain compiler identity, input gates, cache statistics, artifact hashes and BuildKit
traces. The three experiment cache mounts, including the interrupted comparisons,
were removed under the builder lease after inspection. The cleanup recovered
728,788,992 bytes and retained all fourteen existing execution cache mounts.


### Second-Worker Cache Result

The September 8 trial uses the current Trixie compiler recipe for Console BFF and
gateway, including its native DuckDB library. Both workers run on this host with the
same twelve-CPU, 36 GiB budget. The second worker starts with zero BuildKit cache records
in a newly created volume. The primary worker exports a 484,472,083-byte local OCI
cache, and both secondary solves import its exact manifest digest.

| Case | Solve elapsed | Compiler action | Compiled package names | Worker CPU time |
|---|---:|---:|---:|---:|
| Prepare and export current result | 75.184 s | 58.110 s | 7 | 413.4 s |
| Import unchanged result on fresh worker | 4.617 s | 0 s | 0 | 5.1 s |
| Source edit on existing worker | 39.534 s | 37.210 s | 2 | 291.8 s |
| Same source edit on fresh worker | 369.455 s | 360.855 s | 476 | 2,446.3 s |

The unchanged import reproduces both binaries and `libduckdb.so` byte for byte while
leaving Cargo execution-cache mounts empty. Both edited builds also produce identical
artifacts. The edit changes source bytes in disposable contexts; it does not alter the
worktree. The complete command takes 500.808 s, including preparation, storage checks,
worker lifecycle, four solves, identity checks, and cleanup. Phase windows overlap.

The fresh-worker edit is 9.35 times slower in this sample. Its compiler action includes
dependency fetching and full compilation because exported BuildKit results did not
transport the Cargo registry, Git, or target mounts. An exact finished artifact can be
reused quickly even though an edit loses the established worker's incremental state.
This supports keeping a durable worker for normal iteration. A future remote builder
must preserve that state, or a replacement action-cache design must demonstrate a
better changed-input result. The trial does not measure network transfer across hosts,
a cold primary build, a different disk, or a Bazel implementation.

The accepted receipt is
`output/development/compiler-worker-comparison-20260908/comparison.json`. It records
worker identity, immutable cache manifest identity, input and artifact hashes, package
observations, CPU counters, and successful cleanup. Adjacent directories retain each
trace and artifact bundle. The temporary worker and volume and the exported transport
cache were removed. The ordinary worker retained its fourteen execution-cache mounts.
All 25 Veoveo Deployments were Ready after the trial. GPU runtime qualification remains
separate from these compiler and availability observations.

### Automatic Cache Capacity

The reduced worker cache occupies about 124 GiB after the cleanup and compiler
experiments. Its former 240 GiB protected floor prevented automatic collection from
responding to host disk pressure. Both collection policies now use an 80 GiB floor
and retain the 320 GiB worker ceiling.

The live daemon also exposed a percentage conversion issue. BuildKit 0.33.0 turns its
20% setting into 367,000,000,000 bytes on this filesystem, below the release preflight
reserve. Its
[pinned conversion](https://github.com/moby/buildkit/blob/v0.33.0/cmd/buildkitd/config/gcpolicy.go#L127)
divides by binary GiB and multiplies by decimal GB. The configured 22% trigger resolves
to 404,000,000,000 bytes, about 376 GiB, and covers preflight's exact 20% reserve of
about 366 GiB. This is a collection target subject to the cache floor; planned build
growth still needs separate headroom.

The registry-aware builder transition retained the existing worker volume and all
fourteen Cargo execution cache mounts. The UAV staging command, including that
transition, completed in 5.577 s. It performed no compilation or filesystem extraction
and retained runnable digest
`sha256:2beef03e6ed1b4e811f6f82c61d3ef9c89f5692cf970cd1bab6dd91657d45a46`.
The 20 GiB growth preflight passed with 24 GiB above the retained reserve. The Kubernetes
node remained Ready without DiskPressure, and the existing UAV readiness endpoint
reported simulation and visual readiness.

The observation is in `output/development/builder-reserve-observation.json` with the
native worker policies and retained cache identities. Staging evidence is in
`output/development/builder-reserve-uav-stage.json`. These are build and operational
observations, not a replacement for the pending live release and headed GPU acceptance.

### Stable Initialization And Complete Gateway Inputs

Veoveo installer integration found two remaining render dependencies on Helm release counters.
The object-store initialization Job used the release number in its name. Gateway
bootstrap also used that number when its control-plane revision was omitted. The
gateway Deployment could additionally read a live ConfigMap while offline rendering
emitted an unresolved checksum. These inputs prevented one immutable render from
describing both preflight and installation.

Both Job names now hash their complete rendered specs. Chart and Helm metadata changes
preserve their names and Pod templates. Changes to a bucket, database, bootstrap
resource limit, image, or service-account reference produce a new Job identity. The
digest suffix survives Kubernetes name truncation. The gateway chart requires an
explicit public bundle revision and performs no ConfigMap lookup during rendering.

In the Bioma reference configuration, the prior revision covered only `gateway.json` while the mounted ConfigMap also
contained five public key files. The declared revision now covers all six files through
the existing `veoveo.io/gateway-activation/v1` length-prefixed encoding. The shared
deployment contract owns that digest function. Acceptance hashes the actual rendered
ConfigMap and compares the declared value; no Secret values participate.

The Rust Helm configuration harness checks the actual chart at Helm revisions 2 and
29, different chart metadata, 53-character release names, individual Job input changes,
an unrelated Console image change, and absent or malformed gateway revisions. It also
validates both disposable profiles. Completed Jobs retain their one-hour TTL; a later
Helm upgrade can recreate a Job after that cleanup. Component selection must still
prevent an unrelated upgrade from being invoked. These changes have rendered acceptance
and await live activation of Veoveo with the reference configuration. The chart,
digest contract, and installer remain shared Veoveo components; Bioma supplies one
installation configuration used for acceptance.


## Native Component Selection Delivery

The disposable Veoveo installer now accepts exact component IDs or an explicit full
installation. Dependency expansion happens before source checkout and rendering. Every
selected namespace, bootstrap, allocator, claim, public configuration, gateway activation,
and source Helm unit enters the same checked mutation plan. The returned receipt lists
actual applied and reused units, with before/after unselected object and Helm observations.
Enterprise installations continue through their declared GitOps owner; Bioma remains a
reference configuration.

The Rust `component-scope-verify` harness builds independent platform and extension
image revisions and invokes the real CLI against an isolated namespace. Platform-only
and extension-only updates each apply one release and reuse the namespace. The other
source repository is unavailable throughout installation. Native proxy metadata records
requests while unselected Deployment versions, Pod identities, image IDs, restart counts,
and complete Helm storage metadata stay unchanged. An ownership overlap fails before
any write. The namespace is removed and its absence verified. Evidence and retained Git
fixtures live under `output/development/`; the recorder binds the check to build inputs.

This fixture establishes operational scope. Its synthetic extension declaration does
not qualify extension-manifest content, and its sleep Pods do not qualify GPU execution.
General raw adoption, actual GPU compiler-family admission, and independent build-host
comparisons remain separate open work. Cluster coordination is described below.


## Task Runtime Feature Closure

The Bazel trial exposed an unnecessary dependency in the existing Cargo build.
`platform/task-runtime` used a direct path dependency on the MCP contract, which enabled
its default analytics feature. Task lifecycle and subscription code uses the protocol
surface only. The dependency now uses the workspace declaration, whose default features
are disabled. Actual analytics consumers retain their explicit dependency behavior.

Native Cargo trees before and after the change show Stream dropping 21 package/version
pairs, from 629 to 608, with no additions. The removed graph includes DuckDB, its native
build script, and download/archive helpers. A regression resolves the production graphs
of Stream and Reason and requires both to retain the task runtime without either DuckDB
crate. This check uses Cargo's requested feature graph; the image input planner's
all-feature metadata remains conservative for source discovery. Rust tests and strict
Clippy cover all targets of the task runtime and both servers. This is dependency and
compilation evidence, not a measured wall-clock speedup or new GPU runtime admission.

The normal Stream image path successfully stages revision `c91a88d4` in 79.498 s,
including a 69.690 s compilation phase window and publication to the development
registry. Cargo reports 67 s for its optimized build; the native GStreamer runner also
builds successfully. Repeating that exact revision takes 4.648 s with both compilation
actions cached. Both receipts retain runnable digest
`sha256:e71b033b9d96372bda605b2d6854ba0c8ed7382890c412e27f3e8bf6021ea708`.
These are current existing-worker checkpoints, not a matched before/after measurement
of the dependency change. The staged image has not been deployed or GPU-qualified.
Receipts are `output/development/stream-feature-closure.stage.json` and
`output/development/stream-feature-closure-warm.stage.json`; adjacent logs identify the
complete command and solve records.

## Common Compiler Runtime Admission

The Bookworm Stream candidate now passes initialized service readiness inside the
installed DeepStream container. The Rust probe supplies the App HTML from the
candidate's source revision, checks the executable through `/proc/<pid>/exe`, and
binds the accepted HTTP listener to that process's socket descriptor. Its
`startup_verified` receipt identifies the RTX 4090, runtime digest, binary and App
digests, and Pod UID. The additional process and files are removed. The installed
Deployment specification, Pod identity, runtime image, and zero restart count remain
identical. This is service evidence, with zero GPU workload frames claimed.

The authenticated recording attempt reaches task execution, then fails before the
GPU runner. `platform/recordings/video/src/lib.rs::materialize_video` calls
`RecordingReader::materialize_analysis_snapshot`, whose existing implementation
unconditionally rejects reads without a fresh Artifact-read credential. Both Stream
and Reason also construct that reader without a layer cache. Their durable execution
paths need the credential-bearing reader boundary and bounded cache before replay can
qualify either compiler. A bearer token must not be persisted in the durable request.
Inspection also exposed a cache prerequisite: a warm recording layer skipped Artifact
authorization. The cache now rechecks current Read permission through metadata before
validating or pinning local bytes. A native HTTP regression reproduces access inherited
by another caller before the fix, then covers revocation, expired credentials, missing
occurrences, inconsistent metadata, and unavailable authority. This correction does
not supply durable read delegation or qualify a GPU compiler family.
The failed task is `01a080f8-7474-7f31-a640-acb90aaacf4e`; its candidate log is retained
under `output/development/stream-compiler-acceptance/`. It supplies no GPU acceptance.

The smoke harness now selects an installed pipeline explicitly and takes the recording
tenant and Work Context from its environment. Its sample comes from the installed
Stream image. Candidate startup fails promptly when the process exits, and private
producer key files are removed even when a test fails. Stream and Reason scenarios
no longer select the unused conformance CLI, which had expanded Cargo's feature graph
and triggered another dependency compilation during this verification.

Reason has an additional runtime constraint. Inspection of its pinned vLLM image
confirms that `MultiModalConfig.validate_mm_processor_device` rejects CUDA preprocessing
in an instance that also runs the language model. Its supported accelerated processor
configuration requires an encode-only producer and a device-tensor transport.
[vLLM documents that boundary](https://docs.vllm.ai/en/stable/configuration/engine_args/#--mm-processor-device).
Replacing the existing CPU decode, resize, and PNG path therefore requires an admitted
GPU model-input design, including memory ownership; enabling the installed NVIDIA
decoder alone is insufficient. No Reason GPU inference was executed in this inspection.
Version and source hashes are in `output/development/reason-gpu-admission/inspection.json`.

The production compiler families remain separate. The initialized Stream receipt is
`output/development/stream-compiler-startup/compiler-candidate.json`, and
`output/development/common-rust-abi-experiment.json` retains the aggregate unadmitted
status. These findings narrow the remaining implementation work without weakening
the service, GPU, or durable-credential requirements.

After task-read consumer integration, the compiler probe supplies a private recording
cache under its generated temporary path. It preserves the installation's cache byte
and free-space limits while replacing the cache directory in either supported CLI
argument form. The v2 probe receipt requires verified removal of that cache alongside
the candidate executable and App. This lets the current binary start without sharing
the installed worker's cache or requiring a chart rollout to create its default mount.
Recording workload acceptance still requires the matching Artifact read API and schema.

## Bazel Integration Experiment

The September 8 trial now builds Stream's Rust executable, native GStreamer runner,
and OCI image with Bazel 9.2.0, rules_rust 0.74.0, rules_foreign_cc 0.15.1,
and rules_oci 2.3.3. It uses the existing Rust 1.97.1 DeepStream SDK environment.
The prototype remains in a detached disposable worktree. Production recipes and
compiler families remain unchanged by the experiment.

The first five probes failed on Cargo workspace metadata and dependency resolution.
Git crate extraction lost inherited package metadata. Whole-workspace resolution also
included features outside the selected production graph. The corrected resolver derives
external dependencies from Cargo, with separate runtime and build-tool feature contexts.
Its Linux normal/build tree matches all 612 external package-and-feature combinations
of the native Stream tree. Two package-metadata patches retain the pinned MCP SDK fork's
inherited metadata without changing dependency source code.

Native integration required the SDK's installed pkg-config and C++ compiler. Rust build
tools use [Cargo's release build-override settings](https://doc.rust-lang.org/cargo/reference/profiles.html#build-dependencies),
and runtime crates retain release optimization. The trial also separates Rust's code-generation count from Bazel's action
CPU reservation: reserving sixteen CPUs per action serialized work on the twelve-CPU
worker. Rust linking uses the SDK's BFD linker. This is an experimental recipe, not a
claim of byte-identical output between Cargo and Bazel.

The first complete diagnostic build took 393.128 s inside Bazel and included cached
work. It is not a cold-build measurement. Its image contains the exact two built binaries,
with executable permissions, and the read-only App asset. All 22 inherited runtime layer
descriptors match the declared base image. The exported evidence includes the resulting
manifest, configuration, and application layer; it is not a complete standalone OCI
layout or a release-qualified image. Service and hardware GPU qualification are separate.

### Controlled Cache Cases

Every case uses the same declared twelve-CPU SDK environment. The cold case starts
with empty Bazel workspace, repository-download, and disk-action caches; the SDK's
BuildKit layers are already present. Subsequent cases deliberately invoke Bazel in
batch mode to expose its own reuse behavior. The separate wrapper case allows BuildKit
to reuse the complete finished action.

| Case | Complete command | Execution |
|---|---:|---|
| Cold Bazel caches | 450.690 s | full Rust, native, and image build |
| Unchanged Bazel invocation | 47.100 s | one internal workspace-status action; no compiler or assembly process |
| Rust and C++ source edit on the existing workspace | 51.260 s | two Stream Rust actions, one CMake action, and three image-assembly actions |
| Restore unchanged source into an empty Bazel workspace | 95.790 s | all 1,554 executable actions hit the disk cache, including 780 Rust compilations |
| Apply the same edit after restoration | 141.420 s | the same six actions as the existing-workspace edit; dependencies stay cached |
| Reuse the complete BuildKit wrapper result | 4.000 s | Bazel does not execute; its prior output and evidence are reused |

The baseline action cache is copied before either source edit. Its 6,746 files contain
13,623,743,859 bytes. Source and copied cache inventories have the same content digest;
copying and verifying them takes 60.530 s. The restored workspace shares the repository
download cache. This measures a fresh Bazel workspace on the same host and SDK worker.
An independent BuildKit worker, second host, and network transfer remain outside this
trial.

Both edits append the same controlled comments to the Rust entrypoint and C++ source.
Execution logs prove that only the two Stream Rust targets recompile. The native runner,
Rust executable, and complete image manifest match byte for byte between the edited
cases. Cold, unchanged, restored-unchanged, and wrapper-reused artifacts also match.
Image checks verify the embedded binaries, runtime file permissions, App bytes, and all
22 inherited layer descriptors. The cold Rust executable passes CLI help with a cleared
environment. Service startup and hardware workloads remain separate acceptance.

The restored source edit spends 72.303 s staging cached inputs before OCI image assembly.
The trace identifies an inherited runtime blob during that interval. Restoring completed
compiler actions therefore leaves a substantial image-input cost in this recipe. The
47-second unchanged invocation also retains batch startup, module evaluation, and cache
inspection. These observations support keeping the corrected Cargo/BuildKit production
path. They establish no matched Cargo-versus-Bazel speedup, and do not describe the
latency of a persistent Bazel server.

`output/development/bazel-stream-trial/comparison.json` records validated cases, exact
source variants, manifest and binary digests, cache snapshot identity, execution counts,
and cleanup. Adjacent files retain the native Cargo feature comparison, prototype input
archive, Build Event Protocol records, profiles, and detailed spawn logs. Spawn logging
was enabled after the cold case; its empty-cache receipt and execution counters preserve
that case's boundary. The exported image evidence contains the manifest, configuration,
and application layer, while inherited blobs remain available from the declared base.

Cleanup removed all experiment execution caches and preserved all fourteen ordinary Cargo
cache identities. The final three mounts accounted for 53,217,423,360 bytes. Source edits
are restored in the detached worktree. The worker retains its declared twelve-CPU,
36 GiB limits without swap, and all 25 Veoveo Deployments remain Ready. Earlier cleanup removed
the first-generation caches (6,822,387,712 bytes) and diagnostic second-generation caches
(43,417,825,280 bytes). The normal Cargo retention command also reclaimed 16.33 GiB of
superseded executables and incremental variants while retaining dependency libraries
and current executable links.

## Focused Flight Build Boundary

The composed-flight commands now build `veoveo-flight-smoke` and the existing
conformance client. They no longer select the broad smoke package. The flight
workflow still performs world admission, authenticated Stream and Recording checks,
Reason tasks, concurrent NVIDIA workload checks, and headed Console checkpoints.
Its scenario validation and domain assertions are separated into focused modules.

The former Stream service dependency supplied four live-session response types.
Those types now have one pure source module owned by Stream. Both the server and
flight client compile that module; the verifier no longer links the server's task,
recording, or database implementation. Browser code and showcase checkpoint code
were moved byte-for-byte. All eleven existing domain regression tests remain.

A Cargo graph test checks the flight client alone, its exact combination with the
conformance helper, and the independent browser client. It rejects any service
implementation, SurrealDB, DuckDB, or Rerun dependency. Compilation and dispatch
measurements are retained under `output/development/flight-iteration/`. They do not
constitute flight or GPU inference acceptance.

A comment-only verifier edit compiles only `flight-smoke` in 2.66 s; the recorded
Cargo command takes 2.72 s end to end. The unchanged baseline performs no compilation. Three warm runs through `cargo xtask smoke
uav-showcase-up` take 0.883, 0.874, and 0.901 s from command entry to typed scenario
rejection. The deliberately absent scenario prevents cluster or credential activity;
this measures build and dispatch cost. It does not measure flight execution.
`comparison.json` records the source build digest and `dispatch.json` records each
sample. The first test attempt inherited Cargo's package environment and caused
spurious Ring rebuilds; the corrected test uses the repository's existing parent
Cargo environment cleanup before starting the measured command.

## Disposable Installer Coordination

The profile runtime now acquires `kube-system/veoveo-profile-mutation` before
executing a prepared installation or teardown. Kubernetes atomic creation admits
one cooperating writer. The installer repeats ownership and installed-state planning
under the lock, checks it before selected operations, and releases it with UID and
resource-version deletion preconditions. The v2 installation receipt records the
released control object separately from component-owned application resources.

An incomplete execution retains the lock because a disconnected installer may have
left a mutation child running. Recovery requires establishing that the original
processes have stopped and conditionally deleting that exact Lease. There is no
timeout takeover. This coordinates profile commands; enterprise application ownership
continues through GitOps.

The native contention test admits one of two simultaneous callers, rejects a stale
holder's deletion against a replacement, and verifies retention after interrupted
execution. The complete component-selection fixture passes in both update directions.
Its observer sees exactly one lock acquisition and release per successful update,
while the unselected Deployment, Pods, and Helm storage remain unchanged. An ownership
overlap still fails before any API write. Both fixtures verify namespace cleanup.
Evidence is recorded in `testing/local-test-report.json`; the detailed scope trace is
`output/development/component-scope-coordination-final-20260908.json`.

## Acceptance Status After Implementation

| Authorized boundary | Delivered evidence | Remaining acceptance |
|---|---|---|
| Flux triggers and unchanged workloads | Active watch labels, immutable chart/value inputs, metadata-only rollout with unchanged Pods, native cancellation regression | Complete for the measured reference installation |
| Builder resources and durable storage | Enforced twelve-CPU budget, 2.29× matched compiler speedup, cache retention, restored disk reserve | Complete on available infrastructure; separate physical disk and host comparisons are deferred by user direction |
| Presentation and normalized GPU dependency inputs | Frontend-only staging executes no Rust; both source-only UAV revisions meet the thirty-second target; headed Console refresh passes | Complete for those input boundaries |
| Exact staging and elapsed timing | One selected solve, per-target identities, command-level preparation and failure timing | Complete |
| Reusable Rust compilation | Shared ordinary compiler families, extracted recording libraries, focused harnesses, cache/build-engine trials, and active Stream/Reason control compiler with hardware replay | Both migrations are verified. Stream Rust edits stage in 26.2 s; Reason Rust and Python edits stage in 32.6 s and 11.1 s |
| Independent release inputs and selected execution | Per-release lock projection, selected publication, local UI loop, native zero-unselected-write evidence, cooperative cluster lock | Complete for the existing independently owned releases; general ownership transfer is outside this build/deploy goal |

The experiments support retaining Cargo and the durable BuildKit worker. A build-engine
migration has no demonstrated overall advantage from the measured local cases.

Future performance tests need a separate physical build disk or an additional Linux
host. Repeat the matched source-edit workload with durable Cargo caches, record the
hardware and storage identity, and compare total staging time while the GPU workload
stays active. Cross-host transport and the Bazel prototype on an independent worker
also remain unmeasured. These experiments are follow-up work, not release gates.

## Durable Read Prerequisite Checkpoint

Artifact task-read delegation now has typed service, client and Store APIs, with
explicit expiry, revocation and atomic input quotas. Current Artifact grants and
labels are checked for every read. Work Context policy content binds the delegation;
metadata-only timestamp changes preserve it. The component contract lives beside
the [Artifact service](../platform/artifacts/service/DESIGN.md).

This change addresses the authority prerequisite discovered during common-compiler
workload acceptance. It is not a measured build speedup or an activated replay path.
Stream/Reason task-state and recording-cache integration remain separate work, as
does Reason's hardware-accelerated model-input path. Those application changes grew
the investigation beyond the already measured build and deployment improvements.
Their incomplete acceptance must not obscure the delivered iteration checkpoints.

`DEPLOY-SCOPE-023` requires splitting a Helm release only when it mixes independently
selected owners. The current reference installation already has independent releases.
The earlier acceptance checklist incorrectly made general raw adoption and ownership
transfer a completion requirement. Veoveo remains the product under improvement;
Bioma supplies its reference installation configuration and GitOps owner.

## Task Read Consumer Integration

Stream and Reason now issue and persist a bounded Artifact read capability, recover
that task binding, and pass it through the shared video materializer. The reader
requires a cache and explicit read authority. It checks current task scope before
catalog access, including live-only recordings, and matches that scope to the stored
task owner. The source byte ceiling applies before live-part copies. Each committed
occurrence is authorized again during cache reuse or download.

The shared Veoveo chart supplies separate persistent caches for both workers and
passes their managed-byte and free-space limits explicitly. Cancelling an in-flight
download now releases its reservation and removes the partial file. Invalid persisted
request documents fail their claimed task instead of aborting service startup.

These changes remove the identified missing-credential and missing-cache paths.
The activation and Stream hardware acceptance below exercise that integration.
Common-compiler image admission and Reason's accelerated model-input path remain
outstanding. The integration is not by itself a measured compiler speedup.

The consumer test build created another combined dependency variant. Scoped hourly
Cargo maintenance removed 26 superseded executables and 245 incremental variants,
with 29.55 GiB of estimated reclaimable blocks. Current executable links and dependency
libraries remained protected. Host availability returned to 386 GiB; the exact
maintenance receipt is `output/development/task-read-cargo-cache-applied-20260908.json`.

## Task Read Activation

Revision `3e684ec9bd1350fdb96290906fd92aaa51f174b2` activates the committed read
integration in Veoveo with the Bioma configuration. Artifact, Gateway/bootstrap,
Stream, and Reason use qualified images from source `696cb8318c1e3003cc751b00621a939700da519f`.
The platform chart and its values move together through the existing Flux owner.
The bootstrap Job succeeds with the existing gateway control-plane revision, and
all 25 Deployments become Ready. Each of the five replacement service Pods has zero
restarts. This establishes activation and service readiness; hardware replay remains
a separate acceptance step.

The exact four-image staging command takes 532.2 s. Qualification takes 179.5 s and
preserves every staged runnable digest. The passive rollout observer starts before
the Git push and succeeds in 108.4 s, including 50.6 s waiting for the source revision.
These command and observer durations describe different boundaries and are not
combined into a push-to-ready benchmark. Receipts are retained as
`output/development/task-read-activation-{stage,release,convergence}-20260908.json`.
The selected chart bundle is under
`output/releases/helm/696cb8318c1e3003cc751b00621a939700da519f/0.1.0-696cb831/veoveo/`.

Cleanup also removes the unused local Stream and Map images from earlier builds and
the node's unused image manifests after rollout completion. Running containers keep
their image references. A further hourly Cargo pass reclaims 0.89 GiB of superseded
incremental output. The raw cleanup receipts are retained under
`output/development/local-image-cleanup-20260908.log`,
`output/development/node-image-cleanup-20260908.log`, and
`output/development/pre-activation-cargo-cache-applied-20260908.json`.

## Stream Common-Compiler Hardware Acceptance

The current combined Bookworm build uses source
`696cb8318c1e3003cc751b00621a939700da519f`. Cargo reports 6 min 25 s for its one
Stream/Reason compilation action; the completed build command takes 396.6 s.
This cache state differs from the earlier standalone builds and provides no matched
speed comparison. BuildKit metadata and the command log are retained as
`output/development/common-rust-696cb831.buildkit.json` and
`output/development/common-rust-696cb831.log`.

The Stream candidate's SHA-256 is
`b92478bf9c83ec63357c6ec1949eac41f984c8087677e4f0da615bf94c84f09a`.
It completes task `01a081a6-d39a-7ca2-9c08-7e322751b34f` through the installed
TrafficCamNet replay pipeline on the RTX 4090. The task processes 55 frames and returns
277 detections, with results, annotation, and source-clip artifacts. The native probe
verifies its exact executable and HTTP listener. Its cleanup removes the process,
App, executable, and private recording cache while preserving the installed
Deployment, Pod UID, runtime digest, and zero restart count. The hardware receipt is
`output/development/stream-common-gpu-696cb831/compiler-candidate.json`.

The acceptance harness now reads the producer key from an explicitly named Kubernetes
Secret into a private temporary file. It removes the file on failure as well as success.
The test assertion uses the installed Work Context's current policy revision. The
local recording tenant was corrected to match the Bioma configuration. Earlier attempts
stopped during setup with an empty local key and a stale tenant selection; neither
attempt supplied GPU evidence.

This receipt admits the tested Stream executable/runtime pairing. Production Bake
still selects the existing compiler families, and Reason's candidate has no hardware
workload receipt. No common-family migration or Reason inference acceptance is claimed.

## Stream Compiler Separation

Stream now consumes the common scratch-artifact recipe through a Bookworm control
family. Its Cargo feature selection stays separate from Map's analytics family. The
DeepStream stage builds only `stream-gst-runner` and mounts only its native source
directory. The runtime combines that result with the Rust artifact and its separately
packaged App. Reason retains its existing compiler until its own hardware gate passes.

The compiler uses stable Rust 1.98.1. The official Docker catalog still publishes
1.98.0, so the pinned Bookworm image supplies the environment and rustup; the recipe
installs 1.98.1 and removes the bootstrap toolchain before compilation. The resulting
executable needs glibc through 2.34, while the installed DeepStream runtime has 2.39.
Its direct libraries are libc, libm, libgcc_s, and the amd64 ELF loader.

The first cache population takes 506.5 s and compiles 575 packages. Initial warm
source edits take 19.5 s and 17.6 s. After correcting the inherited compiler version
metadata, the final controlled edits take 16.7 s and 15.9 s. Each measured action
compiles only Stream, with identical output bytes across its pair. This measures
the new iteration path; it does not isolate a speedup against the old SDK compiler.
The native CMake build completes successfully, with its compile/link action taking
2.9 s. Evidence is retained under `output/development/stream-separated-final-20260908/`
and `output/development/stream-separated-native-20260908.{json,log}`.

The unmodified-source binary has SHA-256
`ec7cec73075fceecdda35d4ca3eaf9f8df265b5b103efe95dad5667c394eda9a`.
Its RTX 4090 replay processes 17 frames and returns 81 detections with results,
annotations, and a source clip. The native probe verifies cleanup and preserves the
installed Deployment, Pod identity, runtime digest, and restart count. The receipt is
`output/development/stream-separated-gpu-20260908/compiler-candidate.json`.
This admits the tested Rust executable in the existing runtime; publication and
activation of the complete new image are separate checks.

Committed source `75347594985a0b511101b0a6d8e1e4fb87f2e91d` stages in 10.6 s.
Qualification takes 66.1 s and preserves runnable manifest
`sha256:14256c57b1bbf66fed0427669c23b68dc5635c82d7f9086aec55f25feee618ae`.
An isolated committed Rust edit, `2121a7c84ffac6372e8285cd8ee0112585e9caaf`,
stages in 26.2 s end to end, including 18.8 s of Rust compilation. It compiles only
Stream and reuses the CMake action. The benchmark revision is not the deployment
candidate. Stage and qualification receipts are retained as
`output/development/stream-separated-{stage,release,edit-stage}-20260908.json`;
their logs identify the complete command timing records.

Before the new build, scoped Cargo cleanup reclaimed 19.16 GiB of superseded
executables and incremental variants. Current executable links and dependency
libraries were retained. The subsequent preflight passed with 10 GiB of projected
growth above the host's 20% reserve, and the Kubernetes node reported no disk pressure.

## Stream Image Activation

Revision `85bc3807f8b2ec9b6a0245142ea72655284e5b0e` changes only Stream's image
identity in the Bioma reference lock. The native configuration check now classifies
Stream as a shared-artifact consumer. Helm configuration checks, strict Clippy, and
formatting pass before activation. Flux continues to own the application release.

The Git push starts at `2026-09-08T16:25:12.831912286Z`. Passive observation begins
0.553 s later, during the push, and verifies the exact revision at
`2026-09-08T16:26:42.889Z`: 90.1 s from push start. The new Stream Pod is created at
16:26:17 UTC and becomes Ready at 16:26:20 UTC. Only the Stream Deployment changes;
the other 28 running Pods preserve identity, image identity, and restart count.
All 25 Deployments are Ready. The observer receipt and before/after observations are
`output/development/stream-separated-convergence-20260908.json` and
`output/development/stream-separated-{before,after}-20260908.json`.

The installed Rust executable matches the accepted candidate digest. GPU replay
through the installed service completes with 17 processed frames, 81 detections,
and published result, annotation, and clip artifacts. This run uses the complete new
image, including its newly built native runner. Its log is
`output/development/stream-separated-installed-gpu-20260908.log`.

The baseline and isolated source-edit images have 24 filesystem layers. Only the
Rust executable layer changes; the native runner and runtime layers remain identical.
Both OCI manifests are retained as
`output/development/stream-separated-{stage,edit-stage}-manifest-20260908.json`.

After activation, four exact reclaimable compiler-cache mounts were removed: the
retired SDK Rust target and download caches, and the completed 1.97.1 common-compiler
experiment target. BuildKit reports 8.387 GB reclaimed. The current control compiler's
three Cargo mounts remain present. The host has 381 GiB free, and the final preflight
passes with 10 GiB of projected growth above the retained reserve. No registry image
or Kubernetes volume is removed. The inventory, cleanup log, and final preflight are
under `output/development/stream-retired-cache-*20260908.*` and
`output/development/stream-separated-final-preflight-20260908.log`.


## Reason GPU Input Acceptance

Reason's runner now demuxes exact presentation timestamps without decoding on the CPU.
NVDEC exports device RGB surfaces and CUDA resizes the observations. The internal
Qwen3-VL adapter reuses the already loaded vLLM vision tower, including its deepstack
features, then submits those GPU embeddings through the supported precomputed-image
input. The offline engine and single worker share the runner process. Explicit
observation IDs avoid tensor-content hashing, and the configured engine budget
reserves decoder and observation memory before inference begins.

The native candidate probe now supports both Stream and Reason. It packages Reason's
runner source into an executable archive and records its digest beside the Rust
executable digest. Both candidates retain private listeners, isolated process groups,
and temporary recording caches. The probe verifies their removal and the original
installed Pod's identity, runtime identity, readiness, and restart count. The Reason
smoke consumes the installed catalog and checkpoint; its obsolete requirement for
host-side `REASON_CONFIG_DIR` and `REASON_MODEL_DIR` copies has been removed.

Candidate task `01a0820a-6711-7281-b72f-6721061d8b41` processes six frames on the RTX
4090 and publishes typed reasoning results and a Rerun annotation artifact. The
runner reports 123.9 s including model initialization. Rust executable
`2fe11129dfbc9448356ec1479b479768ad6bb81a0aa222ed023b23e797b904a9`
comes from the existing Bookworm compiler experiment at source `696cb831`.
The installed vLLM runtime remains
`sha256:b7f2a35a670b7f723f22f0d1945f3c150b2a44c272130abdc9e0b7cc3dc458c2`.
The receipt is `output/development/reason-gpu-admission/candidate-02/compiler-candidate.json`.

This proves the candidate executable and new GPU input adapter in that installed
runtime. It does not qualify a newer runtime image or activate Reason's shared
compiler family. Those publication and activation steps remain required.

## Repeatable GPU Recording Fixtures

The video fixtures now wait for the forwarder's complete upload and the Hub's
materialized sequence before submitting analysis. Catalog creation alone did not
establish a readable source. Each fixture requests graceful forwarder shutdown after
its task completes, which finishes ingestion through the normal producer API.

That shutdown exposed an ownership error in the forwarder: retaining the gRPC proxy
handle kept its receiver open, and joining the receiver before draining its bounded
channel could deadlock. Shutdown now releases the handle and drains the channel before
joining. The successful Stream candidate run processes 71 frames and returns 360
detections, then finishes its producer and removes its private candidate process.
Its receipt is under
`output/development/reason-gpu-admission/stream-candidate-live-drain/`.

Three explicitly selected stale video-test ingest streams were completed through the
authenticated producer API. The native `recording-fixture-finish` command accepts exact
stream IDs and rejects recordings outside the video-test application. Existing catalog
records and recorded bytes remain intact.

An additional experiment finished ingestion before requesting analysis. The forwarder
exited successfully, but task `01a0822a-c697-7a50-b7a0-580b79510fb7` failed with
`authorized segment set does not contain the requested Rerun recording`. The cause is
unresolved. Its diagnostic log is
`output/development/reason-gpu-admission/stream-candidate-drain-fixed.log`.
GPU compiler acceptance retains the smoke's original acknowledged live-snapshot
workflow; it does not establish that sealed-recording reads pass.

Final recorded acceptance processes six Reason observations in task
`01a08238-c77a-74b0-8052-de68246b54f0` and 75 Stream frames with 383 detections in
task `01a0823a-234d-75c2-b6cf-ece21f060f4c`. Both finish ingestion and verify candidate
cleanup. The source-bound test report also records Python and Rust unit checks,
forwarder tests, smoke dispatcher checks, strict Clippy, and formatting. Receipts and
logs use the `final-reason-01` and `final-stream-01` prefixes under
`output/development/reason-gpu-admission/`. All 25 deployed services remain Ready.

Scoped host maintenance removes another 2.24 GiB of superseded incremental variants.
Its exact removal record is
`output/development/reason-gpu-admission/cargo-cache-applied.json`.

## Reason Compiler And Runtime Assembly

Reason now selects `rust-bookworm-control-v1` alongside Stream. The planner keeps
that Cargo action separate from Map's analytics features. Runner manifests and Python
source belong to the declared image asset context, which removes them from Rust
compilation inputs. The runtime recipe installs hash-locked Python additions from
separate manifest mounts and packages source in a later executable-archive layer.

The recipe pins [vLLM 0.28.0](https://github.com/vllm-project/vllm/releases/tag/v0.28.0)
and [PyNvVideoCodec 2.2.2](https://pypi.org/project/pynvvideocodec/2.2.2/), verified as
the current stable releases on September 8. The vLLM index is
`sha256:61fc8a896b0a4fbbbdc063bc4b0dbc25ce98e02b5050c24aeb7830ac02039b14`.
Its CUDA 13.0.2 base uses Ubuntu 24.04. The runner retains the parent's matching
Torch and Transformers libraries.

Planner and asset-boundary tests, Python unit checks, Helm configuration, strict
Clippy, and formatting pass. This checkpoint changes the build recipe; the new full
runtime image still requires staging, hardware acceptance, qualification, and GitOps
activation. The installed Reason image remains unchanged.

Before the cold runtime build, cleanup removes the forty exact cache records in the
retired vLLM base/SDK chain and its three Cargo mounts. The ordinary and current
control-family Cargo caches remain. Plans and removal logs use the `retired-vllm-cache`
and `retired-reason-cargo` prefixes under `output/development/reason-gpu-admission/`.

The first image stages in 318.5 s, including 44.2 s of Rust compilation and a
182.9 s extraction window for the new GPU base. Rust benchmark revision `d92a1095`
stages in 32.6 s. Python benchmark revision `d6719d9a` stages in 11.1 s with no
Cargo execution. Each image has 37 filesystem layers: only layer 35 changes for the
Rust edit, and only layer 34 changes for the Python edit, using zero-based positions.
The benchmark revisions are isolated and are not deployment candidates.

Qualification of source `19d005ec` takes 143.9 s and preserves runnable manifest
`sha256:0b559467373b4a2669d6f3b044da2f323c7db99b5b8bdb0a87f5863d680d5166`.
The SBOM phase takes 49.3 s. Stage, release, benchmark, and manifest records use
`output/development/reason-{shared,rust-edit,python-edit}-*20260908.*`.

The node's first pull requires a second local copy of the new GPU runtime. The
temporary build-cache eviction removes only the new Reason runtime's 42 regular
records; Cargo caches remain. The observed snapshot set occupies 27.2 GiB, and new
compressed blobs total 8.0 GiB before crediting reusable node snapshots. Activation
therefore budgets 37 GiB above the retained filesystem reserve. Native uv maintenance
also removes unused and downloaded wheel-cache entries; existing environments retain
their installed files. This storage transition leaves published rollback images and
application volumes intact.

## Reason Image Activation

Revision `b88fbd8b3842e9d8e0e575ab19e4842d6946eeea` updates only Reason's image
identity in the Bioma reference lock. Unrelated incoming documentation commits are
retained. Flux reaches verified readiness in 190.2 s from the successful push start.
The new Pod uses the qualified runnable digest and has zero restarts. All other 28
running Pods retain their UID, image identities, and restart counts.

Installed task `01a08255-9b6b-7c81-8814-e09a726aa7c7` processes six observations on
the RTX 4090 and publishes typed reasoning and annotation artifacts. The complete
smoke command takes 191.4 s, including its wait for the image rollout. The installed
runtime reports vLLM 0.28.0, Torch 2.13.0+cu130, Torchvision 0.28.0+cu130,
Transformers 5.15.1, PyNvVideoCodec 2.2.2, and PyAV 18.1.0. All 25 Deployments are Ready.
Convergence and GPU logs are retained under
`output/development/reason-shared-{convergence-final,installed-gpu}-20260908.*`.

After hardware acceptance, the superseded Reason image is removed from the node cache.
The first CRI call times out during collection; a subsequent image inventory proves
the old image absent and the filesystem regains its space. Its published registry
manifest remains available for rollback. The node inventories and removal log are
under `output/development/reason-gpu-admission/`.

## Runtime Packaging After Cache Eviction

Restaging the accepted source after cache eviction takes 323.4 s and exposes
non-reproducible Python packaging. Its runtime digest becomes
`sha256:2fdc5aeced2b9c255471e5bc5839578148e17a0ee8d0aaf411aaa1ca43cc4eb8`;
only the dependency layer and runner archive differ. The installed qualified image
is unchanged. This diagnostic restage is not activated.

The corrected recipe disables install-time bytecode and writes the executable archive
with sorted members, explicit file modes, and timestamps from `SOURCE_DATE_EPOCH`.
It also binds the system account's internal date to that epoch. Qualification must
rebuild the packaging layers after eviction and retain the staged runnable digest.

The first corrected archive remains identical after eviction, but the dependency
layer still differs. A file-by-file comparison isolates three pip HTTP cache metadata
files; all other layer entries match. The parent command's cache flag does not reach
its isolated build-dependency installer. The final recipe uses `PIP_NO_CACHE_DIR=1`
on the install command so subprocesses inherit the setting. The exact comparison is
`output/development/reason-gpu-admission/packaging-layer-differences.json`.

Source `2653ee034cf54310053ef34890f51c415ae380d1` stages in 25.2 s. After
evicting its packaging cache and the preceding recipe's packaging records, native
qualification rebuilds those actions in 124.6 s and preserves runnable digest
`sha256:4a9d2d06e77a5a5e8d1c6ee05c5126db0b3713b98cea3c7a772ca1a5806276af`.
The GPU base and Cargo caches stay resident. Receipts and logs use
`output/development/reason-cache-free-{stage,release}-20260908.*`; the exact
eviction plan is under `output/development/reason-gpu-admission/cache-free-*`.

Activation revision `7779dca4d8b781ce6710372b25daa40fffb22be0` selects that
qualified image through the Bioma reference lock. Passive Flux observation reaches
verified readiness 69.9 s after push starts. The new Reason Pod has zero restarts;
the other 28 running Pods retain their UID, image identities, and restart counts.
Installed GPU task `01a08271-5a36-7b22-babb-f1283f4b349f` processes six observations
and publishes typed artifacts. The complete hardware smoke takes 137.4 s, and all
25 Deployments remain Ready. Convergence, Pod snapshots, and GPU evidence use the
`output/development/reason-cache-free-*20260908` prefix.

The runner pins PyNvVideoCodec 2.2.2 for its admitted NVDEC input path. This overrides
vLLM 0.28.0's package metadata pin of 2.0.4, which pip reports during assembly.
Installed hardware acceptance establishes the supported Qwen3-VL path; it does not
claim compatibility with every upstream vLLM video integration.

## Final Public Acceptance

The September 8 completion check uses the deployed Veoveo installation at
`https://veoveo.bioma.ai`, with Bioma supplying its configuration and GitOps owner.
The native `bioma-verify` gate passes public HTTPS health, Console and authorization
surfaces, GPU workload scheduling, and a governed artifact larger than 8 MiB. Full,
HEAD, and ranged requests preserve exact content without redirects. Its 180.6 s
command includes a 145 s rebuild of the acceptance harness. The log is
`output/development/veoveo-public-final-20260908.log`.

The native `console-apps-browser-verify` gate passes all 16 first-party Apps in
103.3 s. It verifies the grouped authenticated catalog, each App's host bridge and
settled state, and headed hardware graphics. NVIDIA RTX 4090 WebGL supplies hardware
rendering; SwiftShader WebGPU is recorded as software and provides no hardware
evidence. The receipt and captures are under
`output/acceptance/veoveo-public-final/6aae356b673549fa5b0cdb3965013bf0a4f7fc73/01a08280-27c2-7d32-b2e7-df6ac4259791/`.

All 25 Deployments are Ready. The installed Reason digest still matches its qualified
release receipt and completed six-observation GPU task. The committed test report is
green and matches the current build inputs. Resource preflight passes with the node
Ready and `DiskPressure=False`. Unavailable infrastructure comparisons remain recorded
as future performance tests; they are not unfinished deployment acceptance.

## Artifact Upload Implementation Iteration

The upload implementation starts from `20327e93` on September 8. Its delivery tracks
compilation, test execution, image staging and qualification, and GitOps convergence
separately. Record avoidable repeated checks, cache misses, and unrelated Pod changes
beside their measured cause. Functional deployment remains the acceptance priority;
unavailable independent-storage and 100 GiB scale comparisons remain future experiments.

Initial local storage is 362 GiB available on the 1.8 TiB filesystem. `target/debug`
occupies 162 GiB and `target/veoveo-xtask` occupies 206 MiB. This is a capacity snapshot,
not a claim that all build artifacts are expendable. Preserve current cache families
and qualified rollback images when reclaiming obsolete outputs.

The first checkpoint limits test compilation to the shared contract. Shared type
changes necessarily invalidate downstream compilation; repeated full-workspace builds
before the contract settles would add churn without qualifying a deployable endpoint.

The first contract check passes 146 tests. Cargo reports 30.56 s of compilation;
test execution totals 0.31 s. The recorder measures 30.95 s, while log creation to
completion spans 54.72 s. About 23.8 s precedes recorded command execution, including
the quiet xtask build and evidence setup. This is a candidate for reducing tooling
coupling to shared runtime contracts. The subsequent formatting check takes 0.17 s
inside the recorder and 0.82 s overall. Evidence:
`output/development/artifact-upload-contract-20260908.log` and the committed test report.

Immutable blob acceptance passes 23 Artifact tests, including native eight-writer
concurrency and failed-publication rollback, in 10.6 s. Compilation accounts for
9.13 s and execution for 1.39 s. The Store's 46 unit tests take 12.8 s with 12.67 s
spent compiling. Three development attempts preceded acceptance: a test fixture
constructor mismatch, a conflict incorrectly mapped to a transport error, and a
native transaction race whose primary error was wrapped. The latter two are fixed
in publication error mapping and bounded transaction retry. Native database lifecycle
is now shared with the read-delegation fixture instead of copied into another harness.
Logs use `output/development/artifact-immutable-native-*20260908.log` and
`output/development/artifact-store-unit-20260908.log`.

Durable admission and schema upgrade pass 25 Artifact tests in 6.2 s, including
eight simultaneous matching admissions, a 10 GiB reservation, quota/concurrency
denials, current Work Context rejection, and migration of existing storage usage.
The reservation fixture transfers no file bytes. The final command spends 4.71 s
compiling and 1.39 s executing; its observed total is 6.92 s. Store unit validation
adds 11.0 s. Logs use `output/development/artifact-upload-admission-native-*20260908.log`.

This checkpoint exposed avoidable development churn. The beta SQL formatter removed
nested field separators; native syntax validation rejected its output. The migration
runner then hid the primary failure behind a generic transaction wrapper. It now
reports the migration name, failing statement index, and primary database error.
The migration handles an empty tenant collection explicitly, and replay compares
optional descriptor fields individually because absent stored fields and explicit
`NONE` values are not identical objects. These were implementation/debugging costs,
not evidence of slow production upload behavior. Preserve the failed development logs
for diagnosis; only current passing checks enter the committed evidence report.

Part accounting and the restartable storage adapter pass 29 Artifact tests, including
native lease races and unknown-length quota windows, plus 46 Store unit tests. The
final native execution takes 1.40 s. Tiny request frames coalesce into bounded chunks;
whole-object verification streams through a fixed-size digest state. Memory-backend
transfer checks establish adapter behavior, while real S3 transfer acceptance remains
part of integrated service qualification.

Two extra native test attempts came from SQL tooling and query naming: the beta
formatter damaged conditional updates, and `session` is a protected native variable.
Canonical queries now avoid the formatter and use explicit upload variable names.
One validation was started before its prerequisite formatter result was inspected;
subsequent formatting, validation, and execution are sequenced. These are avoidable
development costs, not production timings. The S3 client also had a 30-second total
read deadline that would interrupt slow large downloads. It now bounds connection
and idle time; upload parts retain independent operation deadlines.

Lifecycle acceptance passes 32 Artifact tests: compilation takes 13.84 s and execution
1.57 s. The new native cases cover equal-content concurrent publication, exactly one
receipt/audit per occurrence, cancellation while a part owns memory, stale finalizer
fencing, frozen manifest identity, and revocation before publication. One failed
compile preceded acceptance because native scalar query results require an optional
wrapper in the SDK. The shared publication builder and SQL fragment serve both ordinary
writes and uploads, avoiding a second implementation of blob registration and grants.
Logs use `output/development/artifact-upload-lifecycle-*20260908.log`.

The assembled upload service passes 34 Artifact tests. Its native transfer fixture
streams a 256 KiB object through bounded parts, reconstructs the service on another
replica, publishes a verified receipt, and reads the result through the ordinary
Artifact API. Cancellation of a pending body releases memory before the durable lease
expires; background cleanup releases retained quota. This fixture uses the native
database and an in-memory multipart backend, not installed S3 acceptance.

An extra `cargo check --lib` took 64.6 s before reporting three typed-ID constructor
errors. It compiled a separate dependency/feature graph, including SurrealDB, despite
the warm native test build. Prefer the known acceptance test target during this phase
to avoid warming an extra build graph that does not execute the feature. The first
assembled-service test also attempted to mutate an immutable gateway revision in its
fixture; the fixture now creates its typed upload policy in the original revision.
These costs remain development churn. Logs use
`output/development/artifact-upload-engine-*20260908.log`.

A bounded cleanup removed five generated test/binary executables older than seven days,
including their remaining Cargo hardlinks: 4.08 GiB of unique logical file data across
nine paths. No running executable referenced those inodes. Rust libraries, incremental
caches, and current acceptance executables remain. The deletion manifest is
`output/development/artifact-upload-cleanup-20260908.json`; filesystem availability is
353 GiB after concurrent compilation and cleanup. This remains below the image
builder's operating reserve, so obsolete builder materializations must be reviewed
before image staging rather than waiting for disk exhaustion.

Installed S3 acceptance transfers 33,554,451 bytes through RustFS and verifies the
whole-object digest. It covers orphan multipart cleanup, adapter reconstruction,
lost part acknowledgements, and completion replay. Compilation takes 9.91 s and test
execution 1.10 s; the recorder reports 11.1 s overall. The 35 Artifact library tests
also pass. This is storage integration evidence, not public HTTP, multi-GB, or
Console acceptance. Logs use `output/development/artifact-upload-s3-*20260909.log`.

At this checkpoint the upload experience is still not deployed. Backend implementation
and debugging account for the delay; the measured warm build is short. Completing
multiple backend layers before connecting the public API and Console delayed usable
feedback. The next checkpoint must connect those surfaces instead of adding another
isolated backend layer.

The HTTP checkpoint mounts the Artifact service transport and recovery worker, adds
Gateway policy enforcement, and connects the Console BFF through its existing session
and CSRF boundary. Native acceptance passes 36 Artifact tests, including a real HTTP
admission, raw-body part transfer, completion and receipt replay. It rejects ordinary
server assertions, foreign actors, caller-supplied authority, and mismatched part sizes.
Gateway and BFF binary suites pass 27 and 65 tests respectively; those suites establish
existing route behavior and compilation, while full proxy-chain acceptance remains a
deployment check. The Console queue UI is not part of this checkpoint.

The first Gateway/BFF test build takes 1 min 47 s while the two suites execute in
0.29 s. It compiles their wider dependency feature set, including SurrealDB, DuckDB,
and the MCP runtime. Reusing the resulting test graph avoids repeating that cold
dependency work during focused validation. Logs use
`output/development/artifact-upload-http-*20260909.log` and
`output/development/artifact-upload-proxy-compile-20260909.log`.

The Console queue adds a separately emitted hashing worker without another dependency.
Production bundling takes 1.73 s after TypeScript validation. Four new controller tests
cover wrong-file rejection before PUT, transmission of missing parts only, completion
winning cancellation, and receipt recovery without file access. Their first focused
run takes 186 ms overall. They use controlled HTTP/worker doubles and are not browser
or deployed transfer evidence. Lint caught a render-time ref assignment and a filename
control-character expression before qualification; both are corrected.

The queue and HTTP wiring are now implemented, while exact catalog sizes, Python
streaming consumption, installation policy activation, image publication, and deployed
large-file/browser acceptance remain required. Image staging has not started at this
checkpoint. Keeping that distinction explicit avoids counting local UI compilation
as a usable release.

Size projection inspection also found a live-stream replay inefficiency: the next
page reused the cursor captured before the pagination loop. A full page could be
read repeatedly. The loop now reads its advanced durable cursor for each page.
Snapshot blob metadata follows the selected occurrence window, and live occurrence
reuse hydrates an older missing blob directly instead of publishing a zero size.

An avoidable test-command change selected Gateway alone after the previous check
selected Gateway plus BFF. Cargo compiled another dependency feature graph. Keep
the package set stable across a checkpoint; changing the set is not inherently a
cheaper focused check. No production speed claim follows from these development runs.

Python consumption now uses the existing authenticated download endpoint and its
metadata header, avoiding a separate metadata request. The full SDK enforcement
passes 67 tests, builds its wheel and source distribution, and passes 15 isolated
template tests. Its first qualified run takes 14.2 s. Stream tests cover byte ceilings,
early exit, length mismatch, optional SHA-256 verification, and temporary-file cleanup;
the multi-GB metadata case rejects before reading and does not represent a transfer.
Logs use `output/development/artifact-upload-python-*20260909.log`.

The first staging command incorrectly selected `store-bootstrap` as an image target.
Bootstrap uses the Gateway image, so Bake rejected the name before compilation. The
correct runtime image set is `mcp-gateway`, `artifact-service`, and `console-bff`.
Staging restarted with that exact set from isolated revision `e588b397`. Installation
policy changes can proceed in the main checkout without mutating its build context.
Keep workload names distinct from image-owner names when planning affected targets.

At staging, RustFS contains 123 GiB on the same 1.8 TiB host filesystem, with 341 GiB
available. Its host-path PVC requests 10 GiB but does not impose a filesystem quota.
The explicit upload ledger quota governs artifact admission; independent storage and
large-scale capacity benchmarking remain follow-up infrastructure work.

The three runtime images stage in 104.9 s, including an 86.1 s compilation window.
Qualification reuses those staged artifacts and completes in 10.3 s without another
compilation. The committed installation policy validates, and the final GitOps/Helm
configuration check passes in 6.1 s. Image locks select qualified runtime digests;
the policy and image updates travel in one push. These measurements cover publication
and configuration checks, not rollout or public upload acceptance.

GitOps revision `6b647cc9` converges in 85.1 s: source fetch accounts for 62.3 s
and desired-state application for 21.4 s. A manual source reconciliation was also
requested; the observer itself remained passive. All three runtime digests match
the qualified lock. Bootstrap completes, with one initial Artifact recovery warning
before its new tables exist. Service health alone does not establish feature readiness.

Headed browser preflight accepts hardware graphics and the public upload policy,
then fails because the Console snapshot exceeds the BFF's 15 s upstream timeout.
The newly added blob `WHERE id IN (SELECT ...)` lookup scans the blob table with
a nested occurrence query. Replace it with a bounded set of direct record reads
from the already selected occurrences. Native acceptance must check missing blobs,
exact populated sizes, and foreign-tenant references before another Gateway rollout.
This regression should have been caught against a populated snapshot before the
first feature deployment; small typed projection tests did not exercise its query.

The correction's native database case and 94 Gateway/BFF tests pass in 2.0 s on
the warm graph. Building both consumers of the shared browser module also exposes
a missing `Duration` import in the existing process helper; the explicit import
restores compilation. The harness build then takes 6.6 s.

Gateway-only staging from `fbf44ae3` takes 68.3 s, including 59.8 s of compilation.
Changing from the three-target build to Gateway alone still recompiles shared Rust
crates. Cargo freshness reports 578 changed paths, showing that target selection
changes the compilation input closure despite the small source edit. Stable Cargo
feature sets across affected-target builds need a separate controlled measurement.

Before/after Pod UID comparison confirms that the first rollout replaces Artifact
service, Console BFF, both Gateway replicas, and bootstrap. Other Pod identities,
including all GPU workloads and object initialization, remain unchanged.

The bounded snapshot correction converges in 48 s, including 25.5 s for source
fetch and 21.2 s for apply. The public Console then renders the upload controls.
The real 10 GiB browser test passes selection, duplicate suppression, pause/reload,
wrong-file rejection, and navigation during transfer. It preserves 3,523,215,360
accepted bytes before authentication recovery fails. This is partial-transfer
evidence, not a completed multi-GB acceptance result.

The stalled request trace includes HTTP/3 errors, 125 s part timeouts, and a rejected
Console refresh followed by HTTP 401. Successful parts near the stall take 19–23 s.
A long part can arrive with a cookie whose refresh token a concurrent short request
already consumed beyond the five-second delivery window. The correction keeps
refresh rotation on short control requests. Part requests neither rotate refresh
tokens nor clear a newer browser cookie. A retry checks current authorization before
sending more bytes. This mechanism fits the observed failure; the trace alone does
not establish every ingress delay or the gateway's exact revocation reason.

The observing harness also imposed a hard-coded 25-minute deadline inside its
configurable timeout. It now has one configured deadline and a resume command that
reuses the original file and saved upload identity. Repeating the entire 10 GiB
transfer would discard useful accepted work. During the partial transfer, Artifact
uses about 70 MiB, Console BFF 26–27 MiB, Gateway 38–39 MiB, and RustFS 444–470 MiB.
These are sampled working sets, not controlled peak-memory measurements. SurrealDB
CPU varies; the samples do not prove that it limits transfer throughput. The host
still has about 330 GiB free.

Moving the follow-up from an isolated worktree exposed another local coordination
mistake: its node_modules symlink was accidentally staged, and applying that patch
failed partway through. The preserved source worktree restored the intended files;
the dependency symlink was removed from its index. No deployment used that partial
tree. This extra churn belongs to source coordination, not compiler or rollout cost.

Recovery images from `d679d5fe` stage in 75.3 s with 57.2 s of compilation.
The stable two-target change reports 35 changed input paths. Qualification reuses
the exact runtime digests and takes 9.2 s. The final warm checks take 2.0 s for
97 backend tests, 0.4 s for 58 UI tests, 6.0 s for the production UI build, 4.5 s
for lint, and 5.5 s for both browser-harness consumers. The latter emits an unused
re-export warning in the flight harness; both binaries compile successfully.

The session-recovery rollout converges in 30.3 s with explicit reconciliation.
The original browser upload resumes at 3,523,215,360 accepted bytes and passes
4 GiB without restarting its identity. Completion remains under observation.

The Python repository wrapper now forwards the consumer's explicit byte ceiling;
Datasheet applies its configured dataset/report limits before reading bytes. SDK
and isolated template enforcement passes in 13.8 s. Publishing the Datasheet image
also installs the streaming SDK in the running Python reference consumer. Staging
from `2856e416` takes 34.0 s and qualification takes 19.8 s; Python layer export
accounts for much of this time. This component can roll out while the independent
browser transfer continues. The main source checkout stays unchanged while its
acceptance recorder is active; the isolated worktree supplies this qualified image.

The isolated worktree's Helm configuration check recompiles its native path-based
crate graph in 21.1 s before 5.8 s of assertions. The same check previously reused
the main checkout's graph. Sharing CARGO_TARGET_DIR does not make different source
paths free. The consumer harness also pulls the broad smoke graph; moving its
external HTTP and SDK checks into a focused client harness is a future build-cost
improvement. Two compile errors in the new harness (a shadowed DuckDB crate name
and a NonZeroU64 comparison) are corrected before execution. The warm rebuild takes
18.6 s. Building the browser and flight consumers from the isolated path takes 20.3 s.

The installed Python acceptance distinguishes its credentials explicitly: public
HTTP and MCP operations use the registered machine's OAuth exchange, while direct
Artifact-plane streaming uses short-lived signed conformance identities supplied
by the local Rust harness. The Python process never receives the signing key.
The harness records full-file bytes/hash, peak RSS, maximum delivered chunk, early
exit, byte-ceiling rejection, materialization cleanup, and a foreign-tenant denial.
These checks remain pending until the browser publishes the large upload receipt.

The resumed 10 GiB public upload completes with SHA-256
`2dcb535dbca1ce175de712a3b39bc7fbe19eb71061f5f4626a8ab9eaf755b236` and
canonical URI `artifact://01a083cf-8e2e-7de2-b79a-7688f8bd0735`. Its final receipt
contains exactly 10,737,418,240 bytes. The continuation transfers 7,214,202,880 new
bytes and reaches Ready in 1,756.6 s, averaging 3.92 MiB/s including recovery checks
and verification. This timing excludes the first failed transfer and repair interval.
It exceeds the configured 15-minute access-token lifetime without another rejected
refresh. Public HEAD and Range, the queued CSV, and the narrow artifact drawer pass.
The four saved screenshots were inspected. Their long filename exposes a narrow
header layout defect: the close button can fall outside the viewport. The title
container now shrinks, the button retains its width, and an open drawer locks
background scrolling. This small correction remains a separate Console rollout.

Installed consumer acceptance passes real known/unknown-length HTTP uploads,
immutable part replay/conflict, completion replay, concealed foreign-actor upload
IDs, and a denied independent Work Context download. Public Datasheet MCP consumes
both uploaded CSV and Parquet. The installed Python SDK reads the full 10 GiB object
in 33.7 s with a matching SHA-256, 1 MiB maximum chunks, and 54,764 KiB peak RSS.
Early exit, declared-byte rejection, temporary-file cleanup, and a foreign-tenant
read denial also pass. Evidence is
`output/acceptance/artifact-upload/installed-consumers-20260909.json`.

The first consumer attempt requested only operator:use and artifact:upload, while
the profile requires its complete scope set. HTTP 401 was therefore expected.
The fixture now reuses the installed-profile scope list, requires a successful typed
policy read, and checks the typed concealed-404 upload error. A separate urllib
investigation hit Cloudflare error 1010 and cannot count as application authorization
evidence. Native HTTP and MCP acceptance reaches the authenticated application.

The difference between the resumed public upload's 3.92 MiB/s and the installed
Python download's roughly 304 MiB/s warrants a controlled ingress/uplink comparison.
It does not isolate Cloudflare, QUIC, the browser, or the uplink as the bottleneck.
A 100 GiB run and native multipart comparison remain future capacity experiments;
the certified transfer size here is 10 GiB. Near completion, Artifact stays around
68 MiB, BFF 22 MiB, and Gateway 39 MiB on its active replica. Browser JavaScript heap
samples are 27–45 MB; they exclude browser native buffers and are not process RSS.

The final narrow-drawer Console image stages in 50.2 s, qualifies in 7.8 s,
and converges through GitOps in 18.9 s. Its Helm configuration check takes 6.3 s.
The deployed runtime digest is
`sha256:3924ade680abeeabecc17a8199fa83826680875bf835bcbfd2ebeffb3dd8c4f4`.
This UI-only image change still spends 32.6 s compiling Rust and reports 385 changed
freshness paths after switching from the earlier two-target publication to Console
alone. Keeping the Rust artifact cache stable across target selection and source
normalization deserves a focused follow-up; a CSS correction should reuse the BFF
binary. The first final browser check also fails before upload because the harness
omits Enter's keyboard-generated text. The native CDP event is corrected, and the
check now uses native Tab as well. This is test-harness churn, not deployment time.
The artifact snapshot and live updates already sort by creation time; checking that
code closes a suspected list-order issue without introducing another database change.

Final public UX acceptance passes in 93.2 s. Native Enter/Tab/Escape, clipboard,
filtered detail access, durable cancellation, and wrong-file recovery work against
the deployed Console. Initial narrow screenshots had the active file below the
scroll position even though the DOM assertions passed. The capture helper now
scrolls that row into view and checks that its controls fit the panel. A separate
256 MiB transfer supplies a visible narrow Finishing state, an independent hash,
and live/reloaded list-insertion evidence. All ten final screenshots are inspected.
Evidence is
`output/acceptance/artifact-upload/09f3e37f49a26c9395078c88ede42e91d3421a85/ux-01a0842a-789f-7372-8219-29d320ac6c51/evidence.json`.
The shared browser/flight harness build takes 3.2 s. The final installed smoke passes
the public origin, authorization surfaces, complete server catalog, GPU capacity,
and governed full/HEAD/Range delivery. No image rebuild is needed for these final
harness and evidence changes. Artifact, both Gateway replicas, Console, and the
installed Python consumer are Ready at their locked runtime digests.

The next build/deploy work should stabilize cached Rust artifacts across image
target sets, avoid source-path recompilation across worktrees, and keep external
acceptance clients out of the broad native smoke dependency graph. Public upload
throughput needs a separate measured ingress/uplink experiment. The final UI image
build, qualification, and GitOps rollout total 76.9 s; the hours spent on this
feature also include implementation, real long-transfer recovery, and avoidable
test/source-coordination retries. Available host disk is about 314 GiB at closure.

## Computers Runtime Adoption: September 9

The Computers and contract-evolution goal starts from Veoveo main `cc80fcb2`.
Upstream main remains `11f59d55`. The existing host had 253 GiB available and the
Bioma Kubernetes node was Ready; neither disk capacity nor cluster availability
blocked the initial port. The installed OpenShell CLI reports `0.0.14`, so it is
not accepted as evidence for the selected `0.0.116` profile.

The private provider adapter now has its own workspace crate and does not bring
OpenShell, SSH or code generation into the gateway. The first compile identified
the handoff's omitted generated-type Regress dependency. Typify's unused macro and
Russh's unused RSA/compression defaults were disabled; the canonical Ed25519 SSH
path uses the existing Ring backend. Cargo.lock adds the adapter's dependency graph
without removing or upgrading any original locked package identity.

| Initial check | Measured cost | Meaning |
|---|---:|---|
| Recorded adapter test command | 60.55 s | 52 passing local transport/policy/stream fixtures; Cargo compilation 39.47 s and fixture execution 20.44 s |
| Recorded adapter Clippy | 12.95 s | All targets pass with warnings denied; first check-profile preparation included |

The missing-replay fixture deliberately consumes its timeout window. A future
focused virtual-time check may reduce that cost while preserving one real transport
deadline test. These measurements do not establish native provider behavior, physical
storage enforcement, a warm edit budget, or installed browser/CLI acceptance. Provider
source, native recovery, retained storage and deployment remain active work.

The recovery checkpoint adds eight tests; the full 60-test suite still executes in
20.44 s. Concurrent v2 evidence recorders reproduced a lost-update race: the full
suite passed, but another check overwrote its receipt. The recorder is serialized
until CE-06 supplies immutable receipts and a concurrency-safe aggregate. Restoring
the missing receipt required one unnecessary repeat of the 20-second fixture suite.

The collection/admission fixture runs the complete migration catalog in isolated
SurrealDB containers and tests two clients. Its three scenarios execute in 2.1–4.8 s;
the higher measurement overlaps the provider's first release build. New domain
dependencies reuse all existing registry package identities. Enabling Schemars UUID
support initially rebuilt the shared contract/Surreal dependency closure, which is
a real feature-unification cost of the Rust workspace.

The SQL skill's suggested formatter (`@surrealdb/surql-fmt` 0.1.0-beta.2) corrupted a
CREATE CONTENT expression. Database validation rejected its output, and the query
was repaired and reviewed manually. The formatter has no stable published release;
its output is not accepted without validation. Clippy also identified the existing
large migration-error variant in the shared store. Boxing that rare source error
removed 105 repeated size diagnostics without changing database error semantics.

Provider qualification now has verified source exports for both patch branches and
an independently downloaded, checksum-verified stock CLI. The provider build uses a
separate target cache and eight compile jobs; it leaves the installed CLI and user
Computers untouched. No native provider, storage or public acceptance is claimed yet.

The v2 lost-update repair now merges each completed result under a short worktree
file lock. Twelve concurrent publishers retain every result, including failure;
a stale completed check also preserves newer results. Commands execute outside the
lock. The recorder's nine focused tests pass, followed by concurrent format and
Clippy checks whose receipts both survive. Scoped immutable receipts remain CE-06
work; this repair removes the immediate need to serialize test execution.

The patched OpenShell gateway and Docker driver completed their first optimized
build in 3m 22s. Both binaries report the expected selected version. The gateway
links the host's libz3; a deployable image must include that exact runtime dependency
or a qualified static build. Upstream's example distroless Dockerfile does not by
itself establish that runtime closure.

The supervisor's optimized build took 1m 29s and reports
`0.0.117-dev.5+gea0c605`. The candidate Computer image uses Ubuntu 26.04 and the
September 9 archive snapshot. Minimal Ubuntu needed a pinned CA-package bootstrap
and an explicit APT CA-bundle path to fetch that snapshot with TLS verification;
the initial two attempts failed closed. A cached rebuild takes under one second.
The supervisor's version command runs inside this image as UID 10001, which verifies
its dynamic-library loading but does not prove confinement or lifecycle behavior.

The local candidate image is
`localhost:5001/veoveo-computer-candidate@sha256:7135af2af79213102f0abf0f052fa324ef77fde53441b9986f78587b6097d68c`.
It is available for isolated native acceptance and has not been installed on Bioma.

The first native provider fixture exposed a required provider JWT signer alongside
mTLS. Adding the separate Ed25519 signer resolved creation admission. The complete
native create/terminal/reattach/Stop/Start check then passed in 11.51 s. Its isolated
Docker namespace is cleaned on exit. This evidence does not cover retained external
storage or production; no user workload was changed. Provider-specific validation
currently returns a safe but broad adapter error, which made this setup fault harder
to diagnose. Typed rejection diagnostics remain an integration improvement.

The first native ext4 retention fixture passed in 23.80 s after its login-shell policy
was corrected to permit the image's system profile files. It uses the existing pinned
Computer image's e2fsprogs/util-linux tools; no additional image build was needed. The
fixture preallocates 512 MiB and performs a real ENOSPC write, source-container removal,
block backup/restore and replacement process check. These are isolated physical tests,
not production allocator acceptance. Plain Docker volumes lack the required exclusive
writer policy, and naive plugin mount-ID deduplication is insufficient because the
Docker engine can use the same ID for nested mounts. That integration is still required.

Provider-wait recovery tests now reuse one shared Rust database fixture with Computers
admission. The four Task scenarios execute in 3.47 s against isolated pinned stores;
they do not silently skip behind an environment flag. Splitting lease/recovery code
out of the existing Task file also keeps the cross-component change reviewable. The
new stored enum means deployment must update all affected readers before Computers
starts writing that class. That required release closure is distinct from unrelated
image rebuilds and cannot be omitted from the deployment plan.

Real-store fixtures showed a 3–20 s execution range under concurrent checks. Their
cleanup called graceful Docker stop while the fixture still held WebSocket clients.
The shared in-memory fixture now force-removes only its own disposable container;
there is no retained data to flush. The initial operation-journal scenarios execute in
4.80 s. Native home fixtures keep their physical unmount/detach checks. Removing the
duplicate fixture's unused random dependency changed only the local Computers package
entry in Cargo.lock, with no registry dependency upgrades.

The stock CLI continuity prerequisite executes in 6.10 s using the existing candidate
image and provider binaries. Adding Tokio process support for that owning Rust fixture
caused a 19.63 s dependency-feature rebuild; no image rebuild was necessary. The native
renewal test then exposed no need for a provider patch. A real HTTP/2 backpressure
fixture did expose that dropping a gRPC stream could leave its provider connection
open. Per-attachment socket shutdown fixes that failure without an extra byte-copy
relay. Hyper-Util and Tower were already in the lockfile; their direct use changes only
the runtime package's dependency edges. The local replay-timeout scenario accounts for
about 20 s of the focused suite's runtime and is recorded separately from compile time.

The expanded journal suite exposed a real concurrency defect in preceding-read admission:
two different requests could both create an operation before an unconditional update
overwrote the Computer fence. Eligibility now lives on the conditional write itself.
The same correction applies to quota increments and policy changes. A regression runs
32 competing requests across two database clients for each of eight Computers; the
operation suite executes in 1.57 s. Domain lifecycle scenarios execute in 1.60 s with
no provider image build. Database syntax and typed-record mismatches were caught in
these disposable fixtures before enabling provider dispatch.

The Computers worker combines the existing store and native-runtime dependency graphs.
Its first build required additional Cargo feature combinations; the following focused
compile took 6.51 s. The combined domain/runtime/Task checks took 50.6 s, including the
existing 20.51 s retained-terminal timeout case. No provider or Computer image changed.
The first native worker scenario completed its behavior checks in 26.57 s but failed
cleanup because the reused volume fixture assumed every scenario had created a backup.
The fixture now tracks that operation explicitly. The v2 report's whole-tree digest
still requires unrelated unit checks after this test-only edit; CE-06 scoped immutable
receipts remain the intended correction.

Worker recovery now releases its exact observation lease when its bounded read ends,
allowing another replica to continue without a sixty-second expiry wait. Stop bypasses
home preparation. Existing Task links are read once rather than recreated on every
worker pass, and exhausted recovery leaves the automatic queue. A native database
failure in Task acknowledgement came from dereferencing a linked Task in a variable
expression; an explicit typed Task read passed the qualified 3.2.4 fixture. That query
is retained in its owning SQL file for review and regression coverage.

The corrected native worker scenario passes in 25.56 s. The same candidate binaries
and image also pass the three native lifecycle/terminal/stock-CLI checks in 11.85 s
and physical ENOSPC/backup/restore in 29.65 s. The final incremental native compile
is 4.47 s. These are isolated functional measurements; they are not public-ingress,
production allocator or end-to-end authority performance claims.

Extracting the shared policy evaluator avoids importing gateway-owned agent runtime
and analytics into Computers authority checks. The standalone normal dependency tree
contains the policy and contract crates, without Veoveo gateway/agent or DuckDB/Rig.
Its incremental check took 0.59 s. The gateway consumer qualification rebuilt shared
store/Task/agent dependencies and linked in 2 min 05 s; its 80 library tests then ran
in 0.07 s. Policy equivalence, control-plane and exposure fixtures also passed.

The additional all-target Clippy probe reported four existing gateway-binary warnings:
three mutable-key cases in Console projection and an eight-extractor Artifact upload
handler. This policy extraction changes the gateway library. Its required lint receipt
therefore uses `--lib`; the binary warnings remain visible work when those HTTP owners
are touched. They are not a provider or runtime deployment failure. The v2 receipt
format replaces a check's prior attempt; immutable attempt history remains CE-06 work.

Current policy readers now select the active pointer and its retained revision in
one database round trip. A missing or mismatched revision fails closed. The focused
store/gateway qualification passes in 62.5 s, including compilation and an isolated
3.2.4 test that switches the policy pointer concurrently across two clients. Library
Clippy takes 16.8 s. No image publication or workload restart was needed.

Browser access tokens now retain their durable refresh-family identity, and gateway
requests check that family's current state. Focused qualification compiled in 35.45 s;
the real-store rotation/logout/replay cases took 8.48 s in the recorded run (1.97 s
in the initial isolated run). Concurrent Cargo test and Clippy commands contended on
the same build-directory lock. Run these dependent compiler phases sequentially on
this target directory; parallel command launch does not produce parallel compilation.
No provider artifact or database migration changed. Remaining disk reserve is 196 GiB.

Federated identity review caught a fixture mismatch: refresh state retains the IdP
issuer, while a verified gateway token records the installation issuer and preserves
the canonical principal ID. The corrected session suite passes in 13.0 s, including
a 9.89 s compile and 2.83 s real-store scenario. This was fixed before deployment.

The internal request-context change uses one direct/delegated/automated JSON fixture
in both Rust and Python. Python's 43 focused cases take 0.13 s. Qualifying the changed
Rust consumers requires their real dependency graph; that check takes 1 min 16 s.
No provider or Computer image rebuild is involved. A combined contract/gateway test
probe compiled in 1 min 46 s. Disabling top-level default features alone does not
remove analytics: the gateway explicitly enables that contract feature. Its source
has no remaining analytics consumer. Removing that edge is the next focused build
improvement, with consumer compilation required before accepting it.

The gateway's unused analytics feature edge is removed. Its normal Cargo dependency
tree now excludes DuckDB and libduckdb-sys. All gateway unit, binary, policy-parity,
control-plane and real-session-family checks pass. The first graph rebuild took
1 min 59 s; the first Clippy graph took 1 min 10 s. These cold measurements are not a
warm performance claim. The one-line manifest change removes a concrete native
dependency from future gateway builds without changing provider or Computer artifacts.

Accepted Computer operations now retain verified source identity for dispatch-time
policy checks. Focused domain/worker qualification takes 43.6 s. The unchanged native
provider and Computer image pass the worker lifecycle and recovery scenario in 24.21 s,
with a 0.56 s incremental compile. All-target domain/worker Clippy takes 15.48 s.
The provider binaries were reused; this authority change required no image rebuild.

Identity synchronization now creates missing records inside the transaction and
updates only principal presentation fields. A staged stale-write test preserves
committed principal, tenant and installation disablement. The two real-store cases
take 1.82 s after a 9.61 s compile; store Clippy takes 17.39 s. Consumer qualification
passes, with a 1 min 09 s combined feature-graph compile. An initial consumer command
used the wrong gateway package name and failed before compilation; the corrected
receipt uses `veoveo-mcp-gateway`. Cargo's `--tests` applies to every selected package,
so mixing it with a targeted `--test` also ran the gateway's other integration tests.
Future focused consumers use separate commands to avoid that scope expansion.

Dispatch authority is now mandatory inside the Computers domain, removing the worker
adapter's ability to supply an allow decision. Focused qualification compiles in
19.99 s; its four current-policy cases execute in 4.18 s, including actual source-token
expiry. All-target Clippy takes 10.95 s. The native worker passes in 24.44 s against
the real policy reader and existing provider/image, with a 0.58 s incremental compile.
Neither artifact needed rebuilding. Fixture setup initially retained an unrelated
Media secret, selected an unsupported OAuth method, and omitted a declared auth-mode
client. The policy matrix also attempted a forbidden immutable-revision update.
Those setup errors were corrected against the existing schema; no production contract
was weakened to make the fixtures pass. The store-backed policy fixture is shared by
domain and native-worker cases to avoid repeating that setup across harnesses.

An initial native Docker volume-plugin probe passes in 3.40 s. It rejects a second
registered consumer, preserves ownership across nested `docker cp`, retains files
across Stop/Start, and permits a replacement after removal of the first container.
This probes a disposable directory; quota-backed allocation and provider integration
remain unqualified. The candidate depends on `volume-nocopy`, because Docker can
populate a volume before registering the new container. The selected provider does
not yet supply that guarantee. Docker's nested mount IDs and retryable RPCs also
make decrementing a simple mount counter unsafe for release decisions.

The prototype exposed fixture cleanup debt: this Docker daemon retains external
plugin clients after their specification files are removed. Volume listing took
15.025 s after the two disposable plugin endpoints exited. An empty responder for
those exact cached endpoints restores listing to 0.023 s, without restarting Docker
or changing installed containers. Its ignored source/binary lives at
`output/development/computers-provider/retired-volume-responder`; it watches Docker
PID 3598's start identity and removes its two sockets when that daemon exits. It
denies every request except empty volume discovery and local capability metadata.
Do not terminate it while those cached clients remain. Move the maintained plugin
fixture to an isolated daemon before running it again; removing containers and
volumes alone does not isolate a plugin registration's lifetime.

The maintained volume probe now owns an isolated Docker 29.8.0 daemon with no
network, exact locally imported Computer image, and daemon-scoped plugin registration.
Its recorded run takes 8.29 s after a 1.20 s incremental compile. Runtime all-target
Clippy takes 15.78 s. Normal cleanup verifies removal of the owned daemon and its
data; failure cleanup preserves diagnostic logs. The installation daemon is unchanged.
This fixture avoids both cached-plugin churn and a host-wide Docker restart. The
registered-consumer check still needs exact provider/instance admission and native
allocator qualification before it can establish production writer exclusion.

The seventh provider patch adds typed Docker volume `no_copy`; retained templates
require it and acquire a new fingerprint. All seven patch applications reproduce
their exact trees, and every gateway export file matches the new tree after the
declared workspace-version materialization. The archive normalized one documentation
symlink's trailing slash; its target is unchanged. The six imported patches retain
their original bytes. The new gateway/driver version is `0.0.117-veoveo.1`.

The initial Docker-driver test build took 39.34 s and downloaded the previously
uncached, locked `temp-env` dependency. Its 137 tests pass in 0.05 s. An accidental
`--no-default-features` gateway build was stopped when inspection showed that it
changed the qualified provider profile; the corrected build preserves default
features and takes 1 min 29 s. A subsequent recorded driver test takes 22.61 s to
compile after that feature-graph switch. Reusing one target directory does not avoid
recompilation when test and binary feature graphs differ. Store the exact feature
selection with the provider build recipe before packaging this path.

The new provider passes native terminal/renewal/CLI checks in 11.85 s and physical
retention/ENOSPC/backup-restore in 23.61 s. The retention case also inspects actual
Docker mount options for the original and replacement container. Runtime's 72 checks
pass in 20.49 s after a 6.85 s compile; all-target Clippy takes 4.32 s. The existing
supervisor binary and Computer image are reused. Provider OCI packaging and the
production allocator remain outstanding; these native results do not establish
installed writer exclusion or public availability.

The same new provider also passes the real-store two-worker lifecycle/recovery case
in 24.27 s after an 8.57 s consumer compile. Candidate binary SHA-256 identities are
gateway `20fd9afa9662d4fff62a8de0be8c9ad53ba2d096c9137dde82958417c1ec3ef6`
and Docker driver `eee64d2a0c973071c74fe160eb02bbe5c4094724dee1190be99f742e081bbcf9`.
The reused supervisor is `18966e201952608891fad4b570bab45fdaaf88f4cfd627d7709e3f71743a4844`.

The allocation protocol now requires provider and instance identity. Its eight
schema/TLS cases pass in 0.04 s, with a 0.44 s recorded incremental compile after
the generator's initial 14.81 s rebuild. Runtime all-target Clippy takes 6.21 s.
This client-only private wire change does not rebuild the OpenShell provider or
Computer image. The production allocator must enforce the echoed identity; a
successful transport fixture does not establish that enforcement.

The retained-writer matcher now binds Docker's engine UUID, provider namespace and
full Computer/template/instance labels. Its isolated native probe takes 8.97 s after
a 1.53 s compile. It proves that source removal alone does not admit a replacement
and that a late old instance cannot reclaim the home as its sole consumer. The first
extended run failed on Docker CLI error wording; the fixture now checks the exact
Engine HTTP 404 from the same isolated daemon. It does not turn arbitrary command
failure into source-removal evidence. Durable handoff and filesystem preparation are
still allocator work; the fixture's explicit admission change is held in memory.

The storage-host journal now records private host/home identities and distinguishes
new reservations from incomplete existing work. Five filesystem-metadata tests pass
in 0.01 s after a 1.24 s incremental compile, including exclusion across separate
processes. They cover restart, identity changes, missing/corrupt records, symlinks,
hardlinks and backing-file substitution. The initial dependency graph took 11.70 s
to compile; final all-target Clippy takes 0.87 s. An initial style lint was corrected
before commit. These tests deliberately use sparse metadata fixtures and establish
neither physical quota nor mount safety. No provider or Computer artifact rebuild
was needed. The allocator's filesystem backend and authenticated service remain next.

The production filesystem backend now passes its own native fixture. It preallocates
and formats a new ext4 home once, enforces the free-space reserve before reservation,
reaches ENOSPC, and restores files in a different helper container. Restore preserves
owner-selected home permissions while checking UID/GID, backing-file identity, ext4
UUID and mount options. The initial native run took 3.60 s. The complete recorded case,
including reserve and permission checks, takes 12.24 s after a 1.67 s compile. These are
individual observations, not a stable throughput or latency benchmark. All-target
Clippy takes 9.36 s after the mount-feature graph change. The existing image is reused.

A concurrent journal test fork transiently inherited another fixture's open lock
before exec. The process-containing metadata fixtures now run serially; production
still refuses ownership while any descriptor holds the lock. The filesystem fixture
uses the explicit local Docker socket, removes its own containers, verifies loop
detachment and removes only its own backing files. Docker plugin/service wiring and
durable physical handoff remain outstanding; no installation acceptance is claimed.

The allocator now runs as its own binary with the private worker mTLS service and
Docker volume plugin. Its native fixture reuses the existing Computer image and
isolated Docker daemon image; neither the provider nor a container image was rebuilt.
The initial added HTTP/TLS consumer graph checked in 7.68 s. Recorded storage tests
compile in 2.92 s, then run six local cases in 0.03 s, the ext4 fault case in 7.85 s,
and the shared-mount/restart case in 9.80 s. The latter keeps a Computer writable
while replacing the allocator and rejects an unadmitted instance after source removal.
These are individual fixture timings, not installed latency measurements.

The host root already provides shared mount propagation. A dedicated allocator
`rshared` bind and daemon `rslave` bind work without changing unrelated mount flags
or restarting the installation daemon. Worker and plugin request limits are separate;
the filesystem lock is released before Docker volume creation calls back into the
plugin. This avoids an allocator/plugin callback deadlock during admission.

Fixture churn included a wrong relative helper path, trust files created with the
developer's group-writable umask, and a root-owned socket directory that the parent
could not remove. The harness now creates explicit trust permissions, preserves
startup stderr before cleanup, detects early process exit, and restores ownership
of its empty socket directory. The failed fixtures' exact empty directories were
removed. No user home or installed workload was involved. Physical handoff, provider
integration with this allocator, packaging and public deployment remain next.

The storage service now records the first physical container before acknowledging a
mount and performs durable writer handoff. The native case deliberately retains a
private mount after removing the provider container. Handoff refuses admission while
that mount remains writable, then transfers the same bytes after verified loop
detachment. This tests Linux's documented lazy-detach behavior in
[`losetup`](https://man7.org/linux/man-pages/man8/losetup.8.html): an acknowledged detach
can precede the last active reference. The production check reads the association
again; it does not turn the mutation acknowledgement into a fencing guarantee.

The recorded full storage run compiles in 3.36 s, runs seven local cases in 0.03 s,
the ext4 fault case in 7.94 s and the expanded service/handoff case in 11.26 s.
The latter covers a lost response, restart, late source admission, stale operations,
template change and instance reuse. Combined storage/runtime Clippy takes 7.13 s.
The runtime's 73 tests take 20.51 s after an 11.21 s compile following the private
IDL change. A warm native-only edit compiled in 1.49 s. The existing provider binaries
and both images remain reusable; no artifact publication or installed rollout ran.

The next integration must use the allocator's dedicated worker trust root and native
provider identity, close abandoned-admission recovery for a target that never mounted,
and connect maintenance to the domain's durable authority. Native Docker plugin
qualification is not a claim of complete worker maintenance or public availability.

The worker now composes the production allocator, native provider and durable domain
in one isolated fixture. Its first successful run took 46.04 s after a 7.07 s compile.
Two replicas compete for Create. Stop works while the allocator is offline, and Start
after helper replacement keeps the file. Lost dispatch and settlement still preserve
the operation fence. Integration uncovered a real adapter omission: gateway object
labels do not become Docker container labels. Create now binds the same identity in
the sandbox template. The storage matcher remains unchanged. The container-name
prefix contains the workspace; treating it as the namespace delayed diagnosis.

An experimental host-network Docker fixture interrupted the local registry path.
The second daemon removed the host's default bridge despite disabled bridge/firewall
flags. The registry container remained running and responded on its installation
network, but localhost:5001 timed out. The Kubernetes node remained Ready and the
public Veoveo endpoint returned its redirect; those observations do not qualify the
complete application during the incident. No installed Docker restart was performed.

Recovery recreated docker0 with its original interface index and IPAM subnet, then
reattached the registry's exact orphaned veth. Localhost registry access returned HTTP
200 in 0.000766 s, and a fresh default-bridge container could join and reach it. The
fixture now checks its network mode before starting dockerd, rejects a shared host
namespace and verifies host bridge identity. Its provider profile uses private
networking. A short-lived Unix-socket relay makes the already published local image
available at the same exact manifest reference inside that namespace. It does not
introduce an installed proxy or require another image build.

Additional churn came from a combined feature-graph check that rechecked SurrealDB
and took 68 s, versus 5.81 s for the focused Computers service check. A two-line
Clippy borrow correction invalidated the current whole-tree v2 receipt even though
the runtime unit inputs did not change. CE-06 remains the required repair for that
evidence churn. The temporary responder for the host daemon's previously cached
fixture-plugin clients remains necessary until its next natural replacement;
restarting that daemon without live restore would interrupt installed workloads.

Final recorded worker integration takes 49.03 s after a 4.39 s compile. The focused
runtime suite reuses its binary (0.47 s Cargo overhead) and runs 73 tests in 20.52 s.
The storage suite compiles in 1.70 s, runs its seven local cases in 0.05 s, the ext4
fault case in 6.55 s and the service/handoff case in 11.13 s. Combined all-target
Clippy passes in 4.57 s. Local registry health remains HTTP 200 after cleanup. The
existing Computer image and provider binaries remain unchanged; public rollout is
still pending.

Public projection work now shares a current policy/directory snapshot with dispatch,
avoiding one independent authority read per action button. Initial application cases
run in 1.88 s after a 7.96 s compile and require neither provider nor image rebuild.
They use the real store for concurrent Create retries, default-template rotation,
quota exhaustion, current membership/policy changes and Setup Required. Provider
health is an explicit synthetic input in these cases; it is not runtime evidence.
The full focused domain/contract/application run compiles in 16.35 s, then passes
25 cases across admission, authority, lifecycle, queue recovery, public types and
projection. The native worker case is explicitly ignored in that command and keeps
its separate recorded qualification. Final all-target lint takes 1.46 s.

The MCP contract still described the removed per-tool task-support handshake while
the pinned runtime implements the final extension. Checking the actual SDK and
upstream Tasks schema prevented introducing an unsupported compatibility field.
The documentation now follows the negotiated extension and required-capability error.

The authenticated Computers MCP/HTTP checkpoint passes 29 focused cases. Application
fixtures now include resuming a visible reservation and recovering the original Task
while compute is unavailable. Wire tests use distinct database connections and HTTP
replicas, and the maintained MCP client exercises subscription baselines, foreign
ownership, current policy revocation and assertion expiry. The final run compiles in
4.71 s; its four application cases take 1.71 s and three HTTP cases take 6.55 s.
Final all-target lint takes 1.44 s. Provider binaries and Computer images are reused.

Adding the existing conformance library for schema validation expanded the Cargo
feature graph: the first combined lint took 76 s and the first test compilation took
136 s, including SurrealDB recompilation. The schema check itself needs a small subset
of the conformance package. Narrow its library/CLI feature boundary during build-system
work, and qualify feature closures before assuming that a new test helper is cheap.
Raw wire-fixture assumptions about Task result flattening, standard routing headers
and error HTTP statuses also caused avoidable retries. The SDK now owns subscription
transport. Domain assertions remain in the existing isolated fixture.

The v2 report still invalidates unrelated domain evidence after a wire-test-only edit.
The failed attempt was a test HTTP-status expectation; all domain/application cases
had passed. The corrected full run is green. CE-06 must preserve immutable attempt
history and reuse the unaffected receipt closure. Runnable installation wiring and
public rollout remain pending at this checkpoint.

The runnable Computers service checkpoint validates the selected JSON profile and
trust references before platform-store writes. Its isolated startup fixture serves
Setup Required or Compute Unavailable as appropriate, rejects a stale quota overwrite,
and shuts down without waiting for an unreachable provider. The executable rejects
root-scoped store credentials without echoing environment secrets. Four startup/config
cases take 1.56 s. Together with application and HTTP cases, the service's 11 tests take
42.3 s including a 23.08 s compile. The four application cases took 11.04 s in that run;
the three HTTP cases took 6.46 s. These are local observations, not deployment timings.

The affected runtime suite passes 73 cases in 20.49 s after a 9.86 s compile. The
combined first lint of the service and runtime took 22.15 s as their feature sets
converged; the final recorded lint reused that graph in 0.98 s. Keep command/feature
identity in CE-06 receipts. A service-only check and a combined runtime check can have
different Cargo closures even when source is unchanged. The native provider and
Computer image were not rebuilt for this checkpoint. Packaging and public rollout
remain active work.

The shared browser-family decision now serves gateway authentication and Computers
control. Local real-store subscription cases cover logout and absolute family expiry
before assertion expiry. Rotation preserves the family, while mismatched authority
fields fail closed. A test-client initialization race surfaced when HTTP cases ran
independently; both client constructors now explicitly install their TLS provider.
Review also found that awaiting renewal inside a timer branch could delay closure.
The guard now polls expiry and cancellation during that read and during blocked I/O.

A broad gateway all-target lint found existing warnings in
`admin/console/projection.rs` (`BTreeSet<RecordId>` interior mutability) and
`artifact_upload.rs` (eight extractor arguments). Those binary paths are unchanged
by this checkpoint and remain recorded cleanup work before the gateway route work.
The current receipt scopes gateway lint to its changed library. Combining its feature
graph with the Computers runtime also rebuilt SurrealDB lint metadata; stable command
and feature closures remain relevant to CE-06. No provider or image rebuild was needed.

The browser grant ledger is qualified independently of terminal transport and provider
images. Its first six real-store cases run in 4.77 s after a 12.36 s compile. They use
synthetic Ready rows and do not establish provider execution. Two implementation errors
were found before qualification: `session` is a protected SurrealQL parameter, and a
transaction's final COMMIT also produces a response entry. The queries now use a distinct
family variable and return their admission receipt after commit. No production record
or retained home was used to debug these cases. The new `computer_attach` action avoids
inventing an MCP tool that would deliver browser credentials to automation consumers.

Final recorded grant/service/policy coverage passes 44 cases in 85.6 s, including a
52.29 s compile; the six grant cases themselves take 4.63 s. The shared contract/store
command passes another 187 cases in 114.8 s, with only 0.31 s in test bodies. Selecting
`veoveo-mcp-contract` directly enabled its default analytics feature and pulled DuckDB
into that command's feature graph. Future checks of the minimal policy/identity surface
should declare `--no-default-features` when that matches the affected consumers, with
analytics qualified separately. The full result here remains valid. Service/domain
lint reuses its graph in 1.3 s; shared-library lint takes 18.1 s. No native provider or
Computer image was rebuilt. Last disk observation showed 212 GiB free.

The service now composes durable browser grants with native terminal transport. The
first WebSocket feature build took 132 seconds, including SurrealDB recompilation.
The subsequent native target compiled in 18.53 seconds and completed its retained
lifecycle plus browser cases in 67.83 seconds. Final recorded coverage passes 114
domain/service/runtime cases in 82.0 seconds, including a 31.93-second compile. The
recorded native run reused that build in 0.61 seconds and passed in 56.99 seconds.
The final lint reused its graph in 2.66 seconds. These native cases reuse the exact
qualified provider binaries and Computer image; no OCI build was necessary.

Terminal revocation uses three shared LIVE sources per service replica with bounded,
filtered fanout. This avoids one database watcher set per attachment. Authoritative
renewals still read current state; accepted input and event bursts coalesce for
500 milliseconds. MCP resource listeners still create individual outbox LIVE sources
and remain a separate consolidation opportunity. This checkpoint's native transport
proof does not establish public relay, stock CLI ingress or headed Console behavior.

The extra Cargo feature closure consumed roughly 43 GiB during this checkpoint. The
last observation showed 169 GiB available, with 72 GiB of incremental data and 250 GiB
of dependency artifacts. No retained home or provider image was pruned. The local
registry remained HTTP 200 in 1.45 milliseconds after native fixture cleanup, and the
retired volume responder remains required until the host daemon naturally replaces
its cached plugin clients. Do not restart the installed Docker daemon to clear it.

The terminal relay is a small shared library for the gateway and Console BFF. Its
standalone build took 8.34 seconds; five local deadline/WebSocket cases take about
4.6 seconds. It carries no domain store or native provider dependency. The maintained
reqwest-websocket adapter preserves the owner's TLS builder while the transport client
selects HTTP/1.1 and disables redirects. Service-issued renewal deadlines cross every
relay, and blocked directions cannot extend them.

A test-only Tokio clock feature initially broadened the combined dependency graph and
caused an 86-second metadata rebuild. The tests now use short bounded real timers and
that feature has been removed. A native fixture also exposed a TLS initialization-order
assumption: its first relay client was constructed before the helper that selected the
crypto provider. Fixture entrypoints now select that provider explicitly. The first
failed native attempt stopped after 9.18 seconds and cleaned its owned infrastructure.

Final relay/contract/service coverage passes 25 cases in 64.7 seconds, including a
45.16-second compile. The native command reuses that graph in 0.59 seconds and passes
in 78.26 seconds. It includes a deliberate 31-second uninterrupted attachment through
two relay hops, crossing the original service lease and source-token expiry. Current
policy removal and logout retain the five-second closure assertion. Final lint takes
1.84 seconds. The edge fixtures supply synthetic admission; actual gateway/BFF cookie
and public-ingress qualification remain required. A metadata/test build is distinct
from an OCI rebuild, and all provider binaries and Computer images were reused.

Gateway/BFF integration reused that transport and its qualified dependency pins. The
gateway-only metadata check took 87 seconds; adding the BFF to the selected package
set changed the Cargo feature closure and cost another 72 seconds. The first test
build took 125 seconds. Once the closure stabilized, the complete 190-case gateway/BFF
run took 16.34 seconds, including 11.48 seconds of compilation. Lint took 5.16 seconds.
Keep these related consumers in one affected-package invocation during this integration
to avoid repeating feature-dependent dependency work. The provider and Computer images
were unchanged.

The first gateway admission test used a UUIDv4 for a typed UUIDv7 session-family ID;
the fixture failed before admission and was corrected. Strict lint also exposed
existing BFF nested branching and large response-error values, which were simplified
without changing their HTTP behavior. The gateway's existing exact-blob query now uses
a sorted, deduplicated Vec rather than a mutable-key set; its native SurrealDB 3.2.4
regression passes. The globally installed SurrealDB binary is 3.2.1, so the check uses
the already qualified 3.2.4 binary under `output/tools` explicitly.

The last disk observation showed 151 GiB free, down from 164 GiB at the previous
checkpoint. This growth was host Cargo compilation. No retained home, provider binary,
or installed image was removed. Public deployment and end-to-end Console qualification
remain separate from these local transport and admission results.

Scoped Cargo maintenance subsequently reviewed 931 candidates and applied a fresh
locked plan containing 932 candidates: 171 old executable copies and 761 old
incremental variants, totaling 128.72 GiB. It retained dependency libraries, current
executable links and the newest incremental variant per crate. Disk availability
increased to 278 GiB, and the Computer storage executable remains present. The exact
review and applied receipts are under `output/development/computers-cargo-cache-*`.
Provider artifacts and retained homes are outside this cleanup scope.

The live-feed checkpoint reuses the Console's auth-scoped MCP pool. Unexpected resource
source completion now retires the cached client instead of leaving observers silently
stale until token rotation. Partial filter acknowledgment returns pending capacity.
The fixture now keeps remote listeners alive until cancellation and independently waits
for their cleanup; two old immediate-counter assertions were corrected to reflect that
wire boundary. The final 194 gateway/BFF cases pass in 16.03 seconds with a 14.07-second
compile, and strict lint takes 4.24 seconds.

Selecting only `--bin console-bff` changed Cargo's feature closure despite retaining
both `-p` arguments, causing a 34.03-second preliminary build. The final checks retain
`--lib --bins` consistently. Package selection alone does not identify a reusable build
closure; requested target kinds and dependency features also matter for scoped evidence
and iteration tooling.

The native Console checkpoint adds a pure contract generator and a lazy terminal chunk.
The production Vite phase takes 2.77 seconds; TypeScript plus Vite through the recorder
takes 6.56 seconds. The 74 Node behavior tests take 0.32 seconds (0.44 seconds including
the npm invocation). Frontend lint takes 3.92 seconds. Warm generated-schema/model
verification takes 0.90 seconds and performs no provider or domain-store build. The
terminal chunk is 469.62 kB before compression, 122.80 kB gzip, and is loaded separately
from the Console entry. These are local build measurements, not browser latency claims.

Adding xtask to the gateway/BFF all-target lint invocation changed the Cargo feature
closure and triggered an 85-second preliminary metadata build. Its warm repeat takes
0.63 seconds inside Cargo. Keep the chosen target/feature closure stable; future
ordinary UI work should invoke neither this combined Rust closure nor provider builds.
The shared session DTO and embedded contract documentation initially required a
14.70-second Rust test rebuild. The 196-case gateway/BFF run took 26.93 seconds including
a slower isolated database fixture startup. A subsequent UI-only callback race fix
invalidated the whole-tree v2 evidence digest and required a new Rust receipt despite
unchanged Rust inputs; that repeat took 2.21 seconds overall and reused compilation in 0.55 seconds. CE-06 scoped
receipts should preserve the first qualified Rust result through this frontend edit.

The npm install exposed pre-existing Browserslist and baseline-browser-mapping
advisories. Their locked versions are now 4.28.9 and 2.11.21, with the required browser
data dependency closure refreshed; npm reports no vulnerabilities. New terminal and
generator packages have exact verified pins. No provider binary, Computer image or
installed workload changed. The latest filesystem observation has 272 GiB free.

The first core Computers packaging pass adds four independent Bake targets. The
service and storage helper reuse Veoveo's Rust compiler. The provider builds the
verified OpenShell patch trees with its own pinned Rust 1.95.0 cache. The Computer
template has no Rust build dependency. The initial service/helper image build took
281.20 seconds, with an observed compile window of 250.94 seconds. The first
source-built provider image took 382.69 seconds; its gateway and supervisor Cargo
phases reported 228 and 89 seconds respectively.

The final four-target packaging run took 177.55 seconds. It rebuilt embedded service
documentation and re-exported provider source after narrowing the compiler COPY
inputs to the manifest and patches. That source export changed mtimes and recompiled
the provider's local crates against retained dependencies. Future provider source
updates can reuse the existing source-freshness approach to avoid changing every
exported file's timestamp. Provider documentation now changes only image documentation
layers. This is a recorded optimization, not a selected-profile correctness blocker.

The managed builder also assembled the previously qualified Computer template for the
first time. Its Ubuntu snapshot invocation fetched current archive indexes as well as
snapshot indexes, although package downloads used the pinned snapshot. A direct
snapshot-only source list can remove that redundant metadata work in a focused image
change. The resulting image still needs the exact installed template qualification.

An immediate unchanged four-target repeat took **5.134 seconds**, 5.654 seconds through
the evidence recorder, and performed no compilation. Every image retained both its
configuration digest and runnable manifest digest. Both core Helm renders remain
byte-stable across repeated runs. The 67 deployment-contract tests take 1.96 seconds;
the two real Helm configuration tests take 1.36 seconds. The first affected lint takes
21.29 seconds and formatting takes 3.00 seconds. An initial Helm test used a non-digest
fixture revision and failed schema admission; replacing it with an explicitly synthetic
64-character digest fixed the test input without relaxing chart validation.

Concurrent BuildKit phase windows are not additive. The final mixed build reported an
export window of 112.93 seconds while other targets were still compiling; that window
includes gaps between target exports. Use command wall time and individual vertices
for causal attribution rather than treating the aggregate window as export CPU time.
The registry returned HTTP 200 and the host had 284 GiB free. Installed GitOps smoke
and public acceptance follow immutable publication and the installation's complete
new image/configuration closure; this packaging checkpoint changes no live workload.

Compute-host integration exposed an authentication gap in the native fixture: its
guest supervisor received the worker certificate, while the provider's mTLS user mode
admitted any trusted client certificate as a user. The selected gateway is now
`0.0.117-veoveo.2`, with an exact positive user-CN allowlist and separate supervisor
credentials. This is a selected-profile correctness repair, without an upstream
dependency upgrade. The guest certificate completes TLS and receives Unauthenticated
from ListSandboxes. The same native run completes the retained lifecycle, two-worker
contention, interruption/fencing and renewed terminal journey in 81.86 seconds.

The first repaired provider image took 182.40 seconds, including two source admission
tests. Its sequential compiler stage needlessly rebuilt the unchanged supervisor.
Gateway and supervisor now copy their own verified trees into separate compilation
stages. The one-time three-image rebuild after that restructuring took 180.97 seconds;
the provider executable hashes match those exercised by the native test. The
supervisor hash also matches its previous image. The selected native proof runs those
OCI-built executables on the host; it does not certify the complete container topology.

Warm runtime coverage is 73 passing tests in 21.30 seconds through the recorder. The
affected all-target lint takes 23.93 seconds and formatting 3.09 seconds. The new
cross-crate fixture initially lacked a direct tonic development dependency; adding
the already selected workspace dependency fixed compilation without new package
versions. Private compute-host startup and installed acceptance continue after this
security checkpoint.

Storage enrollment removes a provisioning round trip: configuration no longer needs
the UUID assigned when its private Docker daemon first starts. The helper reads and
rechecks the configured engine, then binds the exact identity in the private journal.
The new test rejects replacement-engine adoption. Eight storage tests pass, and the
native retained worker fixture passes in 79.56 seconds with the canonical configuration.
Only the storage image is rebuilt: 35.97 seconds overall, with a 31.42-second observed
compile window. The provider, Computer template and live installation are unchanged.

The next isolated process-cold-restart probe exposed a real startup cycle. Docker
29.8 activates retained plugins before opening its API, while the allocator initially
required that API before listening. The failed probe hit its 30-second command bound;
its owned daemon and test data were cleaned up. Restart now reopens the locked journal
and serves metadata before Docker readiness. Every physical operation still verifies
the original engine identity. The final native fixture passes in 13.55 seconds,
including retained bytes, helper/daemon replacement and physical writer handoff.
The storage image rebuild takes 11.18 seconds, with a 7.00-second compile window.
Eight storage unit tests, affected lint and formatting pass. This fixture retains
propagated host mounts; replacement of the whole compute-host mount namespace remains
a separate topology qualification case.

The new private `computer-host` target now composes the qualified provider, allocator
and Docker daemon in one owned container. Its first solve exposed a planner defect:
transitive Rust image dependencies contributed source identity but not their exported
binaries. Expanding compiler units over the complete Bake context graph fixes that
failure. Publication still emits only directly selected targets, and asset/standalone
contexts follow the same dependency set. The real planner regression passes alongside
42 image-planning tests; the optional worker-quota experiment remains ignored.

The first successful composite image took 34.38 seconds. A startup probe caught a
group-writable fixture config, which was corrected to the production read-only input
contract. The next probe created a Computer and wrote retained bytes, then exposed
an overly strict restart check on Docker's own data-directory permissions. Restart
now preserves root-owned, non-writable Docker directories below the private 0700 state
root. The image also includes nftables and bounded per-Computer Docker logs.

The final code rebuild took 9.20 seconds. The final recorded image pass took 10.35
seconds after documentation changes; all runnable host layers were reused. The actual
composite-image native test passes in 35.49 seconds. It replaces the full container
and its mount/network namespaces, preserves Docker and provider resource identities,
starts a new process over the original files and denies provider user authority to
the guest certificate. This is stronger than the earlier helper-only restart fixture.
It is still isolated evidence, rather than Kubernetes or public-endpoint acceptance.

The host image's local configuration digest is
`sha256:05c7d166f694bf34a8e348ebc0105e5c0555919230f5961aad982acd88ebb694`;
its declared local size is 480,150,126 bytes. The provider executables retain their
previous hashes. Eleven host/storage unit tests pass. Combined all-target lint takes
33.16 seconds because selecting xtask together with the runtime changes the Cargo
feature union and checks an additional TLS/crypto closure. The native runner briefly
waits for the shared local build directory. Separate focused package invocations can
retain a stable feature selection; parallel commands do not eliminate Cargo's lock.

The five-target publication from `ff845bfb` took 318.790 seconds and produced immutable
host, template, Computers MCP, gateway and Console BFF images. The first combined Rust
compile took 4 minutes 33 seconds; its feature union rebuilt TLS/crypto and Surreal
dependencies as well as changed application sources. Console assets took 3.21 seconds,
and provider compilation reused its cache. Registry configuration recreated only the
BuildKit container in 1.1 seconds while retaining its cache. Overlapping export/SBOM
windows are not additive wall time. Evidence is in the ignored development receipt
`computers-provider/release-ff845bfb.json`.

The published host/template pair passes the native retained restart test in 39.4
seconds. Configured-capacity contract, Helm and renderer checks take 1.4, 1.2 and
25.3 seconds respectively. These chart changes reuse the published application images.
No additional throughput experiment blocks their installation.

Installation configuration exposed an old acceptance constraint: Bioma's profiles,
clients and policy had to equal the disposable local fixture. Recording publication
and upload policy already differed. Typed catalog validation and shared server
contracts remain enforced; the owner-local test now verifies selected Computer
resource exposure instead of requiring installation policy equality.

The first owner-local acceptance compile took 104.3 seconds because importing the
gateway catalog also selected its SurrealDB dependency closure. The repeat took 2.57
seconds. Combined enrollment/runtime/gateway lint selected another feature union and
took 76.6 seconds. Parallel Cargo entrypoints then waited on the same target directory.
This is measurable build churn: narrow catalog ownership and stable feature selections
belong in the efficiency checkpoint. Final checks run after installation inputs are
settled; an input change during an earlier check invalidates that receipt as designed.

The first installed Computers host became ready after pulling its template; both
control replicas reported capacity available at 14:50:26 UTC. Public reads exposed
database session deadlines under the installation's two-core CPU limit. Cgroup counters
showed throttling in 168 of 278 observed periods. Read-only EXPLAIN checks confirmed
that current outbox baseline and replay queries use the sequence index; an independent
connection completed both plans and a clock read in 0.443 seconds. Connection-level
backpressure remains a separate cause to investigate rather than assuming CPU alone.

Helm also waited on an existing Recording Hub crash while replaying a finished stream.
The fix and real-store regression were already committed in `2c5df379e`; its image had
not reached this installation. Publishing current Recording Hub took 142.178 seconds,
including a 45.604-second Rust compile and rebuilding its Python/Rerun runtime layer.
The installation now selects that immutable image and allows eight database CPUs.

That image exposed a second terminal-stream case: journal sequence 4356 had not been
accepted when its stream finished with `next_sequence = 4356`. A read-only database
query confirmed the exact terminal checkpoint. Hub now preserves such bytes with an
immutable quarantine receipt instead of blocking all ingest startup. Accepted duplicate
replay remains separate and unchanged.

Flux upgrade remediation also caused avoidable workload churn. A corrected revision
canceled the old health check and triggered rollback to a chart predating Computers,
which removed the new services before recreating them. Bioma now selects Flux
`RetryOnFailure` for upgrades, verified against the installed v2 HelmRelease CRD.
Health checks stay enabled and failures remain visible. Explicit qualified rollback
is still an installation operation.

Publishing Recording Hub and Computers MCP from `82f41298` took 361.586 seconds.
Changing the selected Cargo feature union triggered a 318-second optimized compile;
BuildKit reported 240 changed input paths. The Python/Rerun runtime layer was cached.
The export window was 27.122 seconds, including 25.054 seconds of timestamp
normalization. Stable feature selections and narrower source invalidation remain
measured improvement work; this was not a warm single-component iteration.

The subsequent storage-host candidate compiled in 14.050 seconds with cached provider
binaries. Its command took 91.003 seconds, including approximately 48 seconds waiting
behind the other xtask operation. Do not launch dependent image work as concurrent
xtask commands and then count queueing as compiler time. The fault fixture removes
private loop-device nodes and interrupts allocation before Ready publication, because
the installed container had missed a device created after its own startup.

Publishing the qualified storage host from `9d06bb1a` took 14.468 seconds and reused
the compiled runtime. The exact published image passed the missing-device,
interrupted-allocation and full host-replacement fixture in 31.62 seconds. The
installation reached Helm revision 142 successfully. Its pending Computer became
Ready after recovering the existing incomplete allocation. Recording Hub also became
healthy; the quarantined journal retained its exact SHA-256, 201484 bytes, terminal
cutoff 4356 and unchanged database revision 8711.

The first public terminal exposed a JavaScript receiver error that Node's native timers
do not reproduce. Calling a browser timer through a clock object's raw function
property throws `Illegal invocation`. The actual browser confirmed the exception;
the regression now runs the real terminal module with receiver-sensitive timers.
The frontend fix passed seven focused tests, TypeScript/Vite build and ESLint.
Its Console publication took 82.150 seconds, including a 45.27-second Rust compile.
No BFF Rust source had changed since the prior deployed binary; the only shared
lockfile difference added existing rcgen/uuid dependencies to xtask. Narrowing
lockfile invalidation to each runtime dependency closure remains concrete build debt.

Operation-receipt publication from `acba3a43` took 111.251 seconds for Computers MCP,
gateway and Console. The shared compile window was 87.300 seconds with 480 changed
input paths after changing the selected family inputs. Export took 5.624 seconds,
including 3.694 seconds of timestamp normalization; push took 2.392 seconds. The
retained host and template stayed on their qualified digests. Helm revision 144
converged with two replicas of each affected service.

At 16:17 UTC, the public Console reloaded its saved requests and read their durable
receipts. Create, Stop and Start for Computer `9942151c` now show Succeeded. The older
unresolved Create shows Recovery Required and retains its fence. The reads did not
redispatch lifecycle work. Headed Chrome retained NVIDIA RTX 4090 WebGL; WebGPU still
reported a fallback adapter. Warm public reads were 110–146 ms, while initial reads
during reload took 643–1064 ms. These are observations, not throughput acceptance.

The focused operation checks take about 37 seconds when warm. A one-line test lint
correction invalidated the v2 report's unrelated successful checks and required them
again. That cost remains an explicit input to the scoped-receipt workstream. The HTTP
fixture also corrected an invalid assumption: cancellation request acceptance does not
settle a lifecycle operation before the worker proves it remained undispatched.

Access-inventory and revocation publication from `16349e88` took 124.303 seconds
for Computers MCP, gateway and Console. Compilation took 70.297 seconds with 51
changed paths. Extraction took 5.765 seconds. Observed SBOM, provenance, timestamp,
export and push windows overlapped; the 44.330-second export window cannot be added
to their individual durations. Host and template digests remained unchanged. Helm
revision 145 installed the matching gateway catalog and application images.

The public Console issued browser access at 16:47:35 UTC. Clicking Revoke closed
the attachment within 897 ms of arming the browser observation; this includes
interaction overhead and is not a direct network-close measurement. Inventory became
empty while the Computer remained Ready at its previous lifecycle revision. A fresh
connection read `public-retention-check.txt` from the original home and returned UID
10001. Headed Chrome and the terminal canvas reported NVIDIA RTX 4090 WebGL; WebGPU
reported a fallback adapter. The inspected capture is in the ignored development
artifact `computers-provider/public-access-revocation-20260910.png`.

Post-rollout access briefly failed around 16:41–16:43 UTC. The gateway reported
five-second current access-token session read deadlines; some Console requests took
10–15 seconds. Reads recovered without a restart, with four subsequent access reads
at 119–169 ms. SurrealDB had an eight-core limit and used approximately two cores.
A new independent connection completed a clock query in 0.378 seconds, which does
not establish that persisted session records were readable during the failure.
Connection backpressure and record-level contention remain unproven causes. This
recurring availability defect stays open; the successful later journey does not
qualify rollout continuity.

The private CLI-ledger checkpoint passes 32 domain tests including its compile-fail
credential-formatting check, 46 store unit tests and nine application/HTTP cases.
Recorded command times were 36.919 seconds for the domain, 13.627 seconds for store,
and 43.691 seconds for the service. Their Cargo compile phases took 15.77, 13.53 and
22.92 seconds respectively. Selecting the standalone store tests then the service
compiled the shared store again under their different feature selections. Warm lint
took 2.93 seconds. These measurements cover the additive ledger and its browser quota
interaction; they do not establish public stock CLI performance or deployment.

The stock CLI worker checkpoint records 49 regular domain/service cases and one
native retained-lifecycle fixture. The combined regular command took 165.088 seconds;
76 seconds were compilation. Its first three store fixtures took 56.31 seconds,
while later store groups took roughly two to five seconds. That startup discrepancy
is observed without an established cause. Selecting both packages widened the test
compilation compared with the earlier focused native target.

The recorded native command took 112.059 seconds, including a 1.37-second Cargo
phase and 110.61 seconds in the fixture. It uses the previously qualified native
provider binaries and helper image, which are distinct from installed product image
acceptance. Two earlier retries were test churn: a process-exit assertion conflated
closed access with the stock client's blocking stdin shutdown, and reordering the
fixture exposed TLS initialization hidden in the browser check. Shared initialization
now precedes both consumers. Actual relay closure is asserted within five seconds;
a subsequent command cannot execute and the Computer stays Ready. The stock idle
ProxyCommand may await local input before its process exits. Public SSO pairing and
ingress remain unqualified at this checkpoint. No image publication was needed for
these local checks.

The public CLI application checkpoint passed 61 domain/service/transport cases in
137.675 seconds, with 57.87 seconds of compilation. The first three database fixtures
took 39.74 seconds; later groups took two to five seconds. The edge command passed
23 BFF/gateway cases in 39.184 seconds, including a 23.40-second compile. The native
retained fixture passed in 114.8 seconds. It now creates the grant through the real
HTTP pairing handlers across replicas, rejects confirmation replay, and uses the
public Computer UUID with the unchanged stock CLI. Its identity remains a test
fixture; public browser sign-in and installation acceptance are separate checks.

Those service checks recorded build identity
`sha256:90aa114b21d83c294a174d8bbf37ef55a4375d047b0423174b90ed5aec1cf6a0`.
A subsequent Helm test correction distinguishes the full preset's ingress from
the extension foundation's intentionally absent ingress. No service, transport,
domain or frontend input changed in that correction. The v2 aggregate discarded
the earlier entries when the test file changed. The correction reruns the owning
Helm check and lint; the preceding service results remain documented here instead
of repeating unchanged native workloads merely to repopulate the aggregate.
Both Helm cases pass. The five frontend contract/pairing cases pass, followed by
TypeScript/Vite, ESLint, generated-type drift and formatting checks. Vite took 2.19
seconds. Exact per-command times for the final inputs remain in the committed report.

At 18:24 UTC, the browser still displayed an OAuth callback failure loaded at
17:49:54 UTC. A fresh Console navigation succeeded; its session read returned 200
in 115 ms. No service restart was needed. This distinguishes the stale error page
from current availability without closing the earlier sign-in/rollout defect.

Publishing the three CLI application images from `ba2161c8` took 153.823 seconds.
The optimized Cargo phase took approximately 88 seconds after source freshness
reported 948 changed paths. The compiler's operating-system package layer also ran
again; cache eviction is a possible cause, not an established diagnosis. Extraction
took 4.443 seconds and the overlapping export window took 17.724 seconds. The
qualified host and template images were reused. The matching chart published as
`0.1.0-ba2161c8`, digest
`sha256:26327edafb5ff8ac5034c60178b0f3abb8dbafce322f62567e283bcf75172e41`.

The installation's existing bootstrap Job specification, trust references and mounted
catalog were reused with the new gateway image for the additive 0059 migration.
Job `veoveo-cli-ledger-ba2161c8` completed and verified runtime database authentication;
the active catalog remained unchanged. The coordinated transition then drains
Computers control workers before admitting new public CLI traffic. The compute host
continues running the retained Computer. Encoding this migration/drain dependency in
release coordination remains deployment debt; a new ledger must not depend on
accidental Kubernetes startup order.

Helm revision 146 became Ready at 18:40:06 UTC with both replicas of all three new
services. The compute host retained its original pod and zero restarts. The bootstrap
qualification ran from 18:37:55 to 18:38:03 UTC. The focused GitOps observer took about
sixteen seconds after publication; this excludes earlier build time and does not
measure the entire drain interval.

The first public browser pairing returned two 201 responses in 143 and 147 ms, but
the ten-second local callback deadline expired while Chrome's Apps on device
permission was still Prompt. The failure path revoked the issued grant in 129 ms.
The local stock callback independently answered an exact-Origin OPTIONS request
with 204. The frontend now resolves that permission and reachability with a bounded
credential-free OPTIONS request before creating any grant, and explains the browser
prompt. Its negative test proves that rejected local consent issues no access.
This is a real-browser gap that the mocked fetch test could not establish.

After allowing Apps on device for the Veoveo origin in Chrome's site settings,
public pairing authenticated the unmodified CLI. The shell returned UID 10001 and
the original retained file. Its variable and file survived more than one connection
lease. Revoking the named CLI returned 200 in 119 ms, removed it from inventory,
closed the SSH connection and rejected a new stock connection with HTTP 403. A fresh
browser attachment confirmed the post-confirmation command had not created its
marker and the retained file remained unchanged. A command submitted immediately
after clicking Revoke ran before the response had been confirmed; that does not
measure the revocation bound. Its owned marker was removed. The native fixture
remains the precise five-second relay-closure measurement.

The consent-first frontend publication from `d3832c35` took 54.111 seconds. Although
only Console TypeScript and documentation changed, narrowing the selected compiler
family from three services to Console reported 491 changed paths and caused a
31.81-second optimized compile. The operating-system and npm dependency layers were
cached. Reusing the BFF binary across frontend-only changes is still concrete build
work; calling this publication a cache hit would hide most of its cost.

Helm revision 147 installs the consent-first Console digest. With Apps on device
blocked, the actual page reports that no access was issued; it sends zero pairing
requests. With that permission allowed, the stock local OPTIONS response took 4.8 ms,
public begin/confirm took 113 and 117 ms, and final credential delivery took 5.5 ms.
These are individual warm observations. The page and stock CLI both confirmed
authentication. The inspected screenshot was returned inline by the headed-browser
tool because its explicit local output path was rejected by that tool's workspace
boundary. No local screenshot file was created for this observation.

Console Sign out closed the stock shell and a fresh command with the old credential
received HTTP 403 at 18:55:59 UTC. Existing identity-provider SSO then returned the
Console to a fresh sign-in; the old access inventory was empty. A new browser
attachment verified the post-logout marker was absent, read the original retained
file, and returned UID 10001. The site's local-device permission was restored to its
original default after qualification. The browser remains headed with hardware
NVIDIA WebGL; its SwiftShader WebGPU adapter is not accepted as GPU evidence.

Inspection traced the 491-path invalidation to `source-freshness.rs`: any input
removal marked every remaining file changed. Selecting Console after a three-service
build therefore refreshed unchanged BFF dependencies. The helper now refreshes the
directories affected by ordinary removals and preserves unchanged source timestamps.
Symlink changes keep conservative invalidation. Seven owning tests pass, including
real offline Cargo builds that require a Fresh result after an unrelated removal,
rebuild a directory-watching generator after deletion, and reject missing source
modules and embedded symlinks. The fixture group took 0.75 seconds after compilation.
No new service image or deployment is needed for this build-helper change. A measured
production comparison will use the next affected image publication; these fixture
results do not establish a new end-to-end release budget.


The CE-06 source recorder now preserves immutable per-attempt receipts and selects
results by admitted command identity. Its initial xtask closure contains 547 source
inputs. Sixteen owning cases pass, including actual mid-command input mutation,
concurrent publication, orphan recovery, tracked deletion across staging, ignored
symlink-target refusal and runtime coverage rejection. A warm recorder suite
ran in about 0.5 seconds, with 0.06 seconds in its test bodies. The final recorded
run, after the display optimization changed the binary, took 3.7 seconds including
compilation. The image-input group
still passes 13 cases in 1.55 seconds of fixture execution.

Qualification exposed two avoidable feedback costs. An unfiltered all-feature Cargo
metadata query attempted to resolve unused platform packages while offline; the
planner now filters to the observed host target. Creating the coverage profile after
the first successful suite legitimately invalidated that suite's input boundary.
The profile should be finalized before qualification; the required rerun was warm.
These are tooling observations, not an installed performance claim. V3 required
checks earned fresh receipts; old v2 results were not copied into the new format.

The first four-check display took 4.87 seconds; coverage verification took 5.37
seconds. Both recomputed identical Cargo/input boundaries per check. An invocation
now observes each distinct boundary once and shares identical source toolchain
observations within coverage verification. Subsequent concurrent warm observations
took 2.79 seconds for display and 2.78 seconds for verification. These are individual
measurements under different contention, not a controlled throughput benchmark.
Eleven immutable attempts occupy about 1.6 MiB for this transition. Input manifests
are currently inline; content-addressed manifest deduplication and bounded archival
remain storage-efficiency work as history grows.

Registering the next Computers check exposed one remaining global boundary: the
initial catalog was a single input file. Planner v2 splits it into owner catalogs
and includes only the selected owner's declaration, even when a test also reads
the broader `testing/` fixture directory. The seventeenth recorder case proves that
another owner's registration preserves a result while editing its own declaration
invalidates it. The real iteration-tools receipts also remained current while the
independent Computers runtime source and design changed.

The Computers runtime now has its own admitted 134-input source boundary. Its first
library run exposed a stale fixture: GetWorkspace had not been implemented after
production readiness began requiring an exact active workspace. Fifty tests failed
before exercising their behavior. The fixture now returns typed workspace state;
new cases reject absent, mismatched and terminating workspaces without mutation.
The corrected 76-case suite passed in 20.51 seconds, plus 9.48 seconds of compilation.
The initial failed attempt remains immutable history. Its successful replacement
uses the same command identity. All four iteration-tools results stayed current
while the runtime sources, fixture and owner catalog changed.

The execution launcher moved the Computer template into the existing shared Rust
artifact family. Its first current-checkout build took 73.381 seconds, including a
4.87-second optimized compile. APT fetched 52 MB of indexes in 13 seconds because
the snapshot invocation loaded live indexes as well as snapshot indexes; package
downloads themselves used the selected snapshot. Avoiding duplicate index acquisition
remains a concrete cold-image optimization. No provider recompilation was required.

Native qualification caught two integration errors before deployment. Setting OCI
WORKDIR to the retained mount collided with the provider's reserved workspace. A
root-owned login profile now selects the retained home while OCI WORKDIR stays
`/sandbox`. The supervisor also deliberately denies memfd_create. Bounded stdin now
uses an anonymous O_TMPFILE in private ephemeral storage, reopened read-only, with
O_EXCL preventing a permanent link. The provider's seccomp restriction remains intact.

The directory correction rebuilt in 4.578 seconds. The final launcher correction
rebuilt in 5.290 seconds, including a 0.99-second optimized compile. These are individual
warm local builds, not published release timings. The native execution fixture then
passed in 13.92 seconds after 4.23 seconds of compilation. It verifies the actual
packaged binary, 100,000-byte input, enabled command-log privacy, initial shell cwd,
and Stop fencing of a detached descendant after an uncertain timeout. Failed fixture
Computers and their isolated block homes were removed by their owners; installed
user Computers and their retained storage were not used by these experiments.


The automation ledger's first native run spent 63.12 seconds retrying an invalid
schema before reporting readiness failure. The shared isolated-store fixture now
fails immediately on deterministic migration or configuration errors. The same
broken migration then reported its exact statement in 4.85 seconds, including
container startup. Healthy tests retain transient connection retries. Syntax-only
validation accepted both an incompatible array/child declaration and a protected
SurrealQL variable; native execution caught them before deployment. The corrected
five-case grant suite ran in 2.92 seconds after 3.12 seconds of compilation. These
are individual local observations, not release timings.

The additive automation migration does not require unrelated service rebuilds:
ordinary runtime connections do not run the migration catalog. Deployment must use
the new qualified migration runner before enabling the new surface. A rollback
retains that runner because an older catalog rejects a database ahead of it.


Adding command encryption to the Computers domain changed its dependency feature
composition and rebuilt the SurrealDB client graph, even though selected dependency
versions were unchanged. The first pure six-case test took 1 minute 48 seconds to
compile and 0.01 seconds to execute. This was a local Cargo build, with no provider
recompilation or installation restart. Domain feature boundaries remain a measurable
source of iteration cost; the following recorded warm check separates that compile
cost from test execution.
