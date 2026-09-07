# Build And Deployment Iteration Audit

Status: implementation authorized on September 6, 2026; delivery is in progress.
The findings below retain the pre-change evidence. The delivery record identifies
implemented changes and their verification.

## Delivery Record

| Concern | Implementation | Verification |
|---|---|---|
| Rollout triggers | Removed chart-version Pod annotations; Stream hashes its rendered runtime files; Flux values ConfigMaps carry the watch label | Rendered chart tests cover metadata-only publication, scoped catalog changes, image-only changes, and generated Flux watch labels |
| Builder resources | Declared 12 CPUs and 36 GiB without swap, rejected resource drift, exposed cgroup CPU snapshots, and upgraded the managed Buildx/BuildKit pins | Eight control tests and strict Clippy pass; the worker was reconfigured with its existing state volume; controlled timing remains in progress |
| Focused configuration checks | Routed `helm-config` through the existing deployment harness and moved its assertions beside that harness | Dispatcher coverage rejects the broad smoke/conformance build unit; the same assertions remain part of the full gateway suite |
| Release inputs | Split Bioma image locks by rendered release closure; selected Helm publication accepts repeated `--chart` with isolated receipts | Render tests compare generated locks with consumed images; packaging tests require only the selected chart artifact |
| Complete command timing | Added command-entry records with preparation, lock waits, solve references, manifest inspection, terminal failures, and per-solve CPU deltas | Failure-path tests retain errors before BuildKit starts; command and solve records have distinct outcomes |
| Exact image selection | Repeated `--target` flags select one sorted Bake solve for planning, local builds, staging, and qualification; affected planning excludes dev-only Cargo edges | CLI selection tests and dependency-closure fixtures cover staging/qualification parity and retained build dependencies |

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
