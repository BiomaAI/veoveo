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
| `veoveo.io/uav-showcase-browser-evidence/v2` | focused visual evidence over a running simulation |
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
for Simulation View, Stream, and Recording, verifies headed hardware graphics, captures
evidence, closes the tabs, and proves that pose sequence advanced. A browser failure can
therefore be retried without repeating a flight. The command has its own
`veoveo-browser-smoke` dependency graph and builds the MCP conformance client only when
an actual run requires it.

The Recording-only command does not depend on a Stream or Simulation View session. It
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
| Simulation View | desired/realized revision, reconciliation phase, failed dependency, next attempt, authorization expiry, camera pose sequence |
| Browser | hardware adapter, video advance, decode identity, Rerun network mode, request cancellation, screenshot digest |

The forwarder uploader is event-driven. Durable enqueue wakes it immediately, and a
durable acknowledgement wakes producers waiting for queue capacity. Network failures
retain bounded exponential backoff because no local event can make the remote endpoint
healthy.

Simulation View is also event-driven. Durable state commits and runtime generation
changes wake reconciliation. The scheduler sleeps until the earliest real authorization
renewal or failed-dependency retry deadline when no event is pending. A converged healthy
session is absent from the reconciliation selection and causes no runtime replay.

Recording live playback watches filesystem changes and transmits only static context,
the configured recent-history window, and newly durable data. It does not scan an entire
active recording for every new viewer.

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
| Polling an empty recording queue | enqueue and capacity notifications |
| Re-reading an unchanged ingest identity and committed checkpoint for every live sample | serialized authorized-stream checkpoint with transactional revision and sequence comparison |
| Replaying healthy Simulation View state | durable event wake plus exact deadline scheduling |
| Guessing where Recording latency lives | boundary-specific queue, ingest, playback, and browser counters |

Remaining misses must be added here with the evidence path, measured phase, cache state,
and exact command. Wall time without phase evidence is not enough to choose an
optimization.

The latest measured misses remain open:

| Sink | Observed cost | Required correction |
|---|---:|---|
| A chart-version annotation changes the simulator pod template during an otherwise unrelated platform chart rollout | one full cached Isaac restart | decouple application chart publication from platform-only chart changes and remove non-runtime release metadata from the pod template |
| `uav-showcase-up` builds the broad smoke binary after a focused browser edit | 30.74 s warm compile | move always-on showcase convergence into its own focused harness |
| Separate reverse-dependent stages repeat overlapping optimized Rust compilation | about 40 s per cold target cache | execute one exact multi-target Bake stage and emit per-target evidence from the shared invocation |

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
