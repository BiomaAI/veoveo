# Development Iteration

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Docker Buildx 0.35.0 and BuildKit 0.31.2 | repository-managed Bake execution and cache worker |
| OCI Image Spec | immutable `linux/amd64` runnable manifests and attested publication indexes |
| Git commit identity | exact source revision and reproducible source timestamp |
| Helm values | complete registry and image-digest map consumed by GitOps |
| Chrome DevTools Protocol | headed hardware-browser acceptance and request cancellation evidence |
| Rerun 0.35.0 RRD | bounded live history and governed archive playback |
| `veoveo.io/image-affected-plan/v1` | repository-owned affected-surface closure |
| `veoveo.io/development-image-lock/v1` | repository-owned non-release deployment closure |
| `veoveo.io/uav-showcase-browser-evidence/v6` | focused multi-camera pixel, cadence, isolated-viewer, simulation real-time-factor, Stream, and Recording evidence over a running simulation |
| `veoveo.io/uav-recording-browser-evidence/v1` | source-clock and camera-pane evidence for one live governed recording |

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
  --registry <profile-registry> \
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

The complete command reads the running simulation and current leader camera, opens dedicated Console tabs
for authoritative live cameras, Stream, and Recording, verifies headed hardware graphics, captures
evidence, closes the tabs, and proves that simulation time advanced. A browser failure can
therefore be retried without repeating a flight. The command has its own
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
| Hub materialization | messages, bytes, opened/frozen segments, quarantine, Blueprint publication and rejection |
| Live Recording playback | current live segment bytes, bounded history seconds, video preroll seconds, canceled and failed browser requests |
| Authoritative live view | logical-camera revision, encoded-product identity, hardware encoder, frame age, connected viewer leases, capacity denials |
| Browser | hardware adapter, video advance, decode identity, Rerun network mode, request cancellation, screenshot digest |

The forwarder uploader is event-driven. Durable enqueue wakes it immediately, and a
durable acknowledgement wakes producers waiting for queue capacity. Network failures
retain bounded exponential backoff because no local event can make the remote endpoint
healthy.

Authoritative live view is event-driven. A logical-camera mutation activates or replaces
one simulator-hosted camera definition. A viewer operation atomically assigns one
preallocated native viewer slot to that logical camera and browser instance. The slot
owns its camera clone, RTX render product, NVENC session, WebRTC endpoint, and exact
release lifecycle. Product state changes and WebRTC signaling wake their consumers
directly. No controller polls or replays a healthy simulator, camera, product, or
browser lease.

Recording live playback watches filesystem changes and transmits only static context,
one live-profile-compacted recent-history bootstrap, and newly durable data. It does not
scan an entire active recording for every new viewer or leave initial compaction on the
browser rendering thread.

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

## Recorded Iteration Sinks

The current controls address the main observed sinks:

| Sink | Control |
|---|---|
| One all-images build after any edit | affected target and consumer closure |
| Release attestations on every test | staging without release attestations, followed by digest-preserving qualification |
| Rewriting large inherited image layers | commit-timestamp clamping with clean reproducibility proof |
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

The latest measured misses remain open:

| Sink | Observed cost | Required correction |
|---|---:|---|
| Concurrent `image stage` commands each reconcile the same named BuildKit worker and can replace it while another stage is active | parallel three-target staging canceled active solves before image compilation began | make builder reconciliation a separately acquired, process-safe checkpoint, then allow one multi-target Bake solve to publish per-target evidence |
| A chart-version annotation changes the simulator pod template during an otherwise unrelated platform chart rollout | one full cached Isaac restart | decouple application chart publication from platform-only chart changes and remove non-runtime release metadata from the pod template |
| `uav-showcase-up` builds the broad smoke binary after a focused browser edit | 30.74 s warm compile | move always-on showcase convergence into its own focused harness |
| Separate reverse-dependent stages repeat overlapping optimized Rust compilation | about 40 s per cold target cache | execute one exact multi-target Bake stage and emit per-target evidence from the shared invocation |
| Image staging accepts a cluster-internal registry authority that the host BuildKit worker cannot reach and infers TLS from its non-loopback name | 6 min 55 s of simulator and Rust compilation completed before the first push request failed against the unreachable host port | model build/push and cluster-pull registry authorities with an explicit transport, validate the push endpoint before starting Bake, and preserve the same cache identity across those aliases |
| Recording Hub periodic counters describe its local proxy but not authenticated forwarder ingest | healthy uploads required durable-queue inspection while Hub counters remained zero | expose accepted-message, accepted-byte, backlog, and last-success counters at authenticated ingest |
| Full UAV acceptance sends an already-looping vehicle back toward one fixed low-speed waypoint | about 13 min when the fleet had moved several kilometres from the fixture origin | derive one nearby bounded maneuver from the current authorized pose |
| Warm `showcase-uav-sim` group staging still rewrites and pushes most of the image lineage | 145.530 s total; BuildKit 143.168 s, provenance 139.423 s, timestamp normalization 139.614 s, export 140.680 s, and push 111.351 s; only 35 of 69 vertices were cached | preserve normalized parent layers across source-only overlays and emit a byte/layer breakdown for the export and push tail; measured by `cargo xtask image stage --group showcase-uav-sim --registry localhost:5001 --revision da70aa968b9a8017d25cefac88cb53c9b90df936 --evidence-output output/development/uav-sim-da70aa96.stage.json`, with phase evidence in `target/veoveo-xtask/evidence/da70aa968b9a8017d25cefac88cb53c9b90df936/stage-group-showcase-uav-sim-1786132506798538397-3968604/run.json` |
| Focused composed browser acceptance assumed that Stream already owned a processing session after a simulator rollout | 180 s spent waiting for a session while the loaded App already exposed an admitted pipeline and `Start live session` action | start one admitted pipeline immediately when no session exists, leave an existing session untouched, and emit partial evidence before a later checkpoint fails; observed with `cargo xtask smoke uav-showcase-browser-verify --public-base-url https://installation.example --chrome-cdp-url http://127.0.0.1:9222` under source revision `82aff1c37124624b37c10241683c74c36786cac9` |
| Task acceptance repeatedly creates short MCP sessions and token exchanges while waiting for one task | repeated closed-subscription warnings and avoidable request latency | retain one authorized task subscription for the acceptance run |
| Long UAV acceptance binds one operator credential and one camera revision for the entire run | a completed mission crossed the 20 min credential lifetime; cleanup then used a stale camera revision | renew the acceptance credential before expiry and read the current camera revision before cleanup |
| Browser acceptance can reuse an operator's interactive Console target | operator navigation canceled the final exclusive-network assertion after all captures completed | create a dedicated authenticated browser target for each acceptance run |
| Local image load and registry stage derive different BuildKit daemon configurations for the same named builder | a one-file Python fix caused two destructive builder reconciliations; local load took 174.265 s and registry stage repeated another 158.391 s with only 31 cached vertices in each solve; stage evidence: `target/veoveo-xtask/evidence/6315083f650c2dc6215f99f630c25361fd2d967a/stage-target-uav-sim-runtime-1786128123199129304-3625626/run.json` | give local load and registry publication one stable builder configuration, preserve worker state across transport changes, and reject any routine command that would destroy a warm builder |
| Build and stage use raw BuildKit progress internally but emit no phase progress while a solve is active | the 174.265 s target build was silent after builder readiness until final evidence emission | stream bounded vertex and phase progress while retaining the machine-readable event trace |
| `uav-sim-runtime` source-only changes traverse the complete simulator dependency graph and large-image export | the targeted build spent 169.203 s in provenance-associated solve work and 170.565 s through export despite zero Rust compilation; evidence: `target/veoveo-xtask/evidence/6315083f650c2dc6215f99f630c25361fd2d967a/build-target-uav-sim-runtime-1786127805069095581-3602194/run.json` | split the frequently changed UAV overlay from immutable PX4, Cesium, Isaac, and Python dependency payloads while preserving one final digest and GPU runtime contract |
| The broad smoke package owns focused live-view browser assertions | one verifier-only edit triggered a 2 min 19 s test build of SurrealDB, Rerun, Recording Hub, Recording MCP, Stream MCP, task runtime, and unrelated deployment scenarios before 120 relevant and unrelated tests ran in under 0.3 s | move composed flight scenarios behind a separate crate boundary and keep the focused browser package independent of platform store, recording services, and deployment smoke dependencies |
| The nominally focused browser and UAV MCP test pair still shares platform-scale dependencies | a two-package run took 1 min 54 s while its tests completed in under one second; SurrealDB, DuckDB, the task runtime, and unrelated service clients remained in the compile closure | move live-view state-machine fixtures behind focused library boundaries and remove database, recording, and task-runtime features from the browser verifier graph |
| An App-only MCP image update replaces the co-located simulator pod | staging the corrected App took 34.304 s, including 24.584 s of compilation and 52 ms of registry push, then rollout discarded the healthy simulator and required about two minutes of Isaac, tile, fleet, and recording recovery; evidence: `target/veoveo-xtask/evidence/05e3f2a684aa7b13d17bfe4bef83413949b613f6/stage-target-uav-sim-mcp-1786129571843377331-3746578/run.json` | separate independently replaceable control-plane and static-App lifecycle from the GPU simulator lifecycle without reintroducing a remote pose mirror or a second renderer |
| A simultaneous-viewer verifier opened two tabs in one headed browser window | the first live acceptance stopped at the visibility preflight before either native viewer slot was allocated because opening the second tab backgrounded the first | create one headed window per simultaneous viewer and retain visibility plus hardware-adapter checks for every window |
| Two fresh viewer windows start independent PKCE flows against one shared pending-authorization cookie | the first two-window retry reached the corrected OAuth scope set, then one callback failed when the concurrent login overwrote its pending state | establish one authenticated Console session before opening actor- and browser-instance-scoped viewer windows |
| Viewer assignment returns before the native AOV signaling listener accepts connections | slot 0 succeeded after a prior preflight had warmed it, while the first slot 1 connection reached its assigned product and failed with a pod-local connection refusal | complete assignment only after render-frame and native-listener readiness events; return the slot immediately when bounded activation fails |
| One failed viewer can strand its peer at an unbounded synchronization barrier | the failed slot 1 run held the healthy slot 0 window until the outer acceptance timeout and left cleanup to cancellation | bound the simultaneous-video barrier independently and close both dedicated targets through the ordinary release path |
| The parent GitOps application waits for its repository refresh interval before advancing immutable child revisions | a pushed configuration correction left the child revision unchanged for more than 30 s; an explicit hard refresh advanced it on the next 5 s observation | expose a source-controlled deployment wait command that requests refresh and reports parent fetch, child render, apply, rollout, and readiness phases separately |
| `image stage` reconciles the managed BuildKit worker before validating the requested immutable Git revision | an invalid revision spent 10.9 s inspecting and reconciling an already-ready builder before returning `Needed a single revision` | resolve the source revision and verify cleanliness before acquiring or inspecting the builder; observed with `cargo xtask image stage --target uav-sim-mcp --registry localhost:5001 --revision 104b4360d8e2a6ac703f848daf518708b27431f0 --evidence-output output/development/uav-sim-mcp-104b4360.stage.json` |
| The live-view cadence gate used one fixed two-second sleep beginning at decoder startup | the first dual-view run measured 43 frames in 2.011 s, reported 21.39 fps, and ended the composed acceptance before steady-state sampling | warm on 12 `requestVideoFrameCallback` events, then measure 48 presented-frame intervals reactively; retain the declared-rate and dropped-frame gates without a polling timer |
| The focused browser run validates cross-cutting performance only after serially collecting every visual surface | a 195.49 s run completed two simultaneous viewers, four additional cameras, Stream, and Recording before rejecting a 0.9795 simulation real-time factor; the late failure wrote no manifest and exceeded the three-minute warm budget by 15.49 s | sample real-time factor throughout the run, persist completed checkpoints before later gates, and schedule independent camera captures in slot-bounded pairs |
| A long-lived Stream session inherited GStreamer's 60-second RTP dropout tolerance across a simulator replacement | the focused browser retry reached Stream after 50.12 s, then rejected an overlay that was 60.886 s stale; the session remained `running` while its source epoch had changed | publish an RFC 3550 source epoch on every producer start, bound admitted dropout and misorder recovery explicitly, and retain the Stream session across source replacement |
| A Stream catalog and RTP-epoch correction invalidates almost the entire Stream image build | targeted staging took 65.78 s; BuildKit consumed 50.304 s, optimized compilation consumed 42.330 s, 20 vertices executed, and only 4 were cached | place the admitted catalog and embedded server documentation after stable binary compilation inputs, split C++ runner and Rust-server cache keys, and retain one runnable image digest; evidence: `target/veoveo-xtask/evidence/73eb196ca61f5979e51ba5c788a266cd503b04d6/stage-target-stream-mcp-1786137623809246800-175638/run.json` |
| The focused browser gate serializes a mandatory 120-second Recording follow check after the other live surfaces | a correct run took 206.64 s and exceeded the three-minute warm budget by 26.64 s; dispatch compiled in 0.41 s, while the product held 1.0000 real-time factor, zero Recording lag, and zero browser frame drops | run the independent Stream and native-camera checkpoints concurrently with the Recording stability window, emit bounded checkpoint progress, and retain one final cross-surface simulation-time assertion; evidence: `output/acceptance/uav-browser/7dc65ac60121550618014e3c69147a7fd3a1471e/019fde24-18d4-7e20-9f20-d0be637a223b/evidence.json` |
| Proving one Stream source-epoch replacement requires a complete cached GPU-simulator pod replacement | the exact-image restart took 86 s before runtime readiness, even though the retained Stream session resumed with the same identity and advancing result counters | add a focused admitted RTP source-epoch fixture for ordinary Stream iteration and reserve the real GPU pod-replacement proof for release acceptance; the release proof must still exercise the authoritative simulator and may not substitute the fixture |
| The isolated external-simulation fixture still dispatches through the platform-scale smoke binary | a three-line SDK digest correction rebuilt `veoveo-smoke` for 7.55 s, used 5.4 GB maximum RSS, and took 18.56 s end to end even though the isolated private-index, package, Bake, and Helm checks are one focused scenario | move the external repository fixture into a focused Rust harness with only package-publication, HTTP-index, Bake-print, and Helm dependencies while retaining the same typed smoke command |
| Documentation PDF publication is a manual CDP sequence without a source-controlled lifecycle or phase record | one three-document headed-Chrome run wrote all outputs but emitted no per-document progress and retained its CDP process for more than three minutes until interrupted; the corrected one-document invocation with explicit disconnect completed in 2.95 s | add a typed `cargo xtask` publication command that performs the mandatory hardware probe, prints each canonical source independently, reports phase timing, terminates its CDP client, validates page identity, and dispatches visual QA |
| The full UAV gate retained the deleted one-product-per-logical-camera acceptance model | 22.14 s reached a healthy authoritative world, five logical cameras, and two correctly inactive physical viewer slots before a stale assertion rejected the slot pool | share typed idle-slot and assigned-slot invariants between focused and full acceptance, and run the contract-only state test before launching a flight; observed under source revision `1456fb2ce3a163835c9a39ee31f32032300590d0` with `cargo xtask smoke uav-showcase-verify --context <context> --public-base-url https://installation.example --chrome-cdp-url http://127.0.0.1:9222` |
| Full acceptance treated every Work-Context-readable Stream session as operator-owned cleanup state | 33.85 s compiled and reached preflight before attempting to stop a healthy browser-owned session; the owner-scoped stop correctly returned not found | reuse the one visible active session without taking ownership, stop only a session created by the acceptance actor, and cover selection plus duplicate-active rejection in the contract test before flight; observed under source revision `f97c87d2969262e32077679f70db18bfb281b043` with the full UAV command above |

## Qualification

After staged behavior is accepted, qualify the same source and require the same runnable
identity:

```bash
cargo xtask release images \
  --target <affected-target> \
  --registry <profile-registry> \
  --revision "$(git rev-parse HEAD)" \
  --stage-evidence output/development/<affected-target>.stage.json
```

Qualification fails if rebuilding changes the runnable digest. A complete release then
regenerates the deployment lock, performs the profile acceptance selected by its
contract, and records SBOM and provenance on every publication index.
