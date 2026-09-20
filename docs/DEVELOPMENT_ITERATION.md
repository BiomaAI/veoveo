# Development Iteration

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Docker Buildx 0.37.0 and BuildKit 0.33.0 | repository-managed Bake execution and cache worker |
| OCI Image Spec | immutable `linux/amd64` runnable manifests and attested publication indexes |
| Git commit identity | exact source revision and reproducible source timestamp |
| Helm values | complete registry and image-digest map consumed by GitOps |
| Chrome DevTools Protocol | headed hardware-browser acceptance and request cancellation evidence |
| Rerun 0.36.3 RRD | bounded live history and governed archive playback |
| `veoveo.io/image-affected-plan/v1` | repository-owned affected-surface closure |
| `veoveo.io/development-image-lock/v1` | repository-owned non-release deployment closure |
| `veoveo.io/component-publication/v1` | exact component lock composition with retained artifact inputs; no cluster mutation claim |
| `veoveo.io/gitops-convergence-evidence/v3` | repository-owned reconciliation mode, observation start, exact Flux source revision, root apply, Helm inventory, rollout, and readiness evidence |
| `veoveo.io/console-apps-browser-acceptance/v1` | composed signed-in Console App catalog, server grouping, per-App headed render, and hardware adapter evidence |
| `veoveo.io/uav-live-view-browser-evidence/v8` | focused authoritative-camera pixels, event-derived source-to-render and motion-to-photon p95, cadence, isolated-viewer products, sensor separation, and simulation real-time-factor evidence over a running simulation |
| `veoveo.io/uav-recording-browser-evidence/v2` | source-clock and camera-pane evidence for one live governed recording |

## Operating Model

Iteration preserves the running simulation unless the changed contract requires a
restart. Source compilation, image publication, rollout, and visual acceptance are
separate checkpoints. A failure resumes at its own checkpoint and does not replay
earlier successful work.

The immutable runnable manifest digest is the handoff between checkpoints. A staged
image and its later qualified publication must share that digest. GitOps receives a
complete digest map, while Kubernetes rolls only Deployments whose selected digest
changed. Qualification adds supply-chain attestations after behavior is accepted.

The accepted [contract evolution](CONTRACT_EVOLUTION.md) permits maintained browser
and SDK harnesses and headless checks for nonvisual behavior. Required visual and GPU
acceptance retain hardware proof. Dependency upgrades are separately qualified work;
an ordinary consumer edit may retain its supported pins. The v3 report now supports
per-check source reuse through owner-reviewed input declarations, as described in
[Continuous Integration](CONTINUOUS_INTEGRATION.md). Unclassified commands retain a
conservative repository boundary and cannot claim reusable environment coverage.

Select checks from the changed input closure. Run focused feedback while editing,
qualify the changed component before deployment, and compose the complete supported
release profile at its release checkpoint. Resume a failed stage from unchanged
qualified artifacts. A UI asset edit must reuse its unchanged Rust binary, and a
no-op deployment must preserve workload identities. Record cache invalidation causes,
rerun causes, phase timings, and any unrelated workload changes in the iteration audit.

## Component Release Inputs

Prepare a configuration-only update from a committed installation checkout:

```sh
cargo xtask release components \
  --profile deployment.json \
  --base-lock deployment.lock.json \
  --component platform \
  --refresh-configuration \
  --lock-output output/releases/platform-config.lock.json
```

Use the component IDs declared by the installation profile. Repeat `--component` for
an exact set. The output retains unrequested components and their dependency inputs;
it does not rebuild images or contact Kubernetes. The output path must be new.

| Input | Requested component behavior |
|---|---|
| `--refresh-configuration` | use the current committed installation values and public resource inputs |
| `--source-revision SOURCE=COMMIT` | use an exact full Git commit for source charts and source values |
| `--image-evidence SOURCE=PATH` | import qualified image evidence after checking its OCI publication and runnable digests |

These inputs can be combined. Omitted inputs retain their locked identities. The base
and current profiles must share their destination, component ownership, and platform
selection. A complete publication handles changes to those boundaries. The adjacent
publication receipt records the selected inputs and retained owners. Enterprise
activation continues through its GitOps repository.

## Evidence And Defect Records

`output/` contains disposable generated evidence, build products, downloaded tooling,
and runtime caches. It is never the authoritative location for a defect, follow-up, or
engineering decision. A generated report may be cited by path while a run is being
examined, but losing the directory must not lose the issue.

Repository-wide iteration defects and their measured costs belong in `Recorded
Iteration Sinks` below. A component-specific correctness defect belongs in that
component's source-controlled design or operating document. Record the observation,
the affected boundary, and the required correction there before ending the work that
found it. Do not maintain Markdown notes, ad hoc TODO lists, or defect backlogs under
`output/`.

## Fast Path

Begin from a clean committed revision. The affected planner also sees working-tree
changes, which is useful while coding, but staging itself requires the exact committed
revision named on the command line.

```bash
cargo test -p <changed-package> --offline
cargo xtask image affected --since origin/main --format json \
  > output/development/affected.json
```

Inspect `imageTargets` and every broadening reason. Repeat `--target` to stage the
exact selected set in one Bake solve. Compatible Rust targets share compilation, and
the stage receipt records each runnable digest. Multiple receipts may also be merged.

```bash
revision="$(git rev-parse HEAD)"
cargo xtask image stage \
  --target <affected-target> --target <another-affected-target> \
  --push-registry <host-reachable-registry> \
  --pull-registry <cluster-reachable-registry> \
  --registry-transport <tls-or-insecure-http> \
  --revision "$revision" \
  --evidence-output output/development/selected.stage.json

cargo xtask image development-lock \
  --base-lock <qualified-deployment-lock> \
  --stage-evidence output/development/selected.stage.json \
  --output output/development/image-lock.json \
  --values-output output/development/images.values.json
```

Commit or otherwise submit `images.values.json` through the installation's ordinary
GitOps repository. Do not patch a Deployment or reuse a mutable tag. The controller
converges the digest change and leaves the simulator untouched when its digest did not
change.

Deployment-profile operations use the focused harness:

```bash
cargo xtask smoke profile-validate --profile <profile.json>
cargo xtask smoke profile-up --profile <profile.json> --lock <qualified-lock.json> \
  --component <component-id> --receipt-output <new-receipt.json>
cargo xtask smoke profile-gpu-verify --profile <profile.json>
```

These commands compile `veoveo-deployment-smoke`, not the broad protocol and visual
smoke graph.

Observe a GitOps rollout with the same focused harness. The expected revision is the
complete Git object fetched by the source and applied by the root Kustomization.

```bash
cargo xtask smoke gitops-converge \
  --context <kubernetes-context> \
  --source <namespace/git-source> \
  --root <namespace/root-kustomization> \
  --release <namespace/platform-helm-release> \
  --release <namespace/extension-helm-release> \
  --revision <full-git-commit> \
  --deployment <namespace/platform-deployment> \
  --deployment <namespace/extension-deployment> \
  --evidence-output output/development/gitops-convergence.json
```

The command defaults to passive observation and issues no reconciliation annotations.
Use `--reconciliation request` when the operation should explicitly wake the named
Flux controllers. Evidence records the mode, verification start time, and separate
source, apply, Helm, rollout, and readiness phases. A timeout records the failed phase.
Source and Helm readiness use Kubernetes watches; the root phase checks status and
terminal Helm failures at intervals of at most two seconds.

For passive publication latency, start the observer against the prepared local commit
before pushing it and retain the publication timestamp. A run against an already-active
revision measures observer overhead. It does not measure event-driven deployment latency.
The [verifier design](../testing/deployment-smoke/DESIGN.md) defines the evidence boundary.

## Release Readiness Checklist

Run the resource preflight before any release expected to compile several Rust image
families or retain a large BuildKit export. The default 320 GiB growth allowance covers
the observed 294.57 GB managed-worker peak, while the 20 percent filesystem reserve
keeps the host below the kubelet image-GC boundary with room for ordinary runtime writes.

```bash
cargo xtask release preflight \
  --expected-growth-gib 320 \
  --kubernetes-node <node-name> \
  --namespace <workload-namespace>
```

The command fails when projected growth would consume the reserve, or when the selected
node is not Ready or reports disk pressure. It reports managed BuildKit usage through
`buildctl`; Docker's aggregate build-cache summary is not evidence for this worker.

Complete these checkpoints in order. Preserve failed evidence beside the later passing
run because a retry does not erase the cost or the cause.

- [ ] Start from a clean commit and resolve the complete source revision from Git.
- [ ] Run `cargo xtask image affected --since <accepted-revision>` and publish only its
      image, chart, SDK, or compatibility closure.
- [ ] Run package and contract checks through `cargo xtask test-report run`; inspect the
      committed green report before publication.
- [ ] Verify the host budget, managed BuildKit cache, node Ready condition, disk pressure,
      and retained Evicted pod inventory with `cargo xtask release preflight`.
- [ ] Confirm that the host push registry and cluster pull registry are reachable through
      their declared transports before starting a solve.
- [ ] Keep registry artifacts, persistent volumes, rollback images, and the warm builder.
      Reclaim only superseded material or rebuildable cache selected by exact identity.
- [ ] Run direct MCP conformance for every changed server, then test the composed gateway
      policy decision for each projected resource, prompt, and tool.
- [ ] For MCP Apps, require the signed-in `/console/api/apps` catalog with no degradations,
      the expected `ui://` resource set, server-grouped navigation, and every App rendered
      by `cargo xtask smoke console-apps-browser-verify`. Each capture must complete a host
      bridge round-trip and reach the App's declared settled state; a loaded document or a
      transient `connecting`, `reading`, `rendering`, or `discovering` label is not evidence.
- [ ] Reconcile Flux to the complete commit, require exact source and applied revisions,
      require every Helm inventory and Deployment to become Ready, and retain any failed
      convergence record.
- [ ] Before visual evidence, attach to headed Chrome and probe WebGPU and WebGL separately.
      At least one must name hardware NVIDIA; record software WebGPU even when WebGL passes.
- [ ] Remove only stale acceptance targets, verified-unowned Chrome singleton links,
      temporary pod binaries, and controller-superseded failed pods after acceptance.
- [ ] Repeat the host and node preflight after publication. Do not end a release with the
      node near eviction pressure.

## Acceptance Checkpoints

Use the narrowest checkpoint that can falsify the change.

| Change surface | First acceptance |
|---|---|
| Rust logic or schema | package unit and contract tests |
| Dockerfile or image payload | affected-target stage and digest inspection |
| Helm values or templates | profile validation and Helm render |
| Deployment wiring | digest rollout plus readiness for the changed workload |
| Recording ingest or playback | focused recording scenario against existing services |
| Simulation renderer or camera | focused headed-browser pass against the existing session |
| Mission or physics | full flight acceptance |
| Release candidate | qualified image publication and complete profile closure |

The focused browser pass never starts, stops, pauses, or commands the simulator:

```bash
cargo xtask smoke uav-showcase-browser-verify \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222

cargo xtask smoke uav-recording-browser-verify \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222
```

The live-view command reads the running simulation and current leader camera, opens
dedicated Console windows for authoritative live cameras, verifies headed hardware
graphics, proves distinct native products for simultaneous viewers, checks that physical
sensor cadence remains independent, enforces a 12 FPS delivered floor plus reactive
85 ms source-to-render and 250 ms motion-to-photon p95 gates, captures evidence, closes every viewer, and proves that
simulation time advanced. It does not open Stream or Recording. A browser failure
can therefore be retried without repeating a flight or depending on another consumer.
The command has its own
`veoveo-browser-smoke` dependency graph and builds the MCP conformance client only when
an actual run requires it.

The Recording-only command does not depend on a Stream or live-camera session. It
holds one live Rerun receiver for 120 seconds, requires native Following mode within two
seconds, compares its final simulation timeline against the running source, rejects more
than one second of end-to-end lag, and requires the leader-camera pane to change without
remounting the viewer.

The full UAV acceptance remains the gate for mission, takeoff, landing, simulator epoch,
or producer behavior. It is not a routine browser retry.

## Runtime Pressure Diagnostics

Each runtime boundary reports the counter that identifies its own pressure. Operators
should not infer producer health from browser latency.

| Boundary | Evidence |
|---|---|
| Producer to forwarder | offered/sent/replaced source counters |
| Forwarder durable queue | queued and maximum bytes, stream count, pending batches and Blueprints, finishing streams |
| Hub authenticated ingest | accepted batches, messages, and bytes; duplicate batches; materialization backlog batches and bytes; last successful append |
| Hub materialization | opened/frozen segments, quarantine, Blueprint publication and rejection |
| Live Recording playback | current live segment bytes, bounded history seconds, video preroll seconds, canceled and failed browser requests |
| Authoritative live view | logical-camera revision, shared encoded-product identity, hardware encoder, frame age, connected viewers, stream-delivery failures |
| Browser | hardware adapter, video advance, decode identity, Rerun network mode, request cancellation, screenshot digest |

The forwarder uploader is event-driven. Durable enqueue wakes it immediately, and a
durable acknowledgement wakes producers waiting for queue capacity. Network failures
retain bounded exponential backoff because no local event can make the remote endpoint
healthy.

Authoritative live view is event-driven. A logical-camera mutation activates or replaces
one simulator-hosted camera definition and continuous RTX/NVENC product. A viewer
operation authorizes one browser instance to consume that camera's exact H.264 access
units. Product state and WebSocket delivery wake their consumers directly. No controller
polls or replays a healthy simulator, camera, product, or browser authorization.

Recording live playback watches filesystem changes and transmits only static context,
one live-profile-compacted recent-history bootstrap, and newly durable data. It does not
scan an entire active recording for every new viewer or leave initial compaction on the
browser rendering thread.

Recording Hub exposes process-lifetime ingest diagnostics through the authenticated
`/internal/recording-ingest/v1/diagnostics` route. The counters advance at the durable
journal and database commit boundary. Materialization backlog remains visible when
projection work fails after that commit, which distinguishes accepted source traffic
from downstream segment construction without placing metrics on the unauthenticated
listener.

## Iteration Improvement Register

This register is the priority source for iteration work. The measured sink tables below
remain an observation archive; an archived observation is not an open task unless this
register says it is. Each active item names a falsifiable acceptance boundary rather
than a general request to make builds faster.

### Completed In The Current Improvement Cycle

| Boundary | Owning component | Completed correction | Acceptance evidence |
|---|---|---|---|
| UAV control-plane rollout isolation | UAV chart and runtime contracts | `uav-sim-mcp` has its own CPU Deployment, Service, and network policy; authenticated NDJSON events replace the removed shared socket | chart contract tests and the restart verifier prove that replacing the MCP pod leaves the GPU simulator pod identity unchanged |
| Registry authority and reachability | image orchestration | staging accepts one typed host push authority, cluster pull authority, and transport; it verifies `/v2/` before BuildKit starts | malformed revisions fail before worker acquisition, unreachable registries fail at preflight, and the local registry returns HTTP 200 |
| Stable BuildKit worker | image orchestration | local and registry operations share one registry-capable builder configuration and preserve its cache state | `cargo xtask image builder ensure` retained builder `veoveo` with Buildx 0.35.0, BuildKit 0.31.2, and the 240/320/80 GB garbage-collection envelope in 7.57 s |
| Selected Rust build closure | image planner and Rust builder families | a selected target builds only its declared package and binary instead of every member of the compatible family | current plans for Console BFF, Map MCP, and UAV MCP each contain one package and one binary and resolve in 3.24-4.49 s |
| UAV dependency boundary | UAV image graph | pinned simulator, PX4, Cesium, native, and Python payload work lives in `uav-sim-dependencies`; runtime source is a thin overlay | the runtime plan selects the dependency and runtime Bake targets without introducing a Rust build unit |
| Long-running build visibility | BuildKit evidence adapter | bounded phase and vertex transitions stream while Bake runs; the complete machine event trace remains in immutable evidence | formatter and image-orchestration tests cover progress reduction and bounded emission |
| Deterministic GitOps convergence | focused deployment harness | the harness requests Flux reconciliation, consumes Kubernetes watch events, verifies the exact source and applied revision plus populated Helm inventories, then attributes fetch, apply, release, rollout, and readiness time | typed unit tests reject stale generations, wrong revisions, and empty inventories; failed phases still produce create-only evidence |
| Recording ingress visibility | Recording Hub | the authenticated ingest path exposes accepted traffic, duplicates, materialization backlog, and last-success state without logging identities or secrets | all 32 Hub unit tests, five spool integration tests, and strict Clippy pass; the focused diagnostics test completes in 4.11 s |
| Focused composed-flight harness | smoke harness ownership | flight scenarios compile the client and conformance helper; server-owned Stream response types no longer import its runtime | Cargo graph rejection tests exclude database, task and recording implementations; a verifier comment edit rebuilds only `flight-smoke` in 2.66 s, and three warm dispatches take 0.87–0.90 s |

### Active Follow-Ups Worth Fixing Next

The [build and deployment audit](BUILD_DEPLOY_ITERATION_AUDIT.md#delivery-record)
records the September 7–8 implementation and measured image boundaries. Flux watch
labels, content-derived rollout triggers, declared worker resources, exact staging,
complete timing, normalized UAV parents, and isolated presentation inputs are now
implemented. Shared recording libraries exclude service lifecycle dependencies.
The authenticated local Console supports source refresh without a document reload.
Controlled BFF/gateway source edits now compile 2.29× faster at twelve CPUs than at
four, with identical measured binary digests and warm dependencies.
The shared charts and Flux changes are active with the Bioma reference configuration;
passive live publication preserves unchanged workload identities. An isolated second
worker reuses unchanged BFF/gateway artifacts in 4.6 s. The same source edit takes
39.5 s on the durable worker and 369.5 s with fresh Cargo caches, which supports
preserving worker state for iteration. Both matched pairs produce identical artifacts.
Stream and Reason now use the shared Rust 1.98.1 control compiler and pass installed
RTX 4090 replay with the Bioma configuration. Reason Rust edits stage in 32.6 s;
Python runner edits stage in 11.1 s without executing Cargo. Each edit replaces only
its executable layer.

### September 17 Reactive Delivery Observations

The RMCP 3.4.0 upgrade crosses the gateway, BFF and hosted MCP servers. Its
qualification passed a workspace check in 152 seconds and workspace Clippy in
43.85 seconds. The headed Workspace fixture took 44.36 seconds on its final run;
an earlier run took 47.3 seconds. Repeating that fixture without a relevant input
change adds avoidable latency.

Nineteen immutable receipts in implementation commit `8b5e14d8` added 250,372
lines. Several commands still select the broad 2,531-file input manifest. Add
reviewed scoped descriptors for the gateway test command and BFF Clippy, then
measure receipt bytes and serialization time. Preserve historical evidence while
reducing repeated manifests through a separately versioned receipt format.

The image graph uses four independently cached Rust families: Bookworm and Trixie
on 1.97.1, plus the browser and control families on 1.98.1. The control compiler
installs 1.98.1 over its pinned 1.98.0 base image. Shared dependency updates compile
across those caches. Evaluate consolidation within compatible system-library and
feature boundaries using matched source edits and binary qualification; changing
only a cache identifier cannot safely merge incompatible artifacts.
The four compiler invocations took 7m 15s, 25m 47s, 28m 55s and 29m 21s while
sharing the twelve-CPU builder. These overlapping durations are not additive.
The next comparison should retain the selected package set and Cargo feature graph
as well as the source edit. The affected planner currently treats a lockfile change
as affecting every workspace image; operator selection still distinguishes active
MCP consumers from unrelated runtime services.

The trace shows large runtime-parent transfers starting after compilation, at
18:28 UTC, although parent metadata was resolved near the beginning of the build.
Active layers included 9.66 GB and 4.63 GB downloads. Qualify prefetching the exact
selected runtime parents during compilation, with a bounded disk budget, to avoid
putting this network work after the compiler on the critical path. Preserve these
parents in the durable worker cache and compare an unchanged rebuild before claiming
a steady-state cost.

Requesting deployment-smoke help after the SDK change rebuilt its focused harness
in 44.55 seconds. Its artifact is reusable for rollout. Read an unchanged command's
source or reuse the built binary when only discovering arguments.

The first 15-image stage took 36m 57.907s. GitOps convergence after publication
took 59.067s, including the schema bootstrap and readiness checks. Installed
verification then exposed a cold-catalog admission defect, which required a
gateway correction and another image stage.

That correction selected the same nine-package Trixie family and identical Cargo
cache identities, yet rebuilt third-party dependencies. BuildKit reported a new
target cache mount created at 19:07 UTC; the previous target mount was absent.
The worker requests 22% free space, or about 376 GiB on this host, while only
232 GiB remained available. Cache eviction under that policy is the leading
explanation, although the daemon log did not retain an explicit eviction record.
Qualify retention against the host's actual storage budget before measuring a
warm rebuild. A stable cache key cannot preserve an evicted mount. Do not enlarge
the cache without checking room for the cluster, registry and build peak.
The correction stage took 13m 56.610s, including 12m 57s in Cargo. It is a
cache-loss measurement, not a warm iteration result. Only the gateway digest was
selected for deployment. The build selected the whole Trixie family to preserve
its Cargo feature graph, which also packaged eight unchanged runtime targets.
Separating compiler-family selection from runtime-image selection would avoid
that packaging work without changing dependency features.
The gateway-only GitOps update converged in 24.601s. Publication and rollout
were small compared with the avoidable rebuild.

### Deferred Or Separately Owned Work

| Boundary | Disposition |
|---|---|
| Dedicated build storage and remote-host comparison | deferred by user direction on September 8; no separate physical disk or host is available. Future trials should retain durable Cargo caches and compare matched source edits against the twelve-CPU baseline. This does not block building, deploying, or accepting Veoveo |
| Component ownership migration | general raw-resource adoption and ownership transfer belong to a future atomic-release split; exact selected execution, per-release inputs, native scope evidence, and cluster coordination are implemented |
| Full release attestation and large inherited-image qualification | reserved for release acceptance; development staging must not pay this cost before behavior is accepted |
| GPU-renderer startup and live-camera latency | runtime performance work under the UAV design, not an image-orchestration fallback or a reason to weaken GPU acceptance |
| Provider and external network recovery | owned by the qualified provider profile under contract evolution; current Media completion remains webhook-only |
| Documentation publication automation | useful, but it does not block the source-to-running-workload fast path |

## Iteration Budgets

Budgets are regression signals measured on a warm developer host. A budget miss does
not authorize skipping evidence; it identifies the phase that needs repair.

| Checkpoint | Warm budget |
|---|---:|
| Package unit test after a local edit | 10 s |
| Affected-target plan | 10 s |
| Ordinary Rust target stage with warm Cargo and base cache | 30 s |
| Python GPU overlay stage with warm base lineage | 30 s |
| Focused deployment harness dispatch | 2 s |
| Focused browser harness dispatch | 2 s |
| GitOps digest convergence, excluding application startup | 30 s |
| Cached Isaac renderer startup and readiness | 90 s |
| Focused headed-browser acceptance | 3 min |
| Full flight and visual acceptance | 5 min |
| Full qualified platform closure with warm cache | 8 min |

The v2 BuildKit record separates compile, SBOM, provenance, timestamp normalization,
export, and push. Diagnose the largest phase before changing tools. A cached compile
with a slow export is not a Rust build problem. A cold SDK/base extraction with no
source change is a cache-retention problem. A slow smoke dispatch that compiles
unrelated crates is a partitioning problem.

### Current Warm Checkpoints

The earlier control-plane measurements below remain useful baselines. The September 7
image experiments now meet the warm 30-second staging budget, including two distinct
UAV source revisions. Single runs establish these checkpoints, not latency distributions.

| Checkpoint | Measured | Budget | Result |
|---|---:|---:|---|
| Affected-target plan | 3.43 s | 10 s | pass |
| Slowest sampled selected-target plan | 4.49 s | 10 s | pass |
| Focused deployment harness dispatch | 1.21 s | 2 s | pass |
| Recording ingest diagnostics test | 4.11 s | 10 s | pass |
| GitOps controller convergence against a healthy fixture | under 1 s | 30 s | pass |
| Managed builder ensure without replacement | 7.57 s | diagnostic only | stable worker retained |
| Console frontend-only revision | 26.0 s | 30 s | zero Rust execution; binary layer retained |
| Two distinct UAV source-only revisions | 14.6 s and 15.6 s | 30 s | all 37 dependency layer blobs retained |
| UAV unchanged stage after integrity-hash optimization | 2.8 s | 30 s | same runnable digest |
| Map and Stream presentation-only revision, one solve | 12.1 s | 30 s | both binary layers retained; zero Cargo execution |
| Source-only UAV runtime stage | not remeasured | 30 s | open pending exporter correction |

## Recorded Iteration Sinks

On September 11, `cargo xtask test-report show` took 32.863 seconds over 75.06 MiB
of immutable receipts. Reading each receipt once and sharing one Cargo dependency
graph within the invocation reduced the next warm run to 6.583 seconds, including
three additional receipts. The recorder's 18 focused tests pass, including rejection
of corrupted superseded history and unindexed attempts. Each new command observes
source and environment again. This local comparison identifies a repeated iteration
cost; it does not claim a general filesystem throughput result. Input manifests still
repeat across receipts, and content deduplication remains a separate improvement.

The September 6 audit reproduced these open sinks at revision
`fd87d2197bbfcd52fa36b397a1666481a74b74e9`. Exact commands, retained trace identities,
measurement limits, and corrections are recorded in
[the audit](BUILD_DEPLOY_ITERATION_AUDIT.md).

| Sink | Observation | Required correction |
|---|---|---|
| Chart version appears in runtime Pod annotations | identical Bioma values rendered with two chart versions change 16 platform and six UAV Pod templates, with identical container specifications | replace chart-version rollout triggers with actual runtime-input checksums |
| Flux values watch marker is an annotation | both live values ConfigMaps lack the label required by the default helm-controller selector; releases reconcile every five minutes | generate the documented watch label and verify a values-only update without forced Helm reconciliation |
| Managed builder capacity is undeclared | live worker has a four-CPU quota on a 32-thread host and lifetime throttling, while builder creation declares no CPU budget | declare and validate worker resource allocation, then measure quota changes against runtime health |
| Build and runtime storage compete below the retained reserve | zero-growth preflight fails with 278 GiB available against 366 GiB required; BuildKit holds 215.68 GB and main `target` holds 227 GiB | give durable build state an explicit storage budget independent of runtime retention |
| Publication locks are outside build timing | exclusive source and builder locks survive the entire stage; `run.json` begins after those waits and preparation | expose exact multi-target staging and measure the entire command lifecycle |
| A failed release delays the Git commit that fixes it | Map's failed image held a 20-minute Helm health check while the corrected digest was already fetched | both Flux controllers now cancel obsolete health checks; the isolated Rust regression converges the fixed revision in 21.6 s against five-minute timeouts and preserves application Pods |
| Image assembly removes traversal permission from new asset directories | Map fails at startup because an octal `COPY --chmod=0444` applies to the parent directories as well as HTML | use `a=rX` and check readability as the image's runtime user; the corrected Map and Stream builds pass |
| Map startup replays unrelated operational events | two canonical Map changesets share an outbox containing about 16.8 million events, exceeding the startup probe budget | recover from indexed canonical Map commits through a transactional committed head; the populated migration regression and all 164 Map/store tests pass |

The current controls address the main observed sinks:

| Sink | Control |
|---|---|
| One all-images build after any edit | affected target and consumer closure |
| One selected Rust image compiling its entire builder family | selected-target package and binary closure; compatible group members still share one Cargo invocation |
| Release attestations on every test | staging without release attestations, followed by digest-preserving qualification |
| Rewriting large inherited image layers | commit-timestamp clamping with clean reproducibility proof |
| Rebuilding pinned simulator dependencies after a runtime-source edit | stable `uav-sim-dependencies` payload beneath the source-only runtime overlay |
| Mutable developer tags | development image lock and complete digest values |
| Full smoke graph for deployment commands | `veoveo-deployment-smoke` partition |
| Full smoke graph for browser retries | `veoveo-browser-smoke` partition |
| Repeating a flight after browser failure | browser-only acceptance over the running session |
| Serial browser tabs masquerading as simultaneous viewers | separate visible headed windows synchronized at the advancing-video checkpoint |
| Polling an empty recording queue | enqueue and capacity notifications |
| Re-reading an unchanged ingest identity and committed checkpoint for every live sample | serialized authorized-stream checkpoint with transactional revision and sequence comparison |
| Aggregating retained producer batches for every quota decision | deterministic fixed UTC quota-window counters updated atomically with the accepted batch |
| Rediscovering the same writing segment for every source batch | active-segment checkpoint evicted before rollover and rehydrated from the catalog after restart |
| Replaying a second renderer mirror of healthy simulation state | removed; the authoritative simulator owns camera transforms and encoded products |
| Guessing where Recording latency lives | boundary-specific queue, ingest, playback, and browser counters |

The baseline measurements that motivated these controls remain part of the source
record. They identify the dominant phase and keep later improvements comparable:

| Run | Measured result | Dominant phase |
|---|---:|---|
| Coordinated 27-image release | about 21 min | image-family fan-out |
| Shared Debian and Rust action in that release | 17 min 20 s | broad optimized compilation |
| Stream and Reason image families | 15 min 39 s and 15 min 44 s | repeated reverse-dependent compilation and export |
| Cached simulation-runtime publication | 3 min 8 s | source-date-normalized layer rewrite and export |
| First `helm-config` smoke dispatch | 2 min 42 s | unrelated Rerun and Surreal dependency compilation; the warm run compiled in 0.57 s and completed in about 7 s |
| Targeted UAV verifier help dispatch | 2 min 19 s | broad smoke dependency compilation before argument handling |
| Gateway-only incremental publication | about 2 min end to end | Cargo consumed 1 min 43 s |
| Source-only UAV runtime publication | about 3 min despite a 0.1 s, 78 KB source copy | SBOM generation consumed 40.7 s and timestamp-normalized layer rewriting consumed 130.6 s |
| Registry-cached UAV runtime qualification | 5 min 6 s across a source-only revision | PX4 checkout and SITL compile were cached; provenance and export consumed the remaining tail |

Remaining misses must be added here with the evidence path, measured phase, cache state,
and exact command. Wall time without phase evidence is not enough to choose an
optimization.

The register retains measured sinks after a correction lands because the observation is
useful when a similar boundary regresses:

| Sink | Observed cost | Required correction |
|---|---:|---|
| The official Rerun Redap test crate combined OTLP hyper and blocking-reqwest features, then invoked a transitive Prost build without host `protoc` | two failed cold compilation attempts preceded the corrected 3 min 19 s official-profile build | enable OTLP's higher-priority async reqwest client for the additive feature set and make `cargo xtask test-report run` inject the pinned repository-managed `protoc` into every recorded command |
| Concurrent `image stage` commands each reconcile the same named BuildKit worker and can replace it while another stage is active | parallel three-target staging canceled active solves before image compilation began | make builder reconciliation a separately acquired, process-safe checkpoint, then allow one multi-target Bake solve to publish per-target evidence |
| A chart-version annotation changes the simulator pod template during an otherwise unrelated platform chart rollout | one full cached Isaac restart | decouple application chart publication from platform-only chart changes and remove non-runtime release metadata from the pod template |
| A two-field Cesium native lifecycle edit invalidates the complete native and Omniverse extension build, then redownloads unchanged Python wheels while assembling the runtime | 499.573 s for `image build --group showcase-uav-sim-overlay-acceptance`; 27 of 64 vertices were cached, while the trace attributed 496.656 s to the combined export path | publish the pinned Cesium extension as a content-addressed layer independent of the simulator application overlay, preserve its compiler outputs across patch revisions, and add a BuildKit pip cache for the locked wheel set |
| Local-load and registry-stage modes derive different builder configuration hashes and replace the named builder before transferring the same large runtime | 143.152 s for the immediately following cached `image stage --target uav-sim-runtime`; 34 of 56 vertices were cached, timestamp normalization consumed 131.345 s, and push consumed 1.992 s | keep one registry-capable builder configuration for both modes and stage the already built immutable manifest without repeating timestamp normalization or full-image export |
| `uav-showcase-up` builds the broad smoke binary after a focused browser edit | 30.74 s warm compile | move always-on showcase convergence into its own focused harness |
| Separate reverse-dependent stages repeat overlapping optimized Rust compilation | about 40 s per cold target cache | execute one exact multi-target Bake stage and emit per-target evidence from the shared invocation |
| Image staging accepts a cluster-internal registry authority that the host BuildKit worker cannot reach and infers TLS from its non-loopback name | 6 min 55 s of simulator and Rust compilation completed before the first push request failed against the unreachable host port | model build/push and cluster-pull registry authorities with an explicit transport, validate the push endpoint before starting Bake, and preserve the same cache identity across those aliases |
| Recording Hub periodic counters describe its local proxy but not authenticated forwarder ingest | healthy uploads required durable-queue inspection while Hub counters remained zero | expose accepted-message, accepted-byte, backlog, and last-success counters at authenticated ingest |
| Full UAV acceptance sent an already-looping vehicle back toward one fixed low-speed waypoint | 1,223.52 s ended at the 20-minute task-token boundary because the fleet had moved far from the fixture origin | derive one nearby maneuver from the current authorized pose, preserve its authorized altitude, fly at the governed profile's 20 m/s cruise speed, and derive a bounded deadline from the returned Map cost; observed under source revision `ee4ace35fc8e055b72134902f3948fd2522f6e8c` in the full UAV gate |
| Warm `showcase-uav-sim` group staging still rewrites and pushes most of the image lineage | 145.530 s total; BuildKit 143.168 s, provenance 139.423 s, timestamp normalization 139.614 s, export 140.680 s, and push 111.351 s; only 35 of 69 vertices were cached | preserve normalized parent layers across source-only overlays and emit a byte/layer breakdown for the export and push tail; measured by the canonical `image stage` command for revision `da70aa968b9a8017d25cefac88cb53c9b90df936`, with phase evidence in `target/veoveo-xtask/evidence/da70aa968b9a8017d25cefac88cb53c9b90df936/stage-group-showcase-uav-sim-1786132506798538397-3968604/run.json` |
| Focused composed browser acceptance assumed that Stream already owned a processing session after a simulator rollout | 180 s spent waiting for a session while the loaded App already exposed an admitted pipeline and `Start live session` action | start one admitted pipeline immediately when no session exists, leave an existing session untouched, and emit partial evidence before a later checkpoint fails; observed with `cargo xtask smoke uav-showcase-browser-verify --public-base-url https://installation.example --chrome-cdp-url http://127.0.0.1:9222` under source revision `82aff1c37124624b37c10241683c74c36786cac9` |
| Task acceptance repeatedly creates short MCP clients and token exchanges while waiting for one task | repeated closed-listener warnings and avoidable request latency | retain one authorized task listener for the acceptance run |
| Long UAV acceptance binds one operator credential and one camera revision for the entire run | a completed mission crossed the 20 min credential lifetime; cleanup then used a stale camera revision | renew the acceptance credential before expiry and read the current camera revision before cleanup |
| Browser acceptance can reuse an operator's interactive Console target | operator navigation canceled the final exclusive-network assertion after all captures completed | create a dedicated authenticated browser target for each acceptance run |
| Local image load and registry stage derive different BuildKit daemon configurations for the same named builder | a one-file Python fix caused two destructive builder reconciliations; local load took 174.265 s and registry stage repeated another 158.391 s with only 31 cached vertices in each solve; stage evidence: `target/veoveo-xtask/evidence/6315083f650c2dc6215f99f630c25361fd2d967a/stage-target-uav-sim-runtime-1786128123199129304-3625626/run.json` | give local load and registry publication one stable builder configuration, preserve worker state across transport changes, and reject any routine command that would destroy a warm builder |
| Build and stage use raw BuildKit progress internally but emit no phase progress while a solve is active | the 174.265 s target build was silent after builder readiness until final evidence emission | stream bounded vertex and phase progress while retaining the machine-readable event trace |
| `uav-sim-runtime` source-only changes traverse the complete simulator dependency graph and large-image export | the first targeted build spent 169.203 s in provenance-associated solve work and 170.565 s through export despite zero Rust compilation; a later one-file writer correction repeated the sink at 176.482 s, including 168.869 s of timestamp normalization and 170.672 s of export; evidence: `target/veoveo-xtask/evidence/6315083f650c2dc6215f99f630c25361fd2d967a/build-target-uav-sim-runtime-1786127805069095581-3602194/run.json` and `target/veoveo-xtask/evidence/5bf21381c4c50da4f3e448516d6a5b252ef9df9c/stage-target-uav-sim-runtime-1786208750374469362-1098408/run.json` | split the frequently changed UAV overlay from immutable PX4, Cesium, Isaac, and Python dependency payloads while preserving one final digest and GPU runtime contract |
| The source-built Cesium extension kept Packman, vcpkg, Conan, and CMake outputs inside one uncached Docker step | the exact-source compile downloaded and built dependencies for 671.4 s before one warning-as-error stopped the extension compile; BuildKit completed the failed vertex at 742.7 s and retained none of those intermediate outputs | give the pinned Cesium build separate persistent cache mounts for its CMake tree and each upstream package store, while copying only the completed extension package into the runnable image |
| Removing one Python dependency from the UAV runtime invalidates the complete Python dependency layer and large simulator-image export | native sensor-encoder staging took 238.222 s; BuildKit consumed 234.201 s, provenance 228.530 s, timestamp normalization 148.319 s, and export 197.803 s while 51 vertices executed and 27 were cached; evidence: `target/veoveo-xtask/evidence/9c1281ecdd1920bf8888b94525e91c870d320de2/stage-target-uav-sim-runtime-1786207158180179505-959532/run.json` | separate frequently changed Python package resolution from the immutable Isaac, PX4, and Cesium payloads, and preserve normalized dependency layers when one package leaves the environment |
| The broad smoke package owns focused live-view browser assertions | one verifier-only edit triggered a 2 min 19 s test build of SurrealDB, Rerun, Recording Hub, Recording MCP, Stream MCP, task runtime, and unrelated deployment scenarios before 120 relevant and unrelated tests ran in under 0.3 s | move composed flight scenarios behind a separate crate boundary and keep the focused browser package independent of platform store, recording services, and deployment smoke dependencies |
| The nominally focused browser and UAV MCP test pair still shares platform-scale dependencies | a two-package run took 1 min 54 s while its tests completed in under one second; SurrealDB, DuckDB, the task runtime, and unrelated service clients remained in the compile closure | move live-view state-machine fixtures behind focused library boundaries and remove database, recording, and task-runtime features from the browser verifier graph |
| An App-only MCP image update replaces the co-located simulator pod | the first observation staged in 34.304 s and required about two minutes of Isaac, tile, fleet, and recording recovery; the full-workspace metadata correction reproduced the cost at 46.80 s to stage and 198 s through simulator plus Console-catalog recovery, despite changing no simulation behavior; evidence: `target/veoveo-xtask/evidence/05e3f2a684aa7b13d17bfe4bef83413949b613f6/stage-target-uav-sim-mcp-1786129571843377331-3746578/run.json` and `output/development/uav-sim-mcp-371508fdc1b7.stage.json` | separate independently replaceable control-plane and static-App lifecycle from the GPU simulator lifecycle without reintroducing a remote pose mirror or a second renderer |
| Two simultaneous 1280×720 native viewer products exceed the provisional 50/200 ms pipeline targets on the designated one-GPU reference | two steady 256-event windows delivered 15.7 FPS with 74.9–77.7 ms source-to-render p95 and 214.5–227.4 ms conservative composed motion-to-photon bounds; no frame or packet loss occurred | retain the qualified 12 FPS, 85 ms, and 250 ms acceptance gates while reducing Isaac/Kit render-orchestration cost toward 50/200 ms without adding a relay, another simulator, or a software media path |
| A developer supplied the cluster pull authority to image staging after the same profile had previously used its host push authority | BuildKit compiled the MCP image for 21.438 s and failed only at the registry request; switching authorities rebuilt the named builder configuration, while the corrected solve then completed in 1.940 s from cache; evidence: `target/veoveo-xtask/evidence/7e21c15f42b6005ac17dadfcc97da21cbc47d217/stage-target-uav-sim-mcp-1786162807986393613-2157016/run.json` | make the deployment profile expose one typed push/pull registry pair to staging and reject the pull-only authority before reconciling BuildKit or starting a solve |
| A simultaneous-viewer verifier opened two tabs in one headed browser window | the first live acceptance stopped at the visibility preflight before either native viewer slot was allocated because opening the second tab backgrounded the first | create one headed window per simultaneous viewer and retain visibility plus hardware-adapter checks for every window |
| Two fresh viewer windows start independent PKCE flows against one shared pending-authorization cookie | the first two-window retry reached the corrected OAuth scope set, then one callback failed when the concurrent login overwrote its pending state | establish one authenticated Console session before opening actor- and browser-instance-scoped viewer windows |
| Viewer assignment returns before the native AOV signaling listener accepts connections | slot 0 succeeded after a prior preflight had warmed it, while the first slot 1 connection reached its assigned product and failed with a pod-local connection refusal | complete assignment only after render-frame and native-listener readiness events; return the slot immediately when bounded activation fails |
| One failed viewer can strand its peer at an unbounded synchronization barrier | the failed slot 1 run held the healthy slot 0 window until the outer acceptance timeout and left cleanup to cancellation | bound the simultaneous-video barrier independently and close both dedicated targets through the ordinary release path |
| `image stage` reconciles the managed BuildKit worker before validating the requested immutable Git revision | an invalid revision spent 10.9 s inspecting and reconciling an already-ready builder before returning `Needed a single revision` | resolve the source revision and verify cleanliness before acquiring or inspecting the builder; observed while staging `uav-sim-mcp` at missing revision `104b4360d8e2a6ac703f848daf518708b27431f0` |
| The live-view cadence gate used one fixed two-second sleep beginning at decoder startup | the first dual-view run measured 43 frames in 2.011 s, reported 21.39 fps, and ended the composed acceptance before steady-state sampling | warm on 12 `requestVideoFrameCallback` events, then measure 48 presented-frame intervals reactively; retain the declared-rate and dropped-frame gates without a polling timer |
| The focused browser run validated unrelated visual consumers after live-view correctness | a 195.49 s run completed two simultaneous viewers, four additional cameras, Stream, and Recording before rejecting a 0.9795 simulation real-time factor; the late failure wrote no manifest and exceeded the three-minute warm budget by 15.49 s | keep native live-view acceptance independent, retain Recording in its dedicated command, and leave Stream verification with its owning server |
| A long-lived Stream session inherited GStreamer's 60-second RTP dropout tolerance across a simulator replacement | the focused browser retry reached Stream after 50.12 s, then rejected an overlay that was 60.886 s stale; the session remained `running` while its source epoch had changed | publish an RFC 3550 source epoch on every producer start, bound admitted dropout and misorder recovery explicitly, and retain the Stream session across source replacement |
| A Stream catalog and RTP-epoch correction invalidates almost the entire Stream image build | targeted staging took 65.78 s; BuildKit consumed 50.304 s, optimized compilation consumed 42.330 s, 20 vertices executed, and only 4 were cached | place the admitted catalog and embedded server documentation after stable binary compilation inputs, split C++ runner and Rust-server cache keys, and retain one runnable image digest; evidence: `target/veoveo-xtask/evidence/73eb196ca61f5979e51ba5c788a266cd503b04d6/stage-target-stream-mcp-1786137623809246800-175638/run.json` |
| The focused browser gate serialized a mandatory 120-second Recording follow check after native live view | a correct run took 206.64 s and exceeded the three-minute warm budget by 26.64 s; dispatch compiled in 0.41 s, while the product held 1.0000 real-time factor, zero Recording lag, and zero browser frame drops | split the gates by ownership: native live view proves camera and product behavior, while the dedicated Recording command proves live follow and source alignment; evidence: `output/acceptance/uav-browser/7dc65ac60121550618014e3c69147a7fd3a1471e/019fde24-18d4-7e20-9f20-d0be637a223b/evidence.json` |
| Proving one Stream source-epoch replacement requires a complete cached GPU-simulator pod replacement | the exact-image restart took 86 s before runtime readiness, even though the retained Stream session resumed with the same identity and advancing result counters | add a focused admitted RTP source-epoch fixture for ordinary Stream iteration and reserve the real GPU pod-replacement proof for release acceptance; the release proof must still exercise the authoritative simulator and may not substitute the fixture |
| The isolated external-simulation fixture still dispatches through the platform-scale smoke binary | a three-line SDK digest correction rebuilt `veoveo-smoke` for 7.55 s, used 5.4 GB maximum RSS, and took 18.56 s end to end even though the isolated private-index, package, Bake, and Helm checks are one focused scenario | move the external repository fixture into a focused Rust harness with only package-publication, HTTP-index, Bake-print, and Helm dependencies while retaining the same typed smoke command |
| Documentation PDF publication is a manual CDP sequence without a source-controlled lifecycle or phase record | one three-document headed-Chrome run wrote all outputs but emitted no per-document progress and retained its CDP process for more than three minutes until interrupted; the corrected one-document invocation with explicit disconnect completed in 2.95 s | add a typed `cargo xtask` publication command that performs the mandatory hardware probe, prints each canonical source independently, reports phase timing, terminates its CDP client, validates page identity, and dispatches visual QA |
| The full UAV gate retained the deleted one-product-per-logical-camera acceptance model | 22.14 s reached a healthy authoritative world, five logical cameras, and two correctly inactive physical viewer slots before a stale assertion rejected the slot pool | share typed idle-slot and assigned-slot invariants between focused and full acceptance, and run the contract-only state test before launching a flight; observed under source revision `1456fb2ce3a163835c9a39ee31f32032300590d0` with `cargo xtask smoke uav-showcase-verify --context <context> --public-base-url https://installation.example --chrome-cdp-url http://127.0.0.1:9222` |
| Full acceptance treated every Work-Context-readable Stream session as operator-owned cleanup state | 33.85 s compiled and reached preflight before attempting to stop a healthy browser-owned session; the owner-scoped stop correctly returned not found | reuse the one visible active session without taking ownership, stop only a session created by the acceptance actor, and cover selection plus duplicate-active rejection in the contract test before flight; observed under source revision `f97c87d2969262e32077679f70db18bfb281b043` with the full UAV command above |
| Full acceptance required detailed aerial sensor content while the vehicle was still landed | 34.08 s rejected advancing NVIDIA NVENC frames because the nadir camera correctly saw a uniform nearby surface at ground level | separate hardware-frame startup from aerial-detail acceptance, admit only typed `frame_uniform` degradation before takeoff, and retain the strict visible/detail gate after the vehicle reaches altitude; observed under source revision `c32ea0b9e86033f1906d9aff67feacdf9aa076e0` in the full UAV gate |
| Browser capability acceptance stopped after Permissions Policy admission instead of initializing the native client in its opaque-origin App frame | Chrome terminated the frame with bad-IPC reason 295 when the pinned client invoked Compute Pressure, leaving a gray renderer-error surface before WebRTC negotiation | exercise client initialization and frame liveness in the focused browser gate; keep the opaque sandbox, suppress the client's optional pressure observer before it loads, and reject any renderer termination before stream acceptance |
| A focused native-camera run immediately after simulator replacement completed every browser capture but reached 0.9745 simulation real-time factor against the 0.98 release gate | two simultaneous 1280×720 products delivered 15–16 FPS with 21 ms frame age, then the late simulator-performance check withheld the evidence manifest; captures remain under `output/acceptance/uav-browser/bf486b4ef9adf913e372cf755b71eab5cb254f27/019fe021-e97f-7171-b456-376c9a444bfb/` | track post-restart physics scheduling and warm-up as a simulator performance follow-up; keep the 12 FPS native-stream acceptance independent and do not weaken the simulation real-time-factor gate as a media workaround |
| Stream live acceptance treated H.264 presentation timestamps as decode-order timestamps and waited on the runner after rejecting the first reordered access unit | the full gate spent 180 s observing stale counters; two failed runners retained the pipeline's exclusive UDP port until process inspection exposed `H.264 timestamps moved backwards` | order encoded chunks by their explicit sequence, preserve legal presentation-timestamp reordering, and terminate plus reap the native runner immediately when its event contract fails |
| Stream live-ingress launch text was duplicated between the source catalog and platform Helm template | a full flight gate spent about three minutes before the deployed jitter buffer froze on an RTP epoch change that the qualified catalog already handled | keep both generated surfaces byte-equivalent in Helm configuration smoke coverage; the runtime now gives live RTP an independent bounded consumer so Recording reconnects cannot change its epoch |
| The DeepStream live runner treated reordered AVC presentation timestamps as detection-order identity and kept waiting after a probe failed | each end-to-end attempt decoded about eight seconds, then spent the remainder of a 180-second freshness window on a session falsely marked `running` | assign live detections a decode-order identity, retain presentation time only for encoded playback, and wake the pipeline bus immediately on any typed probe failure |
| Headed UAV acceptance waited for an ephemeral operator-view product before opening the browser that allocates that product | the complete domain path passed, then the visual gate flew and landed before reporting a circular precondition that leftover viewer state had previously hidden | drive flight checkpoints from the always-on native sensor NVENC sequence and create an isolated per-viewer operator product only when each browser capture begins |
| A one-resource Map App metadata change staged through a cold source-worktree Cargo cache | the targeted `map-mcp` stage took 438.27 s, including 396 s of optimized compilation; it downloaded the registry index and compiled unrelated Bevy, WGPU, Time, and View crates before emitting the immutable image, while the host package test cache was already warm; evidence: `output/development/map-mcp-39ddc188.stage.json` | give source-isolated image builds a reusable locked Cargo registry and target cache keyed independently from the source revision, and remove unrelated visual-server features from the Map runtime dependency closure |
| A Map analytical-schema hard cut had a documented rebuild requirement but no rollout lifecycle | Git push took 3.45 s, then the single Map replica entered `CrashLoopBackOff` on its obsolete 1.8 MB DuckDB marker; push-to-Ready took 303.38 s including diagnosis and a recoverable projection move, while a fresh pod became Ready 39.80 s after that move | add a schema-aware preflight and one explicit projection-rebuild Job before replacing the sole Map replica; preserve SurrealDB and retained release products, then verify replay and readiness before completing the rollout |
| App presentation metadata changed in one MCP server while the deployment closure retained an older Console frontend | the Map image correctly emitted `prefersBorder: false`, but the live Console bundle had no full-workspace rendering branch and continued to constrain the App to 420 px; staging the missing Console image took 139.46 s, and the first GitOps attempt took 214 s through live readiness because an invalid completed commit hash was rejected before apply | derive affected image targets from both ends of a changed cross-component contract, resolve configuration revisions directly from Git instead of completing abbreviated hashes, and verify the live catalog plus rendered presentation branch before declaring the rollout complete |
| A one-line Console presentation-default change touched the shared MCP Apps crate | the targeted `console-bff` stage took 76.77 s even with warm caches, then GitOps took 74 s from refresh to verified Ready; the image solve rebuilt MCP conformance, Gateway, UAV, Timeseries, and the BFF before emitting the new frontend bundle | split the Console static-asset stage from Rust artifacts when only host presentation code changes, and narrow shared Apps-contract invalidation to binaries that consume changed Rust symbols |
| Fresh dual-view products can enter acceptance before native Cesium and WebRTC delivery reaches steady cadence | two event-driven 48-frame startup windows delivered 8.69 and 10.28 FPS while later product counters reached 12-15 FPS; both windows retained visible 1280×720 NVIDIA NVENC frames and no browser drops | retain the 12 FPS steady-state floor, discard at most one reactive startup window, require the next 48 presented frames to pass every cadence, loss, and latency gate, and reduce native product activation warm-up as a performance follow-up |
| A fixed physics-step partition made simulation speed depend on RTX viewer cadence | dual native viewer acceptance delivered steady 1280×720 NVENC streams but ended at 0.8956 real-time factor because every render always advanced only three or four physics steps | derive due fixed steps from elapsed monotonic time, retain bounded physics debt, and coalesce missed visual deadlines into one render of the newest authoritative state without weakening the simulation real-time-factor gate |
| A producer-authored video keyframe column interacted with separately compacted live batches in Rerun 0.35 | focused governed-live verification reached the authenticated viewer in about 16 s, then its video cache panicked while indexing a physical chunk whose sparse `VideoStream:sample` offsets had fewer entries than rows | follow the pinned SDK's sample-only producer profile, derive sync samples from H.264 bytes, omit keyframe columns from the internal live projection, and retain canonical derived markers only in archive materialization |
| Archive acceptance reused a live changing-camera color and edge threshold for one static decoded frame | 24.36 s opened the complete lazy archive and rendered a real dark rooftop camera image before rejecting its low chroma and zero quantized-edge score | require dimensions, color diversity, bounded dominance, and luminance variance for a static archive frame; retain changed-pixel and stronger detail gates for advancing live playback |
| Host Cargo used the machine's 32 logical CPUs during a broad release check | concurrent Rust compilers and linkers consumed about 62 GiB on a host without swap, the host stopped responding, and Kubernetes evicted workloads | default Cargo subprocesses launched by xtask and the evidence recorder to four jobs; keep an explicit `CARGO_BUILD_JOBS` override for measured hosts |
| The managed BuildKit worker grew outside Docker's aggregate build-cache accounting | `buildctl du` reported 294.57 GB while `docker system df` reported zero build cache; the root filesystem crossed the kubelet image-GC threshold and pods were evicted | make `cargo xtask release preflight` inspect the managed worker directly and reject projected growth that would consume the retained filesystem reserve |
| Kubernetes had to pull a garbage-collected SurrealDB dependency during recovery | the public image pull took 4 min 5 s while dependent services remained unavailable and rollout diagnosis was delayed | inventory immutable cluster dependencies before mutation and retain or pre-pull any dependency whose absence would block recovery |
| Helm rollback diagnosis treated controller-owned terminating pods as interchangeable leftovers | five pods remained terminating while their ReplicaSets and Deployments were still converging, which obscured the current owner and delayed safe intervention | report deletion timestamp, owner, current ReplicaSet, desired revision, and readiness before deleting anything; remove only controller-superseded objects |
| A host failure left Chrome profile singleton links after the browser process disappeared | the canonical headed profile could not restart until the stale socket, cookie, and lock identities were distinguished from a live browser | verify that no profile process or DevTools listener owns the singleton identities before removing them; never delete the profile or its session state |
| Chrome exposed software WebGPU and hardware NVIDIA WebGL in the same headed session | a single pass/fail GPU label would either accept SwiftShader as hardware or reject a valid RTX 4090 WebGL path | probe and record both APIs, reject every software renderer as hardware evidence, and pass only when at least one API is hardware-backed NVIDIA |
| The UAV App host preflight passed while nine new Console Apps were absent | the focused command proved only the existing UAV Console and standalone frames; it never inspected the complete signed-in catalog | add `console-apps-browser-verify` to require the expected `ui://` set, deterministic server groups, zero catalog degradations, and one headed render per App |
| The complete Console App browser pass captured several frames while their Apps still reported `connecting`, `reading`, `rendering`, or `discovering` | the first run produced 17 screenshots and appeared green, but its manifest showed that document readiness had been mistaken for App initialization | make every expected App declare its settled status, require a successful host display-mode bridge round-trip, reject visible error surfaces, and require output selectors for rendered Charts, Map, and View workspaces before screenshot evidence is accepted |
| A post-commit Console App rerun evaluated the new Chrome target while its initial `about:blank` document reported `complete` | the catalog probe attempted a relative `/console/api/apps` fetch without an HTTPS base and failed before observing the deployed Console | make headed-target creation wait for both an interactive document and the exact requested URL; test that `about:blank` cannot satisfy browser readiness |
| Direct server App conformance passed while gateway policy removed every new `ui://` resource | each server returned valid App resources in-cluster, but new policy rules admitted only the domain scheme; Datasheet had no policy rules | test `GatewayCatalog::decide` for every first-party App URI under every Console profile and keep `ui` in the canonical resource-scheme policy |
| A long GitOps convergence attempt wrote failed evidence before a warm retry passed in 11.3 s | the first 1,200-second window captured real pull and rollout delay; treating the retry as the only result would hide the release cost | retain create-only failed and passing convergence evidence and attribute source fetch, apply, image pull, rollout, and readiness independently |
| Historical Evicted pod objects made the namespace look unhealthy after live workloads recovered | current Deployments were Ready while old failed pod rows remained visible in broad pod listings | make the preflight count historical evictions as a cleanup hint and remove only controller-superseded failed objects after acceptance |
| Direct MCP conformance was used as a proxy for composed Console availability | server contracts were healthy, yet policy projection yielded a seven-App catalog instead of the expected release surface | treat server conformance and signed-in gateway composition as separate mandatory checkpoints; neither substitutes for the other |
| The affected planner treated a gateway integration-test edit as a runtime image input | a configuration-only hotfix rebuilt the isolated optimized gateway graph for 7 min 44 s even though `platform/gateway/tests/exposure_probe.rs` cannot enter the gateway binary; release evidence: `target/veoveo-xtask/evidence/5918f0287b1848e957f03dbc764e1d2028820b3c/release-target-mcp-gateway-1787779560371360963-3312530/run.json` | exclude a Cargo package's top-level `tests/` tree from runtime package invalidation while retaining source, manifest, build-script, and dependency changes |
| The local-path recording PVC advertised 20 GiB while its unenforced backing directory grew to 288 GiB | the host reached 83% usage with 299 GiB free even though Kubernetes showed a small bound claim; clearing the approved pre-cut spool raised free space to 586 GiB and kept `DiskPressure=False` | make recording diagnostics and `cargo xtask doctor` state that local-path capacity is not a quota, compare mounted usage with node free space before rollout, and set the Bioma Hub's actual-filesystem floor to its retained 200 GiB node reserve |
| The first hard-cut reset issued one delete for 2,974,068 changefeed rows | SurrealDB memory rose past 5.7 GiB toward its 8 GiB limit, and interrupting the CLI left the server-side query running until a controlled pod restart | delete selected record IDs in bounded 100,000-row transactions, verify each committed count, and keep producers stopped until the guarded migration succeeds |
| Recording Deployments and their HelmReleases were suspended while the parent `bioma` Kustomization remained active | Flux reapplied the Git-authored HelmRelease specifications, restored Hub and the simulator during the bounded reset, and created another spool tail | suspend the owning Flux Kustomization before its HelmReleases and Deployments, prove all three layers remain quiescent, then resume from the parent after the compatible Git revision is published |
| Process-substitution provisioning created both new recording publisher Secrets with zero-byte PEM values | Kubernetes mounted the expected filenames, but Hub and Recording MCP entered `CrashLoopBackOff` with `InvalidKeyFormat` until the valid fixture key was copied from the producer Secret | provision multiline PEM through `stringData` or an installation secret manager, then pipe every mounted value through `openssl pkey -check -noout` before rollout |
| A second GitOps convergence command completed an abbreviated hash by hand instead of resolving Git | Flux was already Ready at `dab45f50d8dacf666c325c9e39672d73742f04b9`, while the harness waited for the nonexistent `dab45f5074470413625e8dd86c5629534347dccd` until it was interrupted | make `gitops-converge` resolve and require an exact local commit before its first Flux mutation, and pass `$(git rev-parse HEAD)` in every operator command |
| The recording hard-cut release selected the six `platform-core` images but omitted the independently deployed `uav-sim-mcp` consumer of `PlatformStore` | live ingest succeeded, but UAV state could not deserialize the new `recording.dataset` record reference and browser acceptance stopped before Rerun opened | run `cargo xtask image affected --since <deployed-source>` before selecting a release group, compare every affected target with the installation's image closure, and stage omitted consumers before GitOps activation |
| A Recording MCP test assumed Rerun `EntryId::to_string()` preserved canonical UUID text | live playback emitted mixed-case, unhyphenated TUID hex in `archive.dataset_id`; the Console correctly rejected the v9 manifest because its outer dataset UUID and archive identity differed | compare Rerun entry IDs by their 16 identity bytes, keep UUIDv7 formatting at the public catalog boundary, and run the `redap`-featured library tests that compile the playback module |
| Chrome Safe Browsing classified the development installation's long OAuth callback URL as dangerous | the first headed recording run waited through its 300-second browser window on a `Security error` interstitial; its authorization request expired before the trusted installation was continued manually | inspect a new Console target's title and bounded body text for browser interstitials before entering a visual wait, fail with the exact browser block, and begin one fresh authorization immediately after an operator admits the trusted installation |
| A `kubectl -o custom-columns=...[*]` status probe was passed unquoted under zsh | the rollout mutation succeeded, but the diagnostic command failed immediately with `no matches found` before printing the new pod identity | quote JSONPath and custom-column output specifications in operator examples and copyable commands; this shell-only mistake does not justify another repository command |
| An ad hoc admin MCP token omitted the profile's required `time:read` scope | discovery rejected the token before sealing and advertised the exact required scope set in `WWW-Authenticate` | derive manual service-token scopes from protected-resource metadata or the challenge instead of copying a narrower fixture list; retain `operator:use admin:manage time:read` in the recording-seal checklist |
| Recording sealing required a producer Blueprint Artifact but had no step that published one | the first real seven-layer seal stopped in `sealing` with `recording Blueprint has not been published`, even though live Rerun playback had correctly used the confined staging Blueprint | make seal idempotently publish and stage the Blueprint occurrence before manifest publication, delete the staging copy after commit, materialize sealed playback from Artifact storage, and test cache validation against application ID, Blueprint ID, and message count |
| The affected-image planner returned three exact targets while `image stage` accepted only one `--target` value | a repeated-target command was rejected before BuildKit, forcing three serial invocations and a later evidence merge for one dependency closure | let `image stage` accept repeated exact targets and execute one shared Bake solve, or have `image affected` emit a directly consumable named group; keep the serial merge sequence documented until that tooling work is scoped |
| The Bioma GitOps profile had a Helm `images.lock.yaml` but no typed qualified `DeploymentLock` for `image development-lock` | passing the Helm values file as `--base-lock` failed immediately during JSON decoding, so three valid stage records could not produce the intended merged development closure | publish the profile's qualified deployment lock beside its Helm lock and have the activation checklist name both artifacts explicitly; until then, update only the evidenced digest keys and review the resulting Git diff |
| Recording MCP published the sealed Blueprint occurrence but its Helm spool mount was read-only | the idempotent seal retry stopped at staging-file removal with `Read-only file system (os error 30)`, leaving the recording in `sealing` after the Artifact reference had committed | give only the co-located Recording MCP write access to the confined spool mount, retain its read-only root filesystem, and make Helm render acceptance assert the writable mount sequence required by Blueprint cleanup |
| Git-source convergence succeeded while the changed platform chart remained pinned to its prior OCI digest | Flux applied the new commit and reported the already-Ready Recording Deployment in 7 seconds, but the live pod retained `readOnly: true` because no chart revision had changed | make the affected release plan include chart publication and the profile's OCI source digest whenever `deploy/helm/` changes, and have convergence compare the Helm chart source identity as well as Git, values, and Deployment readiness |
| Omitting `readOnly` from the upgraded Recording MCP volume mount did not clear the live field | the new chart and pod were active, yet Helm's server-side upgrade retained `readOnly: true` from the prior managed object | declare `readOnly: false` explicitly for mounts whose access mode changes and assert the literal false value in the rendered chart; omission is not reliable migration evidence for an existing field |
| Archive transport settled while Rerun remained at a timestamp before the leader camera's first frame | headed acceptance rendered the fleet and OpenStreetMap panes, but the camera pane was one uniform color and failed pixel inspection | select the archive recording's latest `simulation_time` through the WebViewer API, publish that state on the viewer host, and require the headed harness to observe current time equal to the newest time before capturing pixels |
| Sealing removed capture materialization and Blueprint staging but retained the recording-scoped live static context | Artifact-backed replay passed after complete cache loss, yet an 11,595-byte `.static-context` file remained; the first cleanup patch then assumed committed layers retained `staging_path`, which the commit transaction deliberately clears | derive the confined context path from the durable dataset key and recording ID, remove it after the durable seal transition, retry cleanup on idempotent seal, and make acceptance assert that the exact file is absent |
| Final browser acceptance copied a synthetic installation URL from an earlier resolver-scoped run | the headed preflight reached `chrome-error://chromewebdata/` because the current browser had no `installation.example` resolver rule; the canonical `veoveo.bioma.ai` ingress passed immediately | obtain the public base URL from the active installation profile or ingress before browser work, and keep the existing bounded target-load preflight so stale aliases fail before visual waits |
| A newly reopened 7.2 GiB live recording prewarmed every committed layer before selecting its live receiver | the first user interaction returned HTTP 500 after 29.8 seconds; 67 cached files consumed 8.56 GB of the 8 GiB managed ceiling, stale virtual catalogs still pinned the prior revision, and only 28.9 MB of managed headroom remained | make an active viewer manifest live-only until capture ends, expire virtual-catalog pins after the five-minute Redap token window, prune before materialization, and require headed acceptance to reopen a long-running recording near the cache ceiling |
| A GitOps rollout watch used the `recording-mcp` image target as a Deployment name even though Hub and Recording MCP share the `recording` Deployment | Flux activated the correct digest, then the evidence run failed immediately with `deployments.apps "recording-mcp" not found` and required a second create-only evidence path | resolve the live Deployment identity from the rendered chart or cluster before invoking convergence; image target names are not workload identities |
| One UAV process used its pod-lifetime Rerun recording ID for about 37 hours | 235 committed capture objects totaled 30,517,363,704 bytes, while opening the live recording filled 7.9 GiB of the managed playback cache and returned HTTP 500 | rotate the producer to a fresh Rerun recording before a conservative 4 GiB payload budget or four-hour wall age, re-emit Blueprint and static context, and have `cargo xtask doctor` prove the producer ceiling fits two segments inside the managed cache |
| Authenticated ingest skipped a stale staged capture layer whose spool recovery bytes were absent | one 133,960,638-byte staged catalog row remained unresolved while 232 later capture ordinals committed, so catalog health and local recovery state diverged | reconcile mutable authenticated layers after journal replay, resume publication only from retained parts, and fail startup or ingest when staged bytes are absent, mutable layers multiply, or a later ordinal already exists |
| Interactive `surreal sql` accepted a hand-written transaction block during validation but returned `Cannot COMMIT without starting a transaction` at execution | the disposable-data reset required count verification and a switch to independently atomic 100,000-ID deletes before cleanup could be trusted | provide checked bounded reset statements, reject interactive transaction wrappers in the runbook, and verify the affected count after every atomic batch |
| The UAV runtime test directory has no host-managed environment, and its broad contract module imports image-only Isaac dependencies | one root-level `unittest` command could not resolve the non-package `tests` directory; the corrected component invocation then stopped on missing `aiohttp` | keep host-safe policy tests in focused standard-library modules with a copyable component-local command, and state in `cargo xtask doctor` that the broad suite runs in the simulation dependency image |
| A same-Pod UAV process restart left the Recording Forwarder's prior application accumulator resident | the new data UUID ingested and rotated normally, but the forwarder rejected its Blueprint because two recording accumulators shared one application ID; headed playback rendered data yet correctly failed the required Blueprint-response assertion | when `finish-superseded-recordings` observes a new `SetStoreInfo`, flush and retire prior application accumulators before finishing their durable streams and associating the new Blueprint; retain a restart regression test and require a post-restart headed recording pass |
| `release helm-charts` accepted the bare development version `0.1.0` for an untagged commit | all three chart tags were published before the established revision-qualified version was issued; the bare tags were excluded from GitOps activation | reject bare chart versions when the source commit lacks the matching `0.1.0` or `v0.1.0` Git tag, and show the `0.1.0-<12-character-revision>` development form in doctor output |
| Recording Hub constructed its Gateway publisher without installing the workspace's `rustls-no-provider` crypto provider | the new Hub pod entered `CrashLoopBackOff` immediately; the exact GitOps gate kept the dependent UAV release stopped before any producer resumed | install the selected Ring provider at the Hub process entrypoint before any Reqwest client, retain a binary test that builds the client after installation, and require Hub readiness before releasing producers |
| Suspending Flux after an incompatible Helm upgrade had started was treated as cancellation | the Helm action and parent health check continued for their bounded 20- and 30-minute windows even though both resources showed `spec.suspend: true` | suspend and prove the root Kustomization and affected HelmReleases idle before activation; when an action is already running, keep them suspended and let its rollback settle before resuming only the final compatible revision |
| The bounded UAV producer used the pod UID as its first Rerun recording ID | restarting only the Isaac container reused that ID while resetting the process-local byte and age counters, allowing repeated restarts to exceed the intended 4 GiB or four-hour logical-recording bound | mint a fresh UUIDv4 generation on every process start and every in-process rotation, remove the pod-derived environment variable, and make chart validation reject its return |
| Helm 4 server-side upgrade removed the `valueFrom` body but retained the associative `UAV_SIM_RECORDING_KEY` environment entry | Kubernetes rejected all six upgrades because the merged Deployment contained an environment name with neither `value` nor `valueFrom`; the valid rendered chart and dry-run did not model the prior Helm release merge | fail GitOps convergence immediately when a generation-current HelmRelease reports `Stalled=True`, retain Flux's reason and message in failed evidence, and use one bounded client-side upgrade to cross this destructive list-item migration before returning to the declared server-side policy |
| The in-image UAV runtime check copied only the test directory | 70 tests passed, while 11 source-inspection tests failed on missing adjacent runtime files; copying the module reduced the failures to three missing asset and patch fixtures | copy the complete 3 MiB runtime contract tree into the image's writable test path and document that the suite reads its adjacent source, Dockerfile, asset, and patch fixtures |

## Qualification

After staged behavior is accepted, qualify the same source and require the same runnable
identity:

```bash
cargo xtask release images \
  --target <affected-target> \
  --push-registry <host-reachable-registry> \
  --pull-registry <cluster-reachable-registry> \
  --registry-transport <tls-or-insecure-http> \
  --revision "$(git rev-parse HEAD)" \
  --stage-evidence output/development/<affected-target>.stage.json
```

Qualification fails if rebuilding changes the runnable digest. A complete release then
regenerates the deployment lock, performs the profile acceptance selected by its
contract, and records SBOM and provenance on every publication index.

`cargo xtask smoke helm-config` builds only the focused `veoveo-deployment-smoke`
binary. Helm and GitOps configuration edits therefore avoid the full smoke harness’s
Rerun, recording-service, and Stream dependency graph. The full gateway suite includes
the same owner-local assertion module. This changes dispatch cost without weakening
the configuration checks.

## Console Frontend Feedback

The [Console development runbook](../apps/console/web/README.md) defines the loopback
Vite, BFF, and gateway ports and the required authentication origin. Use this loop for
presentation edits before staging an immutable image. Vite proxies API and OAuth
requests to the running local services while React refresh updates frontend modules.

## Agent Management Iteration — September 19, 2026

The registry foundation uses the existing Cargo target and pinned disposable SurrealDB
fixture. Focused store compilation took 6–16 seconds in the initial edit loop and the
five registry tests ran in approximately two seconds. These observations cover this
warm local cache, not release builds or installed agent creation.

| Inefficiency | Observed cost | Correction |
|---|---|---|
| The host SurrealDB CLI is 3.2.1 while the repository qualifies 3.2.4 | A host-only parse pass could not establish the deployed syntax boundary | Validate SQL using the already cached pinned fixture image. |
| The latest published `@surrealdb/surql-fmt` is prerelease `0.1.0-beta.2` and corrupts typed function signatures, nested branches, closure expressions and field-based LIMIT clauses | One migration-failure cycle and one empty-catalog failure after partial repair | Reject this formatter's output, restore reviewed SQL, and validate with the actual database parser and behavioral tests before accepting formatting. |

The gateway API checks reused the debug target. A first gateway test link took about
41 seconds; later gateway-only edits took 12 seconds. Editing the store during a gateway
compile caused another transitive rebuild of store, task runtime and agent runtime.
Complete a dependency-layer edit before starting its downstream test compile, then use
the running interval for documentation or independent frontend work.

Evidence publication is another measured source of churn. The immutable receipt tree
was approximately 189 MB during the authoring checkpoint, and report publication and
`show` took longer than several sub-second warm checks. Changing the check catalog or
running evidence publishers concurrently forced additional recording and index recovery.
Define the focused check scope first, iterate with ordinary native checks, then record
stable results serially before committing. A future recorder optimization should reduce
repeated parsing and hashing while preserving immutable receipt integrity.

The chat-registry checkpoint reused the debug target for the model stream and native
Task tests. Helm qualification still compiled an alternate dependency feature set
(about 33 seconds) even after gateway tests were warm; record this as build-graph work
for a later measured optimization. The Workspace bundle built in about three seconds.
Headed browser acceptance took about 47 seconds, including the existing collaboration
and Task flows plus lost-response revision adoption. Hardware evidence was NVIDIA
WebGL; the exposed SwiftShader WebGPU adapter was rejected as hardware evidence.

The first agent-management image release selected only `mcp-gateway` and
`console-bff` at `fff2744c`. Publication took 382.5 seconds. The independent Rust
families reported 204 seconds for the browser edge and 363 seconds for the gateway;
several common dependencies compiled in both. BuildKit's phase windows overlap and
must not be added together. The chart publisher waited on the shared source checkout
lock until image publication released it. A later improvement can shorten that lock
lifetime after creating an immutable build context, with a concurrency qualification.
The kernel and simulator images were unchanged by this chat release.

Installed Workspace authoring then saved, validated and published a definition without
an image build or rollout. The observed requests took 95 ms, 128 ms and 487 ms. Browser
automation initially waited on labels whose text included populated textarea content;
role-based accessible-name selectors removed those false timeout failures. These were
test-selector delays, not application latency. The authoring list also refreshed after
both explicit mutation completion and catalog SSE; this redundant metadata read is a
small follow-up opportunity. Neither observation establishes a latency percentile.

The managed persistence checkpoint compiled in 9–14 seconds and its six database
scenarios ran in under five seconds. It reused the same store target and disposable
fixture; no image release is needed until the controller and kernel integration close
the managed execution path.
The managed identity checkpoint reused the same targets. An incremental Rust compiler
ICE accompanied an ordinary field-name error once; correcting the field and rerunning
completed without clearing the shared Cargo cache. The combined gateway library and
binary suite now records one scoped result, which avoids two evidence-publication
passes for the same source inputs. New template validation binds immutable configuration
content before resource creation, avoiding runtime surprises that would otherwise
require a deployment repair.

The managed client uses the existing definition SSE connection for instance invalidation.
This avoids a second long-lived stream per editor. Its headed fixture completed in
about four seconds before screenshot capture was added. Console's bundle took 2.6
seconds and Workspace's warm bundle took under one second. Runtime lifecycle acceptance
remains separate from these explicit HTTP fixture results.

Managed runtime qualification now always runs its database tests against the pinned
fixture; the old environment gate could report passing tests without exercising a
database. Seven integration scenarios took seven seconds. Scoped store checks took
30 seconds including compilation, and the Console build took eight seconds.
One formatting evidence attempt was invalidated by new files created during the
recorder's input snapshot; rerunning on frozen inputs passed. Keep broad format
evidence separate from overlapping implementation work. Kernel checks introduced a
different combined dependency feature set and rebuilt SurrealDB/Rerun metadata and
test artifacts. This is local qualification cost, not an image build per agent.

The managed-controller checkpoint added five composition, redaction and cleanup
checks. The real database suites passed 14 agent-management scenarios, seven runtime
scenarios and the gateway suite. The reqwest 0.13.5 update rebuilt shared HTTP
dependencies: the gateway check took 71 seconds while the manager's warm check took
3.3 seconds. Clippy took 12 seconds after its fixes. These are local recorded checks,
not image-build or installed-controller timings. Two inadvertently overlapping test
commands waited on Cargo's build lock; subsequent evidence commands ran serially.

Managed installation qualification used the actual Kubernetes admission API with
server-dry-run workload requests. The recorded fixture passed in 6.7 seconds and
removed its namespace, RBAC and admission objects. Policy corrections addressed
Pod default omission and the fixture namespace before that passing attempt. The
chart tests passed in 42.7 seconds including HTTP dependency recompilation; their
assertions took 0.22 seconds. The warm manager, deployment contract and image-plan
checks took 0.5, 1.2 and 1.8 seconds. Native admission remains point-in-time runtime
evidence and is deliberately not reusable source-only coverage.

The managed runtime release published only gateway, browser edge, kernel and manager
from `cbb0feee` in 417.8 seconds. It reused the existing simulator images. Source
checks had accumulated 240 GiB of older incremental directories, test executables
and superseded library variants; targeted removal preserved current outputs and
freed that allocation. Unused Docker images reclaimed another 6.1 GB. Free space
rose to 428 GiB before publication and remained 424 GiB afterward. The release
preflight passed with a 40 GiB growth allowance and the normal 20 percent reserve.
A later Helm schema check rebuilt a previously cached host feature combination and
took 110 seconds; its assertions still took 0.23 seconds. Cache cleanup exchanged
that one-time rebuild cost for deployment headroom, without deleting registry
artifacts, the managed compiler cache or any production volume.

The first installed manager release (`c2170b83`, Helm revision 181) exposed two
integration gaps: Kubernetes omitted default-false `volumeMount.readOnly` during
readback, and the generated PKCS#8 key did not match the kernel's PKCS#1 signing
boundary. The admission fixture had only checked request success. It now decodes
the actual API response through the manager's resource type, and credential checks
cover the canonical kernel encoding. The failed instance retained its volume and
public identity; repair changes the encoding of the same key without rotating it.

Restoring the already-published UAV simulator required a cold 17.57 GB image pull.
Kubernetes reported 424.3 seconds for pull and extraction, followed by about 52
seconds of simulator startup. Host free space fell from 424 GiB to 370 GiB while
restoring the runtime. This is installed cache recovery cost, not a simulator rebuild
or the cost of creating each managed agent. Keep recently required GPU images
available within the release reserve and budget their uncompressed size.

The gateway configuration rollout also produced transient Console timeouts despite
ready Pods. Authoring recovered and the browser completed managed draft creation and
publication. A deployment retry retained its request UUID and admitted one operation.
Readiness alone does not establish browser acceptance; retain the public-route check
after each rollout. The exact source of the transient latency remains to be measured.

The manager's standalone clippy command was outside the reusable check catalog and
snapshotted documentation too. Editing this iteration log during that command
invalidated its otherwise passing 73-second compile. The frozen-input retry reuses
the compiler output. Do not overlap any repository edits with an unqualified
recorder command; a future catalog entry should bound this package's actual inputs.

The corrected manager-only image published in 220.4 seconds; compilation consumed
209.3 seconds. Unlike the preceding four-target release, the single-target build
recompiled shared HTTP and SurrealDB dependencies. The changed dependency feature
combination is a likely cache-reuse cost and should be measured before changing the
builder's compilation groups. Only the manager image was published.

The first installed token request found a routing gap that resolver-only tests had
missed: `/oauth/token` still selected a resource through the static client catalog.
The endpoint now resolves effective registration first, and the HTTP regression
covers durable registration, implicit resource selection, denied resource selection
and definition revocation. All 68 active gateway binary tests passed in 4.1 seconds.
The focused Clippy invocation compiled another dependency feature combination and
took 75 seconds; use the registered check commands to keep cache and evidence scope
consistent across iterations.

GitHub rejected the manager-repair checkpoint's otherwise passing observations
because its repository-wide receipts included hydrated LFS images while checkout had
LFS pointers. The exact byte comparison remains correct. Re-record the affected
registered component check, whose declared inputs exclude unrelated documentation
images; do not widen checkout or weaken evidence hashing merely to make this pass.

The token-routing repair staged only `mcp-gateway` in 104.5 seconds, with a 94.6-second
compile window. It reused the browser edge, kernel, manager and simulator images.


### Managed pilot cutover — September 20, 2026

The combined gateway/manager stage for source `4295b164` took 90.3 seconds, including
79.8 seconds of shared compilation. The cutover changed their common contract
validator and reused the installed kernel and simulator images. Four managed resumes
then created Ready workloads against retained physical volumes without image builds.

Qualification found two remaining static-client assumptions, first in token routing
and then in catalog validation. Testing a newly created managed client and a profile
with no static clients before migration would have caught both in one build cycle.
Those cases now have focused regression coverage. The first record rehearsal also
needed recursive removal of absent optional fields before comparing typed SDK values
with stored SQL objects; the live records were unchanged throughout the rehearsal.

A standalone contract test rebuilt an alternate Cargo feature graph, and one cold
Clippy pass took 76 seconds. Run checks with the affected component's ordinary feature
closure where practical. Avoid concurrent Cargo commands against the same target
folder, since they serialize on its lock. Exact command registration also matters:
component-scoped evidence avoids the unrelated hydrated-LFS differences observed in
repository-wide fallback receipts.

The UAV packaging cutover's first native pilot smoke compiled its broad dependency
closure in 246 seconds. Warm fixture edits then compiled in 8–18 seconds. The smoke
crate still brings Recording, Stream and SUMO code into this focused agent scenario;
reducing that compile boundary is a follow-up performance opportunity. Its default
local cuOpt image was absent, so qualification explicitly selected the installed
executor digest `14e54f2e0d4192ee1c7dec908b867f766c800e60c3673e76d686219f36065f45`.
Image availability belongs in a prerequisite check before that cold compile.

The old smoke fixture also referenced a deleted memory schema and an obsolete Task
wake phrase. Its script mistook an operator request retained in memory for a new
instruction. Qualification now uses the packaged schema, matches only the current
wake for initial dispatch, waits for completion of the Task-result episode, and
asserts one accepted Task. These fixture repairs preserve the duplicate-dispatch
assertion rather than relaxing it.

The final telemetry assertion exposed a runtime bug: OTLP's unified feature graph
selected an async HTTP client for OS-thread batch processors. Both export threads
panicked without a Tokio reactor. Shared telemetry now selects the blocking client
explicitly and preserves standard export timeouts. The native GPU pilot smoke passed
with actual log and trace delivery. This uses the existing exact Reqwest 0.13.5 pin,
verified against its [upstream release](https://github.com/seanmonstar/reqwest/releases/tag/v0.13.5).
Bioma currently configures service names but no OTLP export endpoint; this defect did
not interrupt its managed pilot execution.

The UAV MCP image for `b47bace9` staged in 220.9 seconds, of which 208.4 seconds
were compilation. The standalone target rebuilt dependencies including SurrealDB;
export and push together took about two seconds. Chart publication completed in
under a second. This rollout reuses the simulator and pilot kernel images. Shared
telemetry's fix reaches other deployed binaries when their owning release rebuilds
them; the locally qualified kernel is not a claim that every installed image changed.

The retained-volume transition temporarily froze the root Kustomization and disabled
UAV upgrade remediation. An automatic rollback to the previous chart would recreate
superseded pilot workloads. The first new upgrade applied the intended resources but
its health check was cancelled by an overlapping explicit reconciliation request.
One reset completed Helm release 101. Normal remediation and root reconciliation were
then restored. When a desired-state edit has already triggered an upgrade, observe
that operation before requesting another reconciliation. The simulator Pod stayed
unchanged. The final passive convergence check took about one second; that measures
an already completed rollout, not the full activation duration.

Long-idle managed pilots also exposed subscription retries with expired credentials.
Request preflight refreshes active sessions, but the idle scheduler skips that work.
This remains a correctness fix for agent management, with a required zero-model-call
renewal check. The browser catalog's initial missing Apps recovered through SSE;
credential renewal and cold discovery must be diagnosed independently.

### Idle kernel renewal qualification — September 20, 2026

The scheduler correction uses the existing maintenance tick. The first scheduler smoke
rebuilt the Media-based dependency feature set, including shared SurrealDB and Rerun
crates; Cargo's compilation window was 183 seconds. The pilot scenario's first compile
took 14.18 seconds. After the test-only correction, compilation took 19.17 seconds for
the scheduler and 10.99 seconds for the pilot. Keep feature-set changes separate from
source-edit cost when comparing iteration times.

The pilot's first run completed its Task continuation but failed an assertion against
Rig's tool-replacement warning. The pinned adapter emits that warning during a valid
generation-fenced listChanged refresh as well as generic replacement. The smoke now
uses its dispatch, single-Task, consumed-result, memory and notification checks as
evidence. The final scheduler and GPU pilot checks both pass. No SDK upgrade was needed.

Deployment is not yet complete. The approved template includes an immutable image,
but managed revision adoption refuses a changed template revision. That restriction
prevents shipping a kernel patch to retained instances through the current API. Add
an explicit, validated image-adoption path that retains existing identity and memory;
do not introduce a second workload owner or bypass the template pin.

### Managed image-adoption qualification — September 20, 2026

Image adoption now uses the existing revision and controller lifecycle. Its test
selection combines the gateway binary and agent-management database tests in one
Cargo invocation. It passed 69 gateway tests and 14 database tests. A first compile
failed on a test fixture's identifier constructor; the corrected check took 55.5
seconds. Controller checks passed, including a five-second Kubernetes fixture that
revokes executable admission while retaining permission to delete the owned workload.
Rust lint took 32.6 seconds. The headed GPU browser regression took 5.3 seconds;
Console build and lint took 6.7 and 7.1 seconds respectively.

The source change also required requalifying kernel behavior against the updated store
and shared contract. Scheduler and GPU pilot compilation took 58.41 and 77 seconds
for their distinct dependency feature sets. Both checks passed. The installed baseline
confirms all four retained runtime identities, signing keys and physical memory volumes
before the upgrade. The simulator still has Pod UID
`ae13626f-71ae-405b-9e10-b9485127cc01`.

The initial preflight requested 293 GiB of free reserve plus 5 GiB growth, exceeding
the 292 GiB then available. The scoped four-image budget now allows 20 GiB growth and
retains 256 GiB free; it passes with no Kubernetes DiskPressure. This is an explicit
build budget, not disk exhaustion or a cache purge. Publication selects gateway,
kernel, manager and browser edge, and leaves the simulator image untouched.
