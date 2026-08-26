# Platform Improvements Plan

Status: canonical multi-cycle implementation plan and delivery record. The `001`–`013`
cycle is closed. Nine requests are delivered, three unfinished themes have one explicit
successor in the `014`–`023` cycle, and private build inputs closed without implementation
because no approved need opened that optional track. The `014`–`023` cycle is approved and
implementation has not started. Existing component designs remain normative until each current
phase lands and deletes the surface it replaces.

## Standards And Protocols

Each cycle records the exact external standards, wire protocols, data formats, and
repository-owned extensions that bound its changes. The completed cycle table is a historical
record of the versions used by its delivered evidence. The current cycle table defines the
implementation boundary and requires dependency currency to be rechecked when a pin is touched.
Together those two tables form this plan's standards register.

## Cycle Register

| Requests | Review input | State | Canonical disposition |
|---|---|---|---|
| `001`–`013` | `veoveo-platform-improvements-2026-08-14` | closed on 2026-08-26 | nine delivered, `003` partially delivered and superseded by `023`, `009` superseded by `022`, `010` superseded by `021`, and `013` closed without implementation |
| `014`–`023` | `veoveo-platform-improvements-2026-08-26` | approved, not started | current hard-cut implementation plan |

No work remains under an old request identifier after it moves to a current successor. Future
evidence and implementation update the current cycle only. Historical commits and acceptance
records remain in the completed cycle rather than being rewritten as if they were produced by the
new review.

## Completed Cycle: Requests 001–013

The approved core and ergonomic work was implemented and deployed to the Bioma installation.
Phase 1 resolves from the immutable Rig fork commit
`1c59bf04ed474cc7bdf8aefb2882bb8fefe557f1` and passed its hardware agent pilot. The application
release at `688d34e4ea517f2da3188a1cddc81b00162db0f7` converged through activation
`185b4bcb452df08cd3b5dd0fb70f1739c0931969`. The gateway startup repair converged through
activation `b65e70de30be73be22cd46a8cce416d83dfcb373`, and both replicas remained ready without
restarts.

Baseline: Veoveo main `3029df8f` on 2026-08-14. The input was the reviewed client
package `veoveo-platform-improvements-2026-08-14`, containing thirteen requests and
six reference diffs. The diffs describe intent from an earlier integration snapshot.
They are not a merge series.

This plan strengthens agent recovery, resource use, deployment isolation, standalone
MCP App hosting, model configuration, GPU admission, provenance, spatial correctness,
and private dependency builds. It preserves the current provider webhook-only job
completion contract and the mandatory hardware-GPU boundary.

The detailed phase text is retained as the rationale and acceptance design used by the completed
cycle. Future-tense statements inside that historical text do not reopen work. The request closure
audit and final implementation record determine the cycle's final state.

### Standards And Protocols

| Standard or profile | Plan boundary |
|---|---|
| Model Context Protocol `2026-07-28` | sole protocol profile for Veoveo-owned servers, the gateway, Rig, and first-party clients. Resource reads use JSON-RPC `Invalid Params` for unknown URIs and subscriptions use request-scoped `subscriptions/listen` |
| JSON-RPC 2.0 | protocol error envelope for malformed requests, unknown resources, authorization-independent invalid parameters, and internal failures that cannot produce a tool result |
| MCP Tasks extension, SEP-2663 | durable detached work, task updates, input requests, terminal results, and canonical task identities |
| MCP Apps SEP-1865 / `ext-apps` `2026-01-26` | `ui://` application resources, `text/html;profile=mcp-app`, sandboxed frames, host context, lifecycle notifications, and the `postMessage` bridge |
| Veoveo reactive App resource adapter | repository-owned projection from final MCP resource notifications to contentless App wakes. It is not part of SEP-1865 |
| JSON Schema Draft 2020-12 | closed agent, deployment, domain-reference, model, provenance, and qualified GPU-admission schemas |
| OAuth 2.1 draft 13, RFC 6750, RFC 7523, RFC 8414, RFC 8707, RFC 9207, and RFC 9728 | browser and machine authorization, `private_key_jwt`, metadata discovery, resource indicators, issuer validation, and protected-resource discovery |
| RFC 9110 | HTTP authority, header, cache, redirect, and error behavior for the BFF, gateway, and hosted servers |
| RFC 7946 | public longitude/latitude coordinate order used by Map GeoJSON and the Map-specific DuckDB Spatial axis policy |
| DuckDB 1.5.5 and its Spatial extension | typed `POINT_2D` distance path, `geometry_always_xy`, materialized scoring, and restart-stable Map analytics |
| Kubernetes/K3s 1.36.2 and Helm 4.2.3 | rendered Secret-reference closure for disposable development profiles. Helm and the installation reconciliation controller retain mutation ownership |
| Kubernetes server-side apply managed fields | optional read-only conflict diagnostics only if selected development scope passes its evidence gate. This plan does not transfer field ownership or replay Helm state |
| NVIDIA DRA Driver for GPUs 0.4.1, `resource.nvidia.com/v1beta1`, CUDA, and NVML | exact GPU identity, full-device or MIG capacity, memory admission, and hardware evidence. The repository implements only its declared qualified DRA profile |
| Docker Buildx 0.35.0, BuildKit 0.31.2, and Dockerfile frontend 1.25.0 | secret-mounted private Git credentials and trust inputs, cache isolation, SBOM, and maximum-mode provenance |
| `veoveo.io/deployment/v6` and `veoveo.io/deployment-lock/v6` | delivered disposable-development baseline. This plan reserves no successor version. An approved evidence-gated profile change uses the next version available when it lands |
| `veoveo.io/image-build-plan/v2` and `veoveo.io/image-build-run/v2` | current typed image plan and execution evidence. Credential source paths and bytes never enter either document |

Every dependency or infrastructure component touched during implementation must be
checked against its authoritative upstream release and pinned to the latest stable
compatible version. An immutable fork revision remains valid only when the required
behavior has no stable upstream release and the reason is recorded beside the pin.

### Intended Outcome

Veoveo will make long-running work recoverable without asking a model to reconstruct
platform state. An accepted task, operator input, or resource update survives token
rotation, process exit, and lease transfer. Correctable input errors remain inside the
current episode and carry bounded guidance.

Disposable development deployment will gain a pure rendered closure for non-optional
Secret references before its existing mutation path can run. It will not create or
rewrite installation Secret material. Selected-release development iteration remains
evidence-gated and cannot become an enterprise installation API or a smoke-harness
orchestration responsibility.

Map will use one explicit longitude/latitude interpretation. Standalone MCP Apps will
reuse the Console security boundary and upstream stream fanout. Frames and Time will
return references that downstream compilers can admit without searching a catalog.
Approved optional model controls will use provider-admitted typed contracts. Qualified
GPU capacity work and private build inputs will proceed only after their evidence or
downstream-need gates pass.

### Delivery Ledger

| ID | Priority | Delivery decision | Primary phase |
|---|---|---|---|
| `AGENT-CONTINUATION-001` | P0 | retain the delivered durable runtime. Audit lineage and add crash-window evidence before adding new persistence | 1 |
| `TOOL-DIAGNOSTICS-002` | P0 | implement against final MCP errors and current Rig, not the obsolete reference module | 1 |
| `DEPLOY-SCOPE-003` | evidence-gated | first prove a concrete disposable-development iteration need. Do not make it an enterprise API or reserve a schema revision | 2B |
| `SECRET-CLOSURE-004` | P0 | add pure rendered validation of owner-supplied Secret references before the existing development mutation path | 2A |
| `APP-HOST-005` | ergonomic | add the standalone route through the existing Console BFF, frame, bridge, and session | 4 |
| `STREAM-FANOUT-006` | retain and verify | retain the delivered auth-scoped per-URI listener pool. Add capacity and multi-tab evidence | 4 |
| `CREDENTIAL-REFRESH-007` | P0, elevated from P1 | repair the missing agent resource listener and make replacement readiness atomic | 1 |
| `RESOURCE-DISCOVERY-008` | ergonomic | retain domain-owned canonical URIs. Add canonical task handoff, bounded domain discovery, pagination, and agent read budgets without a universal token | 5 |
| `REASONING-CONTROLS-009` | optional | add only for an approved provider configuration need, with capability validation | 6 |
| `ACCELERATOR-ADMISSION-010` | evidence-gated | require measured same-device contention and qualified workload data before changing the deployment profile | 7 |
| `SOURCE-PROVENANCE-011` | ergonomic | add used-source Frames references and effective Time authority references | 8 |
| `SPATIAL-TYPING-012` | P1, elevated from P2 | correct Map axis and distance semantics before further spatial query expansion | 3 |
| `PRIVATE-BUILD-INPUTS-013` | optional | add one opt-in BuildKit-secret contract for repository-managed builders without constraining external build systems | 9 |

#### Request Closure Audit

This audit was refreshed against main `3364167f` on 2026-08-26. Every cited delivery commit is
an ancestor of that revision, and the delivered contract remains present in the current tree.
This consolidation verifies source and committed evidence only. It does not claim a new Linux,
cluster, browser, or hardware acceptance run.

| ID | Final state | Current evidence and successor |
|---|---|---|
| `AGENT-CONTINUATION-001` | delivered | durable agent episodes, task descriptors, wakes, leases, retention pins, atomic consumption, and restart handling remain in `agents/runtime`, `agents/kernel`, `platform/task-runtime`, and Store migrations. Phase 1 commits and the hardware agent pilot provide closure |
| `TOOL-DIAGNOSTICS-002` | delivered | the kernel resource boundary retains bounded safe correction, authorization-independent unknown-resource handling, pagination depth, byte, read, family, and wall-time budgets. Protected failures remain generic |
| `DEPLOY-SCOPE-003` | partially delivered, superseded by `DEPLOY-SCOPE-023` | deployment v6 already owns typed platform selection, source roles, image closure, release identity, and collision rejection. It does not yet perform one component-selected mutation with zero writes to unselected releases; that remaining outcome belongs only to `023` |
| `SECRET-CLOSURE-004` | delivered | commit `25404ab0` and the current deployment contract render the complete Secret-reference closure and reject missing references before a Kubernetes or Helm write |
| `APP-HOST-005` | delivered | the Console BFF, web host, standalone bootstrap, shared frame boundary, and focused headed-browser preflight remain present |
| `STREAM-FANOUT-006` | delivered and verified | the auth-scoped Console MCP client retains bounded reference-counted resource listeners, one upstream listener per admitted URI, acknowledged teardown, and shared Console/standalone use |
| `CREDENTIAL-REFRESH-007` | delivered | commit `da284e64` and the current agent client retain make-before-break request rotation and declared-resource listener continuity |
| `RESOURCE-DISCOVERY-008` | delivered | canonical task handoff, exact resource lookup, stable pagination, the shared serialized-response cap, atomic result consumption, retention metadata, and agent read budgets remain present |
| `REASONING-CONTROLS-009` | not implemented, superseded by `REASONING-022` | no reasoning mode or provider capability registry exists. The newly approved three-state provider contract and its current acceptance boundary live under `022` |
| `ACCELERATOR-ADMISSION-010` | foundation delivered, memory outcome superseded by `GPU-ADMISSION-021` | deployment v6 delivers DRA device identity, replica counts, physical/MIG grouping, allocation checks, and hardware evidence. Persistent bytes, maximum peak bytes, qualified capacity, and headroom admission are absent and belong only to `021` |
| `SOURCE-PROVENANCE-011` | delivered | commits `674fbb27`, `14c4bb57`, and `3c9e56e6` retain exact used Frames revisions, effective Time authority references, and contract-fixture parity |
| `SPATIAL-TYPING-012` | delivered | commit `b6ca32ce` retains the Map longitude/latitude hard cut, typed materialized distance score, stable ordering, new cursor domain, and restart evidence |
| `PRIVATE-BUILD-INPUTS-013` | closed without implementation | no repository-managed private dependency need was approved. The proposed environment names, BuildKit secret IDs, and credential-fetch path are absent. A future need requires a new reviewed request rather than reopening `013` |

`STREAM-FANOUT-006` does not authorize a second browser or gateway fanout system. The
current Console BFF already reference-counts one upstream listener per auth scope and
resource URI. Implementation changes only its bounds, refresh evidence, and standalone
host coverage unless tests prove a different deficiency.

#### Delivery posture

| Track | Included work | Completion meaning |
|---|---|---|
| core correctness | agent listener refresh, safe tool diagnostics, continuation crash evidence, rendered Secret-reference closure, and Map spatial correction | required before this plan's correctness release closes |
| downstream ergonomics | standalone Apps, bounded resource handoff and discovery, and Frames/Time provenance | each milestone may ship independently after its own acceptance |
| optional tooling | provider reasoning controls and private BuildKit inputs | starts only for an approved downstream need and never blocks core closure |
| evidence-gated capacity | selected-release development iteration and GPU memory admission | starts only after the phase records the required evidence and an owner approves the contract change |

No client request is accepted as one indivisible implementation. The plan adopts the
useful outcome and rejects a mechanism when it would create parallel identity,
installation authority, or build ownership.

#### Final Implementation Record

| Phase | Final state on 2026-08-26 | Delivered evidence | Remaining closure |
|---|---|---|---|
| 0 | complete | decisions, protocol versions, ownership, hard-cut surfaces, and the test-driven sequence are recorded in `f2130bef` and `49b9d7ad` | none |
| 1 | complete | Rig fork commit `1c59bf04` and Veoveo commits `da284e64` and `ceb1be9d` deliver typed subscriptions, resource reads, request-boundary preflight, make-before-break rotation, graceful cancellation, exact declared-resource wakes, safe diagnostics, cumulative budgets, and a passing hardware-GPU agent pilot | none |
| 2A | complete | `25404ab0` inserts a pure rendered Secret-reference closure before the development mutation boundary. Contract and smoke suites pass. A revision-pinned disposable K3s audit records only Secret and Namespace GETs and zero writes when three owner-supplied Secrets are absent | none |
| 2B | closed; remaining outcome superseded by `DEPLOY-SCOPE-023` | deployment v6 retains typed component selection, source and release ownership, exact image closure, and collision rejection. The cycle did not prove selected-release mutation with zero writes to unselected owners | none under `003`; current component-scoped mutation work belongs only to `023` |
| 3 | complete | `b6ca32ce` hard-cuts Map axis, typed distance scoring, and cursor behavior. DuckDB and Map suites pass, including restart coverage | none |
| 4 | complete | `9cd3ceab` adds the standalone App route through the shared Console host. `99ae7da7` requires both host preflights, and `47f0ef7e` gives that contract its own focused command. Console BFF, web, browser-smoke, and authenticated headed-browser gates pass on the deployed revision with hardware graphics | none |
| 5 | complete | `70617b25`, `7634d03c`, `5e202843`, and `280d634b` close atomic task consumption, serialized response caps, bounded canonical discovery, and retention metadata handoff. Phase 1 adds the kernel read ledger and remote-pin pilot evidence | none |
| 6 | closed without implementation; superseded by `REASONING-022` | no reasoning mode or provider capability registry exists under `009` | none under `009`; current reasoning work belongs only to `022` |
| 7 | placement foundation complete; memory outcome superseded by `GPU-ADMISSION-021` | deployment v6 retains exact DRA identity, replica and consumer bounds, physical/MIG grouping, allocation verification, and hardware evidence. It has no persistent, peak, capacity, or headroom byte admission | none under `010`; current memory admission belongs only to `021` |
| 8 | complete | `674fbb27`, `14c4bb57`, and `3c9e56e6` expose exact Frames and Time provenance and keep the contract fixture authoritative | none |
| 9 | closed without implementation | no repository-managed private dependency need was approved, and the proposed credentials, CA, host override, and BuildKit secret surfaces remain absent | none; a future request starts from a new review |
| 10 | complete; cycle closed | applicable focused suites, broad durable-task suites, Console gates, the remote-pinned Phase 1 suites, the hardware agent pilot, the disposable-cluster zero-write audit, exact GitOps convergence, and the focused hardware browser host gate pass. The locked repository-wide formatting, all-target check, strict clippy, and library/binary test gates passed | none under `001`–`013` |

The Phase 2A environmental check used a temporary Git-owned profile and lock pinned to
Veoveo `35ba4a1b` and the exact platform-chart archive. Its disposable K3s v1.35.5
cluster enabled Kubernetes metadata auditing before `profile-up`. The rendered closure
found 11 required keys across `veoveo-installation-secrets`, `veoveo-surreal-admin`, and
`veoveo-surreal-runtime`, then returned `missing_secret`. The command's audit window
contained only GET requests and `writeCount: 0`; the target Namespace remained absent.
A later server-side dry-run canary appeared as a `create` request in the same audit log
and remained unpersisted, which proves the policy could detect the prohibited verb. The
disposable cluster was deleted after the check.

The Bioma installation now resolves the release-input commit
`fb96bb790c665468e2f1eeac3566cb8b7baa012b` through parent activation
`b65e70de30be73be22cd46a8cce416d83dfcb373`. The typed convergence harness recorded
successful fetch, render, apply, rollout, and readiness for the platform deployment set
in `output/development/gitops-convergence-gateway-repair-b65e70de.json`. The earlier
application rollout is recorded in
`output/development/gitops-convergence-platform-improvements-185b4bcb.json`.

The rollout exposed a gateway replica taking about 39 seconds to finish its SurrealDB
startup work while liveness committed to termination after roughly 30 seconds. The
rendered-chart regression failed before the gateway had a startup probe. Commit
`25866592` adds a 120-second startup window without weakening steady-state liveness,
updates the exact Bioma control-plane digest, and repairs stale Helm smoke expectations.
Both replicas reached readiness with zero restarts after the repair.

`cargo xtask smoke uav-app-hosts-browser-verify --public-base-url
https://veoveo.bioma.ai --chrome-cdp-url http://127.0.0.1:9222` passes against headed
Chrome 151 on the RTX 4090. It authenticates both the Console and standalone routes,
loads the shared App frame, and rejects software-only browser graphics. The broader UAV
two-viewer throughput verifier remains strict and currently reports a baseline simulator
regression of 9.9–11.2 delivered FPS and 93–99 ms source-to-render p95 against its 12 FPS
and 85 ms gates. That failure occurs after both App-host preflights and does not weaken
or reopen the standalone-host milestone.

#### Recorded red-green evidence

| Concern | Red observation | Permanent green proof |
|---|---|---|
| Rig request handoff | `task_descriptor_uses_the_preflight_selected_client` showed a task descriptor retaining the cancelled preflight client | the focused test passes at fork commit `1c59bf04`. `cargo test -p rig-agent --features rmcp` passes 535 unit tests and 20 integration tests, and the matching clippy gate is clean |
| durable retention handoff | the real `agent-pilot` reached terminal Optimization completion, then failed because the hosted task adapter had lost the repository retention pin lifted into RMCP request context | two task-runtime metadata tests pass, all eleven affected crate suites pass, and the same pilot completes on an RTX 4090 with the immutable cuOpt executor image |
| Secret closure | the missing-reference fixture reached the development mutation boundary before a complete rendered closure existed | deployment contract and smoke tests reject the fixture before the mutating boundary; a revision-pinned disposable K3s run records zero API writes and leaves its target Namespace absent |
| Map distance contract | old-axis and old-cursor fixtures exposed ambiguous coordinate interpretation and reusable legacy cursor state | DuckDB runtime and Map suites prove `geometry_always_xy`, one typed materialized distance score, stable ordering after reopen, and rejection of the old cursor domain |
| standalone App route | the authorized standalone route and bootstrap behavior were absent; the focused real-browser verifier initially opened only the Console host | Console BFF route and host tests pass, the web application passes test, lint, typecheck, and production build gates, and the deployed focused host command passes both Console and standalone preflights in headed hardware Chrome |
| gateway rollout startup | one new replica needed about 39 seconds for store startup, while liveness committed to killing it after roughly 30 seconds | the rendered chart requires a 120-second startup probe, the Helm smoke passes, exact GitOps convergence succeeds, and both deployed replicas remain ready with zero restarts |
| serialized task resources | oversized serialized results and growing result discovery exceeded the intended bounded handoff | MCP conformance, task-runtime, and affected domain suites prove the response cap, exact lookup, stable pagination, and atomic consumption |
| exact provenance | conversion results could require catalog search to discover the effective Frames revision or Time authority | Frames, Time, and MCP contract tests accept exact typed references directly |
| copied deployment catalog | the repository-wide test found stale Timeseries and Optimization contract revisions in the Bioma composition | `696935da` aligns the copied typed metadata, the focused Bioma acceptance passes, and the complete locked workspace library/binary gate passes |

The red transcripts remain review evidence rather than expected-failure tests. The green
tests are committed with their owning changes. Phase 1 is delivered from the immutable
Rig fork revision, and the Veoveo remote pin reproduces its focused, runtime, clippy, and
hardware-pilot results.

### Repository Invariants

- The change is a hard cut at every revised schema, resource grammar, cursor domain,
  command, and configuration field. No alias or compatibility reader is added.
- Provider work remains webhook-completed. No phase adds status polling, recovery
  polling, or a polling fallback.
- GPU workloads continue to fail closed without the selected NVIDIA device, driver,
  and accelerated backend. Memory admission cannot select a CPU or alternate model.
- Browser acceptance uses a headed browser with a hardware-backed WebGPU adapter or
  WebGL context. SwiftShader, llvmpipe, and software rasterizers are rejected.
- Known contract shapes use Rust types and closed enums. Raw JSON remains confined to
  provider-owned payloads, Kubernetes object parsing before typed projection, and other
  genuinely open boundaries.
- Authentication, authorization, transport, timeout, and internal diagnostics remain
  generic at the model boundary. No phase exposes database errors, credentials,
  provider bodies, hidden reasoning, or policy internals.
- Typed deployment profiles remain disposable-development inputs. They are not an
  enterprise installation API, and smoke verification does not own installation
  orchestration.
- Canonical identity remains each domain's governed resource URI and MCP resource link.
  No universal hash-derived resource token or parallel public identity is added.
- Real Secret values never enter a deployment plan, error, log, receipt, build
  argument, cache key, SBOM, provenance statement, or committed test fixture. Ephemeral
  synthetic canaries may prove redaction and are destroyed with the test environment.
- Installation Secret bytes remain owner-supplied. Repository tooling may validate
  references and key presence but does not create, replace, or transfer ownership of
  those Secrets.
- Smoke assertions, evidence, and scenario-local lifecycle remain in Rust. Installation
  orchestration remains with Helm and the installation reconciliation controller. The
  `xtask` smoke command only builds and dispatches the typed verification harness.
- Optional and evidence-gated tracks do not block the core correctness release.

### Reference Patch Policy

| Patch | Use during implementation | Rejected behavior |
|---|---|---|
| `0001-typed-spatial-distance.patch` | port the axis setting, typed overload, materialized score, and cursor-domain hard cut | mechanical application against the older analytics layout |
| `0002-explicit-reasoning-controls.patch` | use its request-rendering test as a narrow example | a `none`-only enum, model-specific comments, and missing provider capability validation |
| `0003-actionable-resource-read-errors.patch` | retain the bounded-normalization and safe/generic classification intent | the deleted `resource_read.rs` baseline, the superseded resource error constant, and old Rig APIs |
| `0004-compiler-ready-frame-provenance.patch` | use as a starting diff after the source-use audit | returning every preloaded revision or retaining an unvalidated digest string |
| `0005-shared-app-frame-sandbox.patch` | extract one shared sandbox constant with a behavioral test | treating a source-text search as the only security proof |
| `0006-standalone-mcp-app-host.patch` | port the same-host route, BFF endpoints, and shared frame composition | duplicated URI grammar, raw principal-subject display, and brittle source-layout assertions |

### Phase And Dependency Order

| Phase | Depends on | Exit condition |
|---|---|---|
| 0. Baseline and contract locks | none | accepted decisions, protocol versions, owners, and hard-cut surfaces are recorded |
| 1. Agent connection and correction | 0 | resource wakes survive refresh. Safe invalid input is corrected in one episode. The crash matrix passes |
| 2A. Development Secret closure | 0 | owner-supplied Secret references close before the existing disposable-profile mutation path, with zero writes on failure |
| 2B. Selected development scope | evidence gate in 2A | an approved local iteration design preserves unselected sources without becoming installation orchestration |
| 3. Map spatial correction | 0 | axis, typed distance, cursor invalidation, and restart evidence pass |
| 4. Standalone Apps | 1 for stable resource listeners | any authorized App opens through the shared host. Multi-tab use stays within explicit capacity |
| 5. Resource handoff and discovery | 1 | domain URIs remain canonical while handoff, lookup, pagination, and byte/read budgets pass in the selected domains |
| 6. Optional model reasoning | approved provider need | provider capability and exact request-rendering tests pass without fallback |
| 7. Evidence-gated GPU memory admission | measured same-device need and 2A | qualified co-located groups fit declared capacity with headroom on real hardware |
| 8. Frames and Time provenance | 0 | downstream admission accepts exact domain-owned source references without catalog search |
| 9. Optional private build inputs | approved repository-managed build need | clean-cache builds pass in every opted-in builder family and evidence contains no canary bytes |
| 10. Core closure and independent milestone closure | applicable completed phases | core gates close together. Every other track closes independently with its own evidence |

Phases 1, 2A, and 3 may proceed in parallel after phase 0 because their code ownership
is disjoint. Phases 4, 5, and 8 are independent ergonomic milestones after their stated
prerequisites. Phases 2B and 7 pause at their evidence gates. Phases 6 and 9 do not start
without an approved downstream need. The commit order within each active phase is fixed
below. A later concern must not be folded into an earlier contract commit merely to
reduce the number of schema revisions.

### Test-Driven Delivery Method

Every active implementation concern begins with an executable statement of the missing
behavior. A regression test targets the smallest owning layer that can prove the defect.
A contract addition begins with an acceptance test written against the proposed public
shape. Integration and hardware claims begin with a qualified scenario that cannot pass
through a mock or fallback.

Use this red-green-refactor sequence:

1. Write one focused test with the final behavior in its name and assertions.
2. Run it against the recorded baseline. Confirm that it fails because the behavior is
   missing, not because the fixture, dependency, credentials, or environment is broken.
3. Record the exact command, baseline revision, failing assertion, and bounded output in
   the change or review evidence. Do not commit a deliberately failing shared branch.
4. Implement the smallest change in the component that owns the contract. Do not make a
   neighboring layer compensate for the failure.
5. Run the focused test until it passes, then run the owning crate or application suite.
6. Refactor only while the focused and owning suites remain green.
7. Run the applicable integration, cluster, GPU, or headed-browser acceptance. A unit
   mock cannot close a claim about live transport, Kubernetes mutation, hardware, or
   browser rendering.
8. Commit the test and fix together as one coherent green concern. A test-only commit is
   acceptable only when it passes and captures existing correct behavior without hiding
   an expected failure.

If the first correctly scoped test passes on the baseline, investigate before changing
code. Existing behavior may already satisfy the client outcome. In that case retain the
implementation, strengthen only missing evidence, and record the request as verified
rather than forcing a redundant rewrite.

#### Red-green planning matrix

| Request | First red or characterization test | Green proof |
|---|---|---|
| agent continuation | crash after task settlement, wake claim, and input answer at each documented transaction boundary | restart produces one terminal wake, one consume transaction, and one receipt. If baseline passes, no persistence change lands |
| tool diagnostics | read an unknown governed URI and a protected failing URI through the current agent adapter | safe invalid input reaches the same episode with bounded guidance, while protected failures remain generic |
| selected development scope | no red test before the 2B evidence gate | after approval, two independent releases prove that the development driver issues no request for the unselected owner |
| Secret closure | use an audited fake and disposable cluster with a missing referenced Secret before the current development path starts | the same scenario fails before any mutating Kubernetes or Helm verb |
| standalone App | request a valid authorized `/apps/...` route and exercise bootstrap through the existing BFF | the shared frame opens with Console-equivalent auth, CSP, sandbox, bridge, and return behavior |
| stream fanout | open two tabs for one auth scope and URI, then close them independently | one upstream listener serves both and is cancelled after the final subscriber. If baseline passes, only bounds evidence changes |
| credential refresh | rotate a short-lived token while a declared resource subscription is active | replacement subscription acknowledgement precedes epoch publication and wakes continue without polling |
| resource handoff | run a terminal task and bounded domain listing that currently presents competing identifiers or an unbounded collection | one canonical domain result URI is presented, exact lookup remains possible, and pagination and read budgets close |
| reasoning controls | only after approval, render omitted, disabled, each named effort, and one unsupported provider/model pair | exact admitted wire fields render, unsupported configuration fails before connection, and no fallback occurs |
| GPU memory admission | only after the evidence gate, run qualified co-located workloads whose measured peak exceeds admitted headroom | profile validation rejects unsafe placement and qualified real hardware passes the locked capacity plan |
| Frames and Time provenance | compile output from a conversion that uses known revisions and one prefetched unused revision | only effective typed source references reach the downstream compiler without catalog search |
| spatial typing | run a representative longitude/latitude distance, effective-setting check, and old-cursor fixture | the typed spherical distance and axis are correct, the score is reused once, and the old cursor fails |
| private build inputs | only after approval, resolve an immutable private dependency from an empty cache with a canary credential | opted-in repository builders fetch through secret mounts, compile offline, and leak no canary bytes into evidence |

The red transcript is review evidence, not a permanent expected-failure test. The green
test remains in the repository as the regression or conformance gate.

### Phase 0: Baseline And Contract Locks

#### Delivered state to preserve

The agent runtime already persists wakes, episodes, task watches, input requests,
retention pins, and outbox events. Main also detaches accepted MCP Tasks from bounded
model episodes through `agents/kernel/src/background_tasks.rs`. Task settlement creates
the terminal wake transactionally, and episode completion consumes claimed wakes while
releasing task retention pins.

The Console BFF already owns an auth-scoped MCP client pool. Its App subscription table
maps browser subscription UUIDs to resource URIs, opens one upstream listener for the
first subscriber, and cancels that listener after the final subscriber leaves.

Deployment v6 owns exact source revisions and release lists, but its `ResourceSet` is
profile-global. The disposable development path validates the full profile and then
performs its existing repository-local setup. It checks some installation-owned Secret
requirements only after earlier mutations. This plan may close that validation gap but
does not broaden the path into an enterprise installer.

Map loads the trusted Spatial extension but leaves `geometry_always_xy` at the engine
default. Its distance queries use generic point construction, and source-feature
queries repeat their score expression in filtering, cursor comparison, projection, and
ordering. Cursor query digests do not name the distance algorithm revision.

#### Contract preparation

1. Update `mcp/contract/DESIGN.md` in the first protocol-changing commit. Record the
   safe diagnostic data subset, canonical URI handoff, wire caps, and final MCP error
   mapping. Domain-owned resource identities remain in their server designs.
2. Update `deploy/contract/DESIGN.md` with the pure rendered Secret-reference closure
   before changing the disposable development path. Do not allocate a new deployment
   schema version unless an approved typed field actually changes.
3. Update the owning server `DESIGN.md` beside Frames, Time, and Map when each output or
   algorithm changes. Each design keeps its `Standards And Protocols` section current.
4. Update `mcp/apps-extension/DESIGN.md` when the standalone host becomes an advertised
   browser surface. The reactive resource adapter remains a Veoveo extension rather
   than an ext-apps claim.
5. Record any necessary Rig API addition before changing the exact Git pin. Verify
   whether a stable upstream release contains the required listener and request hook.
   Keep the fork only when no stable release supplies the behavior.

#### Exit evidence

- The accepted and rejected patch behavior is represented in design text.
- Every new public or installation schema has one owner and one version transition.
- No implementation phase relies on an unstated legacy alias.
- The core, ergonomic, optional, and evidence-gated tracks have separate approval and
  completion boundaries.

### Phase 1: Agent Connection, Correction, And Continuation

#### 1.1 Restore declared resource listeners

Current `GatewayConnection::rotate` creates a replacement Rig guard, records the
manifest subscription count, publishes the new epoch, and closes the old guard. It does
not open a `subscriptions/listen` request or connect resource notifications to
`WakeBus`. The manifest promise is therefore unfulfilled on initial connection and on
refresh.

Implementation ownership:

| Concern | Owner |
|---|---|
| final MCP listener handle and accepted-filter result | Rig MCP adapter or a minimal typed Rig API addition |
| token mint, stale threshold, epoch publication, listener swap | `agents/kernel/src/connection.rs` |
| resource-update notification to durable wake | kernel connection listener and `agents/kernel/src/wake.rs` |
| declared filter validation | `agents/kernel/src/manifest.rs` |
| task watcher reconstruction on an epoch change | `agents/kernel/src/tasks.rs` |

The replacement connection becomes live only after all of these steps succeed:

1. Mint one fresh access token through the configured client-credentials flow.
2. Connect the replacement MCP client and verify Discover through the existing Rig
   lifecycle.
3. Build one deterministic `SubscriptionFilter` from the complete sorted manifest URI
   set. Duplicate URIs have already failed manifest validation.
4. Open `subscriptions/listen` and require the server's successful listener response
   before treating the filter as active.
5. Start the notification pump. Only `ResourceUpdatedNotification` values whose URI is
   in the accepted filter become durable resource wakes. List-change notifications
   invalidate discovery state but do not fabricate a domain resource wake.
6. Publish the resolver and listener as one `ConnectionEpoch`.
7. Cancel the previous listener with bounded acknowledgement, then cancel its guard.

The live state owns the guard, listener cancellation handle, listener task, token
timestamps, and epoch. A failed token mint, connect, listener acknowledgement, or pump
startup leaves the previous live state untouched. Cleanup failure after a successful
swap is observable but does not roll back the new epoch.

Credential freshness moves from episode-start-only behavior to the MCP request
boundary. The adapter checks the stale fraction before dispatch. Concurrent requests
share one serialized refresh. An authorization response permits at most one forced
refresh when the transport can prove that the request did not reach domain execution.
A mutating tool call is never replayed after an indeterminate dispatch.

#### 1.2 Add the current-profile resource access boundary

The kernel needs one governed resource adapter that can call `resources/read` through
the active MCP client without exposing an unrestricted protocol peer to model code.
Place connection-independent policy, read accounting, content validation, and model
error mapping in a focused kernel resource module. Keep token rotation and listener
lifecycle in `connection.rs`.

Introduce closed types with these responsibilities:

| Type | Responsibility |
|---|---|
| `ResourceReadLimits` | maximum contents, item bytes, response bytes, cumulative episode bytes, reads per episode, reads per family, wall time, and pagination depth |
| `ResourceReadLedger` | episode-local atomic accounting and the remaining safe budget reported after rejection |
| `SafeCorrectionDiagnostic` | bounded code, kind, requested URI, normalized guidance, and `automatic_retry = false` |
| `ResourceReadFailure` | controlled invalid resource, prohibited content, budget exhausted, unavailable connection, authorization, timeout, transport, or internal failure |

The adapter admits only absolute governed resource URIs. It rejects credential-bearing
URIs, fragments, browser-only `ui://` content, local file and network schemes, HTML,
event streams, and content that exceeds a configured bound. Binary resource bytes are
not inserted into the model context unless a domain contract explicitly admits their
bounded MIME type.

Unknown resource URIs remain JSON-RPC `Invalid Params` (`-32602`) on the MCP wire. The
kernel may project a safe, bounded invalid-resource message to the model when the
upstream error contains only admitted diagnostic data. It discards upstream error data
by default. Authorization, internal, timeout, and transport failures use fixed generic
messages.

Domain tool validation follows the existing final-profile rule. A schema-valid call
with bad domain input returns a completed `CallToolResult` with `isError: true` and
actionable structured content. A protocol failure remains a protocol failure. The same
classification must survive official Tasks.

#### 1.3 Keep correction inside the episode

Rig receives `SafeCorrectionDiagnostic` as a non-retryable tool input result. The model
may issue a corrected call in the same episode while its turn, tool, byte, and wall-time
budgets remain. The kernel does not launch an outer episode retry for a controlled
input error.

Lifecycle compare-and-set errors expose only the current allowed state, record
revision, accepted field names, and a stable error code. Storage text and query details
remain internal. The shared server handler path should own this conversion rather than
duplicating it across domain servers.

#### 1.4 Audit continuation lineage before extending the schema

The audit traces operator message, episode, detached Task, multi-round input request,
terminal task wake, consuming episode, retention release, and outbox receipt. Existing
records remain authoritative. Add a new operator-root or continuation receipt type only
when this trace cannot identify one root and one terminal consume transaction without
parsing free text.

If a gap exists, add strong identities to the agent runtime and platform-store
migration. The root is persisted before episode dispatch. Child tasks and input
requests carry it. Terminal consumption writes one durable receipt in the same
transaction that acknowledges the wake and releases retention. A unique database key
rejects duplicate receipt creation.

The audit does not replace canonical MCP Task IDs, wake IDs, episode IDs, or input
request IDs with a universal string. Those identities retain their current contracts
and reference the root through typed fields.

#### Agent acceptance matrix

| Test | Required proof |
|---|---|
| initial connection | the exact manifest filter is acknowledged and every selected resource update creates one durable wake |
| successful rotation | the replacement listener is live before epoch publication and the previous listener receives bounded cancellation |
| failed rotation | the prior epoch continues serving requests and resource wakes. No partial epoch is visible |
| token expires during an episode | the request boundary performs one serialized refresh and every declared resource remains subscribed |
| concurrent refresh | many callers cause one token mint and one listener replacement |
| invalid resource | the requested URI and normalized safe guidance reach the model within the byte cap |
| protected failure | authorization, transport, timeout, and internal strings do not reach model-visible output |
| same-episode correction | one intentionally wrong URI is followed by a successful corrected read without another episode |
| task completion crash | termination after task settlement but before wake consumption yields one terminal wake and one consumption receipt after restart |
| wake claim crash | an expired lease reclaims the wake without double consumption |
| input answer crash | answer and wake remain atomic across restart |
| budget exhaustion | the machine-readable result reports exhausted dimension and remaining permitted actions without performing a read |

Focused source gates:

```sh
cargo test -p veoveo-agent-kernel
cargo test -p veoveo-agent-runtime
cargo test -p veoveo-mcp-contract
cargo test -p veoveo-task-runtime
```

Integration evidence uses the Rust agent smoke harness and a real SurrealDB instance.
The token-expiry scenario needs a short-lived test issuer and the final Gateway MCP
path. It must not replace resource-notification evidence with polling.

### Phase 2: Development Profile Secret Closure And Conditional Selection

#### 2A.1 Preserve the installation boundary

Typed deployment profiles remain confined to disposable repository-development
environments. Production and enterprise installations continue to reconcile separate
platform and extension Helm applications from installation-owned desired state. This
phase does not create an enterprise deployment API, install a reconciliation
controller, or grant the smoke harness installation authority.

The installation owner supplies every Secret through its secret-management and
Kubernetes reconciliation path. Veoveo repository tooling does not create, patch,
replace, copy, or transfer ownership of a Secret. The required P0 change is a pure
closure check before the existing disposable-development mutation path performs its
first write.

#### 2A.2 Build the rendered Secret-reference closure

Add focused internal types to `deploy/contract` without changing the public deployment
schema unless a new serialized field is required:

| Type | Contract |
|---|---|
| `KubernetesObjectKey` | group, version, kind, namespace or cluster scope, and name |
| `SecretReferenceRequirement` | referring object, exact field path, Secret name, optional key, optionality, and reference kind |
| `SecretClosure` | profile and lock digests, complete sorted requirements, presence results, and terminal validation status without Secret values |

Resolve the complete v6 profile and exact lock through the existing pure contract path.
Render every admitted chart and raw object that the disposable profile already intends
to use. Parsing walks these non-optional references:

- container and init-container `env[].valueFrom.secretKeyRef`.
- container and init-container `envFrom[].secretRef`.
- Secret volumes and projected Secret sources.
- pod `imagePullSecrets`.
- Ingress and Gateway TLS certificate references.
- chart-owned custom-resource fields explicitly registered by the deployment contract.

An admitted custom resource with an unregistered Secret-bearing shape makes the closure
unverifiable and fails before mutation. The closure cannot silently claim completeness
for an unknown shape.

The trusted disposable-development driver may read an existing Secret only when it
already has that authority and key-presence validation requires it. It discards values
immediately after the check. The closure, diagnostics, logs, errors, and evidence retain
only names, required keys, presence flags, and bounded failure codes. Authoritative
NotFound means a missing dependency. Forbidden, timeout, malformed, and transport
failures remain distinct and fail closed.

#### 2A.3 Insert one zero-write gate

The existing disposable-development path must compute and validate the complete closure
before Namespace creation, allocator changes, raw manifest application, ConfigMap
creation, gateway activation, Helm invocation, or hooks. A failed closure returns
without issuing a mutating Kubernetes or Helm request.

This phase adds no selected executor and no new lifecycle command. Rust smoke code owns
the assertions and API-audit evidence. Helm and the installation reconciliation
controller retain installation orchestration.

#### 2B.1 Require evidence before selected-release design

`DEPLOY-SCOPE-003` pauses until a downstream development workflow records all of the
following:

- measured full-profile iteration time and the target bound.
- the exact independently owned release that needs isolated iteration.
- proof that ordinary Helm or GitOps selection cannot meet the disposable-development
  need without unsafe ownership overlap.
- a named owner for the development driver outside smoke verification.
- an architecture review confirming that the command cannot be mistaken for an
  enterprise installation API.

No command name or deployment schema revision is reserved before this gate passes. If
approved, the design may introduce typed release ownership in the next available
profile version. It must read the complete lock, reject selected/unselected ownership
collisions, and preserve all unselected owned fields. The smoke harness verifies those
claims but does not perform the installation.

Read-only managed-field inspection may then explain a selected/unselected collision.
It does not invoke server-side apply, transfer ownership, replay stored Helm manifests,
or claim that repository tooling owns controller-managed objects.

#### Phase 2 acceptance matrix

| Track | Test | Required proof |
|---|---|---|
| required 2A | missing Secret | complete rendering fails before the API audit log records any mutating verb |
| required 2A | missing Secret key | key absence is reported without retaining or printing another key or value |
| required 2A | forbidden Secret read | failure is not classified as NotFound and no write occurs |
| required 2A | unknown custom-resource shape | closure is reported as unverifiable before mutation |
| required 2A | successful closure | the existing development path receives one immutable closure tied to exact profile and lock digests |
| conditional 2B | evidence gate | measured need, target bound, driver owner, and architecture approval are recorded |
| conditional 2B | unselected source | no installation request targets an unselected ownership key and its owned fields remain unchanged |
| conditional 2B | managed-field observation | selected ownership conflicts are diagnostic only and no ownership-changing request occurs |
| conditional 2B | enterprise boundary | enterprise examples and reconciliation contracts acquire no development command or profile dependency |

Focused required gates:

```sh
cargo test -p veoveo-deploy-contract
cargo test -p veoveo-deployment-smoke
```

Required cluster acceptance uses a disposable Kubernetes/K3s installation. The
missing-Secret scenario captures the API audit log before validation and proves that no
mutating verb was issued. Conditional 2B tests do not enter the required gate until its
evidence and architecture review are accepted.

### Phase 3: Map Spatial Axis And Distance Hard Cut

#### 3.1 Type the engine axis policy

Replace a raw boolean proposal with a closed DuckDB runtime setting such as
`SpatialAxisPolicy::{Native, GeoJsonLongitudeLatitude}`. The default remains `Native`.
The non-native variant requires the trusted Spatial extension and configures
`geometry_always_xy` after loading that extension but before external access and
configuration are locked.

Map selects `GeoJsonLongitudeLatitude`. The general DuckDB MCP server explicitly
selects `Native`, because arbitrary SQL owns its own declared geometry interpretation.
The runtime readiness query reads `current_setting('geometry_always_xy')` and fails when
the effective setting does not match the selected enum.

#### 3.2 Use one typed score

Map distance calls use `ST_Point2D` for longitude/latitude inputs. A geometry centroid
is explicitly cast to `POINT_2D` before `ST_Distance_Sphere`. Each distance query creates
one `WITH ... AS MATERIALIZED` score relation and reuses `distance_m` for maximum
distance, cursor comparison, projection, and ordering.

Cursor validation requires a distance for distance-ordered results and rejects one for
feature-ordered results. Non-finite and negative distance values fail before query
execution.

#### 3.3 Invalidate pre-fix cursors

Domain-separate the request digest with a new constant such as
`veoveo.io/map/source-feature-query/v2\0`. The cursor decoder accepts only the new
domain. No v1 cursor reader or fallback is retained. Update the Map design and tool
description to state the axis and cursor hard cut.

#### Spatial acceptance matrix

| Test | Required proof |
|---|---|
| runtime validation | non-native axis policy without the Spatial extension fails |
| effective setting | Map reads `geometry_always_xy = true`. DuckDB MCP retains its native setting |
| typed overload | one longitudinal degree at a representative latitude returns the admitted meter range |
| materialization | query-plan or instrumentation evidence shows one distance score per candidate row |
| cursor shape | distance and non-distance cursor fields are mutually consistent |
| hard cut | a cursor generated with the previous digest domain is rejected |
| restart | the same representative query and ordering survive database close and reopen |

Focused gates:

```sh
cargo test -p veoveo-duckdb-runtime
cargo test -p veoveo-map-mcp
```

The extension-backed tests require the repository-selected DuckDB Spatial extension.
A test skipped because the extension is absent is not acceptance evidence for this
phase.

### Phase 4: Standalone MCP App Host

#### 4.1 Define one route authority

The BFF owns a strong `StandaloneAppRoute` parsed from `/apps/{server}/{page...}`. The
server segment uses `ServerSlug`. The page path is a bounded sequence of decoded path
segments ending in the exact application document name. Empty segments, dot segments,
encoded separators, backslashes, credentials, queries used as identity, control
characters, invalid UTF-8, and overlong paths fail before catalog access.

The BFF maps the validated route to one `ui://{server}/{page...}` resource and checks it
against the caller's authorized App catalog. Browser TypeScript never reconstructs this
URI. It receives the authorized `AppDescriptor` from a same-origin BFF bootstrap call.

#### 4.2 Reuse the Console boundary

The standalone page imports the delivered `AppFrame`, bridge, theme projection, frame
endpoint, resource-read adapter, resource event stream, internal-link rules, and CSRF
client. Extract one exported sandbox constant and keep `allow-scripts` without
`allow-same-origin`. Console and standalone hosts pass that same constant to the same
component.

The entry document may remain public and `no-store`. Its first authenticated bootstrap
request uses the existing OAuth transition. Rename `ConsoleReturnPath` to the accurate
hard-cut name `BrowserReturnPath` and admit exactly `/console/` and `/apps/`. The parser
retains same-origin enforcement and the existing length bound. No second OAuth flow,
session cookie, bearer bridge, or callback endpoint is created.

The minimal host header displays the App's authorized title and a return link. It does
not display the raw principal subject. Any later display identity uses the bounded
principal display metadata already separated from authorization identity.

#### 4.3 Keep one upstream stream per URI

The existing auth-scoped listener pool remains canonical. Add an explicit maximum for
active upstream App resource listeners and active downstream browser subscriptions per
auth scope. Capacity exhaustion returns a bounded machine-readable response. It never
silently drops a listener or opens a second gateway connection.

EventSource reconnect with the same UUID remains idempotent. A refreshed Console token
creates a new auth-scoped MCP client. The browser reconnect re-establishes the selected
resource against that client, and the old listener receives bounded cancellation.
Closing the final tab releases the upstream listener deterministically.

#### App acceptance matrix

| Test | Required proof |
|---|---|
| route grammar | valid nested page paths map exactly. Traversal, encoded separator, authority, and length attacks fail |
| catalog authority | a syntactically valid but unauthorized App route returns no App metadata or frame bytes |
| shared frame | both hosts render the same component, sandbox constant, referrer policy, and bridge |
| CSP parity | the frame response for one App has byte-identical CSP under Console and standalone navigation |
| cold login | authentication returns to the complete original `/apps/...` path |
| bridge parity | tools, resources, links, theme, sizing, and lifecycle notifications match Console behavior |
| multi-tab fanout | two tabs for one resource create one upstream listener and independent downstream deliveries |
| capacity | exceeding the configured bound returns an explicit error and leaves existing streams healthy |
| cleanup | final tab close and token replacement both cancel obsolete listeners within the bound |

Focused gates:

```sh
cargo test -p veoveo-console-bff
cd apps/console/web && npm test
cd apps/console/web && npm run lint
cd apps/console/web && npm run build
```

Browser acceptance belongs in the Rust browser-smoke harness. It launches a headed
browser only after proving a hardware-backed WebGPU adapter or WebGL context. Loss of
the last hardware graphics API stops the run. API-only checks and screenshots cannot
substitute for the visual workflow.

### Phase 5: Canonical Resource Handoff And Bounded Discovery

#### 5.1 Preserve domain-owned identity

The canonical public identity remains the resource URI owned by each domain and the MCP
resource link that carries it across servers. Artifact occurrences retain fresh opaque
UUIDv7 identities. Content digests remain integrity and provenance fields rather than
public addresses.

Do not add a universal `ResourceToken`, a hash-derived parallel identifier, or a shared
index item that erases domain vocabulary. Do not migrate a working domain URI merely to
make identifiers look uniform. `mcp/contract` may own generic bounds and handoff rules,
but every server continues to own its resource grammar, search filters, result
discriminators, and authorization semantics.

Before changing a domain, add characterization tests for its terminal results, exact
lookup, growing collections, pagination, and serialized sizes. A domain that already
presents one canonical URI and bounded discovery receives evidence only. It does not
receive a new schema for consistency with another server.

#### 5.2 Canonical task handoff

When a successful task produces an addressable domain product, its structured terminal
result presents exactly one follow-on `result_uri` using that domain's canonical URI.
Adjacent text contains a short status without another opaque product identifier. Full
provenance, artifacts, child records, and internal routing remain in typed structured
content or the canonical resource document.

The platform Task status URI remains the lifecycle identity. It answers how work
progressed. The domain `result_uri` identifies the completed product. A result names
both roles explicitly when both are present. A task that produces no addressable domain
product is not forced to invent a resource.

#### 5.3 Keep discovery domain-specific and bounded

Every growing collection exposes stable pagination through its owning domain types.
Older known items remain reachable by canonical URI or exact domain identity without
loading the current full collection. Index responses carry only the domain-owned
discriminator fields needed to select an item. Filters, limits, sort order, and opaque
cursors remain typed in the server that understands them.

The agent kernel owns cumulative episode resource-read budgets. The shared MCP layer
owns serialized response-size enforcement and conformance checks. Apply the byte cap
after final MCP serialization rather than estimating Rust heap size. An oversized result
fails before transport with a machine-readable budget diagnostic. Structured content
appears once. Text carries only a short identity-free status unless the protocol
requires a human-readable explanation.

Begin with characterization tests for Optimization and one other task-heavy server.
Continue only into domains whose tests expose an unbounded collection, competing result
identity, duplicate presentation, or missing exact lookup. Static well-known documents
and already-canonical domain resources remain unchanged.

#### Resource acceptance matrix

| Test | Required proof |
|---|---|
| identity audit | each product has one domain-owned canonical URI and no hash-derived parallel public identity |
| canonical handoff | an addressable terminal product contains one domain result URI and no competing product identifier in text |
| no-product task | a terminal task without an addressable product does not fabricate a resource URI |
| exact lookup | an older known canonical URI or domain ID resolves without listing the full collection |
| pagination | each changed growing index returns stable order and an opaque domain-owned cursor |
| duplicate presentation | the model sees structured payload once and a short status beside it |
| response cap | serialization beyond the limit produces no partial response |
| episode budget | read count, family count, bytes, time, and page depth fail independently |
| correction | an unknown canonical URI reports only the requested URI and safe copy-and-retry guidance |

### Phase 6: Provider-Neutral Reasoning Controls

This optional track starts only when an agent-manifest owner needs explicit reasoning
control for an admitted provider and model. The approval records the endpoint class,
models, required modes, and operational reason. Veoveo does not add configuration merely
because a provider exposes a wire field.

#### 6.1 Model the three states

Represent omitted reasoning as `None` at the manifest field. A present value is a closed
`ReasoningMode`:

- `Disabled`, rendered only when the provider profile declares an exact supported wire
  value.
- `Effort(ReasoningEffort)`, where the closed repository enum is `Minimal`, `Low`,
  `Medium`, `High`, `XHigh`, or `Max`. Each provider capability profile admits an exact
  subset.

Omission preserves the provider default. Disabled does not mean low effort. No mode
injects a preamble, changes sampling, selects a different model, or removes tools.

#### 6.2 Validate the provider profile

Add a closed endpoint class and capability profile beside model configuration. The
profile records whether reasoning is unsupported, disable-capable, or effort-capable,
and names its admitted levels. Provider-specific wire fields remain in the adapter.
Manifest loading rejects a mode that the selected endpoint class or model does not
support.

The rendered diagnostic surface reports endpoint class, effective model ID, reasoning
state, tool availability, and non-secret sampling controls. It excludes the API key,
authorization headers, provider response bodies, and base URLs containing credentials.

#### Reasoning acceptance matrix

| Test | Required proof |
|---|---|
| omitted | no reasoning field is emitted and provider defaults remain untouched |
| disabled | the exact provider-supported disable value reaches the request with tools enabled |
| named effort | every admitted level renders exactly and retains tool schemas |
| unsupported | manifest validation fails before the agent connects |
| restart | the same manifest produces the same effective reasoning diagnostic |
| no fallback | an unsupported response does not retry with another mode, model, endpoint, or sampling configuration |

The reference patch's `none`-only enum is not an intermediate state. The implementation
lands the full closed model and capability validation in one hard cut.

### Phase 7: Evidence-Gated Accelerator Memory Admission

#### 7.1 Qualify the need before changing the schema

This track does not begin from a proposed schema. First capture the exact same-device
group, physical device or MIG profile, workload replicas, model and runtime revisions,
steady reservations, transient operations, and observed NVML peaks. The evidence must
show that device identity and replica-count admission alone cannot prevent an unsafe
co-location.

The gate requires a reproducible hardware scenario that fails the proposed headroom
rule on the current profile, a named owner for every reservation, and an architecture
review of the capacity model. Mocked CUDA and estimated vendor marketing numbers cannot
satisfy it. If the current placement is already exclusive or comfortably bounded, this
track remains closed.

Only an accepted gate may add memory fields to `GpuWorkloadPlacement`. That change uses
the next available deployment profile and lock version at implementation time. The plan
does not reserve v7, v8, or a dependency on selected-release development scope.

#### 7.2 Add conservative typed reservations

The accepted profile change extends each affected `GpuWorkloadPlacement` with typed
positive MiB reservations:

- persistent memory held while the workload is ready.
- peak memory required during its admitted maximum operation.
- an exact workload/model/runtime revision that qualifies those values.

Persistent memory cannot exceed peak memory. A group declares its minimum unallocated
headroom. Full-device capacity comes from qualified DRA device inventory and a matching
NVML hardware probe. MIG capacity comes from the admitted MIG profile and the allocated
partition. Missing or inconsistent capacity fails closed.

#### 7.3 Use conservative peak admission

The first implementation admits a same-device group only when the sum of every replica's
declared peak plus group headroom fits the selected physical device or MIG partition.
This conservative rule requires no cross-service runtime scheduler and prevents an
unsafe overlap before startup.

Do not claim serialized transient admission until a durable shared admission service
owns leases across Reason, View, Stream, simulation, and Optimization. Such a service is
a separate design and is not required for the first memory-safe profile.

The Reason vLLM `gpuMemoryUtilization` remains explicit and schema-validated. Its
qualified memory reservation must agree with the selected model and device capacity.
The cuOpt RMM pool, renderer allocations, simulation baseline, inference engines, and
encode/decode reservations receive their own workload entries rather than inheriting a
generic GPU number.

#### 7.4 Report reservations and observations

Deployment diagnostics show physical UUID, partition identity, total memory, declared
persistent and peak reservations, headroom, and current observed use. Observed use is
evidence and drift detection. It never grants admission beyond the declaration.

Pod replacement and release upgrade re-evaluate the same locked memory plan before the
new process starts. A changed model digest or runtime revision invalidates its previous
qualification.

#### GPU acceptance matrix

| Test | Required proof |
|---|---|
| schema | zero, persistent-above-peak, unknown revision, and missing headroom fail validation |
| unsafe group | summed peak beyond qualified capacity is rejected before workload startup |
| device proof | DRA allocation, in-container UUID, NVML capacity, and profile capacity agree |
| MIG proof | partition profile and allocatable bytes agree with the locked reservation |
| pod replacement | the replacement receives the same physical identity and passes admission again |
| release upgrade | changed workload revision requires matching new memory evidence |
| real concurrency | resident and transient workloads reach peak on hardware without allocation failure |
| fallback audit | no CPU, alternate model, alternate runtime, or unselected GPU path appears |

Acceptance requires an accessible NVIDIA GPU. Mocked CUDA, software rendering, and a
container that merely exposes an `nvidia.com/gpu` resource cannot close this phase.

### Phase 8: Compiler-Ready Frames And Time Provenance

#### 8.1 Add a shared digest type

Introduce one general `Sha256Digest` in the shared contract rather than reusing the
gateway-specific `CompositionDigest` or adding another unconstrained string. It accepts
only `sha256:` followed by 64 lowercase hexadecimal digits. Migrate touched provenance
fields to this type in the same hard cut. Unrelated legacy strings can move when their
own contracts change.

#### 8.2 Report only Frames revisions actually used

Add `FrameSourceReference` with revision URI, revision ID, and `Sha256Digest` to the
Frames output. Instrument source and target resolution to collect a set when a world
revision participates in a point conversion. Sort and deduplicate that used set before
returning it.

Do not return every revision loaded into `ResolvedWorlds`. A resolver may prefetch more
than the operation consumes. WGS84-to-ECEF conversion without a world revision returns
an empty source list and retains the coordinate-operation provenance.

Direct and Task-based conversion return the same reference shape. Artifact metadata and
usage records include the canonical source references without a second descriptive
identity.

#### 8.3 Return effective Time authority

Add typed references for the effective TZDB and leap-second releases. Each reference
contains its canonical `time://` release URI, release ID, source ID, immutable source
digest, version label, and acquisition identity when the release came through the
acquisition workflow. Bootstrap data uses an explicit bootstrap source kind rather than
pretending that an acquisition occurred.

Every deterministic conversion returns the authority pair that actually governed it.
Clock-derived operations such as current time additionally return the effective clock
policy, observed quality, and holdover state. Deterministic conversion of a supplied
instant does not attach current node clock quality, because that observation did not
govern the calculation.

Approximation remains explicit in Frames. Time ambiguity and uncertainty remain typed
in Time. A downstream compiler copies the supplied references and verifies them against
its admitted revision set. It does not search unrelated catalogs by display name.

#### Provenance acceptance matrix

| Test | Required proof |
|---|---|
| used Frames sources | source and target world revisions appear once. Prefetched unused revisions do not appear |
| no-world conversion | the source list is empty and operation provenance remains complete |
| digest type | malformed prefixes, uppercase hex, short values, and long values fail schema and runtime validation |
| Time data authority | conversion returns the exact active TZDB and leap release references and digests |
| clock authority | current-time output reports effective policy, quality, and holdover without changing deterministic conversion output |
| restart | persisted revisions produce byte-identical source references after restart |
| downstream admission | a compiler fixture accepts returned references without a catalog search and rejects an unadmitted digest |

Focused gates:

```sh
cargo test -p veoveo-mcp-contract
cargo test -p veoveo-frames-mcp
cargo test -p veoveo-time-mcp
```

### Phase 9: Private Git And Trust Inputs For Image Builds

This optional track starts only when a repository-managed builder must resolve an exact
private Git dependency. It does not make Veoveo's BuildKit workflow mandatory for an
external extension. A downstream publisher may continue to use its own build system and
join Veoveo through the published image, chart, fragment, binding, and conformance
contracts.

#### 9.1 Define one operator interface

Use repository-owned names:

```text
VEOVEO_GIT_CREDENTIALS_FILE=/absolute/owner-only/path
VEOVEO_GIT_CA_BUNDLE_FILE=/absolute/owner-only/path
VEOVEO_GIT_HOST_ADDRESS=git.example.internal=192.0.2.10
```

The first two values are optional secret source files. The host mapping is optional
validated configuration, not a secret. It accepts exactly one DNS host and IP pair and
must match the host of a locked Git dependency. It cannot override a registry, model,
provider, or arbitrary build destination.

`xtask` requires absolute regular files, rejects symlinks, and checks owner-only mode for
the credential store. It parses the CA bundle before invoking Buildx. The execution plan
records only whether each optional input is present. Paths, file metadata beyond the
admitted mode, and bytes are excluded.

#### 9.2 Separate fetch from compile

Every opted-in repository-managed Rust builder family performs a dependency-fetch step
with these optional mounts:

- `veoveo-git-credentials` at a fixed `/run/secrets` path.
- `veoveo-git-ca-bundle` at a fixed `/run/secrets` path.

The process configures Git's credential helper and CA path for that command only. TLS
verification remains enabled. Cargo uses the Git CLI for credential-helper support.
After fetch completes, the compile step runs offline without either secret mount.

Apply the same contract to each repository-managed family selected by the approved use
case. Prefer moving an opted-in standalone image onto a shared artifact family when that
removes duplicate build logic without changing its runtime base. Otherwise reuse the
exact secret IDs, process configuration, and tests. Do not modify an unrelated builder
merely to make every Dockerfile expose the option.

Build arguments cannot carry credentials or CA bytes. A missing credential causes the
immutable private revision fetch to fail. It never substitutes a public dependency,
mutable branch, or alternate registry source.

#### 9.3 Prove evidence isolation

Acceptance uses a unique non-production canary credential and an empty dependency
cache. Scan image history, exported OCI filesystem, local and registry cache exports,
SBOM, maximum-mode provenance, image build plan, run evidence, BuildKit trace, stdout,
and stderr for the complete canary and its encoded forms.

The source URL remains credential-free in Cargo metadata and the lock. A cache may
contain fetched immutable source and a credential-free remote URL. It cannot contain a
credential helper file or authenticated URL.

#### Build acceptance matrix

| Test | Required proof |
|---|---|
| clean cache | the locked private revision resolves with the secret in each opted-in repository builder family |
| missing credential | fetch fails closed without a fallback and without printing the credential-free URL as an authenticated URL |
| CA | the admitted custom root succeeds. Malformed or absent required trust fails with verification still enabled |
| host mapping | only the locked Git host can receive the single validated override |
| compile boundary | compile runs offline and has no secret mount |
| layer and cache scan | no canary bytes or helper file occur in image layers or exported cache |
| evidence scan | plan, run, trace, SBOM, provenance, logs, and metadata contain no canary bytes or source path |

Focused gates include `veoveo-image-build-control` tests and clean-cache qualified builds
for every opted-in repository-managed family. External build systems are outside this
gate. BuildKit versions remain exact.

### Phase 10: Core Closure And Independent Milestone Closure

#### Source gates

Run focused gates with each commit. Before closing the core release or any independent
milestone that changes shared contracts, run the repository-wide non-visual source gate:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins
```

Run Console tests, lint, and production build. Regenerate and validate every changed
JSON Schema, deployment example, compatibility manifest, and lock. Use
`git diff --check` and hard-cut searches for removed schema versions, old cursor
domains, old return-path type names, obsolete resource error constants, and abandoned
configuration fields.

#### Integration gates

Run the core gates together. Run another row only when its independent milestone or
approved conditional track is closing.

| Track | Gate | Environment | Required result |
|---|---|---|---|
| core | agent recovery | Gateway, SurrealDB, short-lived OAuth issuer, and controlled crash points | one continuation and terminal receipt, same-episode correction, and preserved resource wakes |
| core | Secret closure | disposable Kubernetes/K3s cluster with API audit evidence | zero writes when an owner-supplied Secret or key is missing |
| core | spatial restart | pinned DuckDB Spatial extension and persistent test database | stable distance, order, and new cursor behavior after reopen |
| ergonomic | standalone App | headed hardware-backed browser and authenticated Gateway | cold return, shared CSP/sandbox/bridge, and one upstream listener across tabs |
| ergonomic | resource handoff | selected task-heavy servers and agent read ledger | one canonical domain result URI, exact lookup, bounded pagination, and independent budget failures |
| ergonomic | provenance | Frames and Time servers plus downstream compiler fixture | exact references admit directly without catalog search |
| optional | reasoning | approved provider/model matrix without production credentials | omitted and exact admitted modes render while unsupported configuration fails before connection |
| optional | private builds | managed BuildKit with empty local and registry caches | immutable fetch succeeds and canary scans remain empty in opted-in builders |
| evidence-gated | selected development scope | disposable cluster and independently owned release fixtures | the approved development driver issues no request for the unselected owner |
| evidence-gated | GPU admission | qualified NVIDIA DRA cluster and concurrent real workloads | declared peak fits hardware, survives replacement, and uses no fallback |

No integration gate may replace a missing event-driven path with provider polling,
resource polling, task polling beyond the official Tasks correctness contract, API-only
visual checks, or CPU rendering.

### Commit Plan

Each row is a reviewable green checkpoint after its focused red test has been observed.
A commit may split further when one concern grows, but adjacent rows must not collapse
ownership boundaries. Conditional rows do not enter the sequence before approval.

| Sequence | Track | Commit concern | Minimum coherent green result |
|---|---|---|---|
| 1 | core | Rig listener/request surface | typed listener readiness and request freshness with its focused adapter test |
| 2 | core | agent listener rotation | acknowledged make-before-break listener, durable resource wakes, and token-rotation regression test |
| 3 | core | resource reads and safe diagnostics | bounded read adapter, final MCP error classification, redaction, and same-episode correction tests |
| 4 | core | continuation crash evidence | characterization matrix and a migration only if a baseline test proves a lineage gap |
| 5 | core | rendered Secret-reference closure | pure closure, distinct read failures, and API-audited zero-write tests without Secret creation |
| 6 | core | DuckDB spatial axis type | runtime setting, Map/DuckDB selections, and extension-backed failing-then-green test |
| 7 | core | Map distance query hard cut | typed score, materialized CTE, cursor hard cut, and restart regression test |
| 8 | ergonomic | standalone App route and auth return | typed route, authorized bootstrap, shared host, and route/auth tests |
| 9 | ergonomic | App stream bounds | characterization of existing fanout plus only the missing capacity, cleanup, and browser evidence |
| 10 | ergonomic | task result handoff and resource budgets | canonical domain URI rule, serialized cap, agent ledger, and conformance tests |
| 11 | ergonomic | bounded domain discovery | one green commit per domain whose characterization test exposed a gap |
| 12 | ergonomic | shared SHA-256 provenance type | one constrained digest type and schema/runtime parity tests |
| 13 | ergonomic | Frames provenance | used-source references, artifact projection, and compiler fixture |
| 14 | ergonomic | Time provenance | effective data and clock authority with compiler fixture |
| 15 | optional | reasoning capability contract | approved provider matrix, typed model, exact requests, and diagnostics |
| 16 | optional | BuildKit private inputs | opted-in builder interface, clean-cache test, evidence redaction, and docs |
| 17 | evidence-gated | selected development ownership | next-version contract and preservation tests only after the 2B gate |
| 18 | evidence-gated | GPU memory contract | next-version schema and admission tests only after hardware evidence is accepted |
| 19 | evidence-gated | GPU runtime evidence | DRA/NVML diagnostics, replacement proof, and hardware acceptance |
| 20 | applicable track | closure | repository-wide gates, required environment evidence, hard-cut audit, and ledger update for the track being closed |

### Documentation And Generated Artifact Closure

Each implementation commit updates its owning documentation. A phase is incomplete
when code and tests pass but its public or installation contract still describes the
old behavior.

Required documentation updates include:

- `mcp/contract/DESIGN.md` for resource errors, canonical URI handoff, shared bounds, and
  conformance without a universal resource identity.
- `docs/RMCP_3_MIGRATION.md` if the post-migration listener repair changes its
  implementation report or remaining rollout evidence.
- `docs/AUTONOMY_HARNESS.md` and `docs/TECH_DESIGN.md` for delivered continuation and
  model behavior.
- `deploy/contract/DESIGN.md` and `docs/LOCAL_DEPLOYMENT_PROFILES.md` for the required
  Secret-reference closure and any later approved development-profile revision.
- `docs/ARCHITECTURE_DECISIONS.md` only through an explicit replacement decision if a
  future proposal changes installation ownership. This plan does not make that change.
- `mcp/apps-extension/DESIGN.md` and Console documentation for the standalone host.
- Map, Frames, Time, Reason, and Optimization `DESIGN.md` files for their owned
  contract changes.
- `docs/IMAGE_BUILDS.md` and `docs/EXTERNAL_REPOSITORY_INTEGRATION.md` for private
  dependency inputs.
- `docs/CODEMAP.md` whenever a module, document, component, or ownership boundary is
  added or moved.

Generated schemas, compatibility manifests, example profiles, deployment locks, Helm
values schemas, and conformance fixtures change only when their source types change.
Generated outputs are never edited independently of their typed owner. An internal
validation addition does not force a public schema revision.

### Definition Of Done

#### Core correctness release

The required program is complete when its red evidence has been observed, its permanent
tests pass, and these claims are simultaneously true:

- resource notifications remain active across initial connection, token rotation,
  process restart, and lease handoff.
- correctable resource and domain input errors can be repaired within one bounded
  episode without leaking protected diagnostics.
- continuation crash points produce one terminal wake, one consume transaction, and one
  durable receipt, or baseline characterization proves the delivered transaction already
  supplies that result.
- a failed rendered Secret-reference closure causes no Kubernetes or Helm mutation and
  repository tooling creates or rewrites no Secret.
- Map uses the explicit longitude/latitude axis and new cursor domain on every open.
- hard-cut searches find no old cursor reader, obsolete protocol constant, CPU fallback,
  universal resource token, Secret-management path, or provider status polling.

#### Independent ergonomic milestones

Each milestone closes without waiting for the others:

- standalone Apps share the Console security, identity, sandbox, bridge, and stream
  boundaries.
- addressable task products expose one canonical domain result URI with bounded exact
  lookup, discovery, serialization, and agent reads.
- Frames and Time return exact typed source references consumed directly by a downstream
  compiler.

#### Optional and evidence-gated tracks

An optional track is done only after its recorded approval and tests pass:

- reasoning modes are explicit, provider-admitted, restart-stable, and never silently
  substituted.
- private Git credentials and trust inputs never escape the opted-in dependency-fetch
  secret mounts, while external builders remain independent.

An evidence-gated track is done only after its gate and hardware or cluster acceptance:

- selected development iteration, if approved, cannot issue an installation request for
  an unselected ownership key and does not enter enterprise contracts.
- GPU memory admission, if approved, uses qualified revision-specific reservations and
  real NVIDIA capacity without a CPU, alternate-model, or software fallback.

No optional or evidence-gated row blocks the core correctness release or an unrelated
ergonomic milestone.

## Current Cycle: Requests 014–023

Status: approved architecture and implementation plan. Implementation has not started.
Existing component designs remain normative until each phase lands and deletes the surface it
replaces.

Baseline: Veoveo main `6edf9d6b08886a3503067daa7803098d5ea7bc12` on 2026-08-26. The
input is the downstream review package `veoveo-platform-improvements-2026-08-26`, which
reviewed `2e77072ac7c0b88b5eba6336381f621a87171974`. Its ten requests are numbered
`014` through `023`. Its four patches demonstrate behavior against an older integration
snapshot and are not a merge series.

This plan accepts the downstream outcomes where they strengthen exact App resolution,
recording use, artifact admission, release visibility, diagnostics, external simulation
adoption, accelerator admission, provider configuration, and deployment ownership. It replaces
suggested mechanics that would create a second authority, a second recording truth, buffered
large-file paths, or a risky rewrite of an already qualified live-view runtime.

### Standards And Protocols

| Standard or profile | Plan boundary |
|---|---|
| Model Context Protocol `2026-07-28` | sole MCP profile for Gateway aggregation, exact App reads, App-scoped resource listing, tools, resources, Tasks, subscriptions, notifications, and server discovery |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | sandboxed `ui://` applications, `text/html;profile=mcp-app`, host context, standard bridge methods, and repository-owned host extensions declared by the App resource |
| JSON-RPC 2.0 | frame bridge request and error envelope. App-scoped methods use typed invalid-parameter, forbidden, unavailable, and internal outcomes without leaking policy or transport internals |
| JSON Schema Draft 2020-12 | closed App resolution, upload, release, recording, live-view, GPU, reasoning, and deployment contracts |
| W3C Trace Context and Baggage | bounded `traceparent`, `tracestate`, and `baggage` propagation. The authenticated HTTP boundary establishes trust and untrusted App frames never choose trace authority |
| OpenTelemetry Protocol | export of connected Console, Gateway, hosted-server, and bounded data-plane spans. Operator correlation exposes only a shortened trace identity and safe result category |
| Rerun `0.36.3` RRD | canonical immutable multimodal recording bytes at this baseline. Implementation rechecks the latest stable Rerun release before touching the pin and migrates the complete stack together if a newer stable release exists |
| Rerun Data Protocol | official `rerun.cloud.v1alpha1.RerunCloudService` read profile for governed datasets, segments, layers, schema, manifests, chunk queries, and chunk fetches over HTTP/2 or gRPC-Web |
| Rerun Catalog SDK and DataFusion | native Python and Rust dataset reads, segment filtering, content filtering, typed dataframe operations, and Arrow conversion. Veoveo does not claim unsupported Catalog mutation or maintenance profiles |
| Apache Arrow IPC streaming format | sole browser-native recording projection payload. It is a query result over RRD, not another recording format or durable copy |
| Rerun WebViewer `0.36.3` | direct Redap archive playback, producer and dataset Blueprints, and the existing incremental `LogChannel` live path |
| H.264 Annex B, WebCodecs, Media Capabilities, CUDA, RTX, and NVENC | existing tiled live-view v4 media path and its qualified hardware boundary. Browser software H.264 decode remains the one documented exception when the exact configuration is supported and smooth |
| `veoveo.io/live-view/v4` | provider-neutral camera, shared stream-product, authorization, renewal, closure, and health contract. This plan publishes adoption artifacts without changing v4 runtime semantics |
| `veoveo.io/extension-release/v1` | existing immutable external-extension release inventory. Console projects the validated contract rather than introducing an application-specific release model |
| Kubernetes/K3s, Helm, and server-side apply managed fields | deployment compilation and mutation. A component is selectable only at an atomic Helm release or explicit raw-manifest boundary. Managed fields provide diagnostics and never become a replacement ownership authority |
| NVIDIA DRA Driver for GPUs and `resource.nvidia.com/v1beta1` | physical-device or MIG allocation remains the placement authority. Veoveo adds qualified memory admission before Kubernetes mutation |
| JSON Web Token, OAuth 2.1, RFC 6750, RFC 8707, and SHA-256 | authenticated browser and machine access, resource-bound capabilities, immutable content identity, and short-lived Redap sessions |

Every dependency, image, toolchain, or infrastructure component touched by implementation must
be checked against its authoritative upstream release. Stable versions are pinned exactly.
Experimental Rerun dataloading may be evaluated after the catalog read profile is complete, but
it cannot become production acceptance evidence without an explicit product reason and its own
qualification.

### Intended Outcome

An operator who opens a known App will resolve that App and its declared closure without waiting
for unrelated servers. The host will explain recoverable dependency states, admit only declared
file uploads, and attach a safe correlation identity to failures. Independently installed
extensions will appear as exact immutable releases without becoming part of App discovery.

Recording becomes a governed Rerun catalog rather than a recorder that constructs a temporary
catalog only when one recording is opened. RRD remains canonical. Semantic datasets contain
recording segments, immutable shards become layers, and the same authorized data serves the
WebViewer, Catalog SDK, DataFusion, and bounded browser projections. The current JSON row query
and per-recording catalog model are deleted in the cut.

External simulators will receive live-view schemas, typed server helpers, a browser client, and a
parameterized hardware acceptance harness. The working UAV live-view runtime remains the
behavioral reference. It does not move to shared code until the extracted package proves exact
parity on Linux with the existing five-user hardware gate.

Deployment will reject unsafe GPU memory closure and cross-owner release overlap before mutation.
Provider reasoning configuration will distinguish absence, disablement, and named effort without
silent substitution.

### Decision Ledger

| ID | Priority | Decision | Replacement or constraint |
|---|---|---|---|
| `APP-RESOLVE-014` | P0 | accept | one Gateway-owned exact resolver supplies every direct App operation. Broad discovery remains only for the App gallery |
| `APP-HEALTH-015` | P0 | accept | the exact resolver supplies the sole dependency snapshot. The host answers scoped `resources/list` and owns a typed retry state machine |
| `RECORDING-PROJECTION-016` | P0 | replace with a larger hard cut | correct the complete Rerun dataset model, make governed Redap the primary read plane, and use Arrow IPC only as the browser adapter. Do not add a bespoke numeric recording protocol |
| `APP-UPLOAD-017` | P1 | accept with a different byte path | the host owns file selection and streams `File` bytes end to end. The frame receives only the immutable receipt. Do not pass an `ArrayBuffer` through `postMessage` or buffer the body in the BFF and Gateway |
| `EXT-RELEASE-018` | P1 | accept as projection | reuse `veoveo.io/extension-release/v1`; keep readiness dynamic and separate from the immutable release manifest |
| `TRACE-019` | P1 | accept narrowly | complete W3C and OTLP continuity and surface a safe correlation identity. Do not add the reference patch's Grafana reverse proxy |
| `LIVEVIEW-020` | P1 downstream ergonomics | narrow | publish external adoption artifacts around live-view v4. Freeze the qualified first-party runtime until an extracted client passes behavior and hardware parity |
| `GPU-ADMISSION-021` | P1 | accept | use qualified persistent and maximum-peak bytes, replica multiplication, device or MIG capacity, and minimum headroom. Current observed free memory is diagnostic only |
| `REASONING-022` | P2 | accept when the provider capability inventory exists | absence preserves the provider default, disabled maps to one admitted disable value, and named effort maps through a validated provider adapter. No fallback is permitted |
| `DEPLOY-SCOPE-023` | P1 | accept as a deployment-contract hard cut | selections operate on atomic owned components, render the complete object closure before mutation, reject overlap, and issue zero writes to unselected components |

The client priorities describe operator value rather than implementation order. Exact App
resolution precedes App health and upload because those features must consume one authority.
Recording contract and storage work precede every new recording client. Extension inventory and
component-scoped deployment share one ownership vocabulary. GPU admission precedes any newly
qualified shared-device deployment.

### Repository Invariants

- Revised names, schemas, methods, routes, manifest versions, and query shapes are hard cuts. No
  aliases, compatibility readers, dual writes, or hidden fallback paths remain after a phase.
- Gateway policy remains the authorization authority. Console may cache one successful resolution
  only inside the authenticated MCP session and active policy revision.
- Exact resolution cannot grant a tool, resource family, subscription, upload profile, or agent
  target that broad authorized discovery would withhold.
- Unknown or unauthorized App state never reveals whether hidden resources exist.
- No credential, cookie, bearer, Redap token, storage coordinate, private URL, or trace baggage
  enters an opaque App frame, MCP content, model context, log, or error panel.
- RRD is the only durable recording data format. Arrow IPC is a finite query response. Rerun
  Blueprints remain presentation state and never become recording authority.
- Frozen recording bytes and their SHA-256 identities are immutable. Catalog indexes are derived
  and rebuildable.
- Raw Redap chunk access is granted only when the caller is authorized for the complete selected
  recording segments. Projection-only callers never receive a Redap bearer.
- The current live RRD channel keeps its exclusive receiver, reconnect, rollover, and Blueprint
  continuity behavior. Archive changes cannot introduce polling or cursor forcing into Live mode.
- Provider work completion remains webhook-only. Trace work cannot add provider polling.
- GPU visual, simulation, rendering, encoding, perception, and visual verification remain
  hardware-only. Admission never creates a CPU profile or optional GPU mode.
- Browser acceptance stays headed and requires hardware WebGPU or WebGL. SwiftShader, llvmpipe,
  and software rasterizers are rejected.
- Known shapes use typed Rust, Python, and TypeScript models with closed enums. Raw JSON remains at
  genuinely open provider or Kubernetes decode boundaries only.
- Every growing list has an explicit bound and stable cursor. Every potentially large byte path is
  streaming or bounded task-local materialization.
- Helm and the installation reconciliation controller retain mutation ownership. Smoke code
  verifies declared lifecycle and never becomes a parallel installer.

### Delivery Tracks And Dependency Order

| Track | Requests | Depends on | May proceed alongside |
|---|---|---|---|
| A: exact App authority | `014`, `015` | Phase 0 contracts | recording foundation and release inventory |
| B: recording catalog hard cut | `016` | Phase 0 identity and retention decisions | exact App authority until browser projection integration |
| C: App upload and tracing | `017`, `019` | exact App authority; existing Artifact and telemetry boundaries | release projection |
| D: extension and deployment ownership | `018`, `023` | common component and release identities | recording and live-view packaging |
| E: external live view | `020` | published v4 contract; exact App host for external acceptance | GPU contract design |
| F: resource admission | `021`, `022` | qualified workload/provider evidence | nonvisual contract work |
| G: integrated closure | all | every owning phase | none |

The recording cut is one release even though implementation uses several commits. New storage,
new catalogs, and old playback cannot be independently activated. App resolution may ship before
recording if it passes its own complete host acceptance.

### Phase 0: Contract Locks And Red Tests

Phase 0 records the decisions in owning design documents before production code changes.

#### Required design updates

| Owner | Required decision |
|---|---|
| `mcp/apps-extension/DESIGN.md` | exact resolved-App projection, App-scoped resource-family metadata, host upload request and receipt, and frame-visible error categories |
| `mcp/contract/DESIGN.md` | bounded trace propagation and any Gateway-owned exact-resolution metadata that crosses MCP |
| `servers/recording-mcp/DESIGN.md` | governed dataset catalog, read grants, Redap profile, projection endpoint, hard-cut manifest, and live/archive relationship |
| `docs/RECORDINGS.md` | new canonical identities, active and immutable storage, Rerun dataset hierarchy, retention, recovery, migration, and acceptance |
| `extensions/contract/DESIGN.md` | immutable release inventory projection and relationship to deployment ownership |
| `deploy/contract/DESIGN.md` | atomic component ownership and qualified memory admission |
| `docs/GPU_PLACEMENT.md` | capacity sources, qualification identity, formula, drift evidence, and fail-closed operation |
| `agents/kernel/DESIGN.md` and owning model-provider design | three-state reasoning configuration, capability validation, and effective-state diagnostics |
| `mcp/contract/src/live_view.rs` owning design section or a focused adjacent design | published live-view v4 server, simulator-adapter, browser-client, and conformance boundary |

`docs/CODEMAP.md` changes in the same implementation commits when a new SDK package, catalog
module, or conformance component is placed.

#### Required failing tests

- exact App load fails when the existing broad catalog waits on an unrelated server;
- App bridge rejects `resources/list` because the host advertises but does not implement it;
- frame load produces an unowned blank or terminal iframe response when the owner server restarts;
- the existing recording query proves JSON string conversion and cannot select exact component
  columns or return typed Arrow;
- the existing per-recording Redap catalog cannot expose two authorized recordings as segments of
  one dataset;
- an App upload attempt proves there is no declared host capability and no streaming ingress;
- a BFF-originated call proves where trace continuity currently breaks;
- the external simulation fixture proves that it redeclares live-view models and cannot import a
  reusable browser client or generic five-user harness;
- GPU profile validation accepts a same-device group without memory closure;
- model manifests cannot distinguish absent and disabled reasoning;
- a selected platform update proves that its current mutation plan includes an extension-owned
  release or cannot demonstrate zero writes.

Tests remain focused at their owning boundary. Linux deployment, GPU, and headed-browser failures
are recorded only in the applicable remote acceptance environment.

### Phase 1: Exact App Authority And Recovering Hosts

This phase implements `APP-RESOLVE-014` and `APP-HEALTH-015` as one authority change.

#### Canonical resolution

The Gateway owns a typed `ResolvedApp` operation in its MCP library. Console does not reconstruct
the result from independent calls. The resolver accepts the active profile, authenticated actor,
mounted owner server, and exact projected `ui://{server}/{page}` URI.

Resolution performs this ordered work:

1. Parse the URI with the existing closed App grammar and reject traversal, query, fragment,
   missing page, and owner mismatch.
2. Resolve the mounted owner from the active control revision without enumerating other servers.
3. Apply the ordinary profile, scope, label, policy, and App-link decisions.
4. Read the exact owner resource.
5. Read only the owner and declared dependency listings needed to prove linked tools and admitted
   resource families.
6. Return the immutable App resource, HTML content, admitted tools, resource families,
   subscriptions, agent targets, upload grant, owner/dependency status, CSP inputs, actor display
   projection, policy revision, and resolution revision.
7. Record one aggregate policy/audit decision. Internal per-listing checks cannot create a second
   user-visible authorization result.

The projected App resource may carry repository-owned exact-resolution metadata for transport,
but the Gateway library result is the authority. A BFF-only `/console/api/apps/resolve` route is a
same-origin projection, not a new authorization service.

Successful resolutions use single-flight caching keyed by authenticated MCP session, control
revision, policy revision, actor authority fingerprint, owner server, and exact App URI. Failures
are not retained. Relevant `resources/list_changed`, `tools/list_changed`, policy activation, or
session replacement invalidates the entry. A broad App-catalog refresh cannot invalidate an
unrelated exact resolution unless the control or policy revision changes.

#### Required consumers

The following paths must consume `ResolvedApp` and must stop calling the broad App catalog:

- Console direct App navigation;
- standalone App bootstrap;
- frame HTML read;
- App tool admission and task ownership;
- App resource read, list, subscription, and resource-event admission;
- agent-message target admission;
- App artifact-upload grant resolution;
- host CSP and descriptor construction.

The broad catalog remains for the installation App gallery and its reactive partial-degradation
UX. It is never a prerequisite for a known App route.

#### App-scoped `resources/list`

The host answers standard bridge `resources/list` with one finite, cursor-free view of the
already resolved closure. It does not call broad Gateway discovery again.

Each family record contains:

- projected scheme and non-root URI prefix;
- owner or declared dependency server;
- admitted operations from the closed `AppResourceOperation` set;
- server state `ready`, `unavailable`, or `answered_empty`;
- whether policy admitted the family;
- optional bounded public diagnostic category.

The resource array contains only concrete resources already visible in the resolved owner and
dependency listings. An unavailable family remains in metadata without fabricated resource
entries. Undeclared servers and families are absent. A policy-withheld declared family can report
`admitted: false` because the App already declares that dependency, but it cannot reveal hidden
resource identity or count.

#### Recovering host state

Console and standalone hosts share one state machine:

| State | Meaning | Automatic action |
|---|---|---|
| `signing_in` | no valid Console session | enter the existing authentication transition |
| `resolving` | exact authority and resource are pending | wait within the bounded resolution deadline |
| `starting` | descriptor is ready and the sandbox is booting | mount one frame |
| `retrying` | owner or dependency is temporarily unavailable | capped exponential retry with jitter and manual retry |
| `forbidden` | authenticated caller lacks the App authority | terminal safe panel; no existence disclosure |
| `not_found` | the admitted owner answered and the exact App resource is absent | terminal safe panel |
| `failed` | non-retryable contract or internal failure | terminal safe panel with correlation identity |

The host never replaces a healthy mounted frame because an unrelated dependency changes. A
retryable resolution failure does not mount an empty document. Successful recovery remounts once
with the same route and host policy.

#### Ownership and deletion

| Path | Work |
|---|---|
| `mcp/apps-extension` | shared `ResolvedApp`, family-status, upload-grant, and safe host-error types |
| `mcp/contract/src/gateway` | exact resolver configuration and validation types |
| `platform/gateway/src/mcp/` | targeted resolution, policy, status, bounded listing, caching, and invalidation |
| `apps/console/bff/src/mcp_client.rs` | session-scoped exact-resolution client and single flight |
| `apps/console/bff/src/apps.rs`, `app_host.rs` | same-origin projection and migration of every direct consumer |
| `apps/console/web/src/appHost.tsx`, `StandaloneAppHost.tsx`, `apps/bridge.ts` | shared state machine and scoped resource-list response |

Delete every direct-route call to `app_catalog()`. Delete any duplicate family derivation in the
web client. Retain `app_catalog()` only for gallery listing and gallery change events.

#### Acceptance

- Console and standalone direct routes load while an unrelated MCP server never answers.
- Owner unavailability produces `retrying`, then recovers after restart without shell reload.
- An admitted owner that returns no exact resource produces `not_found`.
- Withheld authority produces `forbidden` and no hidden server or resource details.
- Scoped listing distinguishes unavailable, answered-empty, and policy-withheld declared families.
- Console and standalone descriptors, CSP, tools, resources, and host states are identical.
- Exact resolution cannot add any capability absent from an equivalent complete authorized
  catalog.

### Phase 2: Recording Catalog Hard Cut

This phase replaces `RECORDING-PROJECTION-016` and the current archive architecture. It is one
activation boundary even when implementation is divided into reviewable commits.

#### Canonical Rerun object model

The new identity hierarchy is:

```text
Veoveo tenant
└── recording dataset (durable UUIDv7 plus unique tenant-local key)
    ├── dataset Blueprint and optional static assets
    ├── recording UUIDv7 = Rerun segment id
    │   ├── capture-00000000000000000000 = immutable RRD layer
    │   ├── capture-00000000000000000001 = immutable RRD layer
    │   ├── properties = immutable governed metadata layer
    │   └── derived-<kind>-<revision> = immutable annotation or evaluation layer
    └── another recording UUIDv7 = another Rerun segment id
```

The existing producer `recording_key` remains bounded source metadata. It is not the Rerun
segment identity. Archive materialization rewrites recording Store IDs to the Veoveo recording
UUID and the dataset application identity before publishing a frozen layer. All distributed
producers for one recording converge on that identity. Blueprint activation is rewritten to the
same playback application identity.

A new durable recording-dataset record owns its UUID, tenant-local key, display label, default
Blueprint reference, retention policy, and revision. A recording belongs to exactly one dataset.
Moving a recording between datasets is not supported. Reclassification requires a new recording
or an explicit offline migration.

#### Durable byte and manifest authority

The active writer retains task-local or pod-local persistent storage while a segment is open.
Every freeze performs the following transaction protocol:

1. Complete Rerun object-store optimization, footer generation, Store ID normalization, H.264
   GOP validation, and SHA-256 calculation in a staging file.
2. Publish the immutable RRD object to the installation-owned Artifact/object store.
3. Verify the stored byte length and digest.
4. Commit the object identity, layer name, recording, dataset, ordinal, time bounds, Rerun version,
   schema digest, and artifact occurrence in SurrealDB.
5. Announce the catalog revision only after the durable transaction commits.
6. Remove the local frozen source after retention of the staging recovery window.

Historical reads resolve internal object-store coordinates from the durable artifact occurrence.
Signed public URLs are never persisted in the catalog. Production Redap registration uses an
internal object-store URL or a server-owned object adapter. Tests may use absolute `file://` URIs.

SurrealDB plus immutable object digests define recording truth. A Redap catalog is an index and
cache. Restart, eviction, or catalog loss rebuilds it without modifying RRD bytes.

#### Recording properties and tables

The properties layer contains only metadata safe for a caller already admitted to the recording:

- canonical `recording://recordings/{uuid}` identity;
- dataset identity and key;
- application identity and bounded producer recording key;
- lifecycle state;
- start, end, and seal timestamps;
- source and manifest revisions;
- immutable manifest digest;
- non-secret model, runtime, scenario, or environment revision when the producer declares it.

Principal identity, group membership, policy rules, credentials, filesystem paths, private object
coordinates, and internal failure text never enter RRD properties. Classification and labels
remain authorization inputs. A caller may receive admitted labels in a separate virtual segment
table projection, but they are not embedded into broadly reusable RRD bytes.

Rerun tables may expose read-only derived metadata or evaluation joins inside an authorized
virtual catalog. SurrealDB remains authoritative. General Redap table mutation stays disabled.

#### Governed virtual catalogs

`recording-mcp` becomes the public recording control and data boundary. Its catalog modules use
Rerun's official handler and protocol types. They no longer create one dataset for every opened
recording.

A virtual catalog is keyed by tenant, dataset, policy revision, allowed-segment digest, and grant
class. It contains only admitted segment and table rows. Caches are bounded and reconstructable.
The service validates the dataset and segment identity on every Rerun request, including requests
that bypass discovery and present direct chunk metadata.

Three read classes remain distinct:

| Grant class | Consumer | Data authority |
|---|---|---|
| `viewer_segment` | Console or standalone WebViewer | full Rerun read access to one exact admitted segment and its dataset Blueprint |
| `catalog_dataset` | authorized Python/Rust analytics or training client | full Rerun read access to the explicit admitted segment set within one dataset |
| `app_projection` | exact MCP App through its host | no Redap bearer; only the closed projection query and Arrow result |

The standard Redap JWT remains host-limited and read-only. Its subject maps to server-side grant
state containing exact datasets, segments, class, actor, Work Context, policy revision, expiry,
and allowed routes. Tokens do not rely on client-supplied segment filters for authorization.

Viewer and Catalog SDK credentials are redeemed through an authenticated non-MCP route. MCP may
return a non-secret grant descriptor or resource link, but no bearer enters a tool result or model
context. Browser playback obtains its credential through the authenticated host as it does today.

#### Supported Redap profile

The implementation supports and documents the exact read methods required by WebViewer and the
Rerun Catalog SDK:

- version and identity;
- exact entry discovery within the virtual catalog;
- dataset entry, schema, manifest schema, and segment table schema;
- dataset manifest and segment table scans;
- RRD manifest and asset reads when admitted;
- `QueryDataset` latest-at and range chunk selection;
- `FetchChunks` constrained to chunk identities returned for the same grant;
- bounded event watch for immutable catalog revision changes.

Dataset, segment, table, registration, task, and maintenance mutations remain internal or denied.
The design names this a supported read profile until the applicable official Rerun protocol tests
pass. It does not claim complete Redap conformance by delegation.

#### Browser Arrow projection

The App first requests a closed projection through Recording MCP control. The request contains:

- one immutable or captured recording revision;
- exact entity paths or a registered producer-defined selector descriptor;
- exact component identifiers;
- one timeline and inclusive range, latest-at instant, or explicit sample grid;
- sparse-fill strategy;
- maximum entities, columns, samples, rows, serialized bytes, and duration;
- cancellation and idempotency identity.

Opaque producer selectors are resolved by a registered descriptor. Recording core never learns
domain nouns such as mission, trial, vehicle, or episode.

The query planner uses Rerun `QueryExpression`, entity and component selectors, timeline types,
latest-at/range semantics, and explicit `using_index_values` sampling. It reads the same immutable
layers admitted to Redap. Results use canonical column and row ordering and Arrow IPC stream
encoding.

Strict byte-limit acceptance requires complete bounded materialization before response headers.
The service writes the candidate Arrow stream to bounded task-local scratch, rejects overflow,
verifies equal column lengths and finite constrained numeric values, computes the digest, then
streams the completed file. Cancellation removes scratch state. Repeated requests against the
same immutable manifest and query revision produce identical bytes and digest.

The result metadata contains recording, dataset, manifest, query, timeline, units, coordinate
frame references, sample grid, omitted-sample counts, Arrow schema digest, byte length, and
payload digest. It contains no cookie, Gateway bearer, Redap token, object URL, or filesystem path.

#### Live and archive relationship

The existing incremental `rerun_rrd_channel_v2` behavior remains:

- one WebViewer channel for the selected live recording;
- bounded bootstrap followed by newly durable batches;
- event-driven segment rollover;
- no manifest or provider polling;
- no cursor forcing from `time_update`;
- no replay of the bootstrap into the same channel;
- producer Blueprint continuity;
- archive-only credential renewal.

The hard cut advances the playback manifest once. The new manifest names the dataset, segment,
catalog revision, archive URI, live receiver, Blueprint, and access expiry under the corrected
identity model. Manifest v8 and its per-recording dataset semantics are deleted. Live behavior is
changed only where identity fields must align with the new segment.

#### Component ownership

| Path | Work |
|---|---|
| `platform/recordings/hub` | active ingest, normalization, freeze, object publication, and durable manifest transition. Remove general query and historical-serving claims |
| `platform/recordings/rrd` | Store ID rewriting, property layers, optimization, schema/digest inspection, and common Arrow/RRD adapters |
| `platform/recordings/protocol` | hard-cut ingest identity and dataset fields when producer coordination requires them |
| focused `platform/store` recording modules and migrations | keep existing recording lifecycle persistence in `recordings.rs`; place dataset/layer manifests, read grants, and projection receipts in focused modules with typed joins |
| `servers/recording-mcp/src/service/read.rs` | exact authorized immutable read plans over object identities, not retained filesystem paths |
| `servers/recording-mcp/src/playback.rs` | replace per-recording catalog/session construction with virtual dataset catalogs and grant enforcement; split focused modules as responsibilities expand |
| `servers/recording-mcp/src/contract.rs` | new manifest, catalog grant, projection request, Arrow result metadata, and typed limit errors |
| `apps/console/bff/src/recording_playback.rs` | project the new manifest and Arrow route without retaining catalog sessions or buffering bytes |
| `apps/console/web` | new manifest identity and optional native App projection client; preserve the qualified WebViewer lifecycle |
| `testing/smoke` and `testing/browser-smoke` | Redap profile, authorization isolation, multi-segment query, live/archive continuity, deterministic Arrow, and hardware browser evidence |

`playback.rs` must be split if catalog construction, grant enforcement, Redap routing, projection
materialization, and session lifecycle would otherwise compound in one module.
`platform/store/src/recordings.rs` is already a substantial lifecycle module. The hard cut must
not turn it into the catalog, authorization, and projection persistence owner as well.

#### Deletion and migration

Delete:

- per-recording Rerun dataset construction;
- the `MAX_CATALOGS` cache keyed only by recording;
- JSON-string `query_recording` and `hub-query` as user-facing query evidence;
- history paths that depend on local frozen files after object publication;
- playback manifest v8 and its tests;
- stale claims that a separately deployed OSS `rerun server` serves Hub files;
- any bespoke little-endian numeric projection considered during implementation.

If existing recordings must survive, provide one offline migration command. It rewrites Store IDs,
generates properties, publishes immutable objects, commits the new manifest, and verifies the new
Redap segment before activation. Runtime code never reads both architectures. If retention is not
required, the installation discards old recordings explicitly.

#### Acceptance

- One semantic dataset exposes at least two authorized recordings as distinct Rerun segments.
- A caller admitted to one segment cannot discover, query, infer, or fetch chunks from another.
- WebViewer opens one exact segment lazily and preserves the dataset or producer Blueprint.
- Rerun Catalog SDK reads the authorized segment table, filters contents, and returns typed
  DataFusion/Arrow results.
- A native App receives one bounded Arrow window and never receives RRD bytes or a Redap bearer.
- Component, range, latest-at, explicit sampling, sparse-fill, row, and byte bounds are enforced
  before any response byte.
- Immutable repeated projections are byte-identical and carry the same digest.
- Active ingest and live viewers continue while an archive projection runs.
- Process restart rebuilds the catalog from SurrealDB manifests and object-store RRDs.
- The supported read profile passes the applicable official Rerun protocol tests plus Veoveo
  authorization tests.
- Existing live recording hardware acceptance retains its current latency, source-alignment, and
  reconnect gates.

### Phase 3: Host-Mediated Streaming Artifact Upload

This phase implements `APP-UPLOAD-017` after exact App authority exists.

An App resource declares one closed upload grant containing admitted media types, maximum bytes,
maximum file count per request, filename limits, Artifact policy profile, and optional purpose.
Absence grants no upload method. Exact resolution projects the validated grant into the host
descriptor.

The frame invokes a repository-owned host method with a UUIDv7 request ID and optional safe prompt
text. The host opens the file picker. The selected `File` object stays in the host origin. Its
bytes never cross the opaque-frame `postMessage` bridge.

The host accepts the request only during browser-observed transient user activation and opens the
picker before asynchronous work can consume that activation. Background messages, retries, and
restored frames cannot reopen the picker. A retry begins only after the user selects the file and
reuses the same immutable request identity.

The host validates count, name, media type, declared size, and grant before opening the data path.
It streams `File.stream()` through the BFF and Gateway to the existing Artifact plane. Each hop
enforces a hard byte ceiling and cancellation. The Artifact service computes the digest while
writing and commits only after the received byte count matches the declared count.

UUIDv7 idempotency is durable at the Artifact service and keyed by actor, Work Context, App URI,
grant revision, and request ID. A retry returns the original receipt when every immutable input
matches. A conflicting retry fails without replacing data.

The frame receives only:

- canonical artifact URI;
- SHA-256 digest;
- media type;
- exact byte length;
- immutable occurrence or receipt revision.

The reference patch's `ArrayBuffer` bridge and Axum `Bytes` handlers are rejected. Implementors
must extend the Artifact client and service with a streaming body when their current boundary
buffers the object.

Acceptance proves admitted upload, pre-transfer rejection, exact digest, durable idempotency,
cancel-on-frame-close, zero stored bytes after rejected or incomplete transfer, and absence of
file bytes from MCP, model, logs, and telemetry.

### Phase 4: Extension Releases And Distributed App Tracing

#### Extension release projection

`EXT-RELEASE-018` reuses `extensions/contract::ExtensionReleaseManifest`. The installation lock
declares every selected manifest source and digest. Gateway or Console loads them through one
bounded installation-owned projection.

Validation checks schema version, identifier uniqueness, semantic version, source revision,
release timestamp, chart/package digest, immutable image references, optional compatibility
manifest, byte bound, and duplicate ownership. One malformed release produces one typed operator
diagnostic and does not hide another valid release or affect App discovery.

The immutable manifest and dynamic readiness remain separate values. Console may join current
deployment readiness by extension and component identity, but it cannot rewrite the release
inventory or infer App availability from it. The generic UI shows the display label, version,
source revision, chart digest, image digests, compatibility identity, owner, and current status.

No first-party product name appears in projection code. Two independent fixture releases must
render without new code.

#### Trace continuity

`TRACE-019` completes one trace across:

1. signed-in Console or standalone host request;
2. BFF exact App operation;
3. Gateway MCP authorization and selected upstream request;
4. hosted MCP server work;
5. optional Artifact or recording projection request;
6. terminal result or retry.

The BFF and Gateway HTTP routers extract valid W3C context at authenticated boundaries and set it
as the parent before request spans begin. Every owned outbound HTTP and MCP transport injects the
current context. MCP metadata remains bounded and sanitized by the existing contract. Opaque App
frames may supply a host-local request ID for promise settlement but cannot supply `traceparent`,
`tracestate`, or baggage.

Safe span attributes are limited to method class, mounted server, bounded public App URI or its
hash, policy revision, result category, retry count, duration, request/response byte counts, and
non-secret catalog or release revision. Tool arguments, prompts, resource bodies, filenames,
emails, tokens, private URLs, provider payloads, database errors, and policy internals are never
recorded.

Retries remain children of the original operation and carry an attempt number. The host error
panel displays a short correlation value derived from the trace ID plus a safe category. It does
not display full trace context. The installation observability backend retains the complete trace
under its existing access controls.

The reference patch's Grafana configuration, authentication proxy, and `/console/observability`
routes are outside this request and must not be ported. A future observability UI requires its own
authorization and product decision.

Acceptance joins one success and one failure across BFF, Gateway, server, and one bounded data
plane. Redaction tests inspect exported span data, not only log text.

### Phase 5: External Live-View Adoption Without Runtime Disruption

`LIVEVIEW-020` is a developer-experience and productization request. It is not an operator UX
rewrite. Current main already owns:

- provider-neutral live-view v4 Rust types in `mcp/contract/src/live_view.rs`;
- one shared tiled RTX/NVENC H.264 product with camera regions;
- independent viewer authorization, renewal, closure, and audit;
- browser WebCodecs decode and crop in the UAV App;
- an independently packaged anonymous simulation extension;
- headed five-user hardware acceptance proving one product and one NVENC session.

The missing adoption surface is packaging. The Python fixture currently redeclares live-view
models. The browser decoder and renewal logic remain inline in the first-party UAV HTML. The
five-user harness is parameterized around UAV routes, camera names, and DOM evidence.

#### Published artifacts

| Artifact | Owner | Required content |
|---|---|---|
| generated live-view v4 JSON Schema | `mcp/contract` release output | exact camera, region, product, endpoint, authorization, health, open, renew, and close shapes |
| Python server models and helpers | `sdk/python` | typed v4 models, validation, redacted token handling, authorization lifecycle helpers, and simulator-adapter interfaces without renderer code |
| browser client package | a focused package under `sdk/` selected after CODEMAP update | WebCodecs capability check, hardware/software decode label, Annex B keyframe handling, shared-product socket ownership, region crop, renewal, reconnect, teardown, cancellation, and typed evidence callbacks |
| simulator adapter contract | Python SDK plus schema | logical camera inventory, source region, one stable stream-product identity, product health, and private stream endpoint binding |
| generic headed acceptance | `testing/browser-smoke` or a focused Rust crate if responsibilities require it | manifest-driven App URI, camera set, evidence selectors, five users, hardware graphics proof, product/encoder invariance, renewal, close, restart, and screenshots |
| external reference | `testing/fixtures/external-simulation-extension` | consume released SDK types, stop redeclaring the v4 contract, and remain independent of first-party vehicle or simulator source |

The packages never include a renderer, encoder, simulator dependency tuple, vehicle schema, scene
mirror, or physics state. External implementations retain their own GPU qualification.

#### Non-disruption sequence

1. Generate and publish schemas from the existing Rust contract without modifying the UAV runtime.
2. Move the external fixture to the Python types and pass isolated packaging/conformance tests.
3. Build the browser client against protocol fixtures and an external simulator implementation.
4. Parameterize the existing Rust browser harness while retaining the current UAV scenario as an
   unchanged concrete configuration.
5. Run the external implementation and current UAV implementation through the same Linux headed
   five-user hardware gate.
6. Consider moving the UAV App to the shared browser package only after exact DOM, authorization,
   product-count, encoder-count, frame-rate, latency, restart, and teardown parity passes.
7. If adoption is approved, hard-cut the UAV App to the one shared implementation and delete the
   inline duplicate in the same change. If parity is not proven, keep the current UAV code and do
   not claim the client package as the first-party runtime.

No initial phase changes `servers/uav-sim-mcp/src/server/live_view.rs`, simulator camera products,
NVENC behavior, WebSocket framing, or qualified camera layouts.

### Phase 6: Accelerator Memory And Reasoning Admission

#### Qualified accelerator memory

`GPU-ADMISSION-021` extends the current physical GPU placement contract. Each workload placement
adds:

- persistent reservation bytes;
- maximum admitted peak bytes;
- replica count already owned by placement;
- exact image digest;
- model/checkpoint digest when applicable;
- runtime and CUDA compatibility revision;
- qualification evidence digest and timestamp.

Each same-device group adds minimum headroom bytes and a capacity source. Capacity comes from the
qualified full-device ResourceSlice identity or an exact MIG profile. It never comes from current
NVML free memory.

Validation requires `persistent <= peak` for every workload and computes with checked integers:

```text
sum(workload.maximum_peak_bytes * workload.replicas) + group.minimum_headroom_bytes
    <= qualified_device_or_mig_capacity_bytes
```

The persistent sum plus headroom must also fit, even though valid peaks normally make that
inequality redundant. A changed image, model, runtime, CUDA tuple, replica count, device product,
MIG profile, or evidence digest invalidates qualification.

Admission runs during pure deployment-profile compilation before Helm rendering or Kubernetes
mutation. NVML measurements after startup become drift evidence. They may fail acceptance or
require requalification, but they cannot admit an otherwise unsafe profile.

Tests cover overflow, missing values, persistent greater than peak, insufficient headroom,
replica multiplication, capacity-source mismatch, evidence invalidation, and a safe exact fit.
Linux acceptance proves that an unsafe group creates no Pod or Helm mutation.

#### Three-state reasoning

`REASONING-022` uses an optional closed object:

```json
{"reasoning":{"mode":"disabled"}}
```

or:

```json
{"reasoning":{"mode":"effort","effort":"high"}}
```

When `reasoning` is absent, the provider default is preserved. JSON `null`, an explicit
`"omitted"` mode, arbitrary strings, and provider payload fragments are rejected.

A provider capability registry is keyed by endpoint class, provider, exact model identity, and
adapter revision. It declares whether disablement is supported and the closed effort values that
may be mapped. Validation occurs when the model manifest loads, before any provider connection.
The adapter emits only the admitted provider field and value.

There is no retry with a different mode, model, endpoint, temperature, or sampling configuration.
Unsupported configuration fails the manifest. Diagnostics expose the effective non-secret state
as `provider_default`, `disabled`, or the named admitted effort without exposing hidden reasoning
or provider request bodies.

Implementation starts only after an owner supplies the provider/model capability inventory. It
must not guess current provider behavior from model names.

### Phase 7: Component-Scoped Deployment Hard Cut

`DEPLOY-SCOPE-023` advances the deployment and lock schema together. It builds on current source
roles and image ownership but changes mutation selection.

#### Atomic component model

Each deployment source declares components with:

- globally unique component ID;
- owning source and immutable source revision;
- role `platform`, `workload`, `extension`, or `installation`;
- one or more atomic Helm release identities or explicit raw-manifest sets;
- namespace and permitted object identities;
- values and image closure;
- dependencies on other component IDs;
- extension-release identity where applicable.

A Helm release is indivisible. If platform and extension resources share one release, they cannot
be selected independently. The implementation must split that chart into independently owned
releases before component selection is enabled. It must not approximate partial ownership with
label selectors or post-render object deletion.

#### Selection and preflight

A release operation names exact component IDs. The compiler expands dependencies, renders every
selected release and raw-manifest set, and computes the complete Kubernetes object identity set
before mutation. It also loads the locked object identities of unselected components.

Preflight rejects:

- duplicate ownership;
- selected/unselected object overlap;
- a selected source that differs from the deployment lock;
- a Helm release containing objects owned by two components;
- undeclared namespaces or cluster-scoped objects;
- image or values input outside the selected component closure;
- missing dependency components;
- an operation whose tool cannot target the atomic release exactly.

After preflight, orchestration invokes Helm only for selected releases and applies only selected
raw manifests. It does not render or apply an unselected release as a side effect. Managed fields
may verify the expected manager and report drift. They do not authorize taking ownership.

The receipt records selected components, expanded dependencies, source revisions, rendered object
digests, mutation verbs, and zero-write evidence for unselected objects. It contains no Secret
bytes.

Acceptance uses independent platform and extension Git histories. A platform-only update changes
one platform image and proves zero writes, patches, applies, deletes, rollouts, or Helm revisions
for extension objects. An extension-only update proves the inverse. Deliberate overlap fails
before the first API write.

### Phase 8: Integrated Closure

The final release closes only when every included request passes its focused tests and the
cross-component paths pass together.

#### Integrated scenarios

1. Open a known external App while an unrelated server is down. Observe exact resolution,
   unavailable dependency metadata, retry, recovery, and one connected trace.
2. Use the host picker to stream an admitted file. Receive an immutable artifact receipt in the
   frame and pass that receipt through a bounded domain tool call.
3. Open one recording in the WebViewer while a native App loads an Arrow projection from the same
   segment and clock. Seek repeatedly and prove deterministic state.
4. Query two admitted recording segments with the Rerun Catalog SDK while a third segment in the
   same tenant remains undiscoverable.
5. Project two independent extension releases. Perform a platform-only update and prove zero
   writes to both extension releases.
6. Run one external live-view implementation and the first-party UAV reference with five headed
   users on qualified NVIDIA hardware. Product and encoder counts remain constant.
7. Reject an unsafe GPU memory profile before mutation. Start the qualified profile and compare
   NVML drift evidence with its admitted bounds.
8. Validate absent, disabled, and named reasoning configurations. Restart and prove the effective
   configuration remains exact.

#### Cross-cutting security inspection

Acceptance captures browser messages, BFF and Gateway HTTP headers, MCP requests, OTLP exports,
logs, artifact metadata, recording projection metadata, and deployment receipts. It proves that
credentials, file bytes, prompts, private URLs, object coordinates, provider payloads, policy
internals, and hidden resource identities are absent from every unauthorized surface.

#### Test evidence discipline

Every build-input change runs its affected checks through `cargo xtask test-report`. The committed
`testing/local-test-report.json` contains only green current entries. Documentation-only commits do
not replace or invalidate build evidence.

All smoke lifecycle, assertions, retries, cleanup, and evidence remain Rust. Browser acceptance
uses a headed hardware-backed browser. Recording and live-view GPU evidence is collected only on
the Linux deployment with accessible NVIDIA hardware. macOS review or software rendering cannot
close any visual gate.

### Reference Patch Policy

| Patch | Retain as guidance | Reject |
|---|---|---|
| `0001-exact-app-resolution.patch` | exact URI resolution, single flight, direct-route migration, and unrelated-server isolation tests | mechanical application, duplicated authority in BFF, and broad listing/cache changes not required by the targeted resolver |
| `0002-app-scoped-resource-list.patch` | bridge settlement, typed family metadata, bounded one-page result, and unavailable versus empty tests | a second independent BFF listing pass after exact resolution exists |
| `0003-governed-app-artifact-upload.patch` | explicit App grant, UUIDv7 identity, exact media/size checks, governed receipt, and CSRF-preserving host path | frame-to-host `ArrayBuffer`, Axum `Bytes`, cloned bodies, BFF/Gateway whole-file buffering, and in-memory idempotency |
| `0004-distributed-app-call-tracing.patch` | HTTP extraction/injection helpers, BFF and Gateway continuation, upstream transport injection, and redaction tests | Grafana configuration, reverse proxy, identity headers, observability routes, and any unrelated Console product surface |

The package contains no recording patch. Its recording prose supplies requirements, while this
plan replaces the proposed generic binary format with the Rerun-native hard cut above.

### Commit Plan

Commits stay small and coherent even though activation is a hard cut.

1. Add Phase 0 contract types, design updates, schemas, and failing focused tests.
2. Implement Gateway exact App resolution and its policy/cache tests.
3. Migrate BFF and both hosts, add scoped resource listing and recovery, then delete direct broad
   catalog use.
4. Add durable recording dataset/layer identities and migrations.
5. Add freeze-time Store ID normalization, object publication, properties, and recovery.
6. Implement governed virtual Redap catalogs and the supported read profile.
7. Implement bounded Arrow projection and its exact App integration.
8. Migrate Console, Catalog SDK fixtures, playback manifest, smoke tests, and optional offline data
   migration. Delete the old recording architecture.
9. Add streaming Artifact admission from host picker through durable receipt.
10. Project extension releases and complete trace continuity in separate commits.
11. Publish live-view schemas and Python helpers, migrate the external fixture, then add the browser
    package and generic harness without changing the first-party runtime.
12. Add GPU memory and reasoning contracts with their validation evidence.
13. Advance deployment component ownership, split any mixed Helm releases, and add zero-write
    acceptance.
14. Run integrated Linux acceptance, update current architecture documents, and close the plan's
    implementation record.

No commit enables a new route before its authorization, bounds, and red tests are present. No
commit removes an old recording path before all in-repository consumers compile against the new
contract. The activation commit contains the final configuration and deletion together.

### Documentation And Generated Artifact Closure

Implementation updates:

- `docs/CODEMAP.md` for every new package, module, test owner, and moved responsibility;
- `docs/RECORDINGS.md` and `servers/recording-mcp/DESIGN.md` for the final catalog contract;
- `mcp/apps-extension/DESIGN.md` for host extensions and exact authority;
- `extensions/contract/DESIGN.md`, extension examples, and external integration guidance;
- `deploy/contract/DESIGN.md`, `docs/GPU_PLACEMENT.md`, and deployment schemas/examples;
- Python SDK and external simulation documentation;
- Console and standalone App host documentation;
- generated JSON Schemas, compatibility manifests, deployment locks, Helm values schemas, and
  conformance profiles affected by the cuts.

Temporary migration instructions are removed after the final supported installation migrates.
User-facing documents describe only the new canonical routes, manifests, identities, and
configuration.

### Definition Of Done

This plan is complete when:

- every request in the decision ledger has either shipped under its approved boundary or has an
  explicit owner-recorded block that satisfies repository blocking policy;
- exact App routes never depend on installation-wide discovery;
- App hosts provide typed scoped resources, recovery, upload, and safe correlation behavior;
- Recording Hub uses the canonical dataset/segment/layer hierarchy and immutable object storage;
- governed Redap serves WebViewer and Catalog SDK reads across exact authorized segments;
- browser recording projections are bounded deterministic Arrow and JSON row query is gone;
- live recording and live-view behavior retain their qualified hardware acceptance;
- external simulators consume published live-view artifacts without first-party domain imports;
- GPU memory and reasoning configuration fail before connection or workload mutation when unsafe;
- selected deployment operations prove zero writes to unselected owners;
- reference-patch-only mechanisms rejected by this plan are absent;
- obsolete schemas, routes, names, files, tests, examples, and compatibility behavior are deleted;
- owning designs and `docs/CODEMAP.md` describe the implemented architecture rather than this
  future plan.
