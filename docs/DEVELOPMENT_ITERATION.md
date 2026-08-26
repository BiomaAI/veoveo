# Development Iteration

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Docker Buildx 0.35.0 and BuildKit 0.31.2 | repository-managed Bake execution and cache worker |
| OCI Image Spec | immutable `linux/amd64` runnable manifests and attested publication indexes |
| Git commit identity | exact source revision and reproducible source timestamp |
| Helm values | complete registry and image-digest map consumed by GitOps |
| Chrome DevTools Protocol | headed hardware-browser acceptance and request cancellation evidence |
| Rerun 0.36.2 RRD | bounded live history and governed archive playback |
| `veoveo.io/image-affected-plan/v1` | repository-owned affected-surface closure |
| `veoveo.io/development-image-lock/v1` | repository-owned non-release deployment closure |
| `veoveo.io/gitops-convergence-evidence/v2` | repository-owned exact Flux source revision, root apply, Helm inventory, rollout, and readiness evidence |
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

Inspect `imageTargets` and every broadening reason. Stage each selected target to the
profile registry. Multiple stage evidence files may be merged into one closure.

```bash
revision="$(git rev-parse HEAD)"
cargo xtask image stage \
  --target <affected-target> \
  --push-registry <host-reachable-registry> \
  --pull-registry <cluster-reachable-registry> \
  --registry-transport <tls-or-insecure-http> \
  --revision "$revision" \
  --evidence-output output/development/<affected-target>.stage.json

cargo xtask image development-lock \
  --base-lock <qualified-deployment-lock> \
  --stage-evidence output/development/<affected-target>.stage.json \
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
cargo xtask smoke profile-up --profile <profile.json> --lock <qualified-lock.json>
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

The command requests reconciliation, then consumes Kubernetes watch events. It does
not sleep between status reads. Source fetch, desired-state apply, Helm release,
Deployment rollout, and readiness retain separate elapsed times. A timeout writes
failed evidence for the exact phase that did not converge.

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

### Active Follow-Ups Worth Fixing Next

| Priority | Boundary | Owning component | Acceptance condition |
|---:|---|---|---|
| 1 | Large-image normalized export and push tail | OCI exporter and UAV image graph | one committed source-only UAV runtime stage reuses normalized dependency layers and completes in under 30 s while retaining the same dependency payload digests |
| 2 | Console static presentation updates | Console image graph | a web-only edit stages the frontend without compiling unrelated Rust binaries and completes in under 30 s |
| 3 | Map source-isolated Rust cache and dependency closure | Map image graph | a Map-only edit uses the retained registry and target cache, excludes unrelated visual-server features, and completes a warm stage in under 30 s |
| 4 | Shared multi-target staging | image orchestration | one exact Bake solve publishes all selected targets, preserves the named builder, and emits one independent digest and evidence record per target |
| 5 | Focused composed-flight harness closure | smoke harness ownership | a verifier-only edit neither compiles nor links store, task, recording, or unrelated server runtimes, and dispatch remains below 2 s warm |
| 6 | Stream native and Rust cache boundaries | Stream image graph | catalog or embedded-document edits do not rebuild the native runner or unrelated Rust packages, and the warm target stage completes in under 30 s |

### Deferred Or Separately Owned Work

| Boundary | Disposition |
|---|---|
| Full release attestation and large inherited-image qualification | reserved for release acceptance; development staging must not pay this cost before behavior is accepted |
| GPU-renderer startup and live-camera latency | runtime performance work under the UAV design, not an image-orchestration fallback or a reason to weaken GPU acceptance |
| Provider and external network recovery | owned by the relevant runtime contract; provider completion remains webhook-only |
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

These measurements were taken after the current improvement cycle. They validate the
control-plane budgets without claiming that the still-open large-image export tail has
met its 30-second target.

| Checkpoint | Measured | Budget | Result |
|---|---:|---:|---|
| Affected-target plan | 3.43 s | 10 s | pass |
| Slowest sampled selected-target plan | 4.49 s | 10 s | pass |
| Focused deployment harness dispatch | 1.21 s | 2 s | pass |
| Recording ingest diagnostics test | 4.11 s | 10 s | pass |
| GitOps controller convergence against a healthy fixture | under 1 s | 30 s | pass |
| Managed builder ensure without replacement | 7.57 s | diagnostic only | stable worker retained |
| Source-only UAV runtime stage | not remeasured | 30 s | open pending exporter correction |

## Recorded Iteration Sinks

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
