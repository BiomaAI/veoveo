# Build And Deployment Iteration Audit

Status: implementation authorized on September 6, 2026; core changes are committed
and the shared charts are active with the Bioma reference configuration. Local compiler
ABI and throughput experiments have recorded results. Live metadata-only publication
preserves every running Pod. Disposable component-selected execution now passes the independent native Git/OCI
fixture. Cluster coordination now serializes cooperating disposable installers;
GPU compiler-family admission remains pending. The local
build-engine cache trial has recorded results; independent build storage and hosts
remain unmeasured.
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
| Common GPU control compiler experiment | Built Stream and Reason together through the existing Bookworm artifact recipe, with explicit package, binary and cache overrides | One Cargo action completed in 351.3 s; both candidates pass loader/CLI checks and Stream passes initialized service startup; replay stops at the existing missing Artifact-read credential boundary, before GPU execution; family admission remains pending |
| Compiler runtime probes | Added a Rust candidate probe with explicit App input, private listener ownership, exact executable digest, and verified cleanup; video smoke builds omit the unused conformance CLI | Native Stream startup preserves the installed Deployment, Pod identity, and restart count; GPU acceptance is a distinct receipt outcome and remains unverified |
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
| Builder resources and durable storage | Enforced twelve-CPU budget, 2.29× matched compiler speedup, cache retention, restored disk reserve | Separate physical build disk or host is unavailable; that comparison remains unmeasured |
| Presentation and normalized GPU dependency inputs | Frontend-only staging executes no Rust; both source-only UAV revisions meet the thirty-second target; headed Console refresh passes | Complete for those input boundaries |
| Exact staging and elapsed timing | One selected solve, per-target identities, command-level preparation and failure timing | Complete |
| Reusable Rust compilation | Shared ordinary compiler families, extracted recording libraries, narrow flight harness, controlled sccache and Bazel trials | Stream/Reason common-family candidates require actual hardware workload acceptance; recording-read integration needs live verification and Reason's accelerated model-input path currently prevents admission |
| Independent release inputs and selected execution | Per-release lock projection, selected publication, local UI loop, native zero-unselected-write evidence, cooperative cluster lock | Complete for the existing independently owned releases; general ownership transfer is outside this build/deploy goal |

The experiments support retaining Cargo and the durable BuildKit worker. A build-engine
migration has no demonstrated overall advantage from the measured local cases.

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
Actual Stream/Reason hardware workload acceptance, common-compiler image admission,
and Reason's accelerated model-input path remain outstanding. The integration is not
by itself a measured compiler speedup or a live deployment receipt.

The consumer test build created another combined dependency variant. Scoped hourly
Cargo maintenance removed 26 superseded executables and 245 incremental variants,
with 29.55 GiB of estimated reclaimable blocks. Current executable links and dependency
libraries remained protected. Host availability returned to 386 GiB; the exact
maintenance receipt is `output/development/task-read-cargo-cache-applied-20260908.json`.
