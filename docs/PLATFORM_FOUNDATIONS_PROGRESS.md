# Platform Foundations Progress Log

This document preserves historical implementation and installation checkpoints.
Entries describe the source, deployment and test state at the time they were written.
Statements such as “current”, “next” and “remaining” can be superseded by later work.
Read the [active plan](CONTRACT_CONSISTENCY_PLAN.md#current-status) for current status,
requirements, accepted risks and remaining work. This log does not establish completion.

## Time Schedule Completion And Cancellation — October 10, 2026

The maintained `time-installed-schedule-task` scenario passes separate complete
and cancel modes through normal Gateway OAuth on the deployed `6653288ba` repair.
Both use version two of the eight-window calendar and fresh private attempt files.
Completion observes Working, then delivered and current Completed, and compares
all eight occurrences with the independent expectation. Cancellation observes and
receives Working before recording its intent, receives cancellation acknowledgement
and reaches current Cancelled state. Both native processes exit zero, close their
listeners and callers, and remove their temporary credentials. The terminal
receipts under `/tmp/veoveo-time-schedule-8window-prep-0ky2g9up/run-20261010` have
SHA-256 `452862bc3b7843a9a20d6781b1222dc66ddccde78c506f07ff56da424ddbb5d5`
for complete and `5b65a44358cc6f33309e9cf718f9060ca437239a52030517fc3c1ebad3dee40f`
for cancel.

The recovery attempt exits one during `crash_admission`, before Kubernetes target
checks, Task dispatch or any process signal. Its failure hash matches the static
diagnostic `source report requires a new absolute path`: the fixture revalidates
its installation after creating its own journal. The selected Time Pod and
container remain Ready and unchanged. Cleanup closes the caller, listener and
watch ownership. Recovery remains unqualified. The failed terminal receipt has
SHA-256 `0b7811c9e7350f3119745c859c2e15f7a4d7995eb941642397831f226e86a705`;
its append-only journal has SHA-256
`9bfd9ab2e905e4fd39bc277fbfbc271f38389dbb8fbc3946c678c25d946ff8ae`.

The separate completed-state A/B handoff fixture passes strict compiler checks,
focused native controls and independent logic review, and is committed at
`29831d530`. Its final cleanup-admission receipt has SHA-256
`061358b73a8ef070322aff9305ebfdb7f6e5faba0ee79642c5c5d4ec340d56cd`.
No installed A/B handoff has run, and the source result qualifies neither unfinished
recovery nor a subscription update after a mutation.

## Knowledge And Time Repair Rollout — October 10, 2026

Both images at `6653288ba` pass staging, attested release qualification and
node-network manifest/config readback. Knowledge rolls first, followed by Time,
with each old Pod drained before its replacement starts. Both selected Pods pass
health/readiness HTTP 200 and thirty seconds of stability with zero restarts.
Knowledge briefly returns readiness 503 while indexing the changed Time source,
then becomes Ready on the same container without intervention. Old Time Pod absence
is recorded; no retained exit code establishes successful process termination.

The normal OAuth operator reads Time's updated design and Knowledge's source and
collection resources, then searches `time.docs`. The collection reports two members
and ninety chunks. These targeted calls pass. The broader knowledge-source helper
reports incomplete resource-template discovery; that diagnostic does not invalidate
the targeted observations or qualify the broader helper. Its cause requires separate
request/backend evidence.

The final fleet has seventeen enabled Deployments Ready, eight disabled Deployments
and two Ready StatefulSets. All three reconciliation holds stay suspended. Image
operation growth peaks at 879,869,952 bytes against the admitted 8 GiB cap, with a
minimum 291,990,138,880 free bytes. The original baseline is 292,870,008,832 bytes;
subsequent native validation has its own admitted operation. Private final receipt:
`/tmp/veoveo-k-time-665-pair-20261010/final-receipt.json`, SHA-256
`0ff1502de295966e914762ce424da18bc133b0973bb8024642f3bd1cba324767`.
Fixed-image Time cancellation and unfinished recovery remain unqualified. The
separate completed-state cross-replica fixture requires its compiler, native and
logic-review gates before installed execution.

## Knowledge Source Admission Checkpoint — October 10, 2026

The Time image at `2314d96a5` passes stage/release runtime digest agreement,
SBOM/provenance verification and node-network manifest/config readback. Its selected
Pod reaches Ready with health and readiness HTTP 200. The old Pod disappears
0.25 seconds after the drain patch, inside its thirty-second grace; no retained
container exit code or shutdown log proves a successful process exit.

The rollout exposes Knowledge's 64 KiB source-member bound. The served Time design
is byte-identical to its 71,502-byte source, and Knowledge reports the fixed
text-response admission error. Restoring the original Time image brings Knowledge
back to Ready without a manual Knowledge restart. The repaired Knowledge profile
uses one 256 KiB constant at all four ingestion guards. Full-source digests, URI
agreement, the 256-chunk limit and embedding text/batch bounds stay enforced.
Compiler, strict lint, independent logic review and three focused native controls
pass. The controls cover the actual Time design through the maintained SDK,
byte-boundary and digest/URI/count refusals, UTF-8 chunking and indexed-member
admission. Installed qualification of the repaired image pair remains open.
Private native receipt:
`/tmp/veoveo-time-handoff-metadata-20261010/knowledge-eleven-path-native-receipt.json`,
SHA-256 `5c3861606fd9cf9f093f55bf8c2ce72f2efd2ef1e11723758d1c515dbacf3c76`.
The restore receipt records Time and Knowledge Ready with all seventeen desired
Deployments and both StatefulSets healthy, and the three Flux holds suspended:
`/tmp/veoveo-time-activation-20261010/restore-final-receipt.json`, SHA-256
`c5ead9cf4fe7fa58ca9e653aaf5d8c8ac5ed750bedcf574ae63b75c3160140d4`.

The isolated first-empty Knowledge inventory passes SurrealDB 3.3 syntax validation
and independent review. It compares eleven owner tables and eight static indexes,
rejects dynamic chunk tables and returns metadata/presence counts without source
payloads. Its database execution, full read authority, stopped-writer fence and
fresh/no-restore provenance remain unproved. Private preparation receipt:
`/tmp/veoveo-knowledge-first-empty-prep-e7n223dx/first-empty-inventory-receipt-v1.json`,
SHA-256 `f17d90b5d71305a6b8af6be1f967d2fccda47a2fae4a597bd31fafd88c642b4c`.

## Installed Time And Preparation Checkpoint — October 10, 2026

At source `c3371e654`, the maintained `time-installed-schedule-task` complete
profile passes in eight seconds through normal public Gateway OAuth. It compares
an independently expected one-row output with delivered and current Completed
observations and closes its owned clients/listeners. The cancel profile observes
early completion and refuses the required Working precondition. It records no
cancel intent and sends no cancel request. Authoritative `tasks/get` reads return
HTTP 200 for both known Tasks, with Completed status, `isError: false`, complete
result type and the same structured one-row output hash. The operation completed
normally. Recover remains held and unrun. The nineteen focused native lifecycle/API controls
keep their recorded scope. Private execution receipt:
`/tmp/veoveo-time-schedule-prep-20261010.SB2nwl/execution-final.json`, SHA-256
`4ed8fe9b6ae107bddd312bf12899fa370ae328ccaac056fefcdd4320f03ca2e1`.

The Time CPU repair runs hosted calculations on the blocking pool, checks
cooperative stops and joins the retained original job before normal worker exit.
Compiler, strict lint, formatting, documentation and logic review pass. Ten focused
native controls cover two responsiveness/retained-job cases, four existing Task
Store cases and four engine cases. Fixed-image installed cancellation and recovery
remain unqualified; incident attribution still requires Ops' process measurements
and correlated logs. Private final receipt:
`/tmp/veoveo-time-cpu-repair-20261010/final-receipt.json`, SHA-256
`4b76078d4ea5626d5ae971e8c9d967646dc4906ac895384094800c4d321dc376`.

Agent Manager is Ready with immutable template `uav-pilot-037b21aaa992`, qualified
kernel `8b0fae16caafa85ce363e94a6843fd8e4901ca386d43d45c88c4c414abeac558`
and matching admission policies. This establishes the instance journey's setup
prerequisite; the full installed journey remains open. The isolated first-empty
Knowledge preparation selects seven kernel lanes plus Time. Its configuration
remains unadmitted, and no isolated startup or authoritative empty-state inventory
has run.

The authoritative `c3371e654` source plan requires thirty Rust targets plus pending
`cuopt-executor`, thirty-one images in total. The prior C5 twenty-seven-of-thirty-four
qualification does not qualify these changed source inputs. Compiler cache IDs
remain stable. Private closure receipt:
`/tmp/veoveo-c337-authoritative-plan-20261010/final-source-closure.json`, SHA-256
`a5b0b22350d3d667682807e8135011f297af78ae90a63063227cdae0d75d6fbb`.

## Consolidation Checkpoints At `c120b9aaa`

The following qualification and installation status was moved from the active plan
at this revision. It records its accepted checkpoints; later work and the active
plan determine which checks still apply.

The latest local qualification batch for Phases 3–5 is complete. Work now integrates
the remaining generated consumers in Phase 6. Native qualification covers the composed
nineteen-owner schema, installation
commands, normalized Rust/Python identity, Task storage and product URIs, owner SQL
exports, contributed Task lookups, and Media's journaled callback/recovery profile.
These source checkpoints preserve policy checks before paging and decoding,
transaction rollback, grant revocation, retained products and subscription recovery.

The storage checkpoint `4fa68eaa9` and upload cancellation fix `acf740abe` pass
190 selected tests across 25 test groups, including native database suites and
independent schema/contract consumers. The storage batch covers:

| Owner | Qualified change |
|---|---|
| Map | Projection recovery resolves retained tenant/context metadata through Identity; corrupt associations fail without advancing the checkpoint |
| Agents | Managed-instance HTTP reads hydrate checked revisions and verify definition and tenant relationships |
| Time | Acquisition phases use one closed owner vocabulary in Rust and the database |
| Computers | Maintenance source, progress and resume records use typed adapters and closed fields; whole-value fences and recovery receipts are preserved |
| Audit | Profile, whole-target and whole-detail lookups are written with the admitted draft and checked on reads; frozen bytes and hashes are preserved |
| Gateway and Artifacts | Profile policy metadata is declared and checked before upload admission and retained access; missing metadata rejects atomically |

Native acceptance also exposed an upload cancellation race between claim commit and
lease-guard construction. The Artifact service now owns the guard before delivering
the claim result and explicitly bounds that wait. All eleven native upload tests pass;
unknown claim outcomes keep their existing lease-expiry recovery. Gateway control
publication now runs through an isolated native fixture instead of silently skipping
its assertions when an environment variable is absent.

Affected compilation, strict lint, formatting, SQL parsing and source policies pass.
The four composed schema plans come from the current Gateway binary. Independent
contract builds exclude runtime dependencies, and the ordinary Store runtime builds
in isolation. Independent review found no unresolved defect in this batch.

Checkpoint `b91815151` implements controlled storage envelopes across
Gateway, Tasks, Agents, Workspace, Frames, Computers and Media, and replaces
Knowledge's nested document queries with declared lookups. The query audit also
covers Optimization's opaque Task reads and Audit block delivery. The combined
seventeen-package all-target compile and strict lint checks pass. Fresh composition
of all nineteen owner lanes, disabled-owner absence, replay and the installation
command lifecycle pass. The runner admits closed object assertions and the pure
functions needed for byte and control-character validation; commit `d05d54fc9`
contains that qualified admission change. Independent source review has no
unresolved finding in these adapters. Owning native suites pass for Gateway, Store,
Knowledge, Task Runtime, Audit, Agents, Workspace, Frames, Computers, Media,
Optimization and Artifact's service. The Rust/Python storage exchange runs within
the passing SDK, template and independent-consumer checks. The final schema consumer
passes against the regenerated nineteen-owner plans. Corrected diagnostic fixtures
retain their corruption, SQL admission and plaintext-absence assertions; production
decoders continue to reject native database values inside JSON payloads.

Checkpoint `e878b4d3a` adds the five retained Task contribution adapters and four
native row shapes listed in Phase 4. Map, Media and Reason preserve complete admitted
result values; Knowledge and Audit bind their own records. Knowledge's generation
requirements now declare the fields read during activation, and Agent chat admission
reads the declared execution projection. Independent review, strict lint and affected
native suites pass. Fresh schema composition and the independent consumer pass
against all four regenerated installation plans. The Reason failure fixture now
clears the product URI when setting a Task to failed, preserving its retained result
and stale owner settlement for the SQL paging check. Source ownership review reconciles
495 production SQL assets across the
optional owners, including variable targets and stored record links. A further bind
review reconciles 1,799 production database-binding call sites and helper calls.
It identifies the three final nominal-adapter families qualified in Phase 4. Source
review also resolves the six outstanding relationship families as retained or
owner-managed lifetimes; their owning designs now state why deletion must not cascade.

Checkpoint `b4740fb7d` qualifies Knowledge's optional-Agent resolver and observation
port, plan-selected startup and read-only module prerequisite checks. Native tests
cover kernel-only operation without Agent tables, managed authority and observed
revocation rechecks, selected prerequisites and preparation fences. The shipping
profile and an isolated server build without the managed adapter pass their startup
checks. Agent registration, installation commands, Helm, independent dependency
profiles and strict lint also pass.

Checkpoint `e2211a9bf` qualifies Media's nominal cancellation receipt and Task-owned
receipt cleanup. Native codec, late-webhook/cancellation and actual Task-pruning
checks pass, along with fresh schema composition and its independent consumer.
Capability contexts and billing usage keep their separate lifetimes. Neither
checkpoint establishes installed acceptance of the consolidated cut.

Checkpoint `91357aeca` qualifies 447 cases across 112 owner-local input families
and all sixteen Rust servers. The shared fixtures exercise published schemas, byte
decoders and existing hosted admission tests. Independent review confirmed the
inventory and preserved original cases. All sixteen owning hosted admission tests
pass, including Recording's explicitly enabled binary target. Both independent-consumer
profiles pass the expanded matrix. Recording completion also passes its SQL-authority
check through the service adapter corrected in `ea413ca14`.

Commit `faf8b9cee` retires the completed pilot migration helpers and their private
installation fixtures. The generic record-restoration check keeps bound native
values and transactional rollback coverage; it and the six Bioma composition tests
pass. Current pilot provisioning and retained-instance lifecycle stay with the
managed runtime.

Media deployment still requires the host tracing correction described in Phase 3;
real provider generation is not part of local acceptance. Computers' real provider
process suites require a configured, qualified native execution profile before
publication.
The remaining [Phase 3](CONTRACT_CONSISTENCY_PLAN.md#phase-3-module-ownership-of-persistence-and-queries) owner
APIs, [Phase 4](CONTRACT_CONSISTENCY_PLAN.md#phase-4-database-field-types) field families and phases 5–10 keep
the plan open. These source checkpoints do not qualify the installation cut.
The cluster stays stopped during development.

The following installed checkpoints establish the accepted Foundations baseline.
They do not qualify the subsequent consolidation changes.

The sixteen Rust servers passed the shared-host installed batches, including
discovery, authenticated documents, completion, Host admission and the requested
Linux suites. The unified audit log, native changefeeds replacing the outbox,
reference cleanup and separated Computer payloads have qualified checkpoints.
Eighteen participating knowledge sources passed their declared checks.

The composed flight passed at `6d4cd2c5`: mission, live Stream, Recording replay
and reconnect, return, landing and Artifact isolation. Headed RTX 4090 WebGL supplied
visual evidence; source-to-viewer lag was 0.198 seconds against the one-second gate.
The installed sixteen-collection Knowledge selection and CUDA embeddings passed.
Knowledge and Embedding are core services. Reason runs only in its separate batch.

Map/UAV route handoffs and Computers grant typing passed native and isolated-contract
checks. Their five images were published from `0843500c` and selected by `60fd2cf8`.
Rollout and a Map route check completed. Installed handoff preparation and grant
consumer acceptance remain open. A subsequent cold start reached Ready without a
manual pod restart, but the full cold-start acceptance has not been recorded as a
pass. Preserve that distinction when resuming qualification.

The [progress log](PLATFORM_FOUNDATIONS_PROGRESS.md) retains dated checkpoints and
failures. Its historical status statements do not override this plan. Accepted
checks are reused while their source, dependencies and execution environment match;
shared changes invalidate the affected checks, not unrelated acceptance.

## Consolidation Baseline At `d75a5acea`

The following status text was moved from the active plan's opening at this revision.
It records earlier qualification and does not override the active plan's current
status or the gates in its individual phases.

Status: Phase 0 is qualified. The declaration repetition
repair is applied across owners and callers and passes native qualification.
Compact ID and resource attributes now generate standard derives, conversions and
convenience methods through owner profiles. Shared tests, the owner-contract aggregate,
both independent consumers, full workspace compilation, strict lint, source-policy
checks and Task Runtime's database integration pass. Phase 0 acceptance requires
clearer authoring, strong types and qualified behavior. Complete-family line counts
remain visible, but net line reduction is not a completion requirement. Shared address
generation uses one typed field model for parsing, builders and accessors; its affected
native, consumer, compilation and repository checks pass.
Vocabulary, embedded documents, Id, ResourceAddress, Checked models and opaque cursors
preserve their owner admission and wire/schema profiles. Production helper and static
unit-error adoption is complete; the tracked-source macro catalog is enforced.
Explicit owner adapters retain normalization, redundant-field projection, mutable
representations and codec-specific envelopes. Later phases, Foundations and hardening
transfer conditions, and installed acceptance remain open. Phase 1 is active: the
module declarations, native runner, execution commands and rendered installation Jobs
pass their native checks. The staged gateway image passes isolated installed checks
for fresh preparation, lane completion, publication, credential rotation, stale Job
rejection and later module enablement. Installed managed-agent replacement preserves
identity and storage, advances the lease fence and survives replay. The extended
three-generation lifecycle passes, including rejection of the old runtime password
after database readiness and owned fixture cleanup. Full Helm qualification and
reference product activation remain open.
Phase 2 Task contributions, versioned kernel SQL admission and
Optimization's catalog reads pass their native checks. The gateway composition split
and catalog, OAuth and TLS adapters pass native checks. Computers, Speech, Recordings,
Agents and Workspace own their HTTP handlers; contributed route authentication and
worker cleanup pass native checks. Agents owns its public token claim; generic
claim admission and internal execution attribution pass Rust and Python checks.
Recording owns its catalog and ingest policy; optional modules own their policy
actions through a shared registry. Native owner, admission and schema tests pass,
including exported-schema validation of all five installation catalogs. Affected
consumer checks and independent contract/runtime builds pass. Installed catalog
startup admission remains open. Agents' authoring and operator-control models and the
dependent Workspace contracts have moved to their owners. Native tests, affected
consumer checks and isolated contract/adapter builds pass; generated browser schemas
are unchanged. Phase 3 is active: owners now declare their observation tables and
Computers owns its change decoder. Native delivery, replay, stopped-reader recovery,
listener cleanup and schema retention checks pass. Independent schema consumers and
the affected runtime graphs pass qualification. Migration admission now follows exact
function versions and preserves stored read-only callers across definition changes.
Its native schema, transaction and recovery checks pass. Production schema admission
and fresh lane execution pass for the kernel, each extracted repository's selection
and the complete selected catalog, including unchanged replay, disabled-owner absence
and reconnect. The owner-registered Audit target path passes native
checks for reconnect, filtered reads, LIVE delivery, transaction rollback and sealed
export while preserving record bytes and hashes. Agents, Workspace, Map and Recording
persistence has moved into its owners. The combined test build passes, and every
previously failing aggregate test passes in its owning-suite or focused rerun. The
Agent event-stream failure did not recur individually or in its full gateway suite;
its diagnostic now captures the unexpected stream outcome. Real installation commands
pass fresh preparation, selected lanes, publication and stale-generation refusal.
Time, Frames, Media and Agent execution query extraction passes syntax and
statement-equivalence checks. The affected consumer builds, strict lint across
28 packages, 13 isolated dependency profiles and the normal Store runtime build pass.
Generated Audit readers and Console TypeScript pass their checks. Python Task query
assets, packaged-wheel loading and the fresh kernel-lane fixture pass native and
consumer checks. This qualifies the composed persistence batch for a source checkpoint;
runtime kernel access and installed acceptance keep Phase 3 open.
Five additional browser contract
bundles pass generation, consumer tests and builds. Production schema ownership moves
in Phase 3.
Phase 5's MCP input batch closes controlled request fields and nested owner shapes.
The independent consumer checks all 114 production tool-input root schemas; affected
native suites and browser/Python consumers pass. A separate batch closes controlled
HTTP bodies, registered installation configuration and private process inputs.
Affected native suites, Python peers, generated browser consumers and Stream's C++
build pass. Recording's sensor-stack loader now closes its flat variants and validates
sensor IDs during decoding. UAV state, acknowledgements, completion results and events
use typed Python output models. Authenticated native wire checks now cover unknown
arguments on all sixteen Rust servers. Map, cuOpt, Reason, Speech and UAV compare
their complete private JSON protocol graphs across Rust and Python. Exhaustive
controlled-variant coverage and installed process qualification remain open. A
52-branch batch qualifies Computers file transfers, Map spatial and authoring inputs,
Optimization sources and View overlays through independent schema/byte decoding and
authenticated hosted rejection.

## Implementation And Installation Checkpoints

Typed route handoff and automation-grant batch (2026-10-04): Map commit
`01476095` introduces a checked owner-library builder and decoder for route handoffs.
Route/profile addresses, digest and provenance IDs stay typed through Map preparation
and UAV admission. Map checks path bounds, coordinates, state, unique provenance and
timestamp order. UAV keeps current vehicle/profile/advisory grants, ellipsoidal height,
freshness and motion policy. Retained Map route reads reject a different route URI.
Computers grant inputs, views and client choices carry foundational principal/client
IDs. Checked results derive their typed address from the grant and reject mismatched
Computer or grant identities on decoding. Domain, MCP, HTTP and gateway consumers use
those values; generated Console schemas/types and UAV schema snapshots are updated.

One implementation pass covers producers, consumers and fixtures. The aggregate build
finds two stale Computers HTTP fixture assertions; their correction passes the same
build selection in 11 seconds. The native execution batch passes 304 cases, including
75 UAV cases, thirteen Computers HTTP cases, sixteen gateway cases and 75 flight-harness
consumer cases. Separate contract-only profiles pass eleven and twelve cases, and
44 documentation examples/compile-fail cases pass. Strict Clippy, Console type checking,
31 browser unit tests and generated-client consistency pass. The native provider command
suite compiles; its separately provisioned execution is not repeated in this batch.
Logs, exact command selections and result manifests are under
`output/development/map-uav-computers-types/`. The five affected images publish from
`0843500c` in 279 seconds; `image-release.json` records runnable and publication digests.
The reference locks select Map, UAV, Computers, gateway and Console together. Helm
configuration passes. BuildKit stops after publication; installed acceptance is pending.

Composed flight and core Knowledge acceptance (2026-10-04): reference revision
`6d4cd2c5` selects Stream `sha256:8ca9f9cb4a307a530c98cd77238cbafa05465aa5f55496d077d6ca83e3e80ea2`
and Reason `sha256:83ac3b3d987582c7afca39e1e27138ea7e4703c9130eb8f25ce013992d45944d`,
built from `27046a2d`. Reason stays at zero replicas and has no new GPU acceptance.
Publication takes 594 seconds, including image assembly, SBOM and provenance.
The reader correction passes deployed Stream replay over recording
`01a10446-6ff0-7dd3-901e-499bddad5b4c`, simulation time 1089599999999–1114600000000.

The complete composed run passes both domain and visual phases. It rearms, takes
off, completes the Map mission and live Stream check, replays the recording, returns
to the launch surface and lands. Artifact preview succeeds for an authorized context
member and denies an independent context. Rerun follows for 120 seconds and reconnects;
source-aligned lag is 0.1975982406 seconds. Headed Chrome uses RTX 4090 WebGL;
SwiftShader WebGPU is excluded from hardware evidence. Server encoding uses NVENC.
The client declares supported, smooth software H.264 decoding. All three flight
captures have zero dropped video frames in their samples; landing altitude is 0.04 m.
Phase-outcomes v3 now preserves successful visual measurements if another phase fails.
The native report batch passes 75 flight cases, ten Helm cases and configuration/docs checks.

The full installed Knowledge harness passes in 28.47 seconds: sixteen selected
collections, their completion/catalog/statistics, subscriptions, source-linked search,
eleven verified document links and 1024-dimensional CUDA embeddings. Knowledge and
Embedding are Ready alongside the simulator at final observation. This startup needed
an embedding retry and a Knowledge pod restart to clear dependency backoff. The first
embedding process exit has no retained error log, so its cause is unconfirmed; the
previous startup's KV-cache admission failure has its own checkpoint. This run does not
qualify unattended cold startup. Terminal GPU admission failures from node startup
were recorded and removed after replacement pods were running.

Logs and reports are under `output/development/reader-installed-73cc62fb/`.
Flight artifacts are in its `6d4cd2c5afb0001a51f795488367462a3ec27527/01a1044d-1ccf-7f13-b952-cda0a649bb9c/`
subdirectory. The owned log follower and cluster are stopped after acceptance;
BuildKit stays stopped. The complete-flight deferral is closed. Phase 3 and the
remaining installed/recovery inventory remain open.

Composed visual acceptance and core Knowledge checkpoint (2026-10-03): simulator
source `3f10a3c3` is published and selected through `cdcafcd6`. The full flight rearms
a grounded vehicle in Land mode, takes off, completes mission and live Stream checks,
and lands during cleanup. Its headed RTX 4090 WebGL visual phase passes, including
Rerun playback/timing and landing capture. The phase outcomes file records that pass.
The detailed successful visual report is lost when the domain phase returns its
error, so this checkpoint supplies no new numeric lag measurement.

The domain error comes from `stream__run_recording` with eight maximum frames and
source clip enabled over simulation time 345633333333–370633333333. The recording
is `01a1040c-c669-7c82-a336-a3203662eadf`, using the UAV down camera. The Task fails
with `video materialization failed`: copying ingest part 709 returns OS error 2.
Hub commits the layer and removes live parts between the reader's inspection and
copy. Requests, phase outcomes, captures and Stream/Recording/Hub/Gateway/simulator
logs are under `output/development/px4-rearm-installed-3f10a3c3/`.

The reader correction opens each acknowledged part once, admits its length against
the cumulative byte limit, and copies through that handle. It validates and hashes
the copy before normalizing the disposable RRD. The reader rechecks live catalog
states after copying, including empty directory reads. A confirmed publication
causes a fresh authorized plan with Artifact-backed committed layers, up to three
attempts. Other failures stay terminal. Sixteen reader and five Video native cases
pass, including unlink, growth, truncation, producer identity, cache authority and
codec/sample joining. Strict Clippy passes for the selected reader, Video and
Knowledge targets. Publication and installed rollover acceptance are pending.
Native logs are under `output/development/recording-reader-rollover/`.

Reference commit `91bcf2bf` makes Knowledge and Embedding core running services and
keeps Reason at zero replicas. Sixteen selected collections span Artifact, Charts,
Map and Time. Normal operator OAuth source catalog, Map document search and one
1024-dimensional embedding pass; Knowledge, Embedding and UAV are Ready together.
The first embedding cold start rejects insufficient KV cache space while the other
GPU workloads start. Its next start admits the unchanged 32768-token context and
0.25 memory utilization; no CPU fallback or GPU request reduction is introduced.
The existing installed Knowledge harness now expects indexing-client selections
rather than all nineteen server approvals, and searches only selected documents.
Its full installed run is pending. Baseline rollout and focused requests are under
`output/development/knowledge-core-baseline/`. During editing and native checks the
cluster and BuildKit are stopped; the saved running configuration keeps both core
Knowledge services enabled.

Recording rollout and PX4 rearming checkpoint (2026-10-03): Recording and Console
from `f681a23d` are published and selected through `59077ec0`. Console was drained
for the coordinated playback-manifest v10 upgrade. Both deployments reached Ready,
GitOps converged on that revision, and both reconciliation controllers were restored.
Only the Recording and Console image selections changed. Ten rollout cases and
Helm configuration passed. Normal operator OAuth reads of Recording's contract and
agents document passed.

The composed flight at `59077ec0` passed Map and live Stream prerequisites, then
failed before browser capture. Preflight landed the vehicle and issued another Land
command after touchdown. The subsequent `uav-sim__takeoff_vehicle` call for 197 m
returned MCP `-32602`, wrapping adapter HTTP 409 with
`PX4 command deadline expired; inspect vehicle state before another mutation`.
PX4 repeatedly rejected arming; its status reported disarmed, Land mode and no
failsafe. The vehicle was on the ground with no collision. This run does not qualify
the Recording rollover correction or final visual acceptance.

The adapter's mode transition depended on whether this commander had observed
`IN_AIR`. The correction uses the reported ground state and Land mode, then waits
for Loiter before requesting arming within the existing deadline. The unit batch
initially passed 126 of 127 cases; the older rearming fixture lacked a reported Land
mode. After correcting that fixture, all five commander cases pass, giving coverage
of all 127 cases. The first hardware run showed that a normal landing returns to
Loiter, so the regression now reproduces the extra Land command after reconnecting
a commander with no flight history. That RTX 4090 CUDA/PX4 regression passes both
takeoff, movement and landing cycles in 140.807 seconds, including observed grounded
Land mode before the second arming request and healthy estimator checks after both
flights. The test shuts down its owned PX4 process and sensor publisher. Simulator
publication and installed composed acceptance remain open.

Deployment requests and pod logs are under
`output/development/recording-rollover-installed-f681a23d/`; native regression logs
are under `output/development/px4-grounded-rearm/`. The cluster and BuildKit are
stopped during development, with 316 GiB free. The OCI registry stays running.

Installed source alignment and rollover checkpoint (2026-10-03): focused headed
Recording acceptance at `04cf2ed2` passes 120-second stability, reconnect, advancing
camera content and source-to-viewer lag of 0.1704809477 seconds. Chrome uses RTX 4090
WebGL; WebGPU reports SwiftShader and supplies no hardware evidence. The accepted
source bracket encloses the browser observation without extrapolation.

The subsequent composed flight passes Map and live Stream prerequisites, takeoff,
mission capture and Stream replay of simulation seconds 750.7–775.7. Its Rerun
stability check fails before source alignment: the viewer host disappears and Console
shows loading. At 22:20:37 UTC, a successful playback-manifest refresh overlaps the
transition to capture layer ordinal 2. The service can return `live: null` while the
recording is live and its previous layer has left Writing. Console then unmounts the
viewer. The new native rollover case reproduces missing channel admission before any
layer exists. This is distinct from the corrected source-sampling race. Postflight
cleanup lands the aircraft and closes its owned sessions; composed visual acceptance
has not passed. The cluster and BuildKit are stopped with 334 GiB free.

Playback manifest v10 separates the recording-scoped live receiver from private
capture-layer selection. Live channels admit startup and rollover gaps, and idle
channels observe lifecycle changes. The grouped contract, Recording and Console BFF
run passes 200 native cases with none ignored. The startup/rollover regression also
checks denied callers and idle-channel closure on recording interruption. An initial
compile caught a Store/domain layer-ID mismatch; the private plan now preserves the
domain ID. Console passes 108 unit cases, one headless receiver-ownership behavioral
case, frontend build and lint. That renderer test double supplies no visual evidence.
Documentation checks pass. Recording and Console need coordinated deployment
and managed-tab reload under the domain contract's upgrade profile. Native outputs
for the correction are under `output/development/recording-live-rollover/`; installed
requests, screenshots and pod logs for the preceding run are under
`output/development/source-timeline-bracket/`. The initial cold start needed an
existing readiness wait and a second showcase-up invocation; all required pods then
became Ready. No workload image or permanent replica count changed during that run.

Browser source alignment checkpoint (2026-10-03): focused Recording and composed
flight acceptance share the sampler in `testing/browser-smoke/src/source_timeline.rs`.
It admits the running lifecycle through UAV's enum, validates source timestamps and
simulation time, and retains the closest sample preceding the browser observation.
Cached samples can advance within a ten-second deadline that includes reads and
100 ms waits. Backward clocks and invalid state fail. Both timestamps must bracket
the observation before interpolation; playback still must trail the source by zero
to one second. Composed evidence v6 includes the source bracket and fraction.

The grouped browser/flight native run passes 127 cases, including the actual failed
timestamp sequence, repeated cached samples, malformed state, clock regression and
a read that never responds. Extraction initially left the timestamp private from
the existing camera-cadence consumer; its crate visibility is corrected. This batch
changes neither a deployed image nor shared-host code. Headed installed playback
and composed landing visual acceptance remain pending. Native outputs are under
`output/development/source-timeline-bracket/`.

Installed gateway expiry and flight checkpoint (2026-10-03): source `ae638056`
publishes in 96 seconds with cache reuse, and gateway-only selection `5c6639fc`
converges in seven seconds. Ten rollout tests and Helm configuration pass. An ordinary
OAuth credential returns 200 immediately before its signed expiry, then 401 at two
and forty-one seconds afterward with the existing challenge and body. Gateway logs
report `ExpiredSignature`; a fresh credential returns 200. This closes the admission
failure reproduced by the preceding native checkpoint.

Installed verification passes all 24 declared deployments, public Console/OAuth,
DuckDB execution and large Artifact delivery. Discovery validates 111 tool schemas
while Reason and Knowledge stay deliberately stopped. Admin server health returns
all nineteen states and check times; the operator receives 403, and both admissions
appear in the tenant audit view. DuckDB and Timeseries complete `de` to `design`.
The original oversized Computers request returns HTTP 413. Initial raw requests
without MCP method headers and an unsupported completion CLI flag were corrected;
those request mistakes did not establish product failures.

The composed UAV domain scenario passes Map routing, mission execution, live GPU
Stream inference, Recording replay, return to launch, landing and Artifact access
isolation. Its owned cleanup completes. The separate headed Rerun check renders the
fleet, aerial camera and map with RTX 4090 WebGL, but rejects its source bracket:
the immediate after-sample timestamp precedes the browser observation by 105 ms.
This exposes the existing sampler's timing race; it does not determine playback lag
or establish a product regression. The sampler needs a later authoritative sample.
The one-second lag limit and composed landing visual requirement stay open.

Requests, responses, screenshots and pod logs are under
`output/development/gateway-expiry-installed-ae638056/`. WebGPU reports SwiftShader
and contributes no hardware evidence. Temporary token files are removed. The cluster
and BuildKit are stopped after acceptance; useful caches and required images remain.

Gateway access-token expiry checkpoint (2026-10-03): signed-token and actual HTTP
middleware regressions reproduce the installed failure before the correction. A token
expired 41 seconds earlier reaches the test handler with 204 because JWT validation
accepts its default sixty-second grace period. The downstream assertion issuer still
requires an expiry in the future. Access-token verification now checks the signed
expiry immediately after signature/claim validation, before constructing authority.
It rejects the exact expiry second and tokens inside the former grace window.

The grouped native run passes 202 cases across the gateway library, binary and
cross-replica session-family integration. The HTTP regression checks a valid control,
four expiry cases, the 401 challenge/body and four committed authentication denials
read through the second Store client. One existing native blob test stays ignored;
this change does not touch its path. Strict Clippy and documentation checks pass.
Neither shared-host implementation changed. Outputs are under
`output/development/gateway-expiry/`. The correction still needs
publication and installed acceptance with an ordinary OAuth credential.

Installed typed-dispatch checkpoint (2026-10-03): Map, Knowledge and Computers publish
from `e1c5d3d7` and converge through `c9b590ac`. Publication reuses BuildKit and takes
194 seconds. Ten rollout tests and Helm configuration pass. All three deployed servers
pass discovery, documents, document completion, authenticated admin docs, probes and
Host rejection. Invalid resources return -32602. Admin server health returns 200;
the operator is denied with 403, and an anonymous resource read returns 401.

Map passes all 35 source checks, including owned changes, publications, release
activation/restoration and restart probes. Fourteen catalog roots, typed release
cursors, filtered publication features and Artifact reads pass. Knowledge passes its
nineteen-collection catalog/search/subscription/embedding scenario across five sources,
with thirteen verified source links and CUDA execution. Computers reuses the owned
fixture for real start, execute, 25-byte file export and integrity verification,
completed Task notification, stop and grant revocation. Cancellation of a completed
Task succeeds; this does not qualify cancellation during provider execution.

A recently expired OAuth token fails upstream discovery with MCP -32603 and
`internal token expiration is not in the future`. Fresh credentials complete cleanup.
The gateway image did not change in this batch; its expiry admission needs a native
regression and correction. The CLI captured the protocol error and pod logs, but no
HTTP status for that request. A later, long-expired token returns 401. The unchanged
Computer guest template also logs a sitecustomize permission warning while successful
commands still return exit zero. An initial malformed Python command and a Host value
without the configured port were request mistakes; corrected requests pass.

Temporary Flux overrides require suspending the parent Kustomization before its
HelmReleases. The reference runbook now states that order. Source fixtures are cleaned,
the Computer is stopped, its temporary grant is revoked and token files are removed.
Declared replicas and reconciliation are restored before stopping the cluster.
BuildKit is stopped, the registry is retained, and 357 GiB remains free. Requests,
responses, native outputs and pod logs are under
`output/development/typed-dispatch-installed-e1c5d3d7/`. Broader recovery, remaining
Phase 3 types and composed hardware flight/playback acceptance stay open.

Map address and reader checkpoint (2026-10-03): `MapAddress` admits the owner product,
paging and filtered-feature families alongside direct resources and knowledge.
Readers dispatch the admitted typed target. `MapCatalogPage` owns operational cursors,
including collection and dataset-parent checks, and catalog APIs preserve each position
ID until Store binding. The shared host and SQL admission statements are unchanged.

The native batch passes 166 tests, including catalog visibility, wrong-parent cursors,
source and authoring spatial recovery, and both million-feature spatial checks. Strict
Clippy passes all Map targets and features; the contract-only library builds without
the runtime. Two compilation corrections fixed a borrowed URI parameter and conversion
at a test's database binding. Logs are under `output/development/map-typed-dispatch/`.
The installed source scenario remains pending. Publish Map together with the qualified
Knowledge and Computers handler changes, then run the affected installed checks.
The cluster and BuildKit stay stopped during development.

Typed server dispatch checkpoint (2026-10-03): Knowledge and Computers replace their
dynamic catch-all tool routes with RMCP handlers whose parameter types also generate
the input schemas. Knowledge keeps its declared scope, current-policy admission,
sixty-second domain deadline and delivery-time authority check. Computers checks Task
capability and retention metadata before reserving work, then uses current domain
authority for retention adoption and the Task reply. Automation and access handlers
call their application commands directly. Neither shared-host implementation changes.

The native batch passes 57 cases across the two packages. Existing application,
lifecycle, command, file, grant, subscription and Knowledge policy/indexing cases pass.
The extended wire case rejects malformed arguments for all ten Computer tools, rejects
malformed retention before any reservation and reads the requested pin from the other
Store replica. Knowledge's two tools reject malformed arguments and retain the current
revocation check during embedding. Seven installed, GPU-evaluation and native-provider
cases remain explicitly ignored in this batch; it does not qualify their environments.

Two initial assertions were corrected: typed RMCP extraction returns a completed
`isError: true` validation result, and domain admission retains its own operation pin
alongside the requested pin. The final wire case passes. The 23 already passing cases
were reused while the remaining cases ran. Logs are under
`output/development/typed-domain-dispatch/`. The cluster and BuildKit stay stopped.
Installed qualification follows the next composed publication. Map's host address
coverage and its URI-based read dispatch are the next owning contract gap.

Installed gateway HTTP rejection acceptance (2026-10-03): gateway source `e1969257`
publishes runnable image
`sha256:d3a116c7b331a69a2127da1235ad4bd80571a3645cdbe8871b3110e83a6541be`.
The gateway-only lock change at `a5db6d66` converges in 62.4 seconds. Its pod and
Computers are Ready. Replaying the original 2,097,630-byte `computers__create`
request through operator OAuth returns HTTP 413 and the original 2 MiB limit reason.
The maintained conformance client's discovery and resources commands pass, including
111 valid tool schemas and prompt discovery while Reason and Knowledge stay stopped.

Requests, responses, deployment state, native results and pod logs are under
`output/development/gateway-http-rejections-917e7914/`. This closes the installed
body-limit forwarding defect. The source-isolation warnings name the deliberately
stopped services; a Recording ingest 503 during cluster startup is also retained in
the logs and does not establish a composed recovery pass. The cluster and BuildKit
are stopped after acceptance, with about 393 GiB free and useful caches preserved.
Other SDK consumers keep their existing image pins and require qualification when
rebuilt. No real Media generation ran. Phase 3 implementation, domain recovery and
composed flight/playback gates remain open.

Gateway HTTP rejection qualification (2026-10-03): RMCP stable 3.5.0 is pinned through
fork `917e7914`, retaining exact Task subscriptions and adding typed non-MCP HTTP
rejections. Rig `4125e888` selects the same SDK revision. The lockfile contains one
RMCP version. SDK qualification passes 453 tests. Rig's native agent suite passes
584 tests; two provider cases requiring API keys stay explicitly ignored.

Gateway handlers retain protocol errors separately from HTTP rejections until their
final response. An absorbed discovery failure leaves a successful partial list at
200. A definitive HTTP rejection does not reconnect and retry. The production
handler regression sends simultaneous oversized and small calls: one returns 413,
one succeeds, and the upstream observes one dispatch per request with only the small
mutation executed. Domain error text and data cannot select an HTTP status.

The combined gateway, MCP contract and Task-runtime batch passes 467 tests, including
integration and compile-fail cases. The ignored native blob case also passes when
explicitly run against SurrealDB 3.3.0 extracted from the existing pinned test image;
the ambient CLI does not meet that fixture's version assertion. Strict workspace
Clippy passes every target and feature after removing an unused Recording entrypoint
constant. Neither shared-host implementation file changes. Logs are under
`output/development/gateway-http-rejections-917e7914/`. The cluster and BuildKit stay
stopped during qualification; gateway publication and installed 413 acceptance remain
pending. Later SDK consumers need qualification against their newly selected images.


Console restart acceptance (2026-10-03): the parent route retains the selected App
through incomplete discovery in `7583087e`. A Console-only publication and GitOps
update at `f1282cf0` select image
`sha256:e5a4cd158353e08d94b6e7ed59a8ea6cac817ad5e8f04fb5b3db3592f6482b11`.
The production Console regression fails before this correction and passes afterward;
108 unit checks, the production build and lint also pass.

The maintained restart harness at `b63d85a2` passes both the MCP-container and
simulator-container restart stages. Each preserves its App document epoch and viewer
ID, obtains a fresh live-view ID, recovers detailed 1280 by 720 video and passes the
existing cadence and delivery checks. Headed Chrome uses hardware RTX 4090 WebGL.
WebGPU exposes SwiftShader and is excluded from hardware evidence. The UI declares
software H.264 client decoding and NVIDIA NVENC server encoding.

The earlier attempts expose an initially warming camera and uniform frames during
world startup. The harness now waits for healthy cameras and detailed frames within
the existing deadlines; malformed samples and failed camera states still reject.
Its 119 browser/flight native tests pass. Each successful stage writes its own result
before the next restart begins. The final run is
`01a10345-3d32-7693-bbb9-b2fa9c4b9110`; results, captures and pod logs are under
`output/development/shared-host-console-route-7583087e/`. The cluster and BuildKit
are stopped after acceptance, with useful caches preserved and about 433 GiB free.
Computers HTTP rejection, composed flight and broader recovery requirements remain
open. No Media generation ran.


Hosted acceptance follow-up (2026-10-03): ten affected images publish from
`0b50d664`, and the platform chart and image locks converge at `c88bcca5` in
45.7 seconds. Other image pins stay unchanged. Reason, Knowledge and embedding
remain at zero replicas. Six readiness routes return 200 alongside liveness;
their deployments use the readiness paths and the gateway reports fresh healthy
states. Recording's two admin document reads return 200. The previously failing
empty-constraint Optimization Task completes on the GPU with objective 10 and
independent verification. Native qualification includes 54 hosted binary cases,
the GPU model families and nine empty-matrix cases, seven executor Python tests,
108 Console tests, the production frontend build, Helm configuration and docs.

The installed Console restart still fails after the native lifecycle correction:
video recovers at 1280 by 720, but document epoch and viewer identity change. The
simulator-restart stage is not reached. UAV MCP exits with code 0. The first
attempt stops at the singleton-pod check because failed startup pods remain;
removing those terminal objects allows the second attempt to reach the actual
continuity failure. Computers' oversized request still returns HTTP 200 / MCP
-32603 with an upstream 413. Neither failure is accepted. Requests, responses,
rollout state and pod logs are under
`output/development/shared-host-followup-0b50d664/`. The reference cluster and
BuildKit are stopped after acceptance; useful build caches and required images
are preserved. Media's real-provider generation remains unexecuted.

Status: Phase 0 is accepted. Phases 1–3 have the installed gaps listed under
Deferred Work and the remaining type work in the Phase 3 migration inventory.
Phase 4's installed audit checks pass. Composed flight domain checks pass at
`eeaa8442`, including Recording replay and grounded Reason. Final acceptance remains
open for Rerun timeline/playback and landing visual checks. Stream App startup and
its headed capture pass at `bf0fe31d`. Phase 5's native consumer migration,
writer/schema removal and private payload separation pass. Its published deployment
passes installed certification, Artifact byte delivery, Speech CUDA, Recording replay,
Reason grounding and Stream inference. Stream Task/result delivery now passes across
installed replicas. Live-session notifications pass through the public Gateway; the
remaining domain gates are open.

Task input-response batch (2026-10-03): Rust public updates carry the owner query
into each answer's write transaction. The parent guard rechecks owner, profile,
clearance, operation and optional Work Context alongside server and status. Input
writes require matching Task and request-key identities. Trusted workers share the
transaction under their server/status profile. Store's primary transaction-error
selection preserves a guard rejection rather than retrying its masked error.

Strict workspace all-target/all-feature Clippy passes. The grouped Task runtime,
Media, Frames and Timeseries run passes 198 checks, with none failed or ignored.
Five new public integration cases qualify fifteen interleaved authority/status
changes with rollback, independent RocksDB answer races, denied malformed Tasks,
cancellation/completion and damaged input identities. The first run exposed masked
transaction errors; the complete batch passes after adopting the shared extractor.
SQL validates on the pinned 3.3.0 image. Logs are under
`output/development/foundations-task-input-20261003/`. Publication and installed
acceptance remain open; the cluster and BuildKit stay stopped.

Native Task API batch (2026-10-03): shared runtime reads, claims, transitions,
cancellation, input exchange, retention and worker APIs take the foundational Task ID.
Domain workers and callers preserve that type into the runtime; MCP adapters admit
wire handles separately. Speech converts native IDs into its transcription identity
without a text round trip. Frames and Timeseries delegate public Task reads and
subscriptions to shared SQL owner selection. Their cancellation paths and Media's
provider cancellation recheck caller selection in the shared write transaction.
Native admission checks RFC UUIDv7 variants for typed callers and creation.

Strict workspace Clippy passes across every target and feature. The five-package
native batch passes 202 checks, including twenty compile-fail examples, with no ignored
cases. The independent contract consumer passes ten checks and imports the new Speech
conversion without service dependencies. Documentation links pass. The first native run
hits a Docker creation timeout while the disk flushes build output; after writeback
settles, the complete batch passes from the same compiled artifacts. Removing 84
superseded test executables recovers 30.7 GiB while preserving libraries, incremental
state and BuildKit caches. About 79 GiB is free. Publication and installed consumption
remain open; the cluster and BuildKit stay stopped. Logs are under
`output/development/foundations-native-task-20261003/`.

Analytical request batch (2026-10-03): DuckDB owns the inline/HTTPS tabular source
profile and composes it with authenticated Artifact input in its complete source type.
Timeseries consumes the implemented tabular profile, so its schema and decoder reject
Artifact input before Task creation. Forecast columns, horizons and training filters
use checked types and constructors. SQL selects finite observations before decoding,
with source row positions preserved. All 121 native checks pass across DuckDB,
Timeseries and the agent kernel, including isolated Store usage and forecast execution.
The contract-only run passes 59 checks, including sixteen compile-fail examples; the
independent consumer passes ten default-profile and eleven knowledge-profile checks.
Strict all-target Clippy passes for the hosted batch and both runtime-only libraries.
Schema snapshots cover 38 DuckDB public types and the forecast request. Publication
and installed checks will join the next composed contract release. The cluster and
BuildKit stay stopped. Logs are under
`output/development/foundations-analytical-contracts-20261003/`.

Scope adoption batch (2026-10-03): all sixteen Rust MCP server libraries use
owner-local `scope_enum!` declarations, including empty vocabularies. Checked setup
reads the declaration's `ALL` slice. Empty vocabularies reject every name and expose
an uninhabited schema. Recording owns a separate producer enum for ingest and
publication; its OAuth client keeps that type until wire serialization. Hub checks
the same ingest permission. Gateway composes the owner's required-permission check
when constructing, publishing and loading a catalog; MCP core contains no Recording
scope spelling or new domain dependency. The independent consumer now imports all sixteen server contracts.
The shared contracts pass 112 native checks, and the independent consumer passes
nine default-profile and ten knowledge-profile checks. The five-package native batch
passes 444 checks with one measurement ignored; its environment-gated control-store
integration case performs no work without explicit opt-in. After the persistence
admission change, all 117 Gateway library checks pass, including a disposable-Store
regression proving denied producer scopes write no revision or active pointer.
Strict workspace Clippy passes across all targets and features, and documentation
links pass. Publication and installed producer acceptance remain open; the cluster
and BuildKit stay stopped during development.

Flight batch checkpoint (2026-10-03): `bf0fe31d` converges to the selected 24 ready
deployments with Reason, Knowledge and embedding stopped. Stream App capture and
Recording replay pass. Rerun connects and receives 58 frames, but the failing visual
checkpoint has no timeline name or time updates. Its capture acknowledgement fails,
owned cleanup lands the aircraft, and the cluster stops. Both composed phases report
failed, with Reason explicitly `not_run`. The observer records 997 samples over
501.68 seconds with no read failures. The viewer or harness cause is unresolved;
resume its diagnosis after the grouped contract work. This deferral preserves the
playback, camera freshness, spatial-content and headed hardware requirements.
Artifacts are under the resource batch's `flight-batch/` directory.

Reference resource decision (2026-10-02): the user permits separate service batches
and explicitly leaves Reason off. Reason implementation, source conformance and GPU
acceptance stay in scope, but simultaneous residency with the complete deployment is
removed from this plan's acceptance. The reference HelmRelease holds Reason, Knowledge
and embedding at zero replicas while preserving their checkpoints and caches. Flight
batch readiness expects the other 24 deployments. Composed flight verifies live
Stream, Recording replay, Artifact
isolation and headed hardware visuals; its reports explicitly mark Reason `not_run`.
The existing focused `uav-recording-verify` owns grounded Reason acceptance. Start only
its required services for that check and stop Reason afterward. This decision
supersedes earlier full-deployment residency requirements and checkpoints below.
The separation passes 71 native flight checks with one timing measurement ignored,
strict Clippy and the complete Helm configuration check. Rendering the reference
post-render patch verifies stopped batch services, retained PVCs and their NVIDIA
request. Stream publishes from `d8431efa` in 113 seconds, preserving the C++ runner
cache. The reference selects its runtime digest; BuildKit stops before installation.
Installed flight repetition follows this checkpoint. Publication metadata is under
`output/development/foundations-stream-separate-reason-20261002/`.

The first installation attempt confirms that Knowledge's configured-source discovery
does not become ready with Reason unavailable. The rollout is stopped before simulator
startup. Knowledge and its embedding runtime therefore join the separate batch;
Knowledge acceptance starts its approved sources and preserves the complete-discovery
requirement. This follows the user's service-batching decision and changes no model
precision, context length or readiness check.

Map installation checkpoint (2026-10-02): GitOps converges at `9d7d0b6a`, with all
27 deployments ready and eight GPU shares. The composed run passes Map and Stream
prerequisites, recovery landing, re-arm, controlled takeoff, Map handoff and mission
execution, then Recording replay. Takeoff and mission camera captures show the city
through the headed hardware browser. Across 1,565 state samples over 785 seconds,
reads never fail and the maximum reported render cycle is 757.72 ms.

The run fails two later checks. The Stream browser client clicks Start after pipelines
load but before the existing session page arrives; the server rejects a second active
session. A dropped visual-capture sender also passes the domain's timeout-only check,
allowing replay and Reason to proceed after the browser failure. Live Monitor now
disables Start during initial discovery and an outstanding start, or when its page
already contains an active session for that pipeline. The browser client waits for
enabled controls, and all three flight holds require a successful acknowledgement.
Native qualification passes 120 browser/flight harness cases, with one measurement
case explicitly ignored. Both Stream App behavioral cases and the complete Helm
configuration check pass. Installed repetition of this follow-up is pending.

Reason separately refuses startup with 5.48 GiB free against its configured 9.86 GiB.
The full deployment uses about 18 GiB before Reason; the simulator and embedding
processes account for 6,478 and 6,056 MiB. This identifies the capacity limit behind
the failed run. The reference resource decision above removes simultaneous Reason
residency from the required profile. The cluster and BuildKit are stopped after
owned landing cleanup.

Postflight and settled PX4 snapshots have no sensor health warnings/errors or compass
fault, and magnetic heading is consistent. They permit arming in takeoff mode. The
false `pre_flight_checks_pass` value reports arming eligibility in the current Land
mode: the pinned [Commander implementation](https://github.com/PX4/PX4-Autopilot/blob/d6f12ad1c4f70ad3230afd7d86e971421e02fef4/src/modules/commander/Commander.cpp#L1714)
computes it for `nav_state`; the observed mode is Land and its armable bit is clear.
This resolves the previously unexplained postflight flag without changing a PX4 check.
Results and diagnostics are under `output/development/foundations-map-route-20261002/`.

Camera deployment checkpoint (2026-10-02): reference revision `8a7edc9a` selects the
`eae9d400` demand-allocation runtime, companion image and chart. GitOps converges and
all 27 deployments report ready. During 639 seconds of fleet flight and landing,
1,272 state observations have zero read failures and the stream product stays ready.
The runtime reports a maximum render cycle of 726.24 ms and maximum native update of
664.10 ms. These observations do not establish complete visual acceptance: after
landing, PX4 rejects another takeoff with a persistent EKF compass fault, and the
composed test stops before its visual/playback checkpoints. PX4's sensor topic is
fresh with zero read errors; its estimator reports `cs_mag_fault` and inconsistent
magnetic heading. The cluster and builder are stopped after collecting diagnostics.

The CUDA plant replaces its fixed zero-declination field with PX4's own pinned World
Magnetic Model tables, evaluated at each vehicle's GPS position. Independent reference
vectors, body-frame conversion, noise distributions and stationary PX4 validators
pass. A native flight harness reproduces the compass failure after about 65 seconds
despite the field correction. Its ULog shows an emergency yaw reset and saturated
accelerometer-bias estimates. The sensor model also copied SIH's per-sample IMU noise
from the pinned 250 Hz profile into 30 Hz independent samples. The correction scales
IMU noise to preserve integrated variance per second; PX4 health checks stay unchanged.
All eight hardware CUDA/native PX4 checks pass in 169 seconds, including two takeoff,
horizontal-movement and landing cycles, re-arm, postflight health, and equal integrated
noise variance at 30, 60 and 250 Hz. All 125 runtime unit tests pass. Table regeneration
matches the checksum-pinned upstream header, and documentation links pass. This
qualifies the isolated sensor/flight batch. Reference revision `c9704daf` selects both
images published from `20b120c0`. All 27 deployments become ready, and the installed
run passes Map and Stream prerequisites, recovery landing, re-arm and takeoff. Its
1,099 observations over 552 seconds have no read failures; the largest reported render
cycle is 844.12 ms. Map then rejects its own `validated` route at handoff: the connector
from the actual 179.65 m endpoint to a nearby 180 m node climbs 47.935 degrees against
the profile's 45-degree limit. Owned cleanup lands and disarms the vehicle. The final
estimator read has no compass fault, but the aggregate preflight flag is false; full
installed postflight health and visual/playback acceptance remain open. Cluster and
BuildKit are stopped, with 180 GiB free.

The Map correction selects connectors against the mobility envelope in their direction
of travel and rejects avoided-area intersections. Route creation checks primary and
alternative geometry before promoting status or persisting a successful route. The
spatial validator checks vertical segments against climb/descent limits. All 167 native
Map checks pass, including the recorded flight coordinates, directional connectors,
avoided areas, invalid alternatives and vertical segments. The installed source harness
is explicitly ignored in this native run. Strict all-target Clippy, formatting and
documentation links pass. Map publishes from `0388b911` in 107 seconds, reusing its
native dependency layers. The reference selects the new image at `9d7d0b6a`; its
installed handoff and mission pass as recorded above. Map checks are under
`output/development/foundations-map-route-20261002/`.
Installed camera results are under
`output/development/foundations-camera-publication-eae9d400/`; sensor diagnostics and
qualification are under `output/development/foundations-px4-sensors-20261002/`.

Camera preparation checkpoint (2026-10-02): an isolated native Kit trace at
`2cca3c33` records a 1,998.9 ms Cesium update followed by a 2,451.6 ms Kit update.
Inside the latter, 2,048 RTX material updates consume 2,351.8 ms; rendering takes
about 19 ms. The pinned Cesium implementation preallocates 2,048 objects per GPU
resource pool and doubles capacity above 75% occupancy. This identifies a concrete
source of long frames; the diagnostic does not reproduce or close the earlier
5–6-second flight stall. Profiling overhead also prevents treating these durations
as normal-runtime acceptance.

The UAV overlay now starts those pools empty and patches acquisition to allocate
only a requested missing object, preserving released objects for reuse. Each tileset
gets a 5 ms soft main-thread loading budget; Helm and runtime configuration admit
0.1–10 ms and reject unlimited loading. The image build runs a native pool check
against the patched upstream header. The unpatched header fails that check on
speculative growth. The patched pool check, all 125 runtime tests, six Helm budget
admission cases, the full `helm-config` check and documentation links pass. Installed
GPU qualification is pending; camera freshness, textured coverage and playback lag
gates stay unchanged. The trace and analysis are under
`output/development/foundations-camera-trace-2cca3c33/`. Temporary profiling commands,
ConfigMaps and cache files are removed, replica counts restored and the cluster stopped.

Computers construction checkpoint (2026-10-02): `RequestId`, `TemplateId` and
`ProviderInstanceId` now follow admission, encrypted journals, configuration and
runtime calls. Store and private protocol adapters perform the UUID/text conversion.
The public contract reuses Artifact occurrence IDs, and browser/CLI leases keep the
foundational session-family type. Completed lifecycle, maintenance, command and file
results derive their resource addresses and reject inconsistent decoded identities.
Constructors enforce output limits, known command completion and typed file digests.

The eight-package native run passes 634 cases, including the one admission fixture
retried from its existing executable after a Docker creation timeout. Sixteen provider,
measurement and external-service cases remain explicitly ignored. All 107 Console
checks and TypeScript compilation pass. The separately resolved contract consumer
passes eight default-profile and nine knowledge-profile checks without importing
service or asynchronous runtime dependencies. Strict all-target Clippy passes.
Generated client models and owning designs reflect the same profiles. The remaining
public DTO relationships stay open. The reference cluster and BuildKit remained
stopped throughout the native batch.

The Computers host/storage, hosted server, Gateway and Console images publish from
`aa2f3a11` in 363 seconds. The reference selects the four deployed runtime digests;
the storage binary ships inside the host image. The guest template is unchanged.
Publication metadata is under
`output/development/foundations-computers-publication-aa2f3a11/`. Removing 230
superseded native executables recovers 83.9 GiB while preserving current qualification
binaries, libraries, incremental state and BuildKit caches. About 251 GiB is free
after publication. BuildKit stops before reference reconciliation and installed checks.

Installed acceptance at `2beae5f9` reconciles those images and the repaired storage
chart in 220 seconds. All 27 reference Deployments are ready. The Computers hosted
profile passes 31 checks, including checked discovery and K01–K06 for its docs
collection. Three checks are explicitly skipped: the profile selects no readiness
endpoint, listen collection or knowledge search tool. This closes the library/setup
feature gate; C31 and the K09/K10 owner review remain unqualified. Its temporary
certification Pod is removed. Public installation acceptance passes configured
identity, Console endpoints and full/HEAD/range Artifact delivery. These HTTP checks
establish no visual or provider-lifecycle acceptance. Flux is suspended and the nine
heavy Deployments are scaled down before stopping the cluster.

Storage runtime checkpoint (2026-10-02): the diagnostic flight at `f0939bf2`
fails Stream replay with an Artifact 503 and the visual branch with 1.133 seconds
of Rerun source lag against the one-second limit. RustFS repeatedly misses health
probes and restarts. With every GPU workload stopped, its one-CPU quota selects
one Tokio worker; the same store reproduces health timeouts and a failed installed
multipart test. Its CPU profile records a busy RustFS worker with little quota
throttling.

An isolated change to four Tokio workers preserves the image, stored objects and
one-CPU quota. The installed multipart harness then verifies 33,554,451 bytes,
uncertain acknowledgements, repeated completion and owned cleanup in 1.11 seconds.
All 58 paired health observations pass over two minutes with no restart; readiness
has a 3.86 ms p95. The failed baseline's owned upload is explicitly removed and its
absence verified. The chart now configures four workers independently of CPU quota,
rejects single-worker settings and uses `/health/ready` for traffic admission while
keeping `/health` for liveness. The complete native Helm configuration harness and
strict deployment-client Clippy pass. Chart publication and installed reconciliation
follow this runtime experiment. It does not close the composed flight or camera-stall gates.
Diagnostics are under `output/development/foundations-rustfs-f0939bf2/`; the
instrumented flight and rendering traces are under
`output/development/foundations-camera-stall-f0939bf2/`.
The platform chart publishes from `cccbbf50`, and the reference selects its immutable
manifest `sha256:34eaa7c4920830afb12baf8b10409348fdaf75b0e18d81beaf986c93bedb6c5d`.
Reconciliation at `2beae5f9` installs its four-worker setting and readiness path.
The same installed multipart test passes in 1.61 seconds. During reference startup,
all 120 health and readiness probes pass over two minutes with no restart; readiness
p95 is 3.93 ms. The committed chart's installed storage gate is closed. This does
not qualify the composed flight's camera freshness or playback lag.

Recording publication checkpoint (2026-10-02): native RRD fixtures reproduce the
flight's exact missing-samples error while a live layer's final file exists and while
its catalog state is Staged. The reader had omitted acknowledged parts in both states.
It now reads those parts through publication, with the same source digests, byte limits
and task-local normalization. Parts-directory confinement also runs when the final
file exists. Fourteen reader tests and five video contract/snapshot cases pass, as does
strict all-target Clippy for both crates. These checks select encoded fixture data and
establish no GPU or installed acceptance. Publish the reader fix and repeat composed
flight before closing the missing-range or visual/timing gates.
The Recording, Stream and Reason images publish from `10145561` in 429 seconds.
Their runnable digests are selected in the reference image lock. BuildKit stops after
publication; deployment and installed flight qualification follow the native checks.
Publication metadata is under
`output/development/foundations-recording-publication-10145561/`.

Installed qualification at `eeaa8442` converges to all three runnable digests in
83 seconds. The composed flight passes its domain branch: takeoff, mission, live
Stream inference, Recording replay, grounded Reason over the same 25-second range,
Map-approved return, landing and Artifact context isolation. This closes the
installed missing-samples failure. Headed Chrome captures the takeoff, mission,
Stream preview and moving Recording view, but the visual branch fails during landing.
At 17:23:34 UTC, the shared camera atlas reports `operator camera frame is stale`:
its last frame is 5.76 seconds old. The simulator's maximum recorded render cycle is
9.99 seconds. A later observation has fresh frames and healthy cameras; that recovery
does not qualify the failed visual run. Diagnose the render stall without relaxing
frame freshness, hardware requirements or spatial-content checks. The phase report
records domain passed and visual failed under the publication directory's
`captures/eeaa8442b2c8e2962efba15ed37ec0d76d933897/01a0fd98-a460-76f2-baae-439d7858f18f/`.
Flux reconciliation is suspended after convergence, temporary replica-ignore rules
are removed, and the nine heavy workloads are scaled down before stopping the cluster.

Computers identity checkpoint (2026-10-02): lifecycle and maintenance APIs carry
`TaskId` through public receipts, claimed workers, queue cursors, encrypted bindings
and retained allocator handoff. Interactive access uses separate grant, pairing and
connection types. Gateway and BFF route decoding distinguishes interactive grants
from automation grants before forwarding. A provider lifecycle checkpoint accepts a
checked correlation identity that can represent either a lifecycle Task or a recorded
command/file containment operation. JSON UUID representations stay unchanged;
new grant and pairing parsers require the canonical UUIDv7 profile already minted by
the domain.

The grouped domain, contract, runtime and service selection passes 299 cases after
rerunning two Docker fixture lifecycle timeouts without concurrent compilation.
All 33 gateway/BFF cases pass with the corrected pairing fixture, and seven generated
browser contract checks and TypeScript compilation pass. Twelve native-provider or
measurement cases remain explicitly ignored; this checkpoint does not qualify those
profiles or installed deployment. Request, template and provider identities,
result/DTO relationships and remaining Artifact/session references stay in the Phase 3
inventory. Strict all-target Clippy passes across eight affected packages, including
the native host and storage consumers. The runtime-only service library compiles, and
the isolated contract consumer passes both its default and knowledge profiles. The
new types do not pull service or asynchronous runtime dependencies into that consumer.

The Computers read batch moves maintenance actor/parent admission, reservation and
execution receipt resolution, claimed journal reads and Task-link repair into SQL.
Lifecycle, maintenance, command and file workers consume closed typed Task payloads
shared with admission. An existing denied receipt returns an error, preserving the
original fence and preventing another reservation or dispatch. Malformed private rows
exercise denied owners, contexts, parents, providers and claimed Task relationships.
Eighty-eight domain cases and twenty-four application, HTTP/MCP and service cases
pass. All seven new queries validate with SurrealDB 3.3.0. Strict all-target Clippy
for the domain and server, Rust formatting and documentation links pass. Request, template and provider identities, remaining DTO relationships and grouped
installed qualification stay in the migration inventory. The cluster and BuildKit stayed stopped during this batch.

The composed flight at `5cae9ec8` passes takeoff altitude, the mission, Recording
replay, grounded Reason, Map-approved return to launch, landing and cross-context
Artifact access. Both takeoff and mission camera captures show the city. The final
camera check fails because mean luma is 20.50 against a minimum of 25, despite a
standard deviation of 25.32 and a 0–196 range. A subsequent headed Chrome diagnostic
shows a dark street scene with visible lane markings and buildings. WebGL uses the
RTX 4090; WebGPU reports SwiftShader and supplies no hardware evidence. This supports
a false rejection by the brightness check, but visual acceptance stays open until
that diagnosis is qualified through the owning harness. The initial attempt stopped
before flight commands on incomplete UAV tool discovery; fresh public discovery
passed before the completed retry. Its cause is unproven. Phase outcomes and captures
are under `output/development/foundations-flight-takeoff-5cae9ec8/`; the diagnostic
screenshot is `/tmp/chrome-devtools-mcp-56U9uD/screenshot.png`. The native harness
finished cleanup before the cluster stopped for development.

The live-camera content check now admits dark and bright textured scenes through
spatial contrast. Its existing 64-by-36 canvas sample supplies a four-by-four grid;
at least four regions must span sixteen luminance levels with standard deviation
at least five. Global variance still rejects uniform images. The typed observation
requires all sixteen regions and rejects sampling errors or invalid statistics.
Six Rust cases pass in both focused clients, covering the dark landing profile,
bright detail, uniform frames, isolated patches, incomplete observations and invalid
statistics. Four native JavaScript cases qualify the actual sampler's cell placement
and arithmetic. Strict Clippy and both executable builds pass. These native checks
do not close the installed visual gate; hardware, cadence and latency requirements
are unchanged, and the composed flight must pass with the new checker.

The next flight startup exposed a separate Knowledge restart failure. Embedding's
first attempt lacked 0.04 GiB of KV-cache capacity for its declared 32,768-token limit;
it became healthy on its next attempt without changing the profile. Knowledge then
rejected a source's conditional observation as changed content or access. The source
identity and differing fields still need diagnosis; strict validation stays enabled.
The logs are retained under `output/development/foundations-flight-content-4998241e/`.
This reopens installed Knowledge restart acceptance.

Read-only diagnosis at `2fc911e6` checks 156 retained observations against current
public source responses and identifies one mismatched conditional read. Artifact
`01a0fa0d-57de-7681-8feb-057ef7afbb21` restores the same private metadata after the
sharing fixture, but its stored modification time advances. Its revision hashes
only text and access, so the server incorrectly returns `notModified`. Two other
retained members fail their current source reads and are not conditional matches.
The repair binds Artifact's modification time into its revision and closes the
same omission in Map's time/actor metadata and Time's stored timestamp. Reason's
summary already includes its recorded update time; its access includes the owner.
Conditional validation stays strict and now reports the differing field with the
collection identity. All 32 focused native checks pass across the three producers,
extension validation and Knowledge coordination, pipeline and gateway consumers.
Strict all-target Clippy for the five affected packages and 1,054 documentation
links pass. Installed restart acceptance is recorded below.
Diagnosis is retained in
`output/development/foundations-flight-tools-2fc911e6/conditional-diagnosis.json`.
Artifact, Map, Time and Knowledge images publish from `521ded3e` in 164 seconds.
The reference lock selects only those four new runnable digests. The builder stops
before installed rollout; the retained index is preserved for restart qualification.
The reference converges at `12538e12`, and public Knowledge acceptance passes before
and after a real Pod replacement in 24.52 and 21.08 seconds. Both reports contain all
nineteen collections, nineteen initial catalog observations, thirteen verified source
links and the same retained generation and embedding space. The replacement Pod uses
the same published image; the prior Pod is absent. This closes the conditional-source
restart failure without clearing the index or weakening validation.
Flux drift correction initially restores the GPU replicas stopped for this check.
Temporary rules exclude only those Deployment replica counts during convergence;
image and configuration correction stay enabled. Root and Helm reconciliation are
suspended and those temporary rules removed before the test workloads stop. The
simulator, View, Stream, Speech and Optimization stay stopped during acceptance;
Reason and Embedding supply the required source and inference paths. Reports and
Pod identities are under `output/development/foundations-conditional-publication-521ded3e/`.

Flight readiness had been calling `conformance info`, which reads every tool, prompt
and resource-template catalog and therefore required unrelated services. The new
`conformance tools` command preserves full tool pagination and schema validation.
Flight preflight uses it and still requires all of its actual domain and live-view
tools. The wider `info` command keeps its existing catalog checks. No flight commands
were dispatched during the failed full-catalog preflight, and the cluster stopped
before rebuilding this client-only change. All 75 native flight/conformance checks,
strict Clippy and both executable builds pass. Installed scoped discovery and the
composed flight follow that native qualification.

Installed tool-only discovery passes at `2fc911e6` with Knowledge, Embedding, Speech
and Optimization stopped. The composed attempt passes Map/Stream prerequisites,
takeoff and mission camera captures, live Stream viewing and Recording replay.
Reason then fails to materialize video samples for the same replay selection:
recording `01a0fcf8-954a-7383-be2b-cca1cb29a456`, camera `down`, simulation-time range
`1099300000000..=1124300000000`. The cause is unproven. The flight dispatches owned
landing cleanup; a subsequent public state read confirms landed at ENU
`1250.48, 880.79, 0.04`. Its final camera check sees mean luma 0.81 and standard
deviation 3.33, which fails the spatial-content rule. This capture follows abort
cleanup at the mission location; the successful return-to-launch sequence was not
reached. Both phase outcomes are failed, and the new content thresholds stay intact.
Logs, source catalog, captures and phase outcomes are retained under
`output/development/foundations-flight-tools-2fc911e6/`. The cluster stops after cleanup.

Artifact's installed sharing gate passes seven checks through the public MCP and
anonymous HTTP routes: private fixture ownership/retention, read-only byte equality,
one-download enforcement, revocation, expiry, current parent release state and cleanup.
The owner-local harness reuses shared transport and typed Artifact requests. It restores
private state and revokes returned links after success or failure, and never records
link secrets. The installed run takes 15.49 seconds. All four source harnesses and the
new sharing harness compile together and pass strict Clippy. Reports, deployed image
identities and harness hashes are under
`output/development/foundations-artifact-sharing-5cae9ec8/`. Temporary credentials and
body copies are removed. The remaining server-library installed gate belongs to
Computers; the wider Phase 3 type inventory stays open.

Map's checked startup, discovery and direct-address batch is accepted on the reference
installation at `7b534a2b`. All 35 source checks pass, including four actual restarts,
publication creation and search denial. Its remaining paged addresses, DTO relationships
and Store query IDs stay in the Phase 3 inventory.

Phases 6–7 now pass installed source checks for all eighteen participating servers.
Artifact, Reason, Time and Map pass 89 checks, including real change/restart probes;
Map also passes search denial. The other fourteen documentation sources pass 84
checks. Their K07/K08 skips reflect immutable docs and absent domain search tools.
The completed public audit exports contain revision-bearing reads for every declared
collection. Domain K09/K10 owner review is recorded in Phase 7. Rerun is an external
viewer bridge with resources disabled and is outside this adoption scope.

Phase 8's Knowledge service and embedding runtime are deployed. The nineteen-collection
catalog, source-linked search, completion, statistics, subscriptions, CUDA embedding and
network isolation passed their recorded acceptance. The repaired conditional source
revisions pass installed acceptance before and after Knowledge restart at `12538e12`,
preserving the active generation. Indexing audit windows finalize with a verified signed chain.
Phase 9's Reason summaries pass installed publication, result grants, revocation and
separate Reason/Knowledge restarts. The controlled 0.6B/4B/8B retrieval comparison passes
on the recorded corpus and retains 0.6B. These checks do not close the remaining type,
cross-replica, domain and visual requirements elsewhere in this plan.

The retrieval evaluator now binds judged queries and a complete caller-visible corpus
to a generation, verifies source revisions through SQL-admitted pages and searches,
and derives recall at ten from retained ranks. Store appends reports under the immutable
generation specification and cascades them on reclamation. An isolated RocksDB/CUDA
benchmark entry point measures a full rebuild with concurrent search and exports the
generation's evaluation. The domain corpus now has 153 members in nineteen collections
and 78 fixed queries, including Spanish paraphrases, across twelve fictional inspection
scenarios and five actual source designs. Owner contract libraries construct the records
and addresses. Map and Reason expose descriptors through lightweight `knowledge`
features; the independent consumer rejects runtime dependencies. The 0.6B/4B/8B CUDA
comparison passes on this corpus and retains 0.6B; it does not establish production-wide
retrieval quality or installed source conformance.
The 24-image direct qualified publication from `fe77f5b3` passes in 361 seconds.
It includes Store migration 0104, the Time bootstrap identity fix and source-checker
CLI. The earlier `e6220dcf` stage-to-release attempt failed its runnable-digest
identity check after BuildKit reclaimed runtime layers; no installed acceptance used
those staged images. The source-owner review found an Artifact listener gap in expiry
and revoked-member invalidation. Its focused repair is published at `529c2b66` in
59 seconds. The composed release locks select that Artifact image and the other
qualified images from `fe77f5b3`. All ten consumed-image rollout checks and Helm
configuration checks pass. The reference rollout converges at `bf724750` with both
Helm releases Ready and all 21 changed Deployments available. The installed Artifact,
Reason and Time source runs fail on Gateway discovery after their source restarts.
Reason passes fourteen K01–K06 checks; both K07 probes fail. Artifact and Time cleanup
errors hide their reports. Operator reconciliation verifies the temporary Artifact
grant absent and both owned Time events cancelled. Map's installed run remains open.
The Gateway's existing read retry begins after discovery and misses this failure.
The repair adds one transport-only discovery retry before domain dispatch and makes
all four owner harnesses persist available reports before cleanup. Native qualification
passes all 42 Gateway MCP cases, including the real HTTP disconnect/refusal matrix.
Strict all-target/all-feature Clippy passes for the Gateway and all four owners;
formatting and 1,041 documentation links pass. Gateway publication from `aaadcd61`
passes in 109 seconds and the reference converges at `3299bbab`. Artifact's repeat
preserves its report: ten K01–K06 checks pass and K07 fails. Gateway logs identify
TCP connection refusal immediately after the replacement Pod becomes Ready, including
the discovery retry. Kubernetes readiness has not established public Service routing.
The installed lifecycle helper now requires a fresh public contract read after the
native rollout watches; its ten-second readiness window dispatches no domain mutation.
The helper passes focused strict Clippy and compilation for all four owner harnesses.
Installed Artifact, Reason and Time runs now pass 54 checks in total: eleven for
Artifact, sixteen for Reason and twenty-seven for Time. All four K07 change/restart
probes pass, and owned grant/event cleanup succeeds. K08 skips because these sources
declare no search tool. Public routing takes one to four readiness reads after Pod
readiness; the helper bypasses SDK caching on every restart. Map's installed run passes
34 checks, including search denial and three restart probes. Its publication removal
probe fails because archival preserves immutable publications under the owning design.
Public reads confirm that behavior after cleanup. The checker needs a typed creation
probe for immutable collections: subscribe before publication, verify the new member
and collection invalidation, restart, verify persistence, then publish again. Preserve
Map's archival semantics and replace the invalid removal fixture.
All three fixture layers are archived and the original release pointer is restored.
A completed public audit export contains 424 API records, including revision-bearing
knowledge reads for all eighteen collections declared by these four sources. Source
audit review therefore passes for each collection. Knowledge and unused GPU workloads
stay stopped during source checks; the cluster is stopped for further development.
The typed creation probe is implemented and Map now supplies publication creation
through its owner library types. Each change variant holds its applicable driver;
creation returns a checked URI without adding domain vocabulary to the checker.
The checker verifies collection readiness and mutation delivery, new membership,
full/conditional reads, restart persistence and a distinct second creation. Its
native matrix exercises update, removal and creation under direct and gateway
routes, including eleven creation failure modes. All eleven library tests and two
HTTP/independent-consumer integration tests pass. Strict all-target/all-feature
Clippy passes for conformance and all four source owners; all four installed
executables compile. Both Map and conformance images publish from `f64b5c05` in
206 seconds. The reference selects Map runnable digest
`sha256:8060d13b3f1433c144b40a1c191b0a81a3ec7da068e45cb81d694ffd416c0845`.
The builder stops before rollout. Map's installed creation case remains open.
The reference converges at `44652144` and Map passes all 35 installed source checks
in 172 seconds, including creation across restart and search denial. All four restarts
pass public readiness on their first read. Cleanup restores the original release and
archives the three selected layers. The remaining docs batch passes for thirteen
sources, including Python Datasheet and the Knowledge service. Rerun is an external
viewer bridge with resources disabled; its inclusion in this batch was a selection
error and does not require knowledge adoption. A public audit export contains 360 API
records with matching header/footer checkpoints and revision-bearing observations for
Map's seven collections and all thirteen passing documentation collections.

Optimization's first run reports a duplicate declaration while the Gateway catalog
is changing. A later captured catalog has one declaration, and its fresh complete
source run passes. The initial failure is retained for catalog-pagination follow-up;
the observation does not establish which refresh changed the page boundary. Charts
fails because the reference's four user/service rules admit resource reads but omit
`resources_templates_list`. The policy correction adds that action to the existing
Charts selectors and advances the policy and complete public-bundle revisions.
Charts installed acceptance remains open. The cluster and builder are stopped and
temporary source credentials are removed before further development.
Gateway pagination diagnosis reproduces the duplicate locally: inserting one earlier
source moves the old numeric offset back onto the previous page's final declaration.
The Gateway now uses typed last-identity cursors for tools, resources, templates and
prompts. Admission still runs before each page; removed identities cannot shift later
entries out of the traversal. The cursor rejects another surface, malformed types and
unsupported versions. Clients restart enumeration after list-change notifications to
see identities newly inserted before their position. All 47 Gateway MCP cases and
five control-plane/shared-policy cases pass. All-target/all-feature strict Clippy,
formatting and 1,041 documentation links pass. The combined Gateway/Charts policy
rollout and installed catalog checks remain open. Gateway publication from `c41270fb`
passes in 80 seconds; the reference selects runnable digest
`sha256:697bdbe7f2ff2a5e45c3d5a20cc0b62a56bcddf2c16e0ddbb0d84271595eccbd`. BuildKit stops
before the combined image and Charts-policy rollout.
The combined release converges at `67d75ae8` with both Helm releases Ready.
All fourteen documentation-source runs pass six checks each, including Charts and
Optimization, with the two inapplicable domain probes skipped per source. A complete
public audit export contains 126 API records and revision-bearing reads for all fourteen
collections; its matching header/footer checkpoint is sequence 2260. These records
qualify read attribution, while signed-chain verification has its separate Phase 4 gate.
Temporary credentials are removed, heavy workloads scale to zero, and the cluster and
builder stop before development resumes.
The installed Stream harness now accepts two explicit Pods from one rollout and pins
Task dispatch and subscription to separate processes. It requires a working-state
subscription baseline before completion, run/result invalidations, identical typed Task
and result reads, reconnect baselines and cancellation. A create-only report preserves
Pod/image identities and the run ID, including failed qualification. All 26 native smoke
cases, strict all-target/all-feature Clippy, binary compilation, formatting and docs
checks pass. The installed cross-replica run remains open; live-owner notifications
retain their separate acceptance requirement.
The first installed attempt stops before recording or Task creation because the
Pod decoder expects `imageId` instead of Kubernetes' `imageID`. The field mapping and
a Kubernetes-shaped admission fixture correct the harness. All 27 native smoke cases,
strict Clippy and binary compilation pass; the installed case is ready to repeat.
The next attempt reaches Stream but Artifact rejects its direct fixture assertion
because the shared GPU smoke signer omits `GatewayRequestContext`. No Task is created.
The Stream/Reason helper now signs matching automated actor/token attribution and a
fresh audit request under the selected installation profile. The Artifact admission
requirement is preserved. The native smoke suite, strict Clippy and both smoke/recording
forwarder executables pass. Installed repetition uses the deployed signer, Store
credentials and producer key ID in a private temporary environment.
Installed cross-replica acceptance passes at harness commit `b40f63d9` in 5.78 seconds
against the reference Stream image
`sha256:3fbe4dd0e4835e3d538d2bd7f3d10c621a60f6e99a626e1cab01b4819df9758a`.
Two distinct Pods keep their identities, image and restart counts through the case.
The observer receives eight Task and sixteen resource notifications, including the
working baseline, completion, run/result invalidations and a reconnected completed
baseline. Both subscriptions cancel; Task envelopes and typed result resources agree
across replicas. The GPU replay processes sixty frames, detects 295 objects and publishes
its requested artifacts. The Task result installation deferral is closed. Stream C27
still needs its live-owner notification case. The temporary acceptance environment is
removed, Stream scales to zero, and the reference cluster stops after acceptance.
Installed live-session notification acceptance passes at `56974ae2` in 12.14 seconds
against the same Stream image. The UAV camera reports NVIDIA NVENC access units and
the simulator sees the Kubernetes-assigned RTX 4090. The public MCP client receives
initial session, result and preview invalidations in both subscription cycles.
Inference advances from 50 to 58 frames, then from 63 to 67 after reconnect; encoded
preview sequence advances from 52 to 59, then 64 to 68. Both subscriptions cancel,
and the harness stops the session it created. The native batch passes 58 flight
tests, the client dependency check, strict Clippy, both executable builds and 1,044
documentation links. Reports are under
`output/development/foundations-stream-live-56974ae2/`. Stream's C27 installation
deferral is closed. Stream and UAV workloads scale to zero and the cluster stops.
The concrete-reference hard cut replaces `ResourceUri`'s delimiter check with the
existing RFC 3986 parser. Construction and Serde reject templates and malformed
escaping; admission errors omit the input. RFC 6570 declarations keep their separate
type. Network resource identities preserve ports and fragments, while domain owners
retain their stricter component and route checks. Native qualification passes 280
shared checks, 375 checks across sixteen server contract suites, the seven-case
independent consumer, 47 Gateway MCP checks, twelve gateway support checks and five
control-plane/policy checks. Workspace-wide all-target strict Clippy passes. This
batch also corrects a stale registration test and documentation: catalog-only approval
permits source-contract discovery, while member reads still require indexing approval.
The coordinated image publication and installed URI admission qualification remain
open. The cluster and builder remain stopped throughout this native batch.
The URI batch publishes 26 qualified images and both reference charts from `38fcfd95`
in 815.5 seconds. The selection covers 24 images referenced by the installation plus
conformance and stdio tools. Cargo's production dependency closure excludes Computer
host, storage and guest-template images; the earlier lockfile change only adds an
existing dependency to the flight client. Their retained-template inputs stay fixed.
Both image locks, the managed-agent kernel image and chart references select the new
receipts together. All ten rendered rollout tests and Helm configuration checks pass;
the four installed harness executables compile. BuildKit stops after publication.
The coordinated rollout and installed URI admission checks remain open.
The reference converges at `dc9cbf6f` in 282 seconds: both Helm releases and all
22 changed Deployments are ready. The public URI batch passes 91 checks across
eighteen resource-serving servers, including 36 template/malformed-escape denials
with redacted errors. Its first catalog attempt failed while UAV was still starting;
the complete repeat follows readiness. Knowledge passes nineteen-collection catalog,
completion, subscription, search and CUDA embedding checks, with thirteen source links
matching current indexed revisions. Public Artifact full/HEAD/range delivery passes.
The new Stream image passes live GPU inference and preview notifications across two
subscription cycles, reconnect and cancellation; its owned session is stopped. The
simulator sees its Kubernetes-assigned RTX 4090. Reports are under
`output/development/foundations-uri-publication-38fcfd95/`. Reconciliation is suspended,
GPU services scale to zero and the cluster stops before further development.
The Map batch adds checked startup and discovery plus typed direct authoring/catalog
addresses. Domain IDs reach URI builders and return from parsers without a string
round trip. Native qualification passes 63 contract cases, seventeen hosted cases
including all discovery grant combinations, and eight isolated-consumer cases. Strict
all-target/all-feature Map Clippy and workspace-wide all-target Clippy pass. The resource-contract section moves into
an embedded `RESOURCES.md` to keep every source document below the existing Knowledge
item limit. Map publication and installed acceptance remain open.
The 13-image Map dependency closure and both reference charts publish from `d9b46bc2`
in 786 seconds. The selected Map digest is
`sha256:9499537c341df16656c43e9a0cfa74d5b6021a7d58d4989f76223f54eab428d2`.
The six image-lock and chart-reference files select these receipts together. Computer
runtime and guest images stay fixed. BuildKit stops after publication; the coordinated
rollout and installed Map change/restart checks remain open.
The reference converges at `7b534a2b` in 358 seconds with both Helm releases and all
eleven changed Deployments ready. Running Pod image IDs match all twelve selected
runtime images; the conformance image is a tool. Map passes 35 installed checks in
171 seconds, including four restarts, layer and feature changes, publication creation,
release transitions and search denial. Cleanup restores the original synthetic release
and archives the three owned layers. The public `map://docs/resources` bytes match the
source document. Reports are under
`output/development/foundations-map-publication-d9b46bc2/`. These checks qualify the
direct-address and checked-setup batch; the remaining Map type work is listed below.
The next composed flight reaches the mission, live Stream capture, Recording replay
and moving Rerun capture. Reason then fails its GPU startup check: 4.4 GiB is free
against its configured 9.86 GiB allocation. Embedding, Knowledge, Optimization, Speech
and View had unnecessarily stayed running after rollout. They are stopped during owned
landing cleanup; 14.3 GiB becomes free. The run exits 1 and is retained under the same
publication directory. A headed diagnostic with NVIDIA WebGL reproduces a black follow
camera while the other cameras in the shared atlas render terrain. The selected UAV
has landed about 975 m from the launch origin. The reference treats Google buildings as
visual geometry and uses a flat launch contact surface; camera occlusion at that
arbitrary landing site is the working diagnosis.
The flight client now returns through a Map-admitted mission to the declared origin at
flight altitude before landing, checks a 20 m launch-site radius and the world revision,
and uses owner types for mission preparation and completion. It preserves both phase
outcomes before propagating a failure. Native qualification passes 61 flight cases,
the client dependency check and process-environment check; strict Clippy and executable
compilation pass. Three warm dispatcher observations take 0.888–0.904 seconds and pass
the two-second limit. The changed landing path still requires installed acceptance. The
cluster and builder are stopped with 291 GiB free.
The repaired run at `16536ea4` exposes two further prerequisites. Its broad `info`
checks require every server in the operator profile; discovery passes after temporarily
starting Optimization, Speech, Knowledge and its Embedding dependency. All four stop
after the Map and live Stream prerequisites pass, leaving 13.4 GiB free while UAV,
View, Stream and Reason stay running. Takeoff then fails because the harness treats
the first `flying` observation as arrival at the requested altitude; that observation
is at 1.7 m. Owned cleanup completes, and a public state read confirms the selected
aircraft landed. The create-only phase report preserves both failures under
`output/development/foundations-flight-launch-16536ea4/captures`.
The takeoff wait now selects the vehicle by ID and requires both `flying` and the
configured minimum altitude within one deadline, checking session and world identity
on each observation. The regression case rejects an early state transition and another
aircraft's altitude. All 62 flight cases, dependency and process checks, strict Clippy
and executable compilation pass. Three warm dispatch observations take 0.862–0.870
seconds. Installed flight acceptance remains open. The cluster and builder are stopped
for development. On the next run, start full-profile discovery dependencies temporarily
and stop unused GPU workloads after both prerequisites, preserving configured GPU needs.
The Python template now consumes checked MCP setup. The SDK supplies protocol-independent
nominal scope and URI types, an open owner-resource interface, and generic MCP scope/resource
associations. Datasheet owns its complete route vocabulary and closed document IDs; shared
setup verifies discovery against that parser before Store connection or Task recovery.
Owner scopes use closed enums, and membership rejects another owner's enum even if its
wire spelling matches. URI components use pinned `rfc3986` 2.0.0 and `uri-template` 1.3.0;
Artifact IDs remain nominal through wire decoding and resource construction. The libraries'
current stable versions were verified against their upstream release pages. The independent
Python fixture qualifies extension without a domain registry. The SDK suite passes 250
cases, Datasheet 44 and the fork workload eight; the final setup/URI/docs batch passes 70
cases after adding typed permission membership. The SDK source archive and wheel build,
and the Datasheet checkout wheel builds through the same repository-relative hook used
by its image. A fresh process imports both wheels and verifies checked setup and embedded
document bytes. Building Datasheet through a standalone source archive cannot resolve
that hook; this does not qualify standalone archive distribution. Installed acceptance
of this template batch remains open; existing hosted and docs-source acceptance qualifies
the earlier image.
Both Python image consumers publish from `4330024f` in 46 seconds. The reference selects
Datasheet runnable digest `sha256:f93c39200c511931e30408204143caf907554df4d458fc3865e417460d050bf6`.
The independent simulation fixture image is also published; it is protocol/packaging
qualification and supplies no GPU evidence. Unchanged Rust runtime images and chart
content keep their qualified selections. BuildKit stops before reference rollout;
installed acceptance of the Python batch and the corrected flight remain open. Native Datasheet
end-to-end smoke, ten rollout-selection cases and Helm configuration checks pass.
The reference converges at `3aa6ecbe` with both Helm releases Ready and the new Datasheet
image ready. Installed hosted certification passes 32 checks; its two inapplicable
knowledge probes are skipped. A separate public Gateway source run passes all six
document checks, with the same two domain probes inapplicable. The temporary signing
Pod is removed. This closes template checked-setup implementation and installed
qualification; wider SDK types and installed multi-page consumers remain in Phase 3.

The current source-qualification batch adds one shared gateway entry point and
owner-local fixtures for Artifact, Reason, Map and Time. K07 models both updates and
membership removal; removal must survive restart and deny full and conditional reads.
K08 distinguishes a restricted result set from a scope-denied tool, and checks the
excluded resource reads in both cases. Owners supply typed grant, authoring, release
and event mutations plus actual Deployment restarts and cleanup. The grouped native
batch passes 355 tests across the checker and four owners; six installation-dependent
tests are excluded from that run. Affected all-target/all-feature strict Clippy,
formatting and 1,041 documentation links pass. Artifact, Reason and Time now pass
installed source acceptance as recorded above; Map remains open. Cleanup removed 24 superseded test executables
and recovered 8.9 GiB while preserving current executables and compiler caches.
The Map active-release tool now joins pointers and releases in one SQL query.
Tenant, source and dataset selection precede its result limit and document decoding.
Selected release documents must agree with indexed metadata. The focused catalog
batch passes 34 tests, including excluded malformed rows, page limits and activation
changes across connections. Strict all-target/all-feature Clippy, formatting,
SurrealDB 3.3.0 query validation and documentation links pass. The query is published
at `fe77f5b3`; installed qualification remains open.
Time's packaged bootstrap authority IDs now bind their family and source-file digest.
Changed timezone data receives a new immutable resource identity; restarts and file
relocation preserve it. The configurable fixed IDs are removed. The bootstrap test
and 37 public contract tests pass, along with affected strict Clippy, formatting and
documentation links. A broader Time run exceeded its 420-second limit while Docker
fixture creation stalled during image assembly; it supplies no database acceptance.
After image publication and BuildKit shutdown, all thirteen previously failed or
unfinished Time cases pass in 30 seconds using the existing executable. The 47 earlier
passing library cases and bootstrap case remain applicable, covering all 61 library
cases with the 37 public contract cases. The owning design declares the coordinated
Time/indexer drain and fixture recreation. The identity fix is published at `fe77f5b3`;
installed qualification remains open.
Nine native evaluator, corpus and retrieval checks pass. Affected all-target strict
Clippy, SurrealDB 3.3.0 query validation and documentation links pass.
The domain corpus and descriptor batch passes 39 source, contract, consumer and corpus
checks, affected all-target/all-feature strict Clippy, formatting and documentation links.
The first model runs found a benchmark lease-renewal deadlock and a Store write limit.
The corrected harness polls renewal concurrently with indexing. Store now allows its
64 MiB message profile through the outgoing buffer, and atomic member replacements
return no inserted vectors. A native 256-chunk, 8,192-dimension member passes write,
readback and injected-failure rollback. All 23 affected Store/configuration and retrieval
checks pass, as do affected strict Clippy and rendered SurrealDB 3.3 query validation.
The evaluator's first cleanup attempt reported a transaction conflict; the complete
retrieval batch passed on rerun. The corrected three-model comparison passes with one
4.625 GiB KV-cache setting for every checkpoint. All models reach recall at ten of
1.000; rebuild throughput with concurrent search is 98.34, 50.50 and 29.81 chunks/second
for 0.6B, 4B and 8B. Model-process memory peaks at 6,962, 14,456 and 20,964 MiB.
The [measurement record](../platform/runtimes/embedding/verification/retrieval-2026-10-02.md)
retains judgments, ranks, GPU samples and the decision to keep 0.6B. Each isolated
database and runtime is removed after its run. Migration 0104 and the Store write-profile
fix are published at `fe77f5b3` and require deployment.
The gateway source adapter discovers approved
collections and waits for catalog/resource observation readiness before reading.
The library coordinator qualifies lease renewal, source invalidation, conditional
reconciliation and restart reuse against isolated Store fixtures.
The binary runs machine-authenticated tenant workers, rotates credentials and publishes
complete catalogs against current control authority. Native host qualification covers
connection recovery, catalog-only approval, readiness and shutdown.
Broader server adoption is postponed outside this plan's completion scope. BuildKit and
the reference cluster are stopped after the catalog and audit acceptance batch.

Scope decision (2026-10-01): prioritize collections that help users find places and
authored features, discover files, and reuse completed analyses. Finish Map and Artifact
in Phase 7, deliver the knowledge service in Phase 8, and add Reason's analyses and
results in Phase 9. Keep the implemented Time collections and shared documentation
support. Stop Optimization adoption. Knowledge publication is optional for a server;
declaring the extension requires conformance for every collection it publishes. The
postponed collections below do not block this plan and require a later product decision.

The Phase 7 review requires explicit source read policies before domain adoption.
Time's private events and Optimization's owner/context/profile-scoped Tasks cannot
inherit membership sharing from a recorded Work Context. `AccessDescriptor.readPolicy`
now distinguishes tenant, subjects, Work Context sharing and subjects constrained to
the active context and optional profile. Store derives typed admission fields and
enforces each condition in SQL before decoding and LIMIT. This is a coordinated hard
cut of the undeployed knowledge format; old observations and indexes require no adapter.
Native qualification covers the source-policy matrix, malformed denied rows before
LIMIT, Rust/Python closed models and audit disclosure review. Twelve Rust tests,
26 Python tests and six Console schema tests pass. Workspace all-target compilation,
affected all-target/all-feature Clippy, TypeScript and SurrealDB 3.3 SQL validation
pass.

Time now persists creation provenance for calendars, epochs, events and acquired
authority releases. Observations preserve tenant sharing or owner-only access from
the source SQL policy. Versioned epoch URIs enumerate every immutable version.
Packaged bootstrap authorities have separate profile-readable resources, without an
invented owner or Work Context. All five domain collections use typed URI pages and
conditional member reads. Calendar creation rejects documents above the 64 KiB member
limit. The old zero-version body decoder is removed; this batch requires the planned
fresh installation reset. Time's native suite, source-policy and authority-page tests,
URI/authority contracts, Store migration checks and headless Workbench navigation pass.
Affected strict Clippy and workspace all-target compilation pass. Contract-only tests, compile-fail examples, runtime-only strict Clippy and a
separately resolved contract consumer pass. The isolated dependency graph excludes
MCP, Store, HTTP and asynchronous service runtimes. Installed collection qualification and
event change/restart probes remain open. The cluster and BuildKit stay stopped.

Artifact discovery now selects clearance, retention, live grants and the selected
Work Context in SQL before decoding and LIMIT. The previous grant-only candidate
scan and Rust post-filter loop are removed. The metadata index has typed cursor URLs
and bounded URI pages, and the Library App follows them. Native SurrealDB qualification
covers malformed denied rows, tenant isolation, context and grant access, expiry and
page boundaries. Service and MCP checks, affected strict Clippy and six headless
Workbench navigation cases pass. The observation adapter below carries the same
selected-context sharing and retention policy. Installed conformance remains open.

Artifact exact metadata reads now share the discovery SQL predicate and assemble the
occurrence, blob, tenant and grants in one database statement. The typed snapshot
validates record relationships and exposes read subjects with expiry through the
service and HTTP client. Metadata reads audit denied or absent records without decoding
their payloads. The shared access evaluator now receives an evaluation instant and
rejects expired grants for service, delegated-read and Console consumers. The snapshot
contains no administrative grant levels or plane download location. Its timestamp
identifies the metadata update; grant changes remain separate revision inputs.
Qualification passes 18 Artifact contract tests, 36 service tests, 11 shared access
policy tests, the three focused discovery checks including native SurrealDB 3.3, and
five independently resolved contract-consumer checks. Affected all-target/all-feature
strict Clippy, workspace all-target compilation, SQL validation, formatting and docs
checks pass. The adapter and admission checkpoint below completes the local knowledge
integration. The cluster and BuildKit stayed stopped; 130 GiB remained free after the batch.

Artifact now declares `artifact.metadata` over its metadata resource template, with
cursor enumeration through `artifact://index`. The adapter uses service snapshots to
produce bounded JSON members, content/access revisions, source timestamps and
conditional responses. The indexed body is metadata JSON; it does not extract file
contents. Metadata links advertise `application/json`. Source listeners register before
authorization reads and end when a changed member loses access.

The knowledge contract now represents selected-context sharing independently of
owner/grant access. Typed read grants carry individual deadlines, and a record deadline
applies to every read path. Store enforces both in SQL before decoding and LIMIT.
Native SurrealDB checks cover deadline passage without reindexing, inactive contexts,
owner/direct/group admission and malformed denied observations. Python and Rust use
one new grant shape; the coordinated installation reset rebuilds experimental indexes
without aliases or historical decoders. Console audit types are regenerated.
Qualification passes 13 Artifact MCP tests, eight extension tests, three native Store
knowledge tests, three Time knowledge tests, three audit tests, six Python tests,
five independent contract-consumer checks and six generated Console contract checks.
Affected all-target/all-feature and Artifact runtime-only strict Clippy, workspace
all-target compilation, TypeScript and SurrealDB 3.3 SQL validation pass.
Installed Artifact knowledge conformance and mutation/restart probes remain open.

Map now exposes all six selected domain collections through dedicated summary resources.
The summaries link to full records and hash their complete serialized content, preserving
large geometry support while keeping knowledge members below 64 KiB. Typed collection
cursors enumerate source-admitted rows. Authoring SQL selects a feature or publication
and its current parent layer together; observations require membership in the selected
Work Context. Location/facility search uses one SQL limit after active-release identity
selection and returns summary resource links. Source-required scopes are now part of
collection declarations and index SQL admission; Time declares its existing read scope.
Local qualification passes all 138 Map library checks, 19 Map contract checks,
four native Store knowledge checks, eight extension
checks, three Time knowledge checks, six Python checks, six independent contract-consumer
checks and six generated Console checks. Affected strict Clippy, Map runtime-only strict
Clippy, workspace all-target compilation, TypeScript and SurrealDB 3.3 query validation
pass. Installed Map knowledge conformance and mutation/restart probes remain open.
The cluster and BuildKit stayed stopped; 116 GiB remained free after the batch.

Current direction: every contract change in this plan is a coordinated hard cut.
Historical data requires no support or migration. Remove compatibility adapters
introduced during earlier checkpoints; those checkpoint descriptions are not
requirements to preserve them. Current-format restart and failure recovery still apply.

Release checkpoint (2026-09-30): the unified audit cut is committed at
`e543e6e4`. Its affected native tests, seven gateway scenarios, strict workspace checks
and paired development measurements pass. All 29 affected images and both Helm charts
are published from that revision. The release inputs select their digests, including
the agent kernel. The Computers guest digest and template fingerprint are unchanged.
Deployment configuration and rollout checks pass. The fresh reference reset is complete.
The platform Helm release, its 23 Deployments and both bootstrap Jobs are Ready.
Public Frames publication passes with binding digest
`26bb4b786097e08ce9c57ff0cfcaad03d394d6f36f96d77b05803b740d2b9646`;
the activation change selects that binding and removes UAV's Git suspension together.
Installed acceptance is in progress. Phases 1–3 have
remaining work, and phases 5–9 are not implemented.
The pending reference release selects both S3 and OTLP audit destinations and enables
the existing Collector. Rendered workload Secret coverage passes. Model and simulator
caches are restored into the fresh PVCs and pass byte comparisons with the preserved copies.
The new node is Ready with eight NVIDIA GPU shares. Flux and all 15 application Secrets
are provisioned. All 38 required images pass CRI presence and cleanup-protection checks,
with 215 GiB free on the host at preparation completion. Both Helm releases converge
at `7ad6115f`; all 25 Deployments, two StatefulSets and both bootstrap Jobs are Ready.
The existing headed Chrome acceptance profile is open. Its WebGL context uses the RTX
4090; WebGPU reports SwiftShader and supplies no hardware qualification.
The release fixes the gateway bundle checksum, supplies explicit audit-retention values
in all four affected installation fixtures, and updates Console scope expectations.
The S3 initializer now uses its existing version's manifest digest; every image in the
prepared 38-image installation closure is pinned by digest.
The first preparation pass lost 32 unused images to Kubelet's image cleanup after the
shared host disk crossed its 85% threshold. Recovery pulls only missing images and
checks containerd's pinned flag after each pull. A node-local K3s import manifest keeps
the selected release available across restarts. This protection covers the installation's
required images; Rust and BuildKit caches are preserved.

The installed pass verifies public installation health, OAuth and ranged Artifact
delivery. Audit verification passes for both partitions (8 installation blocks and
23 tenant blocks), with no clock findings. S3 object and seal readback matches the
database for four blocks per partition; both S3 and OTLP have committed receipts and
cursors. Hosted certification passes for all 17 servers after the Datasheet recheck.
The grouped source repair
adds Python's missing typed audit correlation and managed-agent fields, sends CLI
diagnostics to stderr without starting hosted exporters, validates browser JSON Schema
with the existing SDK's CSP-compatible interpreter, and corrects Map/Time registration
to match their static catalogs. Native CLI tamper and export checks, shared Rust
request-context fixtures, Python SDK/template tests, both browser applications and
Helm configuration qualify the changes. The three affected images are published from
`b5457e91`; both Helm releases and all workloads converge at `3c2b5726`.
Map preparation reuses acquisition `acquisition-01a0f4ff-18ee-7871-8205-13b887aae208`
and verifies active release `release-01a0f4ff-1bec-7fd0-b011-ddf69496d2e0`.
The headed Console displays audit records and recovers two records committed during
a forced connection loss, using the same view receipt. Its WebGL context uses the RTX 4090.

Installed export exposed a sealer throughput defect: it waited one second after each
32-entry database change-feed page, allowing unrelated recording writes to delay an
export marker by roughly 98 seconds. The repaired worker batches once, drains subsequent
pages immediately and reports initial readiness after catching up. Its RocksDB regression
seals a marker behind 768 unrelated commits within the reader's ten-second deadline.
Flight acceptance stopped before movement because the conformance CLI inspected only
the first tools page. The shared catalog readers now follow all four paginated surfaces,
with cycle detection, a 30-second deadline and page/item limits. Native service and
paginated hosted-certification checks pass. The cluster is stopped during qualification
and gateway publication. Installed export and composed flight acceptance remain open;
the complete hosted certification will run again with the corrected reader.

The catch-up image from `28a6b919` converges at `682ebeca`. All 17 hosted servers
pass the complete catalog reader (421 passed checks and 17 skipped checks), and
authenticated browser export returns complete JSON Lines for both partitions in
0.9 and 2.1 seconds. The continued installed pass finds two further issues. Verification
compares record time with sealing time, misclassifying a delayed block as backdating.
Concurrent sealing and export receipt transactions conflict on their lease fence;
the export worker reconnects repeatedly and withdraws gateway readiness. The gateway
rolls back to the previous image at `3449886c` so flight recovery can proceed.
The next grouped repair compares timestamps with signed database commit versionstamps
and retries only database-confirmed transaction aborts around export persistence.
The continued UAV domain run passes mission execution, direct live Stream, recording
replay, grounded Reason, landing and authorized/denied Artifact access. Its visual
branch fails on the gateway's earlier 503 response, so full showcase acceptance remains
open. The cluster is stopped after the domain harness completes its owned cleanup.
Installed audit verification and composed visual acceptance remain open.

The next repair batch (`9a235acc`) passes 162 native tests, including busy-feed
export, concurrent receipt persistence, delayed sealing and public CLI tamper cases;
strict Clippy, formatting and documentation checks pass. Its installed gateway
verifies both partitions (216 installation blocks/250 records and 320 tenant
blocks/801 records), and recent S3 blocks match their stored hashes with both export
receipts present. Reference convergence exposes a stale `k3d-values.yaml` override
that requires 200 GiB free space for Recording Hub. Removing that override selects
the chart's existing 1 GiB admission floor, following the plan's resource direction.
It preserves the PVC request and does not discard build or model caches.

Both Helm releases and all 25 Deployments converge at `83a1c214`. The next flight
run exposes a harness correlation defect: its visual branch accepts the simulator's
existing flight while this run is still performing preflight landing. Those captures
are excluded from acceptance. The repair gates takeoff and mission captures on the
domain harness's completed operations and holds mission dispatch until takeoff capture
finishes. It changes only the local flight client; installed images need no rebuild.
The focused client's 60 native tests, strict Clippy, formatting and documentation
checks pass. The corrected composed run remains pending.

That run completes its mission, then Recording replay fails after segment rollover.
Committed RRD layers use catalog dataset/Recording IDs while live parts still carry
producer names. The video query sees separate Rerun stores and loses the committed
codec metadata. The shared reader now normalizes verified task-local live copies to
the same catalog IDs; source receipts keep the original byte identities. Video queries
derive their Store IDs from the typed catalog IDs. The UAV lands during owned cleanup,
and the cluster stops for native qualification and publication.
The reader/video batch passes 17 native tests, including committed codec metadata
joined with producer-named live samples after rollover. Strict Clippy also checks
both libraries and the Stream/Reason consumers. Installed requalification is pending.
The repaired Stream and Reason images are published from `6ee6805a`; the reference
image lock selects their new digests. The builder stops before the cluster restarts.

The unified audit cut spans the contract, Store, writer, gateway and deployment inputs.
`platform/audit/contract` sits below Store
and MCP; the writer depends on Store. The working batch includes checked drafts and
identities, partition-scoped SQL/LIVE reads, transactional and grouped writes, signed
blocks, a lease-fenced change-feed sealer, retention admission, request correlation,
OTLP timing, aggregated MCP discovery, cached denial counts, native catalog watches,
completion retry, token lifecycle writers, transactional refresh audit, and partitioned
CLI reads and verification. Retention now requires an installation value. Live View
issuance/renewal await a commit; close/expiry/revocation queue stable completion records.
Speech writes session-open, rejection and terminal counts, with request attribution
validated by the shared signed-context converter. Recording ingress classifies domain
activity directly and omits successful batch/status records. Task routes preserve their
opaque gateway identity rather than parsing it as a platform Task UUID. Daily counts
have SQL date bounds and partition-bound keyset pagination.

Artifact source now uses typed activities, related capability/access-request/share IDs,
grant recipients and release states. Download windows coalesce concurrent reads in a
bounded replica cache and claim the same key transactionally in Store. Upload publication
uses its persisted completion request context and appends in the occurrence transaction.
Admin source now uses closed operation/failure enums, correlated admission records and
queued completions; its free-form metadata API and caller maps are removed. Agent and
Console Artifact routes carry their concrete audit target. Source regressions cover
concurrent ranges, individual denials and retry after an unavailable first window commit.

Console source now separates audit from installation inventory. Its APIs and browser
use typed filters and keyset pages, daily SQL aggregates, scoped LIVE invalidations
and partition block recovery. A committed view receipt gates pages and streams;
reconnects reuse it. Audit owns its scope, and reads use the current actor's tenant
and installation role. Export waits for a sealed marker, freezes the retained block
interval and fails on a retention gap. CLI and HTTP exports share typed JSON Lines
framing with a completion footer. Browser contract generation, the Console production
build and all 102 Console unit tests pass in the grouped qualification batch.
Native export-range, access-receipt, SQL filtering and partitioned LIVE recovery checks pass.
No installed acceptance is implied.

Computer source now appends typed lifecycle, access, command, file-transfer and
maintenance records inside domain transactions. Reservation and observed restart take
the verified actor; maintenance recovery records its submitting actor. Work Context
changes commit with control-plane activation under a checked previous-head lock.
Store owns one append function, shared by these producers and upload publication.
Upload SQL is assembled as one complete transaction before SDK parsing.

The writer now owns its drain handle. Gateway, Artifact, Speech and UAV hosts stop
admission and drain queued writes; Speech and UAV stop their session producers first.
Worker failure closes admission, and a 30-second drain deadline reports uncommitted
work. Replica writes, immutable-identity retries and conflicting-batch rollback pass
against the fresh Store. Native domain transactions and Artifact attribution pass;
installed acceptance remains open.

Gateway source now hosts the sealer and whole-block retention worker. Typed lease
outcomes distinguish a valid standby replica from recovery and integrity failure;
readiness checks worker health. Sealer shutdown drains every pending feed page. Idle
LIVE renewal keeps a caught-up cursor current without scanning records. The hosted
retention pass has a two-second budget and removes expired download-window guards.
The old authentication cleanup loop no longer deletes audit rows. Gateway requires a
dedicated audit signing seed; Helm references its separate Secret, and `audit keygen`
creates a private seed file while printing only public verification material. Host
HTTP shutdown deadlines and Pod grace periods now cover producer and audit draining.
Replica election, takeover and full drain regressions pass. A failed sealing drain
reports failure and preserves the unsealed record.

Destination source now includes typed OCSF 1.9.0 mapping, S3 conditional writes and
content reconciliation, and OTLP/HTTP collector acknowledgements. Store persists
immutable delivery intent and per-destination cursors; permanent rejections survive
replica changes. Retention requires every configured destination's receipt. Native
LIVE wakes the exporter while sealing and lease renewal continue independently.
Gateway/Helm expose public export configuration and Secret references, and the reference
selects bundled S3 with Object Lock disabled. Source fixtures cover retry bytes,
uncertain PUTs, missing compliance proof and fenced export-gated retention. The nine
integrity and export-protocol unit tests pass. Independent OCSF validation accepts all
32 class/outcome cases; the selected Account Change class has its documented deprecation
warning. The full audit foundation batch passes, including real RustFS delivery,
downloaded-record signature verification, persisted receipts and worker restart.
Installed S3/OTLP and compliance-provider acceptance remain pending. The provider fixture
now passes with S3 and OTLP together. Both destinations persist receipts and resume
after restart, and the pinned Collector's debug pipeline reports both exported records.

The obsolete audit DTOs, readers, summaries, row-retention API and online index
preparation are removed from source. Gateway, Store and upload fixtures use the
unified records. Gateway smoke scenarios select typed activities and fixture
partitions in SQL, assert aggregated discovery and omitted successful bearer/status
records, and exercise the scoped public CLI. Whole-block retention has one audit-owned
fixture spanning all six classes. That retention fixture passes on fresh RocksDB;
gateway smoke qualification is in progress; its current results are recorded below.

Native Task cancellation now carries an explicit server and UUID through Console,
gateway policy and audit. Its owner query checks the current profile, tenant, clearance
and Work Context in SQL before decoding and inside cancellation transactions. Shared
MCP cancellation uses the same owner-checked transition. Policy and native denial and
provider-uncertainty regressions pass against the native Store.

Artifact administration carries concrete occurrence or access-request targets.
The Artifact domain contract owns access-request and capability IDs and the shared
private ledger-address builder. Access-request and capability denials record the
verified actor, or no actor when the credential cannot authenticate one.

The workspace regression completed 318 targets with 2,747 reported passes, two failures
and 59 ignored cases. Its failures were a Docker creation deadline and a delegated
Computers fixture that discarded its source identity. Computers now reuses its verified
actor. Container creation has a separate 90-second budget, while later commands keep
their shorter deadlines. The repaired Computers and all seven AgentRuntime cases pass.

The live-database batch exercises Store, gateway, Task runtime and Media alongside the
new audit fixtures. SQL filtering before decoding and limits, access-receipt identity
and expiry, UTC summary pages and partitioned LIVE recovery pass. The detail-schema
guard accepts the reviewed inlined digest shape and rejects free text. Public
`gateway audit verify` detects changed and deleted records, a deleted block, a forged
signature and a backdated insertion. All twelve current-format native Artifact cases
pass after binding the final upload identity to its request context. The obsolete
historical upload-upgrade test is removed under the coordinated hard cut.

The Console stream, two-server, platform-store, chart projection and complete gateway
Task smoke scenarios pass. The Task run includes cancellation, model completion,
generation, Artifact handoff, aggregate audit counts and public audit CLI reads.
Fixture generators update every profile referring to the replaced Media manifest.
Profile counts come from typed fixture input. Conformance model completion uses the
Media contract's reserved-expansion template. Current-policy assertions clear the SDK's
cached response before checking the existing transport against changed authorization.

Successful revocation uses an accepted reason, while a revoked credential is a denial.
Token lifecycle records carry the verified principal's scopes and data labels. Both
gateway authentication smoke scenarios pass. Their fixtures drain queued completion
records before checking them and verify delegated service actors separately from their
user initiators. The other five composed gateway scenarios pass as well.

Time uses exact pointer/release conflict checks in place of its tenant fence; the fence
schema is removed under the fresh-store cut. Cross-family activation and concurrent
pointer/release repair checks pass on RocksDB. Upload reads select typed ownership in SQL
before decoding. Issuer and subject types live in `platform/types`, below Store and MCP.
The grouped build and forty native test targets pass, with 607 reported passes. All
thirteen current-format native Artifact cases pass, including the malformed foreign-upload
regression. Paired development measurements exposed a warm-catalog regression from the
writer's fixed five-millisecond window. The writer now groups already queued records
without a timer. Affected native cases, all seven gateway scenarios and strict workspace
checks pass. The replica test checks each concurrent required write immediately after
acknowledgement. The [measurement record](../platform/audit/measurements/2026-09-30.md)
includes 912 paired samples and the remaining catalog and read-tail limitations. Audit
wait takes about 21% of sequential catalog median latency and 7% of read median latency.
Installed acceptance remains open. The native fixtures remove their databases and
provider containers.

Computer connection lease checks use their domain ledger without per-tick audit rows.
A dedicated audit seed and public verification material are generated in an
installation-owned private directory; the seed has mode 0600. Creating the Kubernetes
Secret and qualifying the installed exporters remain. The pre-cut instrumentation
inputs are reconstructed over `21711f55` for paired measurements on fresh stores.
That baseline and its matching hosted fixture are built and measured with request
timers propagated through MCP dispatch. No audit deployment has run. Do not resume
per-server checks.

Current checkpoint: SurrealDB 3.3.0 native qualification passes. Every Rust SDK
pin, the deployment contract, Helm and fixture images now select 3.3.0. The OCI
index is `sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20`.
One grouped build and runtime pass, followed by checks of the collected fixes,
passes 533 Rust cases and 165 Python cases. Fresh schema creation, database-scoped
clients, conditional writes against the pre-write value, and locked exact reads
of existing and absent records pass on RocksDB. Audit retention uses 128-row
batches within its two-second deadline. Obsolete historical index-upgrade tests
are removed. Helm configuration and documentation checks pass. Installed
acceptance is pending at the composed audit checkpoint; the reference cluster is
stopped. The unified audit source pass covers the contract, writer, producers and
readers together and is now in runtime qualification.

Status: Phase 0 accepted and published on 2026-09-27 at `1177185f`; documentation
checks and reference GitOps convergence passed. Phases 1–3 are in progress.
Phase 4's SurrealDB 3.3.0 hard cut and audit implementation are committed and
native-qualified; their images and charts are published and deployed. Composed UAV
acceptance remains open. Phase 5 has a measured baseline and a native-qualified Rust changefeed
consumer batch. Its remaining consumers and writer removal are open. Phases 6–9 are not implemented.
The database upgrade takes priority over the remaining phase 3 work. The unfinished
Computers batch is preserved separately while the SDK, image and fresh-store
qualification advance together. Remaining phase 3 work stays tracked and does not block
independent implementation in phases 4–9. Existing database state is discarded; no historical
conversion or mixed-version support is required. The platform and UAV images and charts are published at
`5df83706` on 2026-09-29. Strict Rust acceptance passed 2,670 tests; the default-feature
suite passed 2,649. Python passed 156 tests, and documentation and identifier checks
passed. The old reference node and its five owned volumes have been removed.
The fresh GPU installation converged at `c0862f74`; all 24 deployments, both StatefulSets
and both bootstrap Jobs passed readiness. `installation-verify` passed public OAuth
and the full, HEAD and range Artifact delivery checks. Certification corrections are
published at `cf7a382d` and deployed by digest at `c5d56238`. The reference installation
converged to that revision; all 17 hosted servers passed certification (421 checks,
with 17 readiness probes omitted because deployment readiness was checked separately).
The temporary certification Pod and policy were removed. Public installation verification
passed with the installation-aware client after adding its required administrator
`time:read` scope. Composed UAV acceptance then found that the startup binding named a
Frames revision deleted by the reset. A current world publication and simulator deployment
are required before flight acceptance can continue. The world recovery batch adds a
contract-only binding builder, `uav-world-publish`, and runtime admission pinned to the
installation's expected binding. Both Pod templates change with the binding digest,
while cache claims stay intact. Native qualification passes 263 Rust cases and 115
Python cases, including both old/new binding mismatches and the client dependency graph.
Scoped strict Clippy, Helm configuration, formatting, docs and identifier checks pass.
The two UAV images and its chart are published at `bbefad48`. Installed publication
exposed gateway rewriting of a producer URI inside the digest-bound Frames tree. The
correction at `2132f9c5` preserves domain payloads and unknown extension metadata while
projecting explicit MCP resource addresses and the App link. Ten forwarding cases,
ten isolated Frames contract cases, strict library Clippy, formatting, docs and
identifier checks pass. The complete scenario digest also passes with workspace features.
Only gateway production code changed; the other package changes were contract tests
and qualification documentation.
The gateway deployed at `abf2ceab`, and public world publication and resource readback
passed. The reviewed world file, digest, UAV images and chart deployed at `3b538126`.
GitOps convergence passed, and both ready UAV containers report the selected image and
world digests. Public installation verification passed again. Composed flight acceptance
passed world admission, recording catalog and control-grant setup, then failed during
takeoff with an adapter command transport error. The simulator did not restart; its
logs contain PX4 sensor timeouts, stale barometer reports and arming refusals. A later
state read showed the controlled vehicle in standby. The command outcome was not
inferred from the transport error, and takeoff was not retried. Sensor timing and command
deadlines need one runtime qualification batch before another full flight run.
The runtime batch reproduces the stationary barometer problem: the CUDA plant emitted
constant pressure, which PX4's equal-value validator marks stale. Seeded 1 Pa RMS
Gaussian measurement noise now runs in that kernel and preserves body/GPS truth.
Direct commands share a 75-second deadline across takeover, locks, mode change,
arming and acknowledgement, inside the default 90-second adapter HTTP timeout.
Native qualification passes 122 runtime tests and a separate RTX 4090 CUDA plant
test covering 600 stationary samples for each of four vehicles and exact replay.
These checks do not establish recovery of PX4 re-arming or the reported transport
poll timeouts. The UAV runtime overlay and Datasheet image are published together at
`0c015dd8`; their reference image locks select the new immutable digests. Publication
reused the simulator dependency image and completed in 122 seconds. The existing
charts cover these image-only changes. Revision `481ee4d2` converged in 115 seconds;
all changed deployments passed readiness. Public installation verification passed.
Datasheet passed all 26 installed hosted checks, including readiness, with no skips.
Public OAuth profiling, Task completion, catalog page reads, exact usage and completion
passed. The temporary certification Pod was removed. Composed flight acceptance
landed the controlled vehicle but PX4 refused re-arming. The adapter returned a 409
with its command-deadline diagnostic; takeoff was not retried. Native PX4 sensor status
shows BARO healthy and GYRO/MAG stale after landing. The remaining sensor model must
cover stationary inertial and magnetic measurements together, with GPU and PX4 health
qualification before another installed run. The runtime had no container restarts;
UAV 1 settled in standby while the other three vehicles kept flying. Startup MAVLink
poll timeouts remain a separate timing concern. The reference cluster is stopped
after collecting diagnostics.
The next runtime batch separates measured IMU fields from vehicle truth and completes
stationary and powered gyroscope, accelerometer and magnetometer noise using the
pinned PX4 simulation profiles. All 124 runtime unit checks pass. RTX 4090 checks
qualify the distributions, independent vehicle/axis samples, repeatability and unchanged
truth. A native pinned PX4 process consumes the CUDA plant through the production HIL
bridge; all five gyro, accel, mag and barometer validators report OK at 15 and 30 seconds,
with no failsafe or IMU data gaps. This avoids a full installation cycle to discover each
stationary sensor defect. The batch also fixes mission admission's missing arming
deadline. Composed rendering, timing and flight still require installed qualification. The
combined UUID/sensor images at `cede5aa2` published in 126 seconds, reusing the simulator
dependency image. Revision `f1a55ddc` converged with both changed deployments ready.
Public installation verification and all 26 Datasheet hosted checks pass. The public
Task/catalog/usage check confirms the new Task ID carries the current RFC millisecond
timestamp. Native sensor health also holds in the composed runtime, including after
landing. The controlled UAV landed, re-armed and took off again without container
restarts. Full flight acceptance stopped at Map route creation: the operator catalog contains
the configured showcase source and mobility profile, but no datasets or active releases.
Map refused to route without coverage. Postflight landing and live-session cleanup
completed. The next batch must prepare the current aviation release and preflight Map
admission before another flight run. The initial attempt also exposed a readiness-wait
gap: a running session can still have streaming terrain. Both harness improvements are
recorded below. Startup MAVLink poll timeouts still need composed timing qualification.
The reference cluster is stopped after collecting the final sensor, state and runtime
logs; the new runtime completed this acceptance attempt without container restarts.
The next harness batch uses UAV-owned simulation state to distinguish declared warmup
from invalid world identity, failed resources and unsupported encoders. Its one timeout
includes state reads and waits. Map route requests now use the owner contract; the full
flight path tests current aviation admission before world/control work, and
`uav-route-verify` provides the same prerequisite without flight commands. Native checks
pass 56 flight cases, the dependency-closure check and 113 xtask cases. The reference
runbook declares the fixture digest, acquisition and activation steps and the required
Map administration and dataset-read scopes. Installed `uav-route-verify` rejected the
missing release in under a second, before flight commands. Acquisition
`acquisition-01a0ef43-cdeb-7a52-821d-7df36eebeead` verified the registered immutable
fixture digest and staged the current source. Release
`release-01a0ef43-d09b-7d11-9192-8c814057ec47` is active with record version 2, and the
same route check now passes in under a second. Installed flight then completed Map
routing and the UAV mission, including a waypoint. Live Stream failed before replay
and Reason: its native runner reports an H.264 access unit without a timestamp. The
supervisor can kill the runner during event-channel cleanup, so the reported SIGKILL
does not identify the initial failure. TensorRT loaded successfully; the Stream container and
node report no OOM kills. Sensor validators remain healthy, and postflight landing and
live-session cleanup completed. The cluster is stopped after collecting diagnostics.
The next client batch exposes `uav-stream-verify` using the existing live assertions
without flight commands or replay. Full acceptance runs the same prerequisite before
obtaining vehicle control and repeats it after the mission. Reads use Stream-owned
session, result and preview types, validate their shared identity, and share one timeout.
Map route costs stay typed through mission budgeting. Failures print before cleanup,
and a prerequisite failure does not initiate landing. One native check passes 56 flight
cases, the dependency-closure check and 113 xtask cases. The new command awaits installed
qualification with the media timestamp correction before another composed flight.
The isolated Stream GPU reproduction identifies receiver clock slaving as the timestamp
failure: stalled delivery followed by catch-up clamps consecutive presentation times,
and H.264 parsing emits an untimestamped access unit after 56 preview frames. The
reference and example graphs now select RTP-only timestamping through
`rtpjitterbuffer mode=none`. The production UAV publisher and existing Stream image
pass both steady and catch-up cases on the RTX 4090: each sends 180 frames, receives
180 unique preview timestamps and completes 177 NVDEC/TensorRT results. The native
runner and model image digests are unchanged. The regression owns its temporary
container and reuses the compiled model. Helm configuration, documentation and identifier
checks pass. The platform chart at `33143814` is published and selected by immutable
digest. Installed `uav-stream-verify` passes through the public gateway with fresh
inference and preview data, then stops its owned session. Full GitOps convergence
exposed a separate Map cold-restart failure: DuckDB Spatial 1.5.5 asserts while replaying
committed R-tree WAL. The stopped database and WAL were copied without changing the PVC.
Both base-table and index reads pass on the unchanged copy with DuckDB 1.5.6; all nine
source features survive. The engine/Spatial upgrade, current-schema crash regression,
removal of schema-9/10 adapters and deterministic native-library packaging form one
qualification batch. Its 258 native checks pass across Map, DuckDB, Timeseries and
the analytical runtime, including both million-feature Spatial gates. The new
process-exit regression reproduces the installed assertion on 1.5.5 and passes on
1.5.6 with committed rows, rollback and a second reopen checked. The isolated Store
fixture's first Docker create exceeded its deadline; its owned container was removed
and the unchanged test passed on retry. Another 58 checks pass for agent memory and
smoke contracts, bringing this batch to 316. Image publication is limited to Map,
DuckDB, Timeseries and the agent kernel. Its task-specific preflight budgets 60 GiB
of peak growth and keeps 128 GiB free; it passes with 71 GiB beyond that budget.
All four images are published from `400caa87`, and the installation selects their
immutable runnable digests. Registry-layer inspection verifies the owned engine files
in all four images and the matching Spatial files in Map and DuckDB against the
qualified upstream bytes. GitOps convergence passes at `0a901aca` and Map starts
against the existing volume. Composed acceptance passes Map route admission, the
mission with a completed waypoint, and live Stream before and after the mission.
It then rejects the camera-freshness probe: the client calls the archive-only
`create_recording_projection` API on a live recording without committed archive layers.
Hub ingestion is healthy with no materialization backlog. The next client batch must
use the shared reader's live-part snapshot contract and qualify Stream replay and
Reason independently before repeating flight. Owned landing and Stream cleanup finish;
replay and Reason were not reached. BuildKit and the cluster are stopped afterward.
The next client batch adds `uav-recording-verify` and extracts the same typed Stream
replay and grounded Reason assertions for composed flight. It removes the archive
projection gate, validates the selected live recording and its source snapshot, and
compares each result resource with the published Artifact. Scenario v12 removes the
obsolete projection settings. The focused native batch passes 173 checks; compilation
takes 13 seconds. Deployment's normal binary rebuild takes 2.5 seconds and Helm
configuration passes. An initial broad test selection was stopped when its deployment
dev-dependency imported gateway and embedded SurrealDB; those unrelated integration
tests are excluded from this harness batch. Installed `uav-recording-verify` passes
at harness revision `83baf53f`: Stream processes 415 frames and grounded Reason observes
six. Both complete through the public Task path, expose matching typed result resources,
and publish matching Artifacts from acknowledged live parts of the selected recording.
The installed Stream and Reason pods each request an NVIDIA GPU, and the RTX 4090
executes the run. This closes their C02 installed result-delivery gaps. No vehicle
commands or image publication are needed. Full composed flight, headed visual
acceptance and cross-replica notifications remain integration work. The cluster is
stopped after this focused qualification; BuildKit remains stopped.
The cluster is stopped during that development; Rust and BuildKit caches, image layers
and runtime claims are preserved. The architecture catalog
validates, rendering is idempotent, and its unchanged HTML/PDF reuse the headed NVIDIA
qualification at `783e447a`. Sequential preparation of the consumed
images resolved the cold-bootstrap I/O
contention. Fresh recording and Computers trust is enrolled
locally; its public key IDs and configuration digests pass Helm and rollout checks.
The native provider accepts the generated JWT key and signs an extension token;
installed authentication and lifecycle qualification are pending. The Phase 2 catalog fixture
at `883a09ba` passed native SDK reads (16 rows) and grant renewal on 2026-09-27;
reference installation acceptance is pending. All installed flight and browser commands require the installation target; the world
publication command uses the same loader.
The shared loader resolves typed operator and administrator identities from its control
plane, and each token exchange uses the selected client's separate credentials. Flight
checks derive namespace, profile, scopes, principal, Work Context and output ownership
from that selection. Native CLI rejection, identity admission and existing harness
checks pass 117 tests. The deployment contract passes 30 tests, the dispatcher passes
20, and the focused client dependency check passes. Scoped strict Clippy, formatting
and documentation checks pass. Installed flight and browser acceptance remains pending. Phase 3 Reason pagination
is implemented at `3c5d914d` with native Store qualification. Stream run and session
pagination, authorized completion, and the updated Live Monitor pass local Rust,
Store, and browser behavioral checks; installed GPU acceptance is pending.
Time event reads and transitions now enforce tenant and owner predicates in SQL;
latest-epoch reads and bounded completions pass native Store qualification.
Time collection pagination, static discovery, requested-epoch SQL batches, and
workbench navigation pass native and browser behavioral checks; installed acceptance
is pending.
Recording catalog pages now apply tenant and label predicates before the SQL limit.
Native qualification traverses 502 authorized recordings behind 110 denied rows,
checks direct-read isolation and bounded completion, and verifies SQL layer counts.
The shared analysis reader uses the same Store visibility query; installed acceptance
is pending.
UAV discovery now advertises roots and templates. Grant, plan, mission, and usage
pages apply visibility in SQL before their limits; active-grant checks also apply
session, permission, revocation, and time predicates there. Native qualification
covers grants behind 520 expired rows, page traversal, caller isolation, cross-replica
content invalidations, and discovery with an unreachable simulator. Mission lookups
use qualified plan and Task indexes. Pilot instructions and the flight client consume
the paged grant response; installed acceptance is pending.
Map derivations now persist in Store and use SQL-scoped summary pages and completion.
Native tests cover immutable replay/conflict, cross-replica reads and invalidations,
interrupted local-store transfer, and authored-feature projection recovery through a
separate Store connection. The pinned Spatial extension passes both million-feature
index tests. Map authoring reads apply labels in SQL, and publication/product reads select
visible parent layers there. Native qualification checks partial clearance, foreign
tenants and contexts, removed parents, and archive filters. Persisted Map completions
now match and deduplicate in SQL, including parent arguments and caller-owned travel
models. Native cases cover matches beyond the old geography cap, duplicate versions,
and scoped catalog selection. Dataset release roots now use 100-item SQL pages and
Map Explorer follows their cursors. Exact release and layer-product reads bind all
URI parents in SQL. Native page checks cover 125 selected releases behind 220
foreign-tenant or other-dataset rows; client checks cover failed pages, invalid
shapes, cursor cycles, and traversal budgets. Route, matrix, and acquisition indexes
now page in SQL under tenant/owner predicates; exact reads and acquisition updates
apply those predicates there too. Route and matrix pages contain metadata summaries.
Acquisition recovery selects pending jobs outside the synchronized worker inventory
in batches of 100. Route invalidation selects dependencies from complete route JSON
in SQL, preserving correctness when separately written dependency rows are missing
or incorrect. Native cases cover 125 owned rows behind 220 denied rows in each
catalog, matrix payload availability, all acquisition states, recovery through 140
pending jobs, and invalidation across 235 tenant routes. Other Map roots and internal
catalog selection still need SQL filtering and pagination; installed acceptance is
pending.
Phase 3 also includes the accepted CE-13 architecture for foundational types,
server-owned contracts, contract-only library builds, and typed scope/resource
interfaces. Foundational names, opaque references, and extension traits are extracted;
wire/schema tests and an independent consumer build pass. Focused contract, policy,
and gateway tests and strict workspace Clippy pass. Time now exposes isolated
contract, runtime, and MCP features and owns its scope enum. Independent contract
consumer, native server, and database qualification pass. Shared concrete URI components
and builders now serve all Time resource variants and collection cursors. Gateway
completion and audit targets now distinguish templates, with native policy and stored-row
qualification. Installed transition checks, Store key typing, and wider server adoption are pending.
Map now owns its scope enum and authoring metadata request/cursor types. Layer,
publication, product, and composition indexes use SQL-scoped pages, and Map Explorer
follows them before publishing a refresh. Native tests cover 125 admitted records
behind 330 records denied by tenant, context, or labels, plus parent selection and
clearance changes between pages. Other Map resource families remain implementation
work; installed and headed hardware acceptance is pending.
Platform identity, access subjects, and invocation provenance now live in the foundation.
Their eleven pre-extraction schemas and existing wire forms are preserved. The independent
consumer passes ten checks with no MCP, runtime, database, GPU, or provider dependencies;
strict workspace Clippy passes across all targets and features.
The Artifact plane model now lives in `platform/artifacts/contract`, below its service,
client, and MCP adapter. Speech's contract imports that model directly and excludes MCP.
An independently resolved Artifact/Speech consumer passes four checks without service
dependencies; Chrono enables only its Serde feature. Artifact attribution now uses
foundational `InvocationProvenance` and a typed constructor. Its private wire adapter
preserves valid flat metadata while both decoding and schemas enforce mode-specific
identity requirements. Tests cover 196 field combinations, existing 0.1.x wire forms,
a compile-time incomplete-delegation rejection, and publication/readback through memory
and separate SurrealDB 3.2.4 connections. The fixture database is cleaned up after the check.
Artifact occurrence IDs now require RFC-variant UUIDv7 values. The Artifact owner supplies
neutral and server-presented `ArtifactUri` variants through foundational component builders.
Metadata derives its internal ID from that URI, checks repeated wire identity, and keeps
valid 0.1.x JSON fields. Resolution interfaces, Map artifact references and publication/product
Store inputs, Optimization model inputs, and Speech source references use the typed address.
Foundation, Artifact, Speech-contract, and MCP suites pass 215 cases, including URI/UUID
admission, schema compatibility, and compile-fail examples. Migrated consumer libraries pass
302 cases, including Artifact HTTP resolution and malformed-input rejection. Thirteen database
cases are ignored in that library command; Artifact publication/readback through separate
SurrealDB connections passes separately, and the other twelve are not counted as acceptance.
Map's two native Store cases pass with typed Artifact projection inputs and SQL visibility
before limits. The separately resolved Artifact/Speech consumer passes five checks and excludes
MCP, runtime, database, GPU, and provider dependencies. An additional Map regression
rejects server presentations in all three neutral-only release reference fields. Strict
workspace Clippy and Rustdoc pass.
External dependency versions, sources, and checksums are unchanged. Cleanup removed 37
superseded test executables and reclaimed 28.7 GiB; build caches and Docker images were preserved.
Map and Frames now expose independent contract features. Map owns geodetic IDs;
Frames owns worlds and conversions; RRD owns its recording-specific spatial metadata.
MCP core has no coordinate domain dependency. Typed world/revision/frame addresses use
the shared URI builder. The independent consumer passes ten schema, admission, and
metadata checks. Strict runtime-only and workspace Clippy checks pass. Affected
libraries pass 400 cases, including native Map/UAV SQL cases and both million-feature
Map index checks. Eight coordinate contract cases preserve 43 baseline schemas and
qualify ID/URI admission. The Frames adapter's eight cases, Map metadata's four cases,
and the Map-to-Optimization wire test pass. Three compile-fail examples reject wrong
ID and parent types. Strict workspace Rustdoc passes. Cleanup removed 33 superseded
test executables and reclaimed 17.8 GiB while preserving compiler caches and ordinary
server binaries. Installed acceptance is pending for this extraction.
Frames now owns typed world read queries over the existing Store connection. SQL
enforces tenant and current labels, verifies linked revision parents, and requires
head pointer/key/number agreement. Direct frame resources select their node in SQL.
Four isolated SurrealDB cases pass through separate write/read clients, covering
shared tenant visibility, changed clearance, wrong or deleted parents, inconsistent
heads, and node selection without decoding unrelated data. All 15 Frames library
cases, 21 affected adapter/contract cases, ten independent-consumer cases, and strict
workspace Clippy pass. The world catalog now returns 100-item keyset pages with typed
world cursors and resource builders. Two native cases traverse 126 visible worlds
behind 220 denied rows, recheck clearance between pages, and qualify SQL completion
across 125 revisions and 128 frame nodes with their required visible parents. Discovery
uses fixed roots and templates without Store reads; data changes invalidate contents
without announcing list changes. The shared workbench's headless paging behavior passes.
All 17 Frames library cases and 11 adapter cases pass. Twelve independently resolved
consumer cases preserve contract isolation, and four compile-fail cases reject wrong
ID types. The native Frames MCP smoke passes world authoring, paged catalog reads,
conversion, batch tasks, Artifact access, and usage reads. Strict workspace Clippy passes.
Installed acceptance is pending, including the declared catalog array-to-page
coordinated upgrade.
Native Task identity now belongs to `veoveo-types`; Store owns its explicit UUID record
adapter and all consumers import the foundational type directly. UUID wire admission
and generation are preserved, including TaskRuntime's external v7 check. Seventy-five
unit cases and 32 native database cases pass across identity, lifecycle, recovery,
subscriptions, Store bindings, and Media webhooks. The independent foundation consumer
passes 13 cases and verifies schema/JSON admission on 44 samples; the Map/Frames consumer
passes 12 cases with no runtime dependencies. Foundation's 13 doctests and strict
workspace Clippy pass.
Frames usage now uses typed Task cursors, checked page construction, and foundational
URI builders. TaskRuntime owns SQL selection under the linked Task's current owner
policy: server, tenant, principal, profile, labels, and matching stored owner metadata
are checked before grouping and limits. Exact reads use the same predicates, while
subscription admission can precede the first usage row. Two native cases qualify 126
visible Tasks behind 140 denied Tasks, duplicate usage rows, cursor replay under changed
authority, deleted parents, and conflicting metadata. The independent Map/Frames consumer
passes 15 cases without database, MCP, or async runtime dependencies. A SurrealDB 3.2.4
compound-index range repeated the cursor's anchor during qualification. Scalar query
bindings select the server/Task index, and an explicit SQL inequality excludes the
anchor even when the Task/time index is selected. Requalify this case during Phase 4's
database upgrade. All 17 Frames library cases, 11 adapter cases, three usage contract
cases, and five compile-fail cases pass. The native MCP smoke qualifies batch usage,
paged resource decoding, and wrong-principal/tenant/profile denial. Strict workspace
Clippy passes. Installed catalog acceptance remains work.
Frames now owns operation queries, driver records, immutable recording transactions,
and typed operation scopes. SQL applies principal, tenant, profile, labels, and current
Task-parent agreement before returning provenance. Recording checks the Task inside the
same transaction as the operation and outbox event. Native database cases cover direct
isolation and concurrent replay, Task authority changes, required authority fields,
and rollback/retry after injected event-publication failure. The UUIDv7 storage-admission
case also passes. Rust and Store require current operation authority; the optional
historical storage profile and its preservation fixture are removed. Operation addresses
use the foundational builder, and checked provenance construction/decoding enforces
ID/URI agreement without changing existing schemas. Sixteen independent contract
consumer cases pass without runtime dependencies, and six compile-fail cases pass. The
native MCP smoke passes direct and Task-backed operation reads and denial under a
different principal, tenant or profile. After removing the historical storage profile,
the Frames suite passes 60 native cases and eleven compile-fail examples. Nine Store
schema checks, three SQL validation cases, the Frames MCP smoke, and strict all-target,
all-feature Clippy for Frames and Store pass. Installed operation acceptance is pending.
Frames metadata now derives repeated world/revision IDs from typed addresses. Immutable
revision construction requires a positive publication number and a complete admitted
tree; decoding verifies root membership and the canonical SHA-256 digest. Summary heads
and source references have checked construction and decoding. The existing tree validator
now belongs to the contract feature, and UAV consumes that implementation. Native reads
reject corrupted stored roots, digests and trees after SQL visibility checks. The maximum
10,000-node depth case passes without recursion; completed parent paths are memoized.
Twenty-two independent contract cases preserve the published schemas and exclude runtime
dependencies; eight compile-fail cases pass. The affected native library and adapter
tests, Frames MCP smoke, and strict workspace Clippy pass. Installed consumers and
retained-metadata preflight remain qualification work.
Frames world creation and publication now belong to the domain runtime. The old Store
draft APIs, domain record IDs and public driver records are removed. Mutations consume
typed requests and scope labels; publication applies owner, tenant, labels and head
agreement inside the transaction. Five native cases cover concurrent creation/replay,
competing publications, current caller authority, damaged parents and atomic event
failure rollback. The broader affected library regression passes 486 cases with three
existing ignored cases. All 11 Frames adapter cases, nine compile-fail checks, strict
workspace Clippy, and the native Frames MCP smoke pass. Cleanup removed 5.7 GiB of
superseded test executables while preserving build caches; the host has 282 GiB free
after qualification, with no Docker containers running. Frames declares no domain scope vocabulary today; gateway policy
admits actions and Frames enforces its stored world/operation/Task policies. Installed
publication and the coordinated writer upgrade remain work.
Frames dynamic transforms now carry checked concrete producer references and bounded
entity selectors. URI parsing preserves valid source spelling without importing producer
domains. Twenty-six independent contract-consumer cases preserve schemas and the existing
UAV scenario; 132 native Frames/UAV/View cases and eleven compile-fail checks pass.
Native database qualification rejects malformed retained references after SQL visibility
checks and verifies that reads leave stored values unchanged. Strict workspace Clippy
and the Frames MCP smoke pass. Cleanup removed another 3.3 GiB of superseded test
executables while preserving libraries and build caches. Installed dynamic-reference
acceptance and retained-data preflight remain work.
MCP core now exposes open server associations and checked setup without a domain
registry. Time uses the setup for startup, discovery, and typed scope membership.
An independent fixture owns its scope and resource family in a separate library and
passes hosted conformance plus protocol-specific access and denial checks. The native
contract, conformance, fixture and Time suites pass 226 cases, including SQL isolation
and App descriptor preservation; ten compile-fail examples pass. A separately resolved
consumer passes 13 fixture and Time cases across 71 packages with no MCP, asynchronous
runtime, database, provider or GPU dependencies. Strict workspace Clippy passes.
Cleanup removed 6.2 GiB of superseded test executables while preserving compiler
libraries, incremental data and the contract-only test caches. Wider server adoption
and installed qualification remain work.
Checked setup now requires a distinct `ResourceTemplateUri` and a typed template
descriptor builder. The foundation pins iri-string 0.7.14 for its ASCII RFC 6570
profile and expansion; admission guards reject invalid prefix lengths and dotted
variable names that the upstream parser accepts. Time's declarations expand to its
existing typed addresses, including reserved zone paths and collection cursors.
Native checks pass 259 cases and 25 Rustdoc examples, including 24 compile-fail cases.
The separate 72-package consumer passes 19 cases without MCP, database, provider,
GPU or asynchronous runtime dependencies.
Strict workspace Clippy passes. Cleanup removed 21.4 GiB of superseded test
executables while preserving libraries, fingerprints, incremental data and build caches.
Gateway completion and template discovery use `PolicyTarget::ResourceTemplate`.
The shared evaluator preserves lexical selectors, rejects declaration targets for reads,
and keeps ownership, scope and deny checks in force. Policy audit writes and reads
require the current v2 format and matching event/decision targets. The historical DTO
reader is removed. Four native cases qualify current targets, invalid formats, redacted
errors and separate Store connections; invalid new events add no row. Time's obsolete
zone-template adapter and support window are also removed. Its four native MCP tests
qualify current discovery, typed addresses and schemas. Installed checks remain pending.
Cleanup removed 14.6 GiB of superseded test executables and kept the newest binary for
each target, current qualification binaries, libraries, fingerprints and compiler caches.
Time now owns its queries, mutation drafts and private driver records over Store's
connection. Domain IDs, calendar/epoch versions, completion parents and collection
cursors stay typed until driver conversion. Public bootstrap names and the stored
UUIDv7 key profile keep their existing admission rules. Two isolated database cases
qualify SQL visibility, paging, completion and competing authority activations;
27 expanded query variants pass the pinned CLI validator. Time and Store checks pass
91 native cases and eight compile-fail examples; two existing Store cases are ignored
and are not counted as acceptance. The 72-package independent consumer passes 19 cases
without runtime dependencies. Runtime-only and workspace Clippy pass. Cleanup removed
the obsolete Time catalog test executable and reclaimed 0.73 GiB without touching build
caches.
Time catalog decoding now checks JSON identities, immutable versions and indexed
metadata against physical records. Separate-connection corruption cases reject
inconsistent visible reads and pages while SQL still excludes other tenants and event
owners. Historical lifecycle columns supply current release/acquisition/event state;
reads leave retained bodies unchanged. Acquisition updates reject source, digest and
creation-time changes before writing, and competing updates preserve the version fence.
All 48 native Time cases and eight compile-fail examples pass; runtime-only and workspace
Clippy pass. The owning design declares retained-data preflight, drain and snapshot
rollback. Installed transition qualification, active-pointer relationships and broader
public DTO types remain work.
Time clock policies now use a checked builder, matching JSON/schema bounds and checked
stored-scalar decoding. Positive update versions and explicit absent-row guards preserve
numeric wire forms; every lifecycle increment rejects exhaustion. Native qualification
covers corrupt clock scalars, competing replacements and rollback of authority activation
when the previous release cannot advance. All 53 native Time cases and eleven compile-fail
examples pass. The separately resolved 73-package consumer passes 23 checks without
runtime dependencies. Four expanded activation transactions pass the pinned CLI validator.
Runtime-only and workspace Clippy pass. Installed numeric admission and rollback qualification remain pending.
Time active-pointer reads now resolve releases in one SQL statement with tenant,
family, key and active-state predicates. Checked pointer history and identity reject
inconsistent visible records instead of omitting them during reload. Activation
rechecks candidate state, pointer identity/history and previous-release relationships
inside its transaction. Native cases cover damaged and missing links, denied release
payloads, both authority families, and ten transactional relationship changes with
complete rollback and a successful retry. The native Time suite passes 55 cases and
eleven compile-fail examples. The joined read, activation and fixture statements pass
the pinned CLI validator. Runtime-only and workspace Clippy pass. Installed parent
admission and rollback remain pending.
Time tenant engines now read the stored pair and producing provenance before cache
reuse. Cache keys carry the typed Store tenant; each request receives a separate epoch
map. Store signals evict contexts, and requests refresh even when notification delivery
is disconnected. Native cases cover a fresh replica, both families, tenant isolation,
cache reuse, damaged metadata/provenance, file-load failure and recovery, and native
LIVE/reconciliation. Exact release metadata reads remain available independently of
engine loading. Event-page recovery shares one validated engine across new watchers;
existing and terminal events skip authority loading. Watcher keys retain typed tenant
and event IDs. All 58 native Time cases and eleven compile-fail examples pass.
Runtime-only and workspace Clippy pass. Installed restart/replica and activation qualification remain work.
Download URL typing, other URI families, and remaining service interfaces need further work. These model changes preserve valid
persisted representations and current authorization rules; reference installation qualification
is pending.
Optimization usage now selects the current owner and Work Context in SQL before
limits. TaskRuntime exposes an explicit access policy and checks indexed context,
retained authority and owner-envelope agreement. Frames, Timeseries and DuckDB
explicitly retain their owner-only policy. Optimization exposes typed usage builders,
cursors and checked pages through its contract feature; all Rust imports use the
`contract` module, including Map's lightweight travel-model consumer. Valid cursor
and report fields are preserved. Unscoped Store usage reads and post-read ownership
helpers are removed. Four affected server suites pass 151 cases and 15 compile-fail
examples; TaskRuntime's three usage cases pass separately. The independent 90-package
consumer passes five checks, preserving eleven solver schemas without service
dependencies. Map's consumer, runtime-only and workspace Clippy, and documentation
checks pass. The GPU solver test is ignored and does not count as acceptance.
Cleanup removed 15.8 GiB of superseded executables, leaving 226 GiB free while retaining
compiler libraries, incremental data and needed binaries. Other Optimization query
families and Media's separate usage ledger remain work; installed qualification is pending.
Media now owns SQL-scoped usage and prediction reads. Current Task owner policy,
optional-tenant spelling and linked provider identity are checked before grouping or
limits. Usage and prediction catalogs return typed 100-entry pages; discovery declares
fixed roots/templates and data changes invalidate contents. Billing recovery selects
unsettled terminal jobs in SQL and correlates settlement by Task, tenant and native job.
The Media contract feature owns generation result DTOs, provider identity, addresses,
cursors and checked pages; MCP core no longer owns the generation DTOs. The independent
88-package consumer passes eight cases without service dependencies and preserves both
generation schemas. Forty-one native library, adapter, contract and database cases and
four compile-fail examples pass. Thirty-eight expanded query/fixture statements pass
the pinned CLI validator. Four Workbench headless paging cases pass. The shared regression suites pass 224 cases and nine compile-fail examples.
Runtime-only and strict workspace Clippy pass. Native Media MCP smoke qualifies
generation, signed-webhook completion, resource notifications, Artifact access, billing
after cancellation and typed catalog isolation. The CLI now drains its SDK subscription
handle, and the harness checks notification diagnostics on stderr. Installed acceptance requires
current catalog consumers on the rebuilt reference installation. Model paging,
remaining request/URI types, scopes, checked MCP setup and provider recovery budgets
remain work.
Cleanup removed 42 superseded executables (19.8 GiB) and both failed-smoke fixtures.
Current binaries, compiler libraries, incremental data, images and build caches are
preserved. The host has 208 GiB free and no running Docker containers.
Media's completed generation resource uses the typed
`media://prediction/{id}/result` address and checked
`veoveo.ai/media-generation/v1` contract. SQL selects the linked successful Task
under current owner and provider-parent predicates before its limit. The reader
requires the current complete result and checks its native Task parent and output
attribution. The historical decoder and result-conversion path are removed.
TaskRuntime now applies owner policy in SQL for exact public Task reads and
subscription baselines/delivery. Indexed and envelope identities must agree before
decoding. Public notifications select the current Task from payload-free outbox hints;
they cannot replay an older result under historical authority. Native checks cover
revocation, malformed denied Tasks and an entire denied replay page. Trusted internal
event replay preserves every transition. Shared owner bindings also serve usage and
collection reads. Eleven lifecycle/recovery tests now run by default in disposable
fixtures with 60-second deadlines and separate clients for replica cases. Thirty-eight
TaskRuntime cases and one compile-fail example pass, including the explicitly run
query-plan case. Five expanded SQL queries, strict workspace Clippy and native Media
MCP smoke pass. The installation must replace hosted replicas together to establish
current-owner delivery; retained-owner preflight and installed qualification are pending.
Media publishes the checked current generation result with one top-level `result_uri`,
one result link and identity-free terminal text. Task reads and subscriptions validate
the linked generation before returning the stored payload. The CLI consumes the
server-owned contract and verifies completion against its result resource, keeping
opaque gateway Task handles separate from native Task IDs. The old-binary rollback
smoke option is removed; the native harness restarts the current server and verifies
the same stored generation. The Media suite passes 50 native tests and six compile-fail
examples. Direct Media MCP and Gateway Task smokes pass, including CLI downloads,
current-format restart and opaque gateway Task handles. Installed current-format
acceptance remains pending.
Optimization now owns problem, run, solution and completion selection in its runtime
library. SQL matches indexed ownership and Work Context against retained owner and
authority fields before limits or grouping, preserving absent versus explicit
installation tenants. Selected malformed records fail explicitly. Typed domain IDs
reach driver binding, and collection builders preserve version 1 cursor bytes while
checking native Task identity and collection membership. MCP handlers use these readers;
the binary's separate query and retained-request modules are removed.
Optimization's native database tests own completion bounds and selection behavior.
The generic conformance suite's obsolete source-text probe of the removed binary
index module is removed; domain SQL assertions stay in their owning harness.
Thirty-nine Optimization cases and three compile-fail examples pass, including denied
rows ahead of full pages, changed clearance between continuations, mismatched ownership
fields and malformed selected results. Ten expanded SELECT forms pass the pinned
SurrealDB 3.2.4 validator. The independent 90-package contract consumer passes seven
checks without MCP, runtime, database or HTTP dependencies. Runtime-only and strict
workspace Clippy pass. These checks qualify database and contract behavior; the GPU
solver case is ignored. Retained-record preflight and coordinated installed replacement
remain pending. Other resource families, typed scopes and checked MCP setup remain
work. Disposable fixtures have stopped, the cluster stays stopped and the host has
179 GiB free.
Optimization's contract now owns every hosted resource variant and its concrete URI
builders. Handlers parse once and dispatch exhaustively, while problem and solution
loaders preserve typed addresses. Output identities require canonical RFC-variant
UUIDv7 spelling; solver profile names use unreserved path characters. The documented
retained-data preflight covers previously accepted aliases and malformed identities.
Checked MCP setup supplies startup and discovery without changing App metadata, fixed
resource spelling or public solver schemas. The domain scope vocabulary is explicitly
empty, preserving gateway operation policy instead of introducing permission gates.
Verification's prepared-problem lookup now uses the domain SQL reader rather than
scanning Tasks and filtering ownership in Rust. Forty-two package cases and five
compile-fail examples pass. The independent 90-package contract consumer passes eleven
checks without service dependencies, including expansion of all fifteen templates.
Map's travel-model consumer, runtime-only and strict workspace Clippy pass. Map
reference ownership and DTO relationship admission remain work; installed recovery,
reverse/forward replacement and GPU acceptance are pending. All disposable fixtures
have stopped. The cluster stays stopped with 172 GiB free; compiler and build caches
are preserved.
Map now owns travel-model addresses and collection positions in its contract feature.
Optimization imports the Map type directly; the compatibility test belongs to the
consumer, and MCP core has no dependency on Map. Builders retain model IDs through
production and resource reads, require canonical UUIDv5/v7 spellings, and validate
record/URI and manifest/Artifact identity agreement.
Travel-model exact reads, pages and completion now use a Map-owned SQL reader. Indexed
ownership, the Task owner envelope, retained request identity and successful result must
agree before selection limits or grouping. Collection reads return 100-item pages with
version 1 native Task cursors. The documented coordinated upgrade replaces array readers
and preflights retained identity and ownership; stored Tasks and Artifacts are unchanged.
The full Map suite passes 129 cases and six compile-fail examples. Optimization passes
44 cases and five compile-fail examples; its GPU case is ignored. Native database cases
cover 125 visible models behind denied rows, changed clearance across continuations,
28 inconsistent ownership/result fields, duplicate identities and malformed selected
records. Four expanded queries pass the pinned SurrealDB 3.2.4 validator. The isolated
140-package consumer passes fourteen checks and excludes MCP, async runtime, database
and HTTP dependencies. Both runtime-only libraries and strict workspace Clippy pass.
Installed recovery, traversal and reverse/forward replacement remain pending. Other
Map URI families and DTO relationship admission remain work. Disposable fixtures have
stopped; the cluster stays stopped with 156 GiB free and build caches preserved.
Map now owns restriction exact reads, completion and 100-item catalog pages. Typed
addresses and collection-bound version 1 cursors keep restriction IDs through driver
binding. Pages carry checked compact summaries and exact resource links; full geometry
comes from the exact read. Canonical UUIDv5/v7 spelling is enforced for restriction and
cancellation IDs. Selected documents must agree with indexed identity, kind, effect,
families, validity, withdrawal and version fields.
Routing, travel-model construction, reachable-area calculation, validation and spatial
derivation apply tenant, time, cancellation and mobility-family predicates in SQL.
Corridor inspection selects all families at its departure time. The old unbounded Store
restriction list and duplicate completion branch are removed. Queries have a five-second
database deadline; operations above 10,000 effective restrictions fail before applying
constraints. The existing tenant-wide read policy is preserved.
Map passes 137 native/contract cases and nine compile-fail examples. Four native database
cases cover pages behind denied records, changed tenant visibility, half-open validity,
withdrawal, nine retained-field corruptions and selection beyond 10,001 irrelevant rows.
The over-limit case proves constraints are never silently truncated. Fixture insertion
uses batches of 250 after a single large write hit the driver's WebSocket buffer limit;
diagnostics suppress encoded request dumps. Five query forms pass SurrealDB 3.2.4 CLI
validation. The isolated 140-package consumer passes eighteen checks without service
dependencies, and the remaining Store completion case passes. Runtime-only and strict
workspace Clippy pass.
The owning design declares the coordinated array-to-summary-page transition, retained-data
preflight and rollback. Installed acceptance and other Map catalog roots remain pending.
Map's routing authority reader now selects active pointers, releases and enabled sources
in one SurrealQL statement. SQL checks tenant and dataset agreement, active state,
departure validity and compatible map families. Routing, matrices, travel models and
reachable areas consume its complete typed release/family sets. The reader validates
selected documents and pointer identity; it replaces full source/release scans and one
active-pointer lookup per release. Other public catalog roots remain work.
Three native database cases cover 125 eligible releases behind 110 foreign records,
pointer replacement, source disablement, exact validity boundaries, seventeen excluded
relationship/lifecycle cases, twelve inconsistent documents and invalid pointer identity
and version. The full Map suite passes 140 cases and ten compile-fail examples. The
production statement passes the pinned SurrealDB 3.2.4 validator. Runtime-only and strict
workspace Clippy pass. The owning design declares retained-data preflight, coordinated replacement and
rollback; installed routing acceptance remains pending.
Resource cleanup removed eighteen superseded Map, Optimization and Store test executables,
reclaiming 10.2 GiB. The newest two large executable variants per selected test target were
preserved, along with service binaries, compiled libraries, incremental data, build caches,
images and volumes. Both worktrees are preserved and the reference cluster stays stopped.
Map source exact reads, pages and completion now belong to Map's typed catalog reader.
Tenant selection precedes SQL limits, and selected documents must agree with indexed
identity, dataset, name, adapter, authority, families, enablement and version. The public
contract owns source addresses, version 1 collection cursors and checked summaries that
preserve the existing public fields while excluding acquisition endpoints and secret
references. The unbounded Store source list and Store source completion branch are removed.
Map Explorer and the browser acceptance fixture consume the 100-item page envelope.
Two native database cases cover 125 visible records behind 110 malformed foreign rows,
current-tenant continuation, disabled-source visibility, completion bounds and literal
search text, public-field redaction, ten document corruptions and a wrong physical ID.
The full Map suite passes 147 cases and thirteen compile-fail examples, including both
million-feature Spatial checks. Eleven App behavioral tests and four expanded statements
under the pinned SurrealDB 3.2.4 validator pass. The independent 140-package consumer
passes 23 checks without service dependencies. Store completion, runtime-only Clippy
and strict workspace Clippy pass. The owning design declares source-ID and
retained-document preflight, a coordinated server/client replacement and rollback.
Installed paging, reverse/forward replacement and other Map catalog roots remain pending.
Map mobility exact reads, pages and completion now use a domain-owned SQL reader.
Profile ID and numeric version form the keyset; tenant and completion-parent predicates
run before limits. Selected profiles must pass their domain validators and agree with
physical identity, indexed family, name, version and validity. The public contract owns
profile URI builders and version 1 composite cursors. `MobilityProfileVersion` carries
positive signed-integer-compatible values through metadata, routing/matrix requests,
spatial derivations and travel-model provenance, preserving the numeric wire shape.
Map's route handoff now carries the typed profile URI. The Store list/completion branches
and Map's string profile URI helpers are removed. Map Explorer and the browser acceptance
fixture consume the page envelope while retaining complete profile documents.
Two native cases traverse 230 visible versions behind 110 malformed foreign rows, check
numeric ordering, parent-bound completion, current tenant changes and historical-version
visibility, and reject eleven malformed document/index/identity cases. The full Map suite
passes 153 cases and sixteen compile-fail examples, including both million-feature Spatial
checks. Twelve App behavioral tests and five expanded statements under SurrealDB 3.2.4
CLI validation pass. The independent 140-package consumer passes 27 checks without service
dependencies. The UAV route-handoff wire test, remaining Store completion test, runtime-only
Clippy and strict workspace Clippy pass. The owning design declares retained-ID/version preflight, coordinated
server/client replacement and rollback. Installed paging and reverse/forward replacement
remain pending.
UAV grants and mission plans now consume Map's profile address and handoff types from
its contract-only library. The copied handoff DTO and manual UAV/flight-client profile
parsers are removed. UAV explicitly rejects stale, invalidated and unavailable routes;
Map's position decoder preserves strict field admission. Profile and advisory approval
join existing grant predicates in SQL before the limit. Execution repeats that selection
and checks a matching grant in its admission UPDATE after lease acquisition. Selected
plan documents must agree with indexed metadata and physical identity. The flight client
keeps profile IDs and versions typed through scenario/grant decoding and route serialization.
Focused native checks pass for selection behind 110 ineligible grants, current-grant
revocation, lease release after rejected admission, thirteen metadata corruptions and
wrong physical identity. Foreign callers do not decode denied malformed rows. The full
affected suites pass 262 cases, including both million-feature Spatial checks and sixteen
compile-fail examples. The flight graph check admits server libraries only with their
isolated contract feature and rejects database/recording implementations. Its separate
warm-dispatch timing case is not run. The independent consumer passes 27 checks. Strict
workspace Clippy and documentation checks pass. Both changed SQL statements pass the
pinned SurrealDB 3.2.4 validator. Six superseded test executables were removed, reclaiming
3 GiB while preserving compiler caches and application binaries.
The owning design declares retained-reference preflight, coordinated replacement and
rollback. Installed qualification and UAV's isolated contract feature remain pending.
Native reproduction confirmed that an overwritten pending command lease could still
admit a mission. UAV now acquires its lease and admits the plan in one transaction,
using the shared vehicle record as the write conflict on SurrealDB 3.2.4. Finalization
checks the admitting token and settles the plan with its lease release in a transaction.
Lease revisions increase across replacements, and expiry cannot displace any executing
plan for that vehicle. Native RocksDB contention across principals, obsolete-token,
expiry, overflow and injected rollback cases pass. The UAV executing-vehicle migration adds the composite
vehicle/state index without changing records. EXPLAIN confirms IndexScan on that index.
The full UAV suite passes 51 checks, Store migration tests pass nine, and strict workspace
Clippy passes. The transactions, lookup and index validate under the pinned 3.2.4 CLI.
The owning design requires every replica and worker to drain before this coordinated
upgrade or rollback. Rollback builds must retain the additive index and current Store
catalog; restoring an older installation snapshot requires all database writers drained.
Retained formats are unchanged. Installed qualification remains pending.
UAV dispatch now consumes its admission guard, and only a correlated simulator completion
can release a dispatched mission's lease. Cancellation, Task lease loss, HTTP rejection,
timeout, redirects and invalid completion preserve the executing plan and vehicle fence.
The request client disables automatic retries and redirects. Completion checks operation
kind, mission/session identity, lifecycle, waypoint count and timestamp order before it
settles the plan. Recording lookup and Task result publication follow physical settlement;
failures in either cannot mark a completed mission failed. Queued mission recovery reports
an interrupted outcome without replay or a startup decoding failure. Task IDs stay typed
through worker calls. Seven native HTTP/Store cases cover continued remote work after
HTTP cancellation, lease loss, mismatched replies, catalog failure, cancellation races,
transaction rollback and queued recovery; the full UAV suite passes 58 checks and strict workspace Clippy passes. The fixture
owns its background jobs and disposable Store containers and supplies no GPU acceptance.
UAV now creates and pins the queued Task before admission. A domain-neutral TaskRuntime
transaction guard checks the unchanged queued Task while UAV commits its exact Task link,
plan transition and vehicle lease. Mission reads select that link in SQL, so a later
rejected attempt cannot hide the admitted Task. Unknown outcomes retain the Task past
normal expiry. Startup selects settled or never-admitted Tasks in SQL before pagination
to repair missed pin acknowledgements; it preserves other consumers' pins and never
replays simulator work. Native checks cover cancellation, atomic rollback, mismatched
Task input/link retention, and more than a page of ineligible Tasks before eligible
cleanup records. Admission, settlement, correlation and retention SQL use the pinned
SurrealDB 3.2.4 profile. The current Store schema requires exact execution links;
historical execution profiles and their conversion fixtures are removed. Mission reads,
pages and completions require the same exact Task link and current authority in SQL
before selection limits. Catalog fixtures use real admission and settlement; rejected
attempts cannot publish or replace a mission. The UAV suite passes 87 native checks
and six compile-fail examples, nine Store migration checks pass, and ten complete SQL
statements validate. Strict all-target, all-feature Clippy passes for UAV and Store.
Complete observations retained across process loss, installed recovery and operator
reconciliation still need implementation or qualification before deployment.
UAV now exposes contract, runtime and MCP features with optional dependencies.
Its public library owns the live-view v4 types; MCP core has no live-view exports or
dependency on UAV. Gateway output-policy conversion stays in UAV's authenticated
adapter and preserves classification and labels. The flight harness imports UAV's
camera, grant and page models, and decodes session and vehicle IDs through their owner.
The independent consumer passes three checks, preserving all 97 pre-extraction schemas.
Its 134-package graph excludes MCP, async, database, adapter and GPU dependencies.
Contract-only unit, public-consumer and compile-fail checks pass 15 cases. The combined
UAV, MCP-core and flight-client suite passes 282 cases; the separately invoked warm
dispatch check reaches scenario validation in 0.82–0.84 seconds without compilation
or cluster access. Runtime-only and workspace strict Clippy pass. Wider scope/resource
adoption and installed acceptance remain work; this extraction does not settle the
retained-completion recovery gap.
UAV scope checks now use the server-owned `UavScope` enum across tools, resources,
completion, subscriptions and Task admission. Flight and browser token requests import
the same vocabulary. The Task path now requires the same administrative scope as
ordinary scenario and dataset-capture calls before creating a Task or dispatching work.
The affected UAV and client suites pass 164 checks. The routed regression qualifies
ordinary and Task-enabled calls, including rejection before persistence, authorized
completion and rejection of missing mission plans. All 16 scope-grant combinations preserve
their decisions with unrelated external grants. The independent consumer passes four
checks, preserving the 97 existing schemas, and four contract-only compile-fail examples
pass. Strict workspace Clippy passes. Installed replacement of every UAV replica and
a rollback build preserving the Task guard are required.
UAV resource addresses and collection cursors now belong to the isolated contract.
Reads and subscriptions use typed variants and shared permission checks. The URI
builder supplies notifications and response references, and live-view reads check
both the current session and the view's retained session. Version 1 cursor bytes
are preserved, with session binding on active grants and live-view pages. Relative
ID and normalized URI admission are stricter; the owning design declares retained-data
and producer preflight, a coordinated drain, and rollback requirements. Prompts validate
session/mission IDs and publish typed resource addresses. The affected native suites
pass 172 cases, including SQL catalog pagination, read/subscription authority and live-view
parent checks. The final prompt regression passes separately. Eleven independent-consumer
checks preserve all 97 published schemas and qualify each route, cursor family, malformed
component and relative ID. Its 134-package graph excludes MCP, async, database, adapter
and GPU dependencies. Six contract-only compile-fail examples and strict workspace Clippy
pass. Checked MCP setup, broader DTO relationships and cross-language URI construction,
including the browser acceptance script, remain work. Installed qualification is pending.
UAV now implements the shared MCP setup trait using its own scopes and resource types.
Startup checks declarations before connecting to Store or recovering work. Handlers use
the checked configuration, documents, resources and templates; App CSP and caller targets
keep domain ownership. The foundation supplies scalar template expansion through its
existing URI library without a new consumer dependency. Local and reference gateway
registrations declare revision 3, closing C17. The combined foundation, UAV and client
suites pass 223 checks. Every one of the 22 templates expands to its typed builder;
optional cursors preserve collection binding. Discovery passes all 16 scope combinations
with an unreachable simulator, including App CSP and caller metadata. Final registration
and embedded-declaration checks pass for both gateway files. The independent consumer
passes 11 checks, preserving 97 schemas without runtime dependencies, and strict workspace
Clippy passes. Installed readiness, recovery and GPU acceptance remain pending.
Reason and Stream now expose contract, runtime and MCP features with optional
implementation dependencies. The existing recorded-video library separates selectors
and captured source identities from authorized materialization. Reader conversions
stay in its runtime module, preserving source order, optional fields and snapshot
hashes without exposing local paths. Three independent consumers pass nine public
checks; the Linux graphs contain 33 packages for video and 68 for each server, with
no MCP, async, database, Rerun or GPU dependencies and no Chrono clock. Reason's 25
and Stream's 37 captured schemas are unchanged. The flight client imports Stream's
contract feature instead of including its source file, and its dependency checks
continue to reject service execution. Native video, Reason, Stream and flight suites
pass 110 checks; runtime-only and workspace strict Clippy pass. Six image source-input
checks pass, and warm flight dispatch reaches validation in 0.90–0.92 seconds without
cluster access. Removed 17.23 GiB of obsolete linked executables and the temporary
schema exporters while preserving dependency and incremental caches. Domain IDs,
resource builders, checked setup and cross-server grounding ownership remain work.
Installed and GPU acceptance are pending; these native fixtures provide control-plane
qualification.
The full Rust enforcer passed at `ab61a602`; default-feature workspace acceptance
and reference installation qualification are pending.

Reason now owns distinct pipeline, model and analysis IDs, typed resource addresses,
the version-1 analysis cursor and an empty domain scope vocabulary in its isolated
library contract. Catalogs, requests and execution carry those IDs. Resource dispatch
uses the typed route variants, and direct analysis/result reads and subscription
admission use the shared SQL owner read before decoding. Native tests exclude seven
malformed denied-row cases and preserve the 25 captured schemas and cursor bytes.
Checked MCP setup validates startup declarations and catalog descriptors; both gateway
registrations declare revision 3. Static discovery stops advertising Task-driven
list changes. The native suite passes 36 checks; the independent consumer passes
seven and its 68-package graph excludes runtime dependencies. Runtime-only and strict
workspace Clippy pass. The subsequent resource-notification migration replaces
Reason's process-local hub with the shared authorized Task watch. The foundational
`TaskResourceAddress` trait carries the owner-defined relationship; Reason's isolated
`AnalysisResource` admits only analysis and result routes. One subscription combines
backing Tasks and explicit Task handles, while resource-only listeners receive no
Task payloads. Native foundation, Task runtime and Reason suites pass 129 checks with
no ignored cases, including the required query-plan fixture. Connection generations
now force a current-owner SQL baseline when the LIVE source reconnects. A native TCP
outage test removes event history, revokes access to a malformed Task, and verifies
recovery of only the authorized resource. It fails against the previous watch and
passes with the fix, including no idle invalidation across a full reconciliation
interval. Cross-replica delivery, client reconnects and official MCP listener
cancellation also pass. The independent Reason consumer passes eight checks;
strict workspace Clippy passes. Removed 8.60 GiB of obsolete linked executables
while preserving dependency and incremental caches. Reason catalog views now derive
their IDs from typed addresses. Terminal output derives both analysis addresses from
one ID, and analysis views check nested output against the Task and requested pipeline.
Wire decoding rejects conflicting identities while preserving the 25 schema snapshots.
Authorized resource reads report malformed retained success as a recovery error;
explicit tool errors still have no product. The independent consumer passes 12 checks;
the native Reason suite passes 43, and strict all-target, all-feature Clippy passes.
Broader result/reference typing, Stream's typed resource migration, grounding
ownership and installed/GPU acceptance remain work. Reason's C02 declaration is corrected
to pending because its terminal envelope lacks the required canonical `result_uri`.

Reason publishes `veoveo.ai/reason-analysis/v1` with canonical `result_uri`,
identity-free status and one result link. Task and resource reads validate the current
output against its owning Task and requested pipeline after SQL owner selection.
The installed smoke imports the owning contract and dereferences the result link.
The v0 reader and its compatibility fixtures are removed by the hard-cut correction.
Current-format cross-replica delivery and listener reconnects remain native checks.

Reason grounding now consumes Stream's complete result model through its contract-only
library. Stream owns the typed Artifact URI and portable result validation; Reason
requires matching recording, entity and timeline and a covering replay range, then
admits citations from selected frames. The authorized Artifact read supplies labels
and classification to the output write capability. Its service persists and enforces
those requirements. No historical grounding admission ledger or migration is needed.
Native Reason, Stream and shared-video suites pass 90 checks. Three Artifact service
checks qualify mandatory output labels, caller clearance and persistence across service
instances. Independent contract consumers pass 17 Reason and six Stream checks without
MCP, Store, async or GPU dependencies. Strict all-target, all-feature Clippy passes for
both servers and their smoke/flight consumers. Installed and GPU acceptance remain
pending.

Stream's contract now owns distinct pipeline, model, run and live-session IDs,
typed resource addresses and collection-bound cursors. The catalog, native runner
protocols, worker calls and flight acceptance client carry those types. Resource
reads dispatch one parsed address; run reads and subscription admission select the
caller-owned Task in SQL. Public tests reject noncanonical addresses, invalid IDs,
wrong-collection cursors and unknown cursor fields. Stream, Reason and flight native
suites pass 127 checks; the warm-host timing case is intentionally excluded. Ten
independent contract-consumer checks preserve all 37 Stream schema snapshots, and the
70-package Linux graph excludes MCP, Store, async, Rerun and GPU dependencies and Chrono
clock. Four compile-fail examples and strict all-target, all-feature Clippy pass.
Fixture cleanup returns Docker to its 13 retained volumes with no running containers.
DTO relationship validation, checked MCP setup and installed/GPU acceptance remain open.

Stream response builders now derive repeated addresses from one identity. Wire
admission checks catalog ID/address agreement, perception/model presence, live-session
addresses and run products against their parent Task and pipeline. Checked MCP setup
owns the documents, capabilities, static descriptors and templates before Store access.
The empty server scope vocabulary keeps gateway operation policy with its owner. All
11 templates expand to their owning builders, and subscription admission accepts only
requested Tasks and mutable run/session addresses. Both gateway registrations now declare
revision 3 and disable static catalog list-change notifications. The final Stream suite
passes 40 native checks; flight and Reason suites pass 96, with Reason's three setup
checks repeated after the registration correction. Six compile-fail examples and 16
independent-consumer tests pass, preserving all 37 schema snapshots. Strict all-target,
all-feature Clippy passes for Stream, Reason, flight and smoke. The review identified C02
terminal-result naming and C27 shared run-resource notifications as the next Stream work;
the owning compliance declarations and Deferred Work list record both gaps. Installed
and GPU qualification remain open.

Stream now returns one canonical `result_uri` from all three tools. Replay completion
links to the run results resource; live start and stop link to the session resource.
Task reads, completion subscriptions, synchronous replay completion and run resources
validate the current output, parent and content link after SQL owner selection. Native
cases cover cross-instance delivery, reconnect, denied callers and corrupt products.
The Stream suite passes 45 checks and flight passes 47. Its independent contract consumer
passes 17 checks, six compile-fail examples pass, and only the five affected output/resource
schema snapshots change. Live Monitor follows the returned session URI; its navigation,
start and stop behavior passes the headless browser harness. Strict all-target, all-feature
Clippy passes for Stream, Reason, flight and smoke. Installed clients now read and validate
canonical results, but reference and GPU qualification remain deferred while workloads are
stopped.

Stream run resources now implement `TaskResourceAddress`. One shared authorized Task
watch supplies run invalidations and requested Task status; a request-scoped listener
composes live-session updates from their GPU owner's hub. The listener checks live access
before delivery and reconciles only live addresses on reconnect or hub overflow. Native
qualification covers independent Store clients, current-result reconnect baselines,
malformed denied records, session removal and official MCP cancellation. During a forced
Store outage, live updates continue and run completion reconciles after reconnect even
when retained events have expired. The Stream suite passes 51 native checks, its isolated
consumer passes 18, and six compile-fail examples pass. All 37 DTO snapshots remain
unchanged. The independent Linux consumer resolves 71 packages, including the consumer, without MCP, asynchronous runtime,
Store, Rerun or GPU dependencies. Strict all-target, all-feature Clippy passes for Stream,
Reason, flight and smoke. Reference cross-replica and live-owner qualification remain open.

Task owner reads, collection pages and current-state subscriptions now compose through
`OwnerTaskQuery`. Its typed operation selection is preserved on updates and Store
reconnect. Stream and Reason own their operation enums in their contract-only libraries;
the shared runtime accepts foundational names without enumerating server variants.
Real-store qualification adds malformed owner-visible rows of another operation type,
multiple selected types, limits, operation changes and recovery after event removal.
The 80 shared-runtime/foundational checks and 102 Stream/Reason native checks pass.
All 25 documentation examples, 35 isolated contract checks and ten generated SQL
variants pass, as does strict all-target, all-feature workspace Clippy. Domain-specific
post-read operation checks still require review. Installed qualification remains pending
while reference workloads are stopped.

Task results now use Store's typed `TaskResultRecord` with one required `payload`
field. This preserves scalars, JSON null and objects such as `{"value":42}` without
collisions. Outbox schema 3 distinguishes absent results from completed JSON null;
the shared JSON driver adapter preserves unsigned integer precision. Media's atomic
writer and every direct SQL consumer use the same format. Schema 99 requires no Task
rows or Task outbox events. Reference installation requires stopped writers and a
fresh platform database; no old result conversion or reader is provided.

Python's Task runtime writes the same result envelope and event schema 3. Its
`TaskResult` type distinguishes absence from a completed JSON null, and the Store
adapter preserves nested nulls and unsigned 64-bit integers. Native checks qualify
the stored shape, independent-connection reads, event replay, rejected envelopes and
official Tasks projection. `cargo xtask enforce python` passes 110 SDK cases,
15 template cases and eight fork-workload cases. Full Rust workspace and installed
acceptance remain pending.

Native qualification passes 277 Rust checks across Store, Task runtime, Media, Stream,
Reason, Map, Optimization and Computers. The Store suite runs with its database gate
enabled. That qualification also fixed quota checkpoint timestamps when appends reach
a shared producer counter out of acceptance-time order. Map and Computers tests now
verify current-format replay and result agreement without reconstructing old formats.
Strict all-target, all-feature workspace Clippy passes.

Task operation names now stay typed through admission, Store records, snapshots,
outbox delivery and Console summaries. Fourteen domain libraries own checked operation
enums. `declare_task_types!` generates their vocabulary and conversions; malformed and
duplicate declarations fail compilation. Map, Time and Optimization dispatch parsed
enums exhaustively. SQL operation selectors bind names from owning declarations, and
native tests prove that unrelated malformed operations are excluded before decoding.
The current string wire format needs no conversion or compatibility reader.

Qualification passes 314 Rust checks, including the explicitly invoked TaskRuntime
Store test, compile-fail examples and thirteen independent-consumer checks. That
consumer imports twelve domain contracts plus a new local vocabulary; its 128-package
Linux graph contains no MCP runtime, async runtime, Store or GPU implementation.
View and SUMO expose their operation enums in their existing libraries. View's feature
isolation is recorded below; SUMO remains open. The pinned SurrealDB 3.2.4 CLI validates fifteen
expanded current-source SQL forms. Strict all-target, all-feature workspace Clippy
passes. Installed acceptance remains pending with reference workloads stopped.
Installed result-format acceptance remains pending while reference workloads are stopped.

View now exposes separate contract, runtime and MCP features. Its contract includes
camera, capture and composition models, `ViewTaskKind` and the domain-owned `ViewScope`.
The three permissions share a typed guard between ordinary requests and capture Tasks.
Resolved invocation authority, membership and capability levels, and output defaults
now belong to `veoveo-types`. Every Rust consumer imports those values directly.
MCP retains Work Context configuration and membership matching; authentication and
policy decisions keep their existing owners. Composition identity retains the complete
authority value and its serialized bytes.

The foundational, policy and MCP suites pass 223 checks. Five authority schemas,
serialized composition-authority input and nested identity admission are covered.
View's independent consumer passes five checks with a 117-package Linux graph free of
MCP, async, database and GPU implementations. The contract-only profile passes nine
unit cases and one compile-fail example; the runtime-only build and all 62 native View
cases pass. Strict all-target, all-feature workspace Clippy passes. View resource
builders and checked setup are recorded below; installed behavior and GPU acceptance remain pending.
Cleanup removed 22 unlocked, unfinished Rust incremental sessions older than a day,
reclaiming 0.54 GiB. Another 114 superseded test executables reclaimed 54.17 GiB;
each had changed source inputs, was older than a day and had a newer executable with
the same test target and build configuration. All newer executables, service binaries,
library artifacts and finalized incremental sessions were preserved. Free space after
cleanup is 76 GiB; reference containers remain stopped.

View now owns concrete addresses through `ViewResource` and distinct layer, composition,
view, frame, scene and tile URI types. Records and manifests carry those types, while
the tile registry keeps `TileKey` through lookup. The shared parser and builder handle
components; domain admission rejects wrong routes, relative IDs, unsupported or repeated
parameters and noncanonical spellings. Scene construction checks numeric inputs before
the runtime applies installation limits. Public URI fields keep their string wire shape.

Checked MCP setup validates fixed discovery before Store access or renderer startup.
The nine declarations cover roots, documents and the permission-gated preview App.
Seven templates agree with typed builders. Scene mutations invalidate contents;
subscriptions admit mutable resources and explicit Task handles. Both gateway
registrations declare revision 3 and omit resource-list changes. The frame template
leaves media type to the PNG or JPEG response.

Qualification passes 15 contract cases, three compile-fail examples and 68 native View
cases. Runtime-only compilation and strict all-target, all-feature workspace Clippy
pass. The independent 117-package consumer runs eleven checks without MCP, async,
database or GPU implementations. View's broader record relationship validation and
owner-typed governed input references remain open, along with installed and GPU acceptance.

View composition and view records now have private fields and checked constructors.
Composition decoding validates its request, authority-bound identity, URI, revision and
both digests. View creation resolves its camera rig, derives its parent and resource
addresses, and validates camera updates before changing the revision. Checked revision
increments reject overflow. The contract profile now includes pure WGS 84 camera math;
JSON float round trips preserve coordinates used in content identity.

Resolved compositions validate overlay metadata, geometry and retained artifact bytes
on construction and decoding. Captures reuse that immutable checked value. Admission
checks declared media type, digest, byte limits and Frames bindings for local geometry;
unreferenced retained bytes are rejected. Capture snapshots check composition parents,
layers and digests. Saved Task requests must name the snapshot's view and revision.
Before Task creation or lease claiming, the adapter checks principal, tenant, Work
Context and capture limits. Direct snapshot capture checks ownership before layer or
renderer work. Captures may use a different invocation policy revision from composition
creation within that ownership scope. No historical readers or conversion paths were added.

Record and recovery qualification passes 27 contract tests, 87 native View tests and
six compile-fail checks. The independent 118-package consumer passes twelve checks;
runtime-only compilation, strict workspace Clippy, formatting, docs and identifier
checks pass. Reference containers remain stopped, with 75 GiB free. Frame and preview
record admission, owner-typed governed input references, and installed/GPU acceptance
remain open.

The unused `TaskRuntime::list_for_owner` helper is removed. It decoded an unbounded
collection before testing label clearance in Rust. All caller-facing collections use
`for_owner(...).page(...)`, which applies owner, operation and label predicates in SQL
before decoding and limits. Repository call-site inspection found no callers of the
removed helper. Seven native SQL collection, ownership and subscription cases pass
against disposable pinned Store fixtures; every fixture container was removed.
Strict all-target, all-feature workspace Clippy and four View discovery checks pass.
The audit also found that View's public Task
get, update, cancel and subscription methods use the runtime's owner-only selection.
Add an explicit Work Context query policy and apply it consistently to those methods
before accepting View's documented ownership scope. Capture snapshot admission already
checks Work Context, but that does not qualify public Task observation.

View public Task methods now share an owner query with checked Work Context selection.
The shared builder validates caller tenant agreement and binds the indexed context,
authority context key, and retained request authority in SQL before decoding or limits.
Task get, update and cancel helpers accept the same query as subscriptions. Their
other hosted callers adopt the signature while preserving their existing policy.
Usage reads reuse the context predicate and binding module. No stored format changes.

Native qualification passes ten Task collection/subscription cases, three usage cases,
and 88 View cases. The added cases exclude malformed foreign-context rows before
decoding, preserve selection through mutation admission, and recover subscriptions
after a connection gap with no retained events. View's caller-to-query case uses two
independent Store clients. The pinned 3.2.4 CLI validates 28 expanded SQL variants.
The independent contract consumer passes 12 cases, and three Task compile-fail
examples pass. Strict all-target, all-feature workspace Clippy, formatting, docs and
identifier checks pass. Fixture containers are removed; reference containers stay
stopped, with 72 GiB free. Installed cross-context delivery and GPU acceptance remain open.

Frame and preview records now have private fields and checked construction/decoding.
The capture builder checks its view/composition relationship, derives repeated
identities and provenance, and binds immutable bytes to their length and digest.
`FrameEncoding` supplies the supported MIME values through rendering and public metadata.
Preview records derive their local frame, validate affine tile transforms and byte-size
status, and report partial detail when truncated or carrying oversize tiles. The
constructors do not claim image decoding or GPU execution qualification.

Qualification passes 38 contract cases, nine contract compile-fail examples, 99 native
View cases, ten default-feature compile-fail examples, and 19 independent consumer cases.
Runtime-only compilation and strict workspace Clippy, formatting, docs and identifier
checks pass. The 118-package independent contract graph excludes service dependencies.
The Store fixture dependency is attached to the hosted-server feature; contract tests
also exclude Store, SurrealDB, MCP and GPU crates. Reference containers remain stopped,
with 70 GiB free. Public Task completion validation and owner-typed governed input
references remain work. `renderer.rs::encode_image` has a `TODO(GPU)` for its existing
CPU RGB conversion/JPEG encoding and GPU readback; qualify CUDA/nvJPEG before counting
that encoding path as GPU acceptance.

View now validates completed capture results after SQL selection and before public
Task reads or subscription projection. The validator checks the saved capture owner
and rebuilds the result through the same checked constructor and formatter as the
writer. Metadata, image bytes, MIME type and presentation must agree with that result.
The frame builder always includes governed-input attribution and orders unique lines;
decoding rejects missing attribution. A shared snapshot-subscription helper preserves
native handle admission without making Task runtime depend on View models or adding
a second database read. View Task queries also select the capture operation.

Qualification passes 103 native View cases and thirteen shared Task/usage cases,
including corrupted completions, malformed rows excluded by SQL, cross-replica reads
and reconnect after Task events have been deleted. Four fixture query forms pass the
pinned 3.2.4 CLI validator. The contract profile passes 39 cases and nine compile-fail
examples; the independent 118-package consumer passes twenty cases without service
dependencies. Runtime-only compilation, thirteen default-feature View/Task compile-fail
examples, strict workspace Clippy, formatting, docs and identifier checks pass.
Fixture containers are removed and reference workloads remain stopped, with 68 GiB
free. Owner-typed governed input references, installed cross-context delivery and GPU
acceptance remain open. These tests establish metadata and byte agreement, not image
decoding or GPU rendering.

Recording now exposes an isolated library contract for its public recording and
playback models, catalog grants and Arrow projection requests and handles. Catalog
and projection definitions moved from MCP core to `contract/catalog.rs`; the gateway's
Recording adapter imports that library directly. The declarations and wire schemas
are unchanged. Core exports and the server's local import aliases are removed.
Runtime, MCP and Redap features own their optional dependencies, including native
test fixtures; contract tests do not activate Rerun or database implementations.

The independent Recording contract resolves to 24 packages and passes four request,
response and schema cases. The same four cases pass in the server's contract profile.
Runtime-only compilation passes. Native qualification passes 31 library cases,
including four upstream Rerun read-profile cases, five hosted-adapter cases and one
isolated SQL catalog case. The shared MCP suite passes 139 cases, and both gateway
playback cases pass. Strict workspace Clippy, formatting, docs and identifier checks
pass. Five superseded Recording executables were removed after checking their feature
fingerprints and replacements, reclaiming 7.45 GiB while preserving build caches.
Reference containers remain stopped, with 62 GiB free. Recording ID/URI builders and
View's governed references remain open; the audit found that View's singular Recording
route does not match the owner's published plural route.

Recording now owns typed RFC UUIDv7 IDs, resource variants and catalog cursors in its
isolated contract. Reads, subscriptions and prompts use the shared owner parser;
Video selections and Reason/Stream results and views import that same URI type. The
reader's unused parser and Video's second parser are removed. SQL still selects tenant,
labels and cursor position before limits; Store cursor conversion occurs at the query
call. Discovery templates expand to the typed builders. Malformed IDs, wrong parents,
unsupported queries and noncanonical cursor encodings fail at wire admission.

Qualification passes ten Recording contract cases and two compile-fail examples.
Independent Recording, Video, Stream and Reason consumers pass 10, 4, 19 and 18 cases;
their Linux dependency graphs contain 58, 68, 71 and 72 packages and exclude MCP,
Store, async and Rerun implementations. The native four-package suite passes 155 cases,
including all four upstream Redap read profiles, SQL-authorized catalog paging and
cross-replica Task reads/subscriptions. Runtime-only Recording compilation and strict
workspace Clippy pass. Reason's first concurrent SQL checks hit their 90-second bound;
both pass alone in three seconds and in the complete two-thread rerun. The cause was
not reproduced. Current query statements and stored formats are unchanged.

Thirty-two superseded executables with matching target/profile/feature replacements
were removed after inode and active-process checks, reclaiming 32.47 GiB. Rust object,
fingerprint and incremental caches are preserved. Fixture containers are removed and
reference containers stay stopped, with 67 GiB free. Installed and GPU qualification
remain open.

Remaining Recording consumers include View, Gateway, Hub, UAV and smoke. Hub cannot
import the server library while the server depends on Hub for Blueprint validation,
live-message handling and publication. Resolve that demonstrated package dependency
cycle before migrating Hub's URI construction: extract a protocol-independent Recording
contract below both packages, or remove the service's dependency on Hub implementation
through the appropriate shared owners. Do not duplicate the route grammar in Hub or
move Recording vocabulary into MCP core. Dataset/layer identities, playback/projection
relationships and checked MCP setup also remain open.

The Recording domain contract now lives in `platform/recordings/contract`, below Hub
and the MCP server. This separate crate resolves their package dependency cycle; the
server library exposes the same public types through its `contract` feature, with no
second model. Hub builds ingest response addresses through it, and Video imports it
directly. Gateway playback admission and grant policy targets use the owning IDs and
URI builder. Domain tests moved with their implementation; the server facade tests
assignment to the shared public types. No external dependency pin or wire field changes.

The shared contract passes ten cases and two compile-fail examples. Independent
Recording, Video, Stream and Reason consumers pass 11, 4, 19 and 18 cases across
59, 68, 72 and 73 packages without MCP, Store, async or Rerun implementations. The
native combined suite passes 156 cases; Hub passes 39 and Gateway playback passes
two. Runtime-only compilation and strict workspace Clippy pass. Ten superseded
executables were removed after replacement, inode and process checks, recovering
15.67 GiB. Build caches are preserved; reference containers stay stopped, with
50 GiB free after qualification. Installed and GPU acceptance remain pending.

Native fixture diagnosis: the first two Reason tests again exceeded their 90-second
outer timeout. Process inspection caught both `docker run` children waiting before
container creation; Docker events show creation only about 105 seconds after their
UUIDv7 names were generated, followed by immediate owned cleanup. No SQL ran before
that delay. `testing/fixtures/store.rs` uses blocking subprocess calls inside async
setup, so its caller deadline cannot interrupt the command. Give fixture subprocess
startup and cleanup explicit bounds and stage-specific redacted diagnostics in a
separate fixture change; keep domain SQL deadlines independent of fixture startup.
The delayed Docker response's underlying cause is still unknown.

The Store fixture now uses cancellable Tokio subprocesses with a 30-second command
limit, two-second kill/reap and stdout bounds, and a 4096-byte output cap. Cleanup
ownership starts before creation; creation and startup use separate commands. Drop
removal has a ten-second limit and reports failure against the owned fixture name.
It fails the test on cleanup errors and avoids a second panic during unwinding.
Credential setup and each runtime connection have ten-second limits; migration
readiness keeps its 60-second bound. Reason's two index tests start their unchanged
90-second SQL deadline after setup. These controls enforce lifecycle deadlines;
they do not claim to eliminate daemon delays.

Six lifecycle fault cases pass, including caller cancellation, child reaping, redacted
creation failure, endpoint admission and cleanup timeout. Native Store, Reason,
Recording catalog and Task admission checks pass another 23 cases, including the
RocksDB transaction profile. Strict workspace Clippy passes. No fixture containers
remain; reference workloads stay stopped and 44 GiB is free. The harness lifecycle
Deferred Work item is closed. Installed and GPU acceptance remain pending.

A later native run exposed a second fixture race: the Docker daemon created the owned
container after the CLI timeout and after removal of the still-missing name had
reported success. Creation and startup were already separate, so the late container
stayed stopped. Cleanup now observes the allocated name for up to 120 seconds before
removing it. An unresolved creation fails the owning test with cleanup unconfirmed;
failure to spawn the CLI establishes that nothing was dispatched. The nine lifecycle
cases cover late creation and both failure outcomes. They pass alongside the native
Store authority check and strict workspace Clippy. The observed stopped fixture was
removed by its exact name, and no fixture containers remain. The underlying daemon
delay is still unexplained.

UAV and the three smoke clients now consume the shared Recording IDs and URIs. UAV
owns an atomic catalog state: readiness carries one admitted URI and derives its ID;
other states carry no recording identity. Decoding checks the public fields together.
Mission, scenario and capture results carry the same owner type. CLI arguments reject
invalid IDs before dispatch, and replay sources use the owner's URI builder. Store
conversion requires native UUID keys and the Recording table. Completion receipts defer
producer-key admission until after physical settlement; a malformed recording reference
fails result resolution while preserving the completed plan and releasing its vehicle.

The independent UAV consumer resolves 104 packages without service dependencies and
passes 14 cases. Contract-only qualification passes 25 cases and seven compile-fail
examples; 96 public schema snapshots preserve the fields while removing the duplicate
UAV Recording ID schema. The four-package native suite passes 196 cases with one existing
timing test ignored, including malformed Recording metadata after physical completion.
Runtime-only compilation, strict workspace Clippy, formatting, docs and identifier
checks pass. Seventeen superseded executables with qualified replacements were removed
after fingerprint, inode and process checks, recovering 13.47 GiB. Build caches are
preserved, fixture containers are removed and reference workloads stay stopped, with
39 GiB free. View's governed references, broader Recording relationships and installed
GPU acceptance remain open.

View's governed inputs now carry the Map, Frames, Recording and Artifact owners' URI
types. Recording uses its published plural route. Map owns six product address
families for dataset releases, source features, rasters, derivations and routes;
builders require domain IDs and readers use the admitted parent components. Their
seven ID types require RFC UUIDv5/v7 in lowercase hyphenated spelling. View imports
Map release references directly and checks source-feature membership in its declared
releases. The smoke fixture uses the same builders. Map route and restriction
subscriptions use their owners' parsers.

Contract qualification passes 72 cases and 23 compile-fail examples. An independent
View consumer resolves 106 packages without service or GPU dependencies and passes
25 cases. The complete native Map, View and smoke suite passes 284 cases with one test
thread, including the million-feature spatial-index check and cross-replica Task
delivery. Optimization's cross-server contract passes three cases. Runtime-only
compilation and strict workspace Clippy pass. The first native run had two Docker
creation timeouts before SQL with two test threads; cleanup removed the owned fixtures,
and the serialized rerun passed. This result does not explain the daemon delay.

Twenty-nine superseded executables, including old integration-test executables, were
retired after checking their replacements, feature fingerprints, inodes and active
processes. This recovered 20.27 GiB while preserving compiler and Docker caches.
Reference workloads stay stopped, fixture containers are removed, and 28 GiB is free.
Installed and GPU acceptance remain pending.

Recording's checked MCP setup now runs before Store, cache and Redap initialization.
It supplies static discovery and accepts only owned content subscriptions. The binary
separates protocol handlers from HTTP playback and dependency initialization. The
Recording contract owns `RecordingScope::Seal` (`recording:seal`); gateway rules keep
the administrator profile and existing role/principal restrictions, and administrator
client allowlists plus Console scope requests include the new domain permission. This
is a coordinated hard cut with fresh tokens; the service accepts no broad-admin alias.
Reference, local and catalog-fixture registrations declare revision 3 with static
resource discovery. The reference configuration checksum matches its rendered bundle.
Reference and local admin policies separate ordinary Recording reads and projections
from sealing and its prompt. The extra `recording:seal` scope gates only the sealing
rule; profile, administrator authority and service-principal checks still apply.
Four App exposure cases pass, including 80 human/service policy decisions covering
the separate permissions and denied subjects, scopes and profiles. Helm configuration
checks pass with the updated gateway bundle digest. Installed acceptance is pending.

Native qualification passes 50 cases, including the official Redap read profile,
all four template expansions and tenant/label/lifecycle rejection after scope admission.
The independent contract consumer passes 12 cases across 59 packages without service
or GPU dependencies; the MCP facade and runtime-only compilation pass. Six reference
configuration cases, Helm/GitOps rendering, strict workspace Clippy, formatting,
document links and identifier checks pass. One native rerun hit Docker's 30-second
create deadline before SQL. Cleanup removed that owned fixture; the isolated SQL
rerun passed in 3.65 seconds. The daemon delay is still unexplained.

Forty-eight superseded test executables were removed after checking matching feature
profiles, newer replacements, source changes where required, file identities and active
processes. This recovered 19.97 GiB. Compiler caches, BuildKit, registry images and
volumes are preserved. BuildKit accounts for about 313 GiB and the active registry
for 68 GiB; Docker's reclaimable estimate does not establish that their contents are
safe to remove. Reference workloads stay stopped, no fixture containers remain, and
about 36 GiB is free. Broader Recording DTO relationships and installed acceptance
remain open.

OCI cleanup retired ten obsolete image tags and 309 old chart versions. Offline
registry garbage collection reclaimed 16.68 GiB after its dry run matched the reviewed
manifest closure. All 224 retained manifests, 848 layer links and 82 tags were verified
through the registry afterward. Current checkout, origin/main, retained publication,
dependency images and registry build caches were protected. BuildKit and Rust caches
were untouched; the registry and reference cluster are stopped, with about 52 GiB free.

Recording projection downloads now use a typed Store scope for tenant, actor, Work
Context, policy revision and data labels. SQL selects the ready, unexpired receipt and
checks its single Recording parent, current label visibility, dataset and App projection
grant relationships before Rust decodes it or opens scratch. The two native catalog
query tests pass, including malformed denied receipts, changed caller authority, expired
or inconsistent grants, deleted parents and admitted file length/digest validation.
Projection persistence is a focused Store module; all ten extracted lifecycle and
helper functions preserve their implementation. The full Recording suite passes 51
cases, including the official Redap read profile. Runtime-only compilation, strict
workspace Clippy, formatting, document links and identifier checks pass. Dataset/layer
identity types, broader playback/projection construction, reservation/lifecycle SQL
admission and installed acceptance remain open.

At the user's request, a full Rust build cleanup removed the two generated target
trees found across the local repositories, recovering 915.93 GiB and leaving 967 GiB
free. The publication worktree and xtask state inside the main target directory were
preserved and verified. Source checkouts, installed tools, downloaded Cargo dependencies,
Rust toolchains and Docker caches were untouched. Subsequent checks rebuild only their
required artifacts; the reference cluster stays stopped. This one-time cleanup
explicitly discards the prior compiler cache and does not change the plan's normal
cache-preservation rule.

Projection reservations and state transitions now apply caller authority, current
Recording visibility and grant relationships inside the write transaction. A typed
request binds the actor-scoped idempotency key to its original Work Context, policy,
dataset, Recording and input digests. SQL rejects mismatches before receipt decoding.
Lifecycle predicates bind the Rust state enum; concurrent completion and cancellation
cannot both commit. Exact terminal retries preserve the original result and timestamp.

Two native Store cases qualify memory and RocksDB through separate clients, including
same-key races, denied authority, changed parents, malformed receipts, expired grants,
conflicting transitions and injected rollback. The 51 Recording cases pass, including
SQL download admission and the official Redap profile. One initial fixture creation
hit Docker's 30-second deadline before SQL; owned cleanup completed and the isolated
rerun passed. Strict all-target, all-feature Clippy for Store and Recording, formatting,
document links and identifier checks pass. Targeted rebuilds leave about 942 GiB free;
no fixture containers remain and the reference cluster stays stopped. Dataset/layer
identity types, broader playback/projection construction and installed acceptance
remain open.

Recording's public catalog, layer, playback and projection models now carry distinct
Recording, dataset, layer, read-grant and projection IDs. All five types share canonical
RFC UUIDv7 admission. The catalog-request constructor checks its 1–500 input limit and
produces a sorted unique selection before gateway authorization. Token subjects and
reusable-grant headers use the same owner type; malformed or duplicate headers fail
before catalog work. Store adapters require native UUID keys and reject string aliases.
Console consumes the shared playback DTOs through the server's isolated contract feature
and constructs Recording HTTP URLs from typed IDs through URL path components.

Qualification passes 175 native cases across Recording, its reader, Console and the
gateway, plus 14 independently resolved contract cases and three compile-fail examples.
The independent graph has 59 packages and excludes service runtimes. Runtime-only
compilation, strict Clippy for affected packages, formatting, document links and identifier
checks pass. An initial Docker fixture creation timed out before SQL; cleanup completed
and the final Recording run passed both database cases. Standalone Console qualification
also exposed TLS test-order dependence; explicit provider setup fixes all three affected
tests. No external dependency pins changed. The cluster stays stopped, no fixture
containers remain, and about 925 GiB is free. Remaining Recording work includes broader
playback/projection construction and SQL admission for grant reuse and Redap grant classes;
grant reuse must also bind Work Context. Installed acceptance remains pending.

Recording grant creation now admits dataset tenancy and every selected Recording's
current parent and label visibility in the write transaction. The checked request owns
the normalized selection, class and catalog revision; viewer and projection grants
require one Recording. Grant and projection admission share a typed caller scope.
Reusable hints match tenant, actor, Work Context, policy, dataset, class, selection,
digest, revision and expiry in SQL before decoding. Redap selects only unexpired viewer
or catalog grants after signed-token verification. The service's Rust post-filters and
unscoped grant lookups are removed.

Fifty native cases pass across Store and Recording, including memory/RocksDB grant
admission, malformed denied rows, parent and clearance changes, expiry, injected
transaction rollback, projection races and service Work Context renewal. The catalog
integration case now uses an owned fixture, a 90-second query deadline and a second
client for grant readback instead of silently returning without SQL when its environment
flag is absent.
Runtime-only compilation, scoped strict Clippy, formatting, document links and identifier
checks pass. The CLI accepts all three grant statements. One full Recording run hit the
recurring Docker creation deadline before SQL; daemon events confirmed late creation
and owned cleanup, and the final run passed. Its cause remains unconfirmed. No fixture
containers remain, the reference cluster is stopped, and about 923 GiB is free. Broader
playback/projection DTO construction and installed acceptance remain open.

Playback manifests now use a checked builder and a sealed model with immutable field
access. JSON decoding runs the same archive-parent, catalog-revision, lifecycle,
capture-time and live-layer checks as Rust construction. The domain owns the closed
Recording state and manifest version. Timestamps use UTC values; Blueprint revisions
and byte lengths use nonzero types, and its digest uses the foundational SHA-256 type
with the existing lowercase-hex field encoding. Console keeps only its requested-ID
check. The producer uses the shared builder, and Rerun origin construction uses typed
URL host and port components instead of assembling a string.

Qualification passes 179 native cases across the contract, server, reader, Console and
gateway, plus 17 independent-consumer cases and four compile-fail examples. The isolated
graph still has 59 packages and no service runtime dependencies. A service case signs a
grant and constructs a live manifest against the owned database fixture. IPv6 origin,
invalid relationships, lifecycle and Blueprint integrity cases pass. Runtime-only
compilation, scoped strict Clippy, formatting, docs and identifier checks pass. Two
post-link attempts hit the 30-second Docker creation deadline before SQL and cleaned up;
the unchanged compiled suite passed with database cases completing in about five seconds.
Linux reported elevated recent I/O stalls after linking. This timing supports further
investigation of build I/O pressure but does not establish the timeout's cause. All
fixtures are removed, the reference cluster stays stopped, and about 921 GiB is free.
Projection construction, remaining URI/digest field admission and installed acceptance
remain open.

Projection queries and requests now use sealed Recording-domain types with checked
builders. JSON admission uses the same positive bounds, unique selectors, ordered
sampling, temporal-value and metadata checks as Rust construction. Requests compose
an admitted query with typed dataset and Recording IDs while preserving the flat wire
shape. Unit keys must name selected components. RRD imports the owner's sampling model
and limits, then prepares the query with the pinned Rerun parsers before source access.
The service prepares that expression before Artifact materialization or receipt creation.
Duplicate resolved entities fail admission, and range output now enforces maximum_samples.
Hub, smoke and Console reuse the shared contract; the duplicate RRD query model is removed.

Qualification passes 193 native cases across the owner, RRD, four focused Hub round-trip
cases, Recording and Console, plus 21 independent-consumer cases and six compile-fail
examples. The isolated consumer still resolves 59 packages without service or Rerun
runtime dependencies. One Hub fixture initially used Rerun's reserved static marker as
a temporal bound; its caller now supplies the first valid temporal value. The focused
Hub run excludes the unrelated H.264 case with its existing optional software decoder
probe; that probe supplies no GPU acceptance. Runtime-only, Hub CLI and smoke compilation,
scoped strict Clippy, formatting and doctor checks pass. Both database-backed Recording
cases pass after compilation, and no fixture containers remain. The reference cluster is
stopped and about 900 GiB is free. Projection result construction, remaining URI/digest
fields, coordinate-frame reference admission and installed qualification remain open.

Projection results now use a sealed domain model with typed SHA-256 values, nonzero
byte lengths and UTC expiry. The builder checks output counts, sampling, metadata and
limits against the admitted request. Reuse checks the SQL-selected receipt, request
identity and actual payload before returning a handle. The owner supplies typed query
identity serialization with exhaustive field binding; the service hashes it without
editing raw JSON. This changes query fingerprints under the plan's coordinated hard cut.

Scratch accounting now includes metadata, with a 512 KiB read/write ceiling and a
reservation allowance. Startup preserves valid Arrow/metadata pairs across repeated
restarts; the previous orphan-removal pass deleted metadata after indexing its pair.
New reservations reclaim expired completed pairs while preserving active partial files.
Concurrency admission runs before source Artifact materialization.

Qualification passes 253 native cases across foundational types, Recording, RRD, four
focused Hub cases, reader, gateway and Console, plus 25 independent-consumer cases and
seven compile-fail examples. The independent contract still resolves 59 packages without
service or Rerun runtime dependencies. Tests cover real scalar RRD-to-Arrow output,
receipt tampering, repeated restart, corrupt and expired pairs, maximum metadata shape
and quota reuse. Both Recording SQL cases pass in the serialized run after compilation.
Runtime-only, Hub CLI and smoke compilation, scoped strict Clippy, formatting, doctor,
docs and identifier checks pass. Native scalar and filesystem checks establish no GPU
acceptance. The one-time Rust cleanup remains complete; targeted rebuilds occupy about
89 GiB, with 879 GiB free. No fixture containers remain and reference workloads stay
stopped. Coordinate-frame references, remaining URI/digest fields and installed
qualification remain open.

Recording now owns typed Redap origins, catalog entries and playback segment addresses.
URL component setters construct their hosts, ports, paths and query values. Playback
admission compares the URI's dataset and Recording identities with the manifest.
Catalog responses use a checked immutable model with entry/dataset agreement, sorted
unique Recording selection, a closed schema version and UTC expiry. Service producers
and Console consume these models without adding Recording vocabulary to MCP core.
The direct URL dependency reuses the workspace's 2.5.8 pin; the upstream Cargo registry
confirmed it as the latest stable, unyanked release on 2026-09-29. Independent consumption
still resolves 59 packages without service or Rerun runtime dependencies.

Qualification against pinned Rerun builders and parsers exposed a port mismatch: Rerun
0.38.1 rewrites loopback HTTP port 80 and HTTPS port 443 to its Redap default, even when
explicit. Recording rejects those combinations with an actionable configuration error.
Public HTTP(S) defaults and explicit nondefault loopback ports are supported. The owning
design declares the profile and the condition for removing the restriction. The reference
installation's public HTTPS hostname needs no configuration change.

The final run passes 197 native cases across the domain, Recording, reader, gateway and
Console, plus 29 independent-consumer cases and ten compile-fail examples. Tests qualify
IPv6, TUID byte mapping, wrong parents, ambiguous selectors, redacted errors, the upstream
loopback behavior and a service-produced catalog response from a current SQL grant.
Runtime-only compilation, scoped strict Clippy, formatting, doctor, docs and identifier
checks pass. Both SQL cases complete after the build, and their fixtures are removed.
Twenty-three superseded test executables were retired after checking replacement results,
feature fingerprints, file identities and active processes, reclaiming 20.31 GiB.
Compiler and Docker caches are preserved; reference workloads stay stopped with 883 GiB
free. Artifact references, remaining digest/DTO relationships, coordinate-frame references
and installed qualification remain open.

Artifact contract qualification now validates current metadata and attribution against
the generated schemas. The two historical-schema fixtures and the owning designs'
mixed-version, retained-data audit and rollback instructions are removed. Fourteen
native contract cases and four compile-fail examples pass. Native construction,
identity admission and attribution rules are unchanged; current-format recovery stays
with the service. This follows the plan's coordinated hard-cut rule.

Recording catalog, layer and seal models now carry the Artifact owner's `ArtifactUri`,
typed integrity digests, closed layer enums and UTC timestamps. Immutable builders
check layer kind/name/ordinal agreement, publication state, positive finalized counts,
manifest occurrence uniqueness and Blueprint facts. The Recording domain depends on
the lightweight Artifact contract; its independent consumer resolves 60 packages with
no service, MCP, Store, Rerun or async runtime dependencies.

Store-to-domain conversion lives in `service/views.rs`. Native UUIDv7 Artifact keys
and digests are admitted fallibly, replacing persisted-key `expect()` calls and manual
URI formatting. Fresh seals validate committed metadata before changing lifecycle
state. Result construction precedes seal completion, and sealed retries validate their
output before deleting local static context. SQL continues to select visible parents
before public conversion.

Qualification passes 202 native cases across the Recording domain, service, reader,
gateway and Console, plus 34 independent-consumer cases and 12 compile-fail examples.
The SQL fixture rejects malformed visible references and digests, excludes denied
malformed rows, preserves state after rejected seal admission and preserves local
context after a contradictory retry. All nine fixture statements pass CLI validation.
Runtime-only compilation, scoped strict Clippy, formatting, doctor, docs and identifier
checks pass. Both SQL cases finish and remove their containers. Eleven superseded test
executables with matching qualified replacements were removed, recovering 7.52 GiB;
compiler and Docker caches stay intact, with 876 GiB free and the cluster stopped.
Playback-plan digest relationships, coordinate-frame references and installed acceptance
remain open. Frames runtime depends on RRD, which imports the Recording domain;
frame-reference adoption must resolve that dependency cycle through the owning contract.

Frames public types now live in `platform/frames/contract`; the MCP library re-exports
them through its isolated `contract` feature. Recording imports this owner directly,
resolving the Frames-runtime → RRD → Recording-domain dependency cycle. The owner
keeps its existing Map geodetic value dependency and introduces no service or deployment.
Independent Frames and Recording consumers resolve 103 and 104 packages respectively,
without MCP integration, Store, Rerun, async or GPU runtimes. The existing schema fixture
and domain assertions moved with their owner; a facade test proves public type identity.
The focused flight and browser client graph check admits this extracted contract by
name. All three graph selections pass without Store, Rerun, DuckDB or server runtimes.
Image qualification checks production features separately from conservative source
discovery. Stream and Reason consume Recording and Map contract features without
Hub, Forwarder or DuckDB dependencies. Six input and cache checks pass, including
required contract files, native inputs and separate runtime assets.

Recording projection requests and results carry `WorldFrameUri` values. They admit at
most 64 distinct immutable-revision references, preserve caller order and keep URI wire
strings in query identity. JSON decoding rejects unpinned, malformed or duplicate frame
references. Request/result comparison rejects a different frame revision. These are
caller-supplied metadata: projection does not resolve frame worlds or transform coordinates.
The maximum metadata-envelope test now exercises the Frames owner's longest admitted
addresses. Owning designs, CE-13 and the code map record the dependency decision. Frames'
obsolete retained-data preflight and compatibility-only rollback instructions are removed.

Qualification passes 264 native tests across both domains, both services, reader,
gateway and Console, plus 56 independent-consumer cases and 23 compile-fail examples.
Frames and Recording runtime-only builds, scoped strict Clippy, formatting, docs, doctor
and identifier checks pass. The service suites include current SQL visibility, parent,
publication and recovery cases; all disposable fixtures are removed. Eleven superseded
test executables with matching qualified replacements were retired, reclaiming 7.53 GiB.
Compiler and Docker caches remain intact, and the reference cluster stays stopped with
858 GiB free. Playback-plan and reader/video integrity types, other inventory work and
installed acceptance remain open.

Recording's cache validators, read snapshots, playback plans and Blueprint checks now
carry `Sha256Digest`. The foundational types crate owns the explicit bare-hex Serde
adapter used by these wire fields and the public Recording models. Cache filenames,
projection fingerprint inputs and the captured-source JSON hash keep their declared
bytes. RRD inspection text is admitted at its adapter; the cache API rejects raw
strings at compile time.

Video owns immutable source and snapshot builders with distinct Recording, dataset
and layer IDs. They check positive bytes, source-kind/part agreement, layer identity
consistency, duplicate sources and total-byte overflow. Materialization admits the
snapshot before video extraction. Reason and Stream require its Recording to match
the selection before dispatching a runner; portable Stream results and Reason grounding
check the same parent. Both publishers carry typed snapshot digests and Recording IDs
into their Artifact descriptors. Contract schemas declare the stronger source profile.

Qualification passes 356 native tests, 77 independent-consumer cases and 16 compile-fail
examples. The independent Video/Reason/Stream graph resolves 109 packages without MCP,
Store, Rerun, async or GPU runtimes. Recording runtime-only compilation, Hub and smoke
clients, scoped strict Clippy, formatting, doctor, identifier and docs checks pass.
The service suites remove every disposable fixture. Thirteen superseded test executables
with matching qualified replacements were retired, recovering 13.14 GiB; compiler and
Docker caches are preserved. The cluster stays stopped. Remaining Recording/RRD
identity and publication adapters, broader inventory work and installed acceptance are
still open. These checks establish metadata, protocol and process behavior, not GPU
execution or installed acceptance.

This plan tells an implementing agent how to deliver six changes. The first moves
every repository-owned identifier onto the `veoveo.ai` domain in one hard cut. The
second makes installed smoke checks run against any installation, not only the Bioma
reference installation. The third fixes resource contract violations found while
surveying the servers. The fourth upgrades SurrealDB to 3.3 and replaces the separate audit paths with one
audit log. The fifth replaces the outbox with SurrealDB change feeds and moves other
hand-built mechanisms into the database. The sixth adds selected knowledge sources
and the `knowledge-mcp` catalog and index.

The target contracts live in the owning documents:

- [Knowledge source extension](../mcp/knowledge-extension/DESIGN.md)
- [Knowledge sharing](KNOWLEDGE.md)
- [Audit log](AUDIT.md)
- [MCP server contract](../mcp/contract/DESIGN.md), rule C32
- [Foundational types](../platform/types/DESIGN.md)
- [Contract evolution](CONTRACT_EVOLUTION.md), CE-10 through CE-13
- [Deployment contract](../deploy/contract/DESIGN.md#installation-target)
- [Naming rules](../AGENTS.md#naming) and [Database First](../AGENTS.md#database-first)

The plan is complete when every phase is implemented and deployed to veoveo.bioma.ai,
the reference installation, and its acceptance checks pass there. Delete this plan, and
its CODEMAP row, in the change that completes the last phase.
Update each owning design when its phase lands, because the designs hold the current
state after this file is gone.

DuckDB database catalogs now return checked 100-item pages from the authenticated
owner directory. The filesystem selector keeps 101 filename candidates and reads no
database bytes; each request derives its directory again. Structured owner-key input
keeps an absent tenant distinct from a tenant named `installation`. Resource dispatch
uses the library's typed database, document, Artifact, usage and App variants, and
checked MCP setup owns discovery and startup. Both registrations declare revision 3.
Schema responses use typed DTOs and reject incomplete results. Thirty-eight native
cases pass, including directory isolation, current-format identity recovery, Spatial
execution and SQL usage visibility. The independently resolved 59-package consumer
passes 13 cases over 25 schemas without service dependencies; four compile-fail
examples and five headless Workbench behavioral cases pass. Strict runtime-only and
all-feature Clippy pass. Installed paging, fresh owner-directory selection and headed
hardware acceptance remain work. Before this checkpoint, a further user-requested
Rust cleanup reclaimed 135.4 GiB; targeted checks rebuilt only their required outputs.

DuckDB execution now keeps `TaskId` through scheduling, recovery, usage and Artifact
publication. Direct calls carry no invented Task identity; the redundant local owner
models are removed, and database paths and lock keys use `PathBuf`. The isolated
contract owns checked Artifact origins and closed operation-usage metadata. Publication
checks the capability's Task association and the origin's Task and operation before
sending bytes; labels come from the verified execution identity. These metadata shapes
are a hard cut, with no historical decoder. TaskRuntime's existing string lookup APIs
remain a separate migration. Export requests now distinguish tabular formats from
snapshots in their Rust variants and wire admission. Checked query-result constructors
reject row-width/count disagreement and mixed inline/Artifact output. Truncated counts
retain their observed lower-bound meaning; response text reports returned rows. Fifty-five
native cases pass, including retained-request execution, separate-connection usage reads,
real-engine row/byte limits and ingest followed by a quoted-table read. The 59-package
consumer passes 26 cases over 36 schemas, and eleven compile-fail examples pass. The Rust
decoder and JSON Schema agree on 30 result cases and 75 request cases. Eight lexical
cases also pass with ECMAScript regex semantics; five source-admission schema cases pass.

Source addresses now use the foundational `HttpsUrl` through DuckDB, Map and Timeseries
materialization. The parser checks canonical HTTPS syntax and preserves accepted signed
query bytes; runtime policy still checks each host and resolved address. DuckDB owns the
nonempty source list and neutral Artifact-source address. HTTP request errors omit URLs,
and redirect admission excludes credentials, fragments and non-HTTPS destinations.
Foundation qualification passes 42 native cases and 16 doctests. The shared runtime
passes 20 native cases and one compile-fail example; seven focused Map cases pass.
Timeseries passes 28 native cases and seven independent-consumer cases in a 60-package
graph. Strict Clippy passes for the foundation and shared runtime, all three services'
full feature sets, and DuckDB/Timeseries runtime-only builds. Forecast still rejects
Artifact materialization; its advertised source profile needs a separate correction.
Installed source, catalog and Artifact publication/recovery qualification remain work.
The latest user-requested Rust cleanup reclaimed 18.95 GiB and left 971.6 GiB free;
checks rebuild only their required outputs with the reference cluster stopped.

DuckDB request admission now owns checked SQL text, table names, reader-option names
and values, and query construction. The builder rejects repeated attachments, attachment
of the primary database, zero limits and inline row limits on Artifact output before
Task creation or file access. Table names preserve whitespace and embedded quotes through
execution and metadata. Reader-option builders reserve controlled fields and their
aliases, reject unsupported value shapes and NUL, and render SQL without late validation.
Timeseries consumes the same option contract. DuckDB still owns SQL grammar and
reader-specific option meaning; the service's configured limits bound execution.

Python Task change streams now keep one pending LIVE read across idle deadlines.
Cancellation closes the reader and removes the server subscription; transport errors
reach the consumer, and a terminated source requires a new subscription. The native
SurrealDB regression receives two writes from an independent connection after repeated
idle deadlines and verifies cancellation cleanup. It fails against the previous reader.
The current-owner query batch adds typed Python operation selections, UUIDv7 query
inputs and creation-time/ID page cursors. Exact reads, bounded pages, pending inputs
and subscription baselines select owner, profile, tenant, labels, optional context
and operations in SQL before decoding. Cancellation and input-response transactions
repeat that selection after concurrent policy changes. Public notifications select
current Tasks from event identities and never decode retained snapshots. A 15-second
current-state read covers event-retention gaps, and replacement subscriptions admit
their IDs again. The request adapter closes readers even when acknowledgement fails
before iteration. Datasheet's Task adapter now uses this query and its own typed
operation name. Native SDK/template qualification passes 169 tests; the dependent
fork fixture passes eight. Query parser validation passes, and the real Store cases
run against the current 3.2.4 fixture.

The next Datasheet batch adds report and usage catalogs with 100-item pages and
separate collection-bound cursor types. The usage query selects current parent Task
authority and matching usage metadata in SQL before grouping, limits or decoding.
Exact usage reads and prefix completion use that same selection. Static discovery
lists roots and templates without Task scans. URI variants and standard-library
builders replace manual parsing, and the Workbench follows the server's next-page
URI. Both gateway registrations declare contract revision 3, with the reference
installation's bundle digest updated in the same change. Obsolete unbounded SDK
catalog APIs are removed. The combined SDK/template run passes 192 cases and the
fork fixture passes eight; the headless Workbench check qualifies page navigation,
not visual appearance. Helm configuration and the three generated query statements
pass validation. Native Datasheet smoke passes discovery, profiling, artifact output,
usage completion and both catalog responses. It exposed a Python identity mismatch:
display names were treated as identity fields, and stale upserts could overwrite
concurrent security changes. Identity creation now checks current identity fields in
one transaction, creates missing records, and preserves existing names and security
fields. Five native database regressions qualify named-principal reuse and concurrent
field changes. Installed qualification at `481ee4d2` passes 26 hosted checks without
skips and public OAuth profiling, Task completion, report/usage catalog reads, exact
usage and completion. The public fixture has one item per catalog; native and headless
checks establish multi-page traversal. Wider SDK type work remains. Development kept
the cluster stopped; the combined installed pass runs after publication.

Inspection of an installed Python Task ID found that `uuid_extensions.uuid7` implements
the draft-02 layout with seconds and fractional seconds. Interpreting its timestamp
under RFC 9562 yields 2202 for a Task created in 2026. Version and variant checks had
not exposed this mismatch. The SDK now uses the exact `uuid-utils` 1.0.0 pin and its
standard-library UUID adapter. A 4096-ID check qualifies the RFC millisecond timestamp,
ordering, uniqueness and native driver value. SDK/template checks pass 193 cases, the
fork fixture passes eight, and native Datasheet smoke passes with the new generator.
All three lockfiles replace only the UUID package. Installed public Task/catalog/usage reads pass at `f1a55ddc`; the new Task ID has the
current RFC millisecond timestamp. No historical-format adapter is added.

The Phase 3 library batch isolates the remaining Artifact, Computers and Speech server
contracts. Artifact grant/share DTOs move into the existing Artifact plane contract;
core policy and transport keep their current owners. Artifact resource helpers adopt
typed component construction and strict family parsing. Computers and Speech expose
their existing public contract crates through their server libraries. One independently
resolved consumer passes both checks across all three gates. The grouped native batch
passes 479 checks across the three servers, Artifact plane contracts/client/service,
MCP contract, Gateway and Console BFF. Fourteen prerequisite-dependent cases are ignored;
they are not installed evidence. Artifact's complete contract suite passes 21 checks,
including five compile-fail examples and its existing wire/schema fixtures. Runtime-only
library compilation passes for all three feature configurations. All 15 Rust MCP
server libraries now expose an isolated contract feature. Installed publication and
runtime acceptance join the next Phase 3 integration checkpoint.

The following Phase 3 batch adopts checked hosted setup for Artifact and Speech.
Speech owns distinct transcription and private dictation IDs and resource families;
MCP reads/subscriptions, application execution, Gateway policy targets and Console
receipt decoding consume them. Dictation receipts reject identity/URI disagreement,
and transcription result reads check their Task. The accepted current UUID profile
is explicit, with no historical-data adapter. Artifact setup covers the Library App,
index and existing document/occurrence templates; both registrations declare revision 3.
The grouped qualification covers both servers, Speech contracts, Gateway, Console BFF,
isolated library consumption, generated Workspace types and Helm configuration.
The grouped native pass completes 307 tests, with one prerequisite-dependent test
ignored. The independent contract consumer passes both checks; two compile-fail
examples reject wrong IDs and raw strings. The CUDA harnesses compile, with execution
reserved for hardware qualification. Helm configuration passes after updating the
reference control-plane bundle digest. Client generation exposed boolean-reference
limits in the pinned TypeScript and Zod converters. Shared client tooling uses equivalent
object forms, preserves `never` and nullable branches, and keeps canonical JSON intact.
Generated Speech types now reflect the current Artifact provenance and typed addresses.
Console passes 102 native JavaScript cases, Workspace passes 20, both TypeScript
checks pass, and generated-schema verification passes. These are consumer behavior
and type checks; they establish no visual or GPU workload acceptance.
Installed workload qualification joins the next checkpoint.

The next Phase 3 batch adopts complete hosted resource vocabularies and checked setup
for Frames and Timeseries. Their contract features expose closed document identities,
Artifact address construction and the existing world/usage owners without service
imports. Resource reads parse one owner enum; Frames subscription admission uses the
same vocabulary. The fixed catalogs, App metadata and subscription capabilities are
preserved. Timeseries constructs forecast handoffs from typed Artifact occurrences.
Sixty-six affected native checks pass across both hosted servers and their libraries,
including Store visibility, concurrent publication and usage-page cases. The first
Store container creation exceeded its 30-second deadline; that case passes on retry,
with the 23 completed cases excluded from the remaining batch. The independent consumer
passes three checks across five server libraries and excludes runtime dependencies.
Two compile-fail cases pass. Twelve of the fifteen Rust MCP servers now consume
checked setup; Computers, Map, Media and the templates remain. Installed resource and
workload qualification joins the Phase 3 integration checkpoint.

The Media batch extracts public model, Artifact and generation requests/responses into
its isolated contract feature. Model identities stay typed through registry lookup,
provider submission and output attribution. `MediaResource` owns every hosted route;
startup, discovery and subscription admission consume the owner contract. Provider HTTP
paths and callback queries use URL components. Template qualification found that opaque
prediction IDs used different reserved-character encoding in constructors and RFC 6570
expansion; prediction and result builders now agree with the declared templates. The
model template uses reserved expansion for its slash-separated identity.
The grouped Media/conformance selection passes 67 native checks after the encoding
correction and fixture retries. The isolated consumer passes four checks across six
server libraries; four resource checks and eight compile-fail cases pass with only
the Media contract feature. Docker took 50 and 32 seconds to create the first Store fixture; both late
containers were removed. The unchanged compiled billing case passes in four seconds
when retried after compilation. Thirteen Rust servers adopt checked setup; Computers,
Map and the templates remain. Installed acceptance stays at the integration checkpoint.

The Computers batch introduces distinct Computer, execution, file-transfer and
automation-grant IDs in its public contract. Domain services, persistence adapters,
gateway routes, relays and generated browser schemas consume those identities.
`ComputerResource` owns every hosted address and builds it through foundational URI
components. Checked server setup supplies capabilities, five fixed resources and nine
templates. Computers keeps its existing gateway actions and domain authorization;
its scope enum is empty.

The grouped selection qualifies 579 native checks across seven affected packages.
Two Console CLI fixtures used UUIDv4 Computer IDs; owner constructors now supply their
identities, and the affected Computer checks pass. Store fixture creation exceeded its
30-second deadline after both compilations; the two affected cases pass in under two
seconds each when rerun from the current executables after compilation. Owned containers
were removed. Six checks still require native provider, native database or BuildKit
prerequisites and remain ignored. The isolated consumer passes five checks, and five
compile-fail cases pass. Browser qualification passes 34 behavioral unit checks and
TypeScript compilation. Documentation, identifier and formatting checks pass.
Fourteen Rust servers now adopt checked setup; Map and the templates remain.
The next Computers batch must complete SQL admission before decoding and page limits,
including owned and delegated reads, collections and completions. Remaining
lifecycle/access/provider identities and DTO relationships are recorded below.
Installed qualification stays at the integration checkpoint.

The Computers SQL batch admits owner identity, Work Context, retained labels and
classification before decoding exact reads, collections and completions. Grant reads
bind their parent and verified grantee in SQL. Current owner policy and effective
grant lifetime are resolved before decoding the Computer. Private typed permits feed
a final SQL transaction that rechecks control and grant revisions, account state,
session expiry and installation limits before ordering and limiting public rows.
Denied candidates cannot consume a page slot; the cursor identifies the last returned
Computer. Admitted malformed rows fail the read.

The grouped native selection and affected follow-up qualify 584 unique checks. Five
new regressions cover malformed denied rows, complete owner identity and clearance,
page filling, owner policy, grant lifetime, control replacement and logout. Six
prerequisite-dependent checks remain ignored. SurrealDB 3.2.4 validates the nine
queries. A post-build filesystem flush took 14 seconds before the grouped fixtures;
the run had no Docker creation timeout. This is a resource observation for this plan,
not a fixture guarantee. The cluster and BuildKit stayed stopped, and native fixtures
removed their containers. Operation Task reads, command/file authorization metadata
and access/session grant queries still need SQL admission review. Remaining typed
lifecycle/access/provider identities and installed acceptance stay open.

The Task and access SQL batch separates lifecycle policy lookups from operation state
decoding. A shared command/file admission module resolves direct-owner or named Execute
policy before its private typed permit loads Task metadata. SQL binds the parent,
provider, actor and retained clearance. Browser and CLI ledger reads now apply ticket
or credential hashes, session/connection identity, expiry and provider predicates
before decoding authority. Owner inventory and revocation bind the retained parent
and owner; pairing filters its parent and sign-in session before reading callback
metadata. Mutation transactions keep their own admission checks.

The grouped selection and focused Task-access follow-up qualify 583 unique native
checks, including six new SQL admission regressions. Two checks require a native
SurrealDB binary or BuildKit quota prerequisites and remain ignored; provider-host
and installed suites were outside this selection. The current JSON representation of
an absent file grant is normalized to the SDK's optional-value representation in the
lookup. Cross-client owner recovery and both public file HTTP paths pass. Compilation
has no warnings, and the eight new query files validate against SurrealDB 3.2.4.

Maintenance resource reads still decode state before owner checks. The next ownership
batch must address that path and audit maintenance resumptions, accepted-request
receipts and internal worker lookups alongside the remaining lifecycle/access/provider
IDs and DTO relationships. Installed qualification stays at the integration checkpoint.

## Computers Validation Batches

### Current Computers Validation Batches

The last broad selection ran 41 executables across seven packages. Compilation took
2 minutes 45 seconds and reported test execution totaled 6 minutes 2 seconds, excluding
the filesystem flush and orchestration. Its affected six-target follow-up compiled in
30 seconds and reported 77 seconds of tests. Those measurements support selecting
execution separately from the stable compilation graph; they are not runtime budgets.

| Batch | Implementation checkpoint | Qualification checkpoint |
|---|---|---|
| SurrealDB 3.3.0 upgrade (committed) | Client/server pins and fresh-store behavior select 3.3.0 | Native qualification passes; group installed smoke with audit integration |
| Unified audit (current) | Phase 4 audit types, writer, producers and readers are committed; their images and charts are published | Native checks, paired measurements and deployment configuration pass; run composed installed acceptance on the fresh installation |
| Computers ownership and public identities (current) | Maintenance, reservation and execution receipts, Task-link repair and all four claimed journal reads admit relationships in SQL. Lifecycle/maintenance Tasks, interactive access, pairing and connection IDs have typed direct consumers. Request/template/provider IDs and DTO relationships remain | SQL denial, replay/conflict and recovery regressions cover the read batch. Task/access native tests, gateway/BFF checks and generated schemas pass. Complete the remaining identity batch before grouped installed qualification |
| Provider identities | Trace private runtime and retained-instance identity requirements, then update the owning types and all transport consumers together | Native runtime and provider fixtures, including current-format recovery; preserve required GPU acceptance |
| Composed Phase 3 delivery | Finish remaining Map/template setup and cross-component contract work | Run the shared contract closure and installed acceptance in the selected service batches, with Reason separate, then stop the cluster |

These checkpoints guide this plan's work. They do not add a repository-wide testing
rule or remove required acceptance. Report implementation completed since the last
checkpoint separately from checks still pending.

## Phase 5: Store Simplification

This phase applies [Database First](../AGENTS.md#database-first) to the platform store.
Native changefeeds deliver committed mutations to typed consumers, and database
references own child cleanup where the parent controls the child's lifetime. The
outbox API, writers, tables and shared sequence have been removed. The remaining
acceptance work records final costs and deploys the cut.

The [native baseline](../platform/store/measurements/2026-10-01.md) records 5,184
confirmed commits across 18 fresh RocksDB fixtures with the cluster and builder stopped.
At eight concurrent writers on one WebSocket client, the shared-sequence profile's
median elapsed time is 999 ms, the otherwise equivalent independent-event control is
315 ms, and domain writes with only native feeds take 243 ms. No conflict reaches the
client and no client retry occurs; internal retry counts are not observed. This is
the pre-change baseline; final measurements remain open.

The 2026-10-01 consumer batch adds typed Store decoders and persisted native cursors,
then moves Rust Task subscriptions, agent wake scheduling and managed revocation,
Artifact notifications, and Computers notification sources onto that path. Public
Task delivery selects current authorized rows in SQL; trusted workers replay committed
Task states and repeat the last transaction on resume. Timers use the next wake
availability or lease expiry. The six-package compile check and strict all-target Clippy pass. Thirty-eight native
cases pass: two Store feed cases, 25 Task cases, eight Agent cases and three Computers
subscription cases. Formatting, migration SQL validation and documentation checks pass.
Computers subscriptions anchor their feed at the observed head before the baseline,
preventing prior writes from producing a duplicate initial notification. Agent manager, gateway catalog heads, Map projection,
writer removal, Computer authority recheck timers, relationship adoption and the final
measurements remain open.

The Python delivery batch removes its Task and domain-usage outbox writers and deletes
the SDK's outbox API. ID-only LIVE queries wake native commit replay; public notifications
read current SQL-authorized Tasks. Trusted worker cursors repeat the last complete
transaction on resume. Request-owned readers start from fresh admission and do not
persist a checkpoint across caller sessions. Socket loss interrupts an idle reader;
renewal selects a current baseline. Native paging, multi-Task resume, retention recovery,
cross-replica delivery and a zero-query idle interval pass against the pinned 3.3.0 image.
The grouped SDK, Datasheet template and fork-fixture run passes 227 checks (191, 28 and
eight). The temporary database is removed after the run; the cluster and builder stay
stopped with about 178 GiB free. Installed acceptance remains open for this batch.

The remaining-consumer batch moves Agent Manager and gateway catalog/Console wakes
to native feeds. Manager inventories page through typed operation cursors and wake
from Kubernetes metadata watches or persisted deadlines. A watch resumes bookmarks
after normal expiration and relists on HTTP 410. Helm grants and tests list/watch
permissions for every observed resource. Catalog SSE revisions hash SQL-authorized
metadata; private edits leave another user's revision unchanged, while publication
changes it. Policy replacement and token expiry close the stream. Feed ownership
also closes readers when the producer ends.

Map authoring allocates its sequence from the Map head in the same transaction and
stops writing outbox events. Unrelated domain traffic cannot advance its sequence.
DuckDB commits each projection page with its checkpoint and resumes after restart.
The deployment profile now declares native changefeed recovery and LIVE wakeups.
The grouped run and affected follow-up pass 183 distinct Rust tests and 125 browser
client unit tests. Both clients pass TypeScript checks, five affected packages pass
strict all-target Clippy, and docs pass link validation. The installed Kubernetes
admission case remains unrun while the cluster is stopped. About 165 GiB is free.
Installed qualification, remaining outbox writer/API/schema removal, Computer authority
timers and Task wake-source failure ownership remain open, followed by the relationship
review and paired measurements. The existing headed Chrome profile on port 9222 is
connected; WebGL uses the RTX 4090, and WebGPU reports a software fallback.

The writer-removal batch deletes the outbox writers, Rust API, schema and sequence
across Store, Tasks, Agents, Computers, Media, Frames and installation acceptance scripts.
Domain transactions retain their required audit appends. Tests inspect committed native
states and typed audit records, including rollback and idempotency. Task readers hold
only a weak reference to their producer's sender, allowing producer termination to
close subscribers. Artifact release and share-revocation mutations return their committed
records through single-statement updates. Reconnect tests admit overlapping native replay
while checking current SQL authorization and the subsequent idle interval.

The grouped run and targeted repairs pass 460 Rust tests; 18 explicitly ignored cases
are excluded. Store and gateway environment-gated cases execute against an owned
disposable database. All 12 affected packages pass strict all-target Clippy. Formatting,
241 complete SurrealQL files on the pinned 3.3.0 image, and 917 documentation links pass.
The repair pass compiles its packages together before selecting the affected native
harnesses. Disposable containers are removed after qualification. The cluster and builder
are stopped, with about 117 GiB free. No implementation or test references the deleted
outbox API or tables.

The installation cut requires drained writers and a fresh database using the matching
schema catalog. No Phase 5 image is deployed. The relationship review and paired
measurements remain open.

The reference installation converged at `8e4b36e7` with both Helm releases and all 25
Deployments current. The corrected composed flight harness used the existing headed
Chrome profile on port 9222; WebGL reached the RTX 4090. The run failed during takeoff
with a PX4 command deadline. Simulator logs show MAVLink poll timeouts and health-based
arming refusals. Owned cleanup completed, and a fresh read confirmed `uav-1` in standby.
Recording replay was not reached, so the new Stream/Reason Recording identity repair
still needs installed acceptance. The cluster was stopped for development with about
190 GiB free. Diagnostic files are under
`output/development/foundations-audit-publication-e543e6e4/repair-installed/`:
`uav-rollover.log`, `flight-px4-deadline.log`, and `landing-state.json`.


The authority-observation batch shares one native source across Computer workers,
MCP subscriptions and browser/CLI attachments. It observes directory and grant-policy
changes alongside Computer, grant, Task and execution-journal state. Typed Computer,
Task, execution, file-transfer and session-family identities select invalidations.
Consumers subscribe before their authority baseline. Source loss closes the listener
epoch; dropping the last store stops its producer. Four additional table feeds cover
command/file journals and installation grant policies without original payloads.

Fixed one- and two-second authority checks are removed. Renewal follows the current
permit deadline, while independent expiry and execution deadlines remain armed during
blocked reads. Browser and CLI attachments also respect the provider profile's maximum
renewal interval. Known source loss takes priority over ready I/O. Provider outcome
uncertainty still follows the existing containment and fencing rules.

The grouped Store/Computers run passes 291 tests with seven explicitly ignored cases.
The six affected harnesses pass all 50 rechecks after final review. Native cases cover
journal deletion, Task cancellation, grant revocation, directory and policy changes,
source shutdown, idle renewal, expiry and public subscription revocation. Strict
all-target Clippy, formatting, 241 complete SQL files and documentation links pass.
The provider fixtures and installed acceptance are not claimed. The cluster and
BuildKit remain stopped, with about 85 GiB free after qualification. The database-feature
review and paired measurements are next; measurements must include the encrypted
execution-journal payloads copied by their new feeds.

The schema review keeps original rows on the 15 tables whose deletion consumers need
a tenant or parent. Native `INFO FOR DB` checks all 154 tables and 113 feeds against
that policy. Six reference fields cascade Task idempotency/input children, Artifact
shares, refresh tokens, audit seal memberships and export deliveries. Task retention
uses one admitted parent deletion in both SDKs. Audit keeps its lease, contiguous
block order, cutoff and destination receipts before deleting records and blocks;
database cascades remove their dependent rows. Native cases prove rollback and
unrelated-parent isolation. The owning designs record rejected relationships and
the reasons to keep database-local views and the Map-head event synchronous.

Full-schema qualification exposes and repairs an Artifact feed decoder error: grants
carry the parent in their graph `in` endpoint, while shares use `artifact`. The earlier
synthetic fixture used the wrong grant field too. The repaired fixture and full-schema
creation/deletion cases now cover both representations.

The seven-package grouped run and focused repairs qualify 569 distinct Rust tests;
ten explicitly ignored provider, external-service, native-binary, fork and measurement
cases are excluded. Five affected feed/relationship rechecks pass after the decoder
repair. The Python SDK run and its 34-case Task recheck qualify 191 distinct tests.
Strict all-target Clippy, formatting, 241 SQL files and documentation links pass.
Docker creation briefly stalls under post-link disk I/O pressure, then the native
fixtures complete and remove their containers. Cleanup removes 268 superseded linked
test executables, recovering 96 GiB while preserving the current executable outputs,
compiled libraries, Rust dependencies, BuildKit caches, images and volumes. About
136 GiB is free after qualification. The cluster and builder are stopped; no Phase 5
image is deployed. Final latency/storage measurements, including encrypted journal
payloads, and installed acceptance remain open.

The paired measurement batch completes 45 successful RocksDB fixtures: 5,184
latency-comparison transactions, 4,608 storage-comparison transactions and 288
encrypted-journal metadata updates. Eight-writer native-feed-only transactions take
83.3% less elapsed time and 80.4% fewer attributed block writes than the recreated
shared-sequence/event control. No client-visible conflicts or retries occur.
The [measurement record](../platform/store/measurements/native-after-2026-10-01.md)
preserves per-run and per-operation data and the observation limits.

The real command-journal workload exposes a separate storage problem. Sixteen
metadata updates with 1 MiB stdin copy 28.4 MiB of unchanged ciphertext into the
feed. Median attributed writes are 132.2 MiB with the feed and 34.2 MiB in the
feed-disabled control. The next source pass separates immutable command/file
request payloads from changing journals while preserving atomic admission, typed
reads, retry identity, current-format recovery and parent-owned cleanup. Qualify
that pass as one batch and repeat the affected payload measurements before
publication. The measurement targets pass strict Clippy; the fixtures are removed,
the cluster and builder are stopped, and 134 GiB is free. Phase 5 is not deployed.

The payload-separation batch stores command and file request envelopes in private,
read-only rows without changefeeds. Admission commits each payload with its journal,
request identity, execution slot and audit record. Private reads resolve the typed
envelope through the journal's read-only link; parent deletion cascades to the payload.
The full schema rejects payload mutation, preserves payloads on rollback and leaves
the execution fence intact when missing input prevents recovery.

The grouped Store/Computers/Computers MCP run and affected repairs qualify 296 distinct
functional Rust tests. Initial admission exposed a SurrealQL projection over `NONE`;
both request lookups now filter absent records in SQL before decoding. Existing
concurrent retries, corruption, policy, active renewal, containment, completion and
HTTP admission pass. The complete 241-file SQL validation, strict all-target Clippy,
formatting and documentation checks pass. Provider/installed cases are not claimed.

The [repeated journal experiment](../platform/store/measurements/journal-separated-2026-10-01.md)
commits all 288 metadata updates across 18 fresh RocksDB fixtures. Each native-feed
run emits 39,232 normalized JSON bytes and zero request ciphertext bytes. At 1 MiB
stdin, median attributed block writes fall from 132.195 to 0.465 MiB and median update
time from 317.705 to 19.379 ms. All owned fixtures are removed. About 117 GiB is free;
the cluster and builder are stopped. Phase 5 publication and installed acceptance are
the next checkpoint.

The Phase 5 release publishes 29 affected reference and verification images and both
charts from `6ec504d8`. Reference locks select their runnable digests, including the
agent kernel and the Computer guest's recomputed template fingerprint. Fresh Computers
trust is prepared outside Git. UAV desired state is suspended until the fresh Frames
world is published. Before publication, 56 superseded test executables release 18.2 GiB;
current executables, dependency libraries and compiler caches are preserved. The current
Reason, Stream and simulator caches are copied and byte-compared before the node reset.
Publication completes in 925 seconds with a minimum 150 GiB free, and the builder stops.
The grouped installed-tool build, ten release-rollout tests, Helm configuration and
documentation checks pass. Fresh credential preparation and rendered Secret coverage
pass. The reference reset removes only the old node's five volumes. The replacement
node exposes eight hardware GPU shares. All 38 rendered installation images are
downloaded, pinned and recorded in a byte-verified restart import manifest. Reason,
Stream and simulator caches are restored to fresh claims and byte-compared in 74.54
seconds. Another 137 obsolete test executables release 49.97 GiB while current
executables and compiler caches are preserved. About 216 GiB is free after restoration.
The platform Helm release, its 23 Deployments and both bootstrap Jobs are Ready.
Public Frames publication passes with binding digest
`506c4676f8b5daaf9b92db9bd0354dfbfeca914e561dfb21fb1830cb34d68c42`.
The activation commit selects that binding and removes the reviewed UAV suspension.
Both Helm releases and all 25 Deployments converge at `5dd357c4`. The installed batch
passes 421 hosted checks across 17 servers, with 17 declared skips, and removes its
temporary Pod and NetworkPolicy. Public full/HEAD/range Artifact delivery passes.
Headed Chrome uses the RTX 4090 through WebGL; its software WebGPU adapter is excluded
from hardware evidence. Speech dictation, a 2.5-second injected chunk delay,
cancellation, explicit send, recording transcription, reload observation and verified
JSON/VTT downloads pass in 19.75 seconds. Audit CLI verification passes both partitions;
export readback matches eight S3 blocks and observes 2,163 collector log records.
Live Recording acceptance passes Stream replay over acknowledged parts and grounded
Reason over the same selection in 156.63 seconds. Live Stream inference and its encoded
preview pass in 7.08 seconds. Map acquisition, activation and route admission pass.
Composed flight and the deferred cross-replica/domain assertions remain.

The first image-import manifest used shorthand repositories and tag-plus-digest
references. K3s pulled them but failed its image-store lookup, leaving the imported
references unpinned after CRI replaced their labels. Garbage collection removed four
unused installation images, including the simulator. The corrected manifest uses
fully qualified digest references; K3s completes the import and pins all 38 images.
Speech backpressure and audit-readback observations made during the repeated simulator
extraction fail. Both pass after extraction finishes, with unchanged assertions.
The failed observations remain in local diagnostics. About 212 GiB is free, and BuildKit
is stopped.

## Phase 6: Extension Crate And Shared Plumbing

The extension crate now provides separate contract and MCP features. Its dependency
direction is foundational types → knowledge extension → MCP core, with no dependency
on domain servers. Six grouped native tests qualify closed declarations, collection
and access checks, content-bound conditional responses, stable docs paging, bounded
search snippets and compile-time document digests. Python observation support and
live document conformance are implemented. Store schema and transactional APIs pass
native qualification. Owner change/search/create probes and domain fixtures are implemented. All eighteen
participating sources pass the installed checks summarized in Status.

The gateway records successful resource reads after validating the source observation
and before delivery. It checks the source URI, bytes, collection owner and conditional
revision, and forwards metadata only to declaring callers. The audit contract validates
member/target and status/observation relationships. Its reviewed observation preserves
external system and record identities while excluding navigation URLs that may carry
signed credentials. Private zero-TTL delivery and explicit upstream request metadata
keep conditional reads on the source authorization path. The kernel uses Rig's existing
credential preflight, validates original text and retains the observation beside a
provenance line. Both count against its existing item, response and episode limits.
Grouped native qualification passes 25 Rust tests and six Console schema tests.
The gateway fixture checks committed records through a separate Store connection after
each delivered read, reauthorizes conditional reads, records revocation, and blocks
content when its audit writer stops. Kernel checks preserve the original body while
counting provenance at item, response and episode limits. Workspace compilation,
affected all-target/all-feature Clippy, Console TypeScript and docs checks pass.
Generated Console audit schemas and types match the reviewed contract. Installed
qualification remains open. Five superseded linked test executables were removed;
library, incremental, BuildKit and model caches are preserved.

The Rust docs integration declares the extension through checked setup and routes
ordinary and conditional reads through the same authenticated path. Map uses the
shared helper through its existing discovery adapter. Python and Node document indexes
use the current typed page shape; the Node adapter also emits negotiated observations.
C32 is now in the checklist and every server declares its pending qualification explicitly.
The repository compiles with `cargo check --workspace --all-targets`; 19 Python
document tests, the fork fixture's document checks and the Node syntax check pass.
The grouped Rust run passes 164 tests, including real stateless HTTP negotiation,
content validation, matching conditional reads and denial under an unauthorized
profile. RMCP's explicit request options carry the knowledge capability because its
discover lifecycle overwrites capabilities in request params. The client helper also
forces conditional reads through the server instead of its response cache.
Workspace Clippy passes; the subsequent full-workspace test/doc sweep from
`enforce rust` was stopped because the affected checks run in the focused batch.
Map's authoring, acquisition and routing designs are separate embedded documents;
the main design fits within the kernel's 64 KiB item limit with provenance overhead.
Its document test guards that budget for every embedded document.

Python's shared adapter negotiates observations per request and reauthorizes matching
conditional reads through each server's existing identity admission. The Hatch hook
embeds SHA-256 manifests in both Datasheet and the independent fixture; package loading
rejects missing manifests and modified bytes. Source-tree development is explicit.
Native conformance validates K01–K06 through live declarations, enumeration, reads,
content hashes, matching conditions and unauthenticated conditional denial. The
independent fixture also qualifies an authenticated caller without the required scope.
K07/K08 skip capabilities that are absent and require typed owner probes for declared
change and search capabilities. The shared runner executes mutation/restart and
authenticated search/denial cases. Domain fixtures and installed integration remain open.

The grouped document batch passes 42 Python tests, including real stateless HTTP
negotiation and conditional denial. Both wheels load embedded documents without a
source-tree fallback and match their build-time hashes. The independent Rust server
passes live K01–K06, and Map's document-budget test passes. Affected all-target Clippy,
Rust formatting and documentation links pass. The cluster and BuildKit stay stopped;
about 170 GiB is free and useful build caches are preserved.

The Store batch adds typed knowledge and embedding contract crates below persistence
and the future service. This avoids a Store/server dependency cycle and keeps runtime
dependencies out of consumers. The dependency tree contains neither SurrealDB nor an
MCP runtime for either contract. Index generation IDs, approval fingerprints, embedding
spaces, vector dimensions and chunk settings are checked at construction and decoding.
Migration 0101 defines the catalog, generation, member, coverage and active-pointer
records. Each generation owns a separate BM25/HNSW chunk table. Source-read epochs
fence late results; SQL checks current approval and access before candidate decoding
and limits. Coverage and active-pointer compare-and-set gate generation activation.
Native references reclaim retired members and chunks before the empty table is dropped.

The native fixture passes incomplete activation, malformed denied rows before LIMIT,
Work Context and tenant isolation, stale-reader fencing, definitive deletion, embedding
space separation, generation-bound cursors, approval revocation and stale catalog writes,
active-generation cleanup refusal and retired-generation cascades. It uses two editor
connections to an isolated SurrealDB 3.3 container and synthetic vectors; it claims no
embedding or GPU qualification. The contract tests, workspace all-target compilation,
affected all-target Clippy, SQL validation, formatting and documentation links pass.
The cluster and BuildKit stay stopped, and about 162 GiB is free. The indexing loop,
metadata-only ingestion, ranking, evaluation receipts and GPU runtime belong to Phase 8.

The Node/probe batch implements Chart image document manifests, strict observation
negotiation, typed conditional validators and cursor handling. The pinned Node SDK's
HTTP fixture covers authentication, unmodified reads, malformed metadata and tampering.
K07 now runs owner mutation/restart callbacks and checks member plus collection
subscriptions, changed revisions and retained state. K08 executes declared searches
with two authenticated readers, checks resource links and observations, and requires
full and conditional denial for excluded hits. Shared `SearchResults` types and the
resource-link builder keep domain adoption on one response contract. The synthetic
probe fixture qualifies the checker; each domain still supplies its real Store/source
lifecycle and access cases. The grouped batch passes 16 conformance tests, six shared
contract tests and six Node tests. Workspace all-target compilation, affected
all-target/all-feature Clippy, formatting and documentation links pass. Chart
image document hashes were also built and checked from the current Markdown bytes.
Installed qualification remains open.

The installed composed run at `442ba70b` completed mission execution, live inference,
Recording retention, Stream replay, grounded Reason and cross-context access checks.
Its subsequent visual stage rejected a black live-view frame: 1280×720 H.264, 34
decoded frames, zero measured luma. The failure and diagnostic images are retained
under `output/development/foundations-phase5-publication-6ec504d8/composed-flight`.
The run exited 1 after owned postflight recovery. The reference cluster is stopped
while Phase 6 source development continues; visual acceptance remains unqualified.

## Phase 7: Source Owner Review

Source-owner review (2026-10-02) traces K09 and K10 through the implemented reads,
writes and listeners. It does not establish installed K01–K08 acceptance.

| Source | Provenance and revision review | Change delivery review |
|---|---|---|
| Map | Authoring summaries select the stored body and parent access together. Feature mutations replace the revision timestamp and recorded actor; publications use stored publisher attribution. Layers omit an unrecorded modifier. Revisions hash summary text and access, with a digest of the full source. | Store observation covers layer, feature, publication and release changes. Geographic summaries declare periodic revalidation rather than listen. |
| Time | Calendar/epoch/release observations use stored ownership and source timestamps. Events omit the unrecorded modifying actor. Revisions hash content and access; packaged authorities have content-derived immutable IDs. | Store observation covers event writes and scheduler transitions. Immutable versions and compiler references keep their declared immutable profile. |
| Reason | Findings compare retained Task provenance with the current Artifact snapshot. The revision hashes the returned summary and access descriptor. No caller-supplied attribution enters an observation. | Task, occurrence and grant observations wake caller-specific SQL fingerprints. Stored grant/retention deadlines wake idle listeners; revoked members invalidate before closure. |
| Artifact | Metadata observations use the service's checked snapshot, preserve stored access deadlines and omit an unrecorded modifier. Revisions hash the metadata text and access descriptor. | The review found missing deadline wakeups and invalidation before closure. The repair selects deadlines using Store's read predicate and invalidates admitted members and their subscribed index before ending a revoked stream. Qualification is recorded with the repair below. |

Artifact listener qualification (2026-10-02): all eight server cases pass, including
real MCP HTTP delivery and Artifact service authorization over an isolated Store.
The fixture verifies member-first mixed subscriptions, grant revocation, grant expiry
without a database write, denied-row deadline selection and occurrence retention
outside the first 100-member page. Both member and root invalidations precede stream
closure; full and conditional reads deny access afterward. Deadline selection precedes
HTTP reads so a deadline crossed during reconciliation still wakes the listener.
Affected strict Clippy, formatting, SurrealDB 3.3.0 query validation and documentation
links pass. The Artifact MCP image is published from `529c2b66`; installed
qualification remains open. The other 23 images from the qualified `fe77f5b3` batch remain applicable: the new
Cargo dependencies are test-only and no shared runtime contract changes.

## Phase 8: Knowledge Service

Retrieval comparison checkpoint (2026-10-02 UTC): the native RocksDB/CUDA workload
passes all three pinned Qwen checkpoints against the same 153 members and 78 judgments.
Each rebuild produces 1,197 chunks and serves searches against the preceding generation
before activation. All models reach recall at ten of 1.000. Throughput is 98.34, 50.50
and 29.81 chunks/second for 0.6B, 4B and 8B, with observed model-process peaks of 6,962,
14,456 and 20,964 MiB at the common 4.625 GiB cache allocation. The
[runtime measurement](../platform/runtimes/embedding/verification/retrieval-2026-10-02.md)
records the complete rankings and GPU samples, the smaller-model choice, and the
constructed-corpus and single-run limits. This closes the controlled model comparison
and concurrent-search rebuild measurement. It does not close installed source conformance,
GPU coexistence, or deployment of migration 0104 and the Store write-profile correction.

Client checkpoint (2026-10-01): upstream still lists vLLM 0.30.0 as its latest stable
release. The typed HTTP client implements model discovery, document/query batches,
Qwen query formatting, vLLM priority, shared bulk/request limits, complete-operation
deadlines and streamed response bounds. Five native HTTP cases and two input/vector
contract cases pass, together with affected strict Clippy and workspace all-target
compilation. The provider's `/v1/models`
response identifies the served model; revision, dimension and image identity come from
deployment configuration. The pinned 0.6B checkpoint's ten runtime files are staged
locally and verified against upstream Git/LFS identities; `checkpoint.sha256` records
their SHA-256 digests. This checkpoint covers the client and local artifact staging.

Runtime checkpoint (2026-10-01): the full Helm preset now
includes `embedding-runtime`, with a pinned official vLLM image, model-cache PVC,
checksum init, NVIDIA allocation, private API-key Secret and a dedicated network policy.
The reference installation accounts for eight GPU shares. Three rendered Helm checks
and all 31 deployment-contract tests pass. The CUDA reference generator has produced
four 1,024-dimensional vectors using the pinned image's PyTorch 2.13.0, Transformers
5.17.0 and cuDNN attention on the RTX 4090. The initial 0.12 GPU-memory fraction could
not allocate the 3.5 GiB KV cache required by the model's full 32,768-token context;
the declared fraction is 0.25. All four served vectors pass the 0.999 cosine threshold
(minimum 0.9997335). The interactive request completed in 81 ms before all six bulk
batches; this 192-input fixture measured about 300 inputs/second. The runtime refuses
a corrupted checkpoint and missing CUDA; unauthenticated model discovery returns 401.
The real provider rejects `dimensions` unless its model metadata enables Matryoshka
resizing, so the client requests native dimensions and checks the resulting vectors.
The owned runtime was stopped and removed after qualification, preserving its compilation
cache. Installed namespace isolation, simultaneous GPU memory qualification,
Knowledge retrieval evaluation and model-size comparison remain open.

Library checkpoint (2026-10-01): `servers/knowledge-mcp` now owns an isolated contract
feature with typed scopes, resource routes and search inputs. Its runtime implements
bounded source enumeration, authenticated gateway reads, source-byte chunking, bulk
embeddings, metadata-only ingestion and inactive generation builds. Member refreshes
fence old chunks before I/O. The shared access evaluator accepts owner grant types
through `AccessGrant`; Knowledge uses the same decision function as Artifact without
creating Artifact identities. Store shares one SQL admission fragment across candidate
pages and search, uses native BM25/HNSW and `search::rrf`, and groups results by member
before returning rows. A 128/512/2,048/8,192 candidate sequence handles duplicate chunks.

The native pipeline qualifies source-to-search behavior with synthetic vectors, including
140 malformed denied rows, three readable members, vector-only retrieval, required source
scopes, selected-context membership, metadata body exclusion, failed-refresh invalidation,
changed labels and 200 chunks for one member. A failed rebuild preserves the active index.
This is library qualification. Control-plane approvals and indexing-client registration,
catalog discovery, subscription/revalidation/restart coordination, the hosted MCP surface,
Helm/image delivery, actual-model retrieval evaluation and installed qualification remain
open. All 42 focused native tests pass, together with strict affected Clippy, workspace
all-target compilation, the contract-only dependency/build checks, SurrealDB 3.3 query
validation and 997 documentation links. The cluster and BuildKit stayed stopped. The
shared-type compilation created additional Rust artifacts; removing an abandoned
September 29 linker temporary reclaimed 1.27 GiB without deleting library or incremental
caches. About 68 GiB remains free before publication work.

Resource-admission checkpoint (2026-10-01): Knowledge carries the current profile's
scheme, prefix and restricted-template selectors into SQL before both ranking limits.
The foundational types own lexical matching and checked template literals; Store binds
them as data. The shared policy evaluator supplies per-source read selections through
the same rules as concrete gateway reads. `SearchCaller::from_policy` derives collection
admission and tenant-scoped context memberships from one current catalog, and checks
the signed active context's membership and policy revision. Final access evaluation
also checks the member URI.

Native retrieval tests return three readable results behind 145 profile-denied members
whose observations cannot decode, for keyword and semantic-only queries. A repeated
suffix case proves the SQL matcher preserves first-delimiter behavior. Store cursor
pages agree with the foundational matcher across all selector forms and encoded
characters. Exposure revocation removes results without reindexing. Hosted identity
verification, active control-revision refresh and the source coordinator remain part
of the integration work below. Nine Knowledge tests, five Store tests, all 546
concrete-read/selection comparisons, 140 shared MCP contract tests and the foundational
suites pass. Strict affected Clippy, workspace all-target compilation, contract-only
compilation and dependency isolation, SurrealDB 3.3 query parsing and documentation
validation pass. The cluster and BuildKit stayed stopped; about 61 GiB remains free.

Indexing-admission checkpoint (2026-10-01): source manifests now carry typed collection
approval, steward groups, authoritative subjects and a retained-data label ceiling.
OAuth registration binds an automated private-key machine client to approved collections
and a dedicated read-only resource profile. MCP core imports generic Knowledge domain
types; source-owned scopes and resource vocabulary stay in their libraries. Source reads
carry collection and enumeration/member intent. Gateway admission verifies current
approval before reading and again before delivery. Returned observations must match the
collection, tenant and label ceiling. The indexer rejects outside-approval text before
embedding. Member subscriptions require a non-stale observed active member; enumeration
subscriptions may precede the initial build. Raw Artifact bytes and unrelated operations
are denied for indexing clients. Native configuration, Store lifecycle, gateway audit
and source-to-search checks pass in one 170-case batch; strict affected Clippy passes.

The batch exposed short HNSW results for identical vectors on SurrealDB 3.3.0. An isolated
database reproduction confirmed the failure with the predicate pushed into `KnnScan`.
Search now completes a short ANN candidate window with exact cosine-distance ranking
inside SQL, applying the same admission predicate and a 10-second query timeout.
Semantic-only tests cover denied malformed observations and 200 chunks for one member.
No Rust post-filter or permission bypass was introduced. Actual-model retrieval quality
and installed performance still require qualification. Indexing reads retain per-read
audit until five-minute aggregation ships. Installation client provisioning, catalog
discovery, hosted MCP, source subscription/revalidation/restart coordination and packaging
remain open. The cluster and BuildKit stayed stopped throughout this batch.
Workspace all-target compilation, the Knowledge contract-only build and dependency
isolation, expanded SurrealDB query validation and all 1,000 documentation links pass.

Hosted API checkpoint (2026-10-01): `knowledge-mcp` serves typed search and embedding
tools, source/collection catalog resources and embedded docs over the shared stateless
HTTP transport. Real-JWT native tests use the maintained MCP client and isolated Store
fixtures. Search links work across replicas. Current OAuth scope reductions, browser
session revocation, enterprise/tenant/principal disablement and revocation during
embedding prevent delivery. Discovery evaluates policy per descriptor. Gateway directory
synchronization now includes a distinct delegated actor.

Catalog approval and scope admission run in SQL before decoding. Source grouping and
cursor selection run before the 101-row page limit; exact source and collection reads
bind their selection. Tests cover 105 owners, malformed hidden registrations and changed
approvals. Registrations include the source's positive contract-declaration revision in
their fingerprint. This is a hard cut of the undeployed cache contract.

Native qualification covers 13 Knowledge tests, six Store tests, two domain-contract
tests, two affected gateway checks and 140 MCP contract tests. Strict affected Clippy,
workspace all-target compilation and the contract-only dependency check pass. Synthetic
vectors establish HTTP, indexing and access behavior; GPU retrieval quality and installed
acceptance remain separate. Schema conformance, catalog completion/statistics/listeners,
source discovery/reconciliation/restart coordination, five-minute indexing audit windows,
machine-client provisioning and packaging remain open. The cluster and BuildKit stayed
stopped during this batch.

Coordinator checkpoint (2026-10-01): the library establishes listeners before source
enumeration, renews a tenant lease during slow reads, reconciles queued changes before
activation and reuses a matching generation after restart. Store checks process,
collection and member epochs in index transactions. Collection-completion tickets
reject late traversal receipts. SQL excludes unready source epochs, expired mutable
leases and expired observations before decoding or ranking. Missing members from a
fresh enumeration stay hidden under their earlier epoch without asserting deletion.

Conditional reads reuse text and embeddings only when revision, digest, access,
provenance and title agree. Source loss and cancellation exclude mutable results;
unchanged revisions with changed access fail closed. Native qualification covers
16 Knowledge tests, eight Store Knowledge tests, nine extension tests and eight migration
checks. The coordinator suite also qualifies scheduled conditional revalidation without
a listener. Strict affected Clippy, workspace all-target compilation and the contract-only
build pass. Cleanup removed 17 superseded test executables (3 GiB), preserving compiler
and incremental caches; the cluster and BuildKit stayed stopped. The core
coordinator is not yet wired into the binary: gateway listener admission, automatic
source discovery, authenticated connection rotation/recovery, catalog subscriptions,
five-minute audit windows and installed delivery remain open. This batch hard-cuts the
undeployed Knowledge cache schema; it adds no historical-data adapter. The reference
publication still predates that schema.

Source-integration checkpoint (2026-10-01): Knowledge discovers its approved collections
through a native gateway catalog listener, waits for pending discovery to complete,
and verifies source ownership, URI scheme and hosted contract revision. A typed
source-contract read permits discovery before Store registration, including catalog-only
approvals. Domain reads and root subscriptions still require indexing approval.
Subscription roots are selected in SQL by tenant, current approval, required scopes
and exact enumeration URI. Ambiguous approved roots fail closed. Native tests prove
denied malformed registrations are excluded before decoding.

The pinned SDK acknowledges filters before asynchronous listener setup. Sources now
send initial resource/catalog invalidations after registering their receivers and
admitting the caller. Gateway indexing listeners and internal catalog watchers wait
for these signals before exposing observation readiness. Mixed filters preserve catalog
changes during startup. K07 separately checks initial readiness and mutation delivery;
baseline-only streams cannot qualify as change delivery. Native qualification covers
18 Knowledge tests, nine Store Knowledge tests, 39 gateway MCP tests, 140 MCP contract
tests, nine extension tests, the live conformance-checker fixture, 13 Artifact tests
and five Task resource tests. Strict affected Clippy, workspace all-target compilation,
the Knowledge contract-only build and all 999 documentation links pass. Cleanup removed
22 superseded test executables (4.43 GiB), preserving compiler and incremental caches.

The source adapter consumes an authenticated peer; machine OAuth provisioning and
rotation, catalog persistence bound to the active control revision, binary coordinator
wiring, public catalog subscriptions/completion/statistics, five-minute audit windows
and installed delivery remain open. This checkpoint adds no historical cache adapter.
The cluster and BuildKit stayed stopped throughout the native batch.

Machine-host checkpoint (2026-10-01): the Knowledge binary runs one configured indexing
worker per tenant. Each worker derives its machine profile, scopes, source approvals
and Work Context from the active control document. It signs a short-lived JWT assertion,
uses the maintained MCP HTTP client and rotates its connection before token expiry.
Control and catalog notifications restart discovery. Source loss releases the index
lease and starts bounded connection recovery. Shutdown drains workers and HTTP serving.
Readiness requires every configured tenant to have an active index or a complete
catalog-only selection.
Knowledge's CPU Helm workload, shared embedding identity, configuration/Secret mounts
and independent liveness/readiness probes pass native rendering checks.

Store captures control authority and the previous tenant catalog before discovery, then
checks both inside the transaction that publishes its complete replacement. Removed
approvals disappear atomically. Source fingerprints cover the declaration and approval;
the separate publication check covers the control revision. An unrelated installation
edit therefore preserves generation identity and cached vectors after reconciliation.
The machine-host fixture qualifies these paths with signed assertions and real HTTP,
including invalid token responses and redirects. Provisioning, packaging, public catalog
subscriptions/completion/statistics, five-minute audit windows and installed delivery
remain open. Synthetic vectors qualify lifecycle behavior; GPU inference acceptance
belongs to the embedding runtime. This batch adds no historical cache adapter.
Grouped qualification passes 19 Knowledge tests, ten Store Knowledge tests, two
Knowledge contract tests and the gateway indexing-admission test. Strict affected
Clippy, workspace all-target compilation, contract-only dependency isolation and all
999 documentation links pass. Cleanup removed 18 superseded test executables
(3.58 GiB); compiler and incremental caches were preserved. The cluster and BuildKit
stayed stopped.

Packaging checkpoint (2026-10-01): `knowledge` joins the full deployment selection and
requires Gateway, Store and the shared embedding runtime. Its OCI target participates
in the existing Rust Trixie build family and offline catalog. The chart renders one
replica with Recreate updates, installation-owned tenant JSON configuration and a
separate read-only signing Secret. The qualified embedding-space identity agrees with
the GPU reference fixture. Knowledge requests no GPU or local storage; the shared
runtime keeps its mandatory GPU allocation. Independent liveness permits initial
indexing and reconnection to proceed without a probe-triggered restart. Configuration
changes advance a public bundle revision in the Pod template.

Native qualification passes 32 deployment-contract tests, twelve rendered Helm tests
covering Knowledge and the existing Computers, Embedding, Speech and Workspace charts,
and three hosted Knowledge HTTP tests. The image planner resolves one shared-build
Rust binary. The offline catalog test, strict affected Clippy, workspace all-target
compilation and all 1,002 documentation links pass. Image build/publication,
reference machine-client provisioning and gateway
registration, installed source/embedding acceptance, public catalog subscriptions,
completion/statistics and five-minute audit windows remain open. The cluster and
BuildKit stayed stopped during packaging development.

Reference wiring checkpoint (2026-10-01): the installation configuration registers Knowledge
and a dedicated `knowledge-indexer` client, with viewer membership in Operations and
sixteen approved collections from Map, Artifact, Time and Chart documents. Catalog
discovery and mutable-source subscriptions have separate policy grants. Public worker
configuration and its rollout digest are separate from the private signing Secret.
The installation's new public Ed25519 JWK matches its mode-0600 private key outside
the repository. User profiles request Knowledge read, search and embedding scopes;
source resource links preserve their owning schemes.

Eight owner-local composition checks, four gateway fixture checks, strict affected
Clippy and 1,005 documentation links pass. Kustomize's public worker data matches its
configured digest. The Knowledge image staged successfully from `c66fabdc` in 101
seconds. All 32 platform images and both UAV images published with SBOM and provenance
from that revision. Both OCI charts published as `0.1.0-c66fabdc2b70`; the reference
selects their immutable digests together with the image locks and managed-kernel pin.
Ten rollout checks and the complete Helm configuration smoke pass. The rendered
platform has 25 Deployments across its platform and managed-agent namespaces.
The generic full-render fixture supplies explicit Knowledge configuration
and signing references. Secret provisioning and deployed acceptance remain open.
The reference nodes stayed stopped during publication, and BuildKit stopped afterward.
Obsolete simulation certification images freed 23 GiB; BuildKit's configured collection
reclaimed old layers while retaining compiler mounts. Publication finished with more
than 160 GiB free.

The installed retrieval harness compiles and passes strict Clippy. It uses an ordinary
caller token through the public HTTPS gateway, checks the complete approved catalog
and active generation, searches each indexed documentation collection, and verifies
every returned source link's content digest and revision. It also calls the embedding
tool. The five-minute read-only check writes a private JSON report. Execution awaits
the rollout; hardware execution, domain recall and mutation/restart acceptance remain
separate requirements. The unchanged Computers guest keeps its qualified image and
template identity through this rollout.

Installed rollout checkpoint (2026-10-01): Flux fetched `9ab558d8` after the reference
node restarted. Both new signing/API-key Secrets were provisioned from private local
files. All ten embedding checkpoint files passed their manifest hashes on the host
and in the new model PVC; the transfer Pod was deleted after the directory was
published. Embedding and Knowledge reach readiness, and both Helm releases converge
on the published charts at reference revision `7da7f84b`.
The agent manager is Ready in `veoveo-agents`; its separate namespace keeps it outside
the platform namespace's installation-target Deployment list.

Installed retrieval checkpoint (2026-10-01): an ordinary operator client traverses all
sixteen approved collections through the public HTTPS gateway and finds one active
generation. Searches across the approved documentation collections return eleven source
links whose content digests and indexed revisions match fresh source reads. The public
embedding tool returns the declared 1,024-dimensional space. The check completes in
6.70 seconds. The running embedding container reports an RTX 4090 and CUDA 13.0.
These checks establish initial delivery; domain recall, mutable-source changes,
revocation, restart recovery and composed load remain separate acceptance work.

Four installed network probes qualify the embedding API's access: an authenticated
platform request succeeds, an unauthenticated request receives HTTP 401, and pods with
the excluded `computer-host` label or in another namespace cannot connect. The live CNI
firewall uses ICMP port-unreachable rejection. The verification guide accepts that
denial as well as a timeout, with a successful control request against the same Service
IP and port. All owned probe pods and the temporary namespace are deleted. Cargo's
installed harness resolves relative configuration and report paths from the repository
root, independent of its package working directory.

Catalog checkpoint (2026-10-01): Knowledge completes source, collection and document
arguments. Source/collection prefix matching, deduplication and pagination run in SQL
after approval and scope admission. Store returns typed source or collection identities.
Collection resources report caller-visible member/chunk counts and source timestamps
using the search admission predicate before aggregation. Checked statistics are shared
through the domain contract, with no MCP runtime in its dependency boundary.

Catalog listeners share one Store LIVE/changefeed observer per host and compare
authorized snapshots. The two-replica HTTP fixture proves initial invalidation,
source-sync changes, hidden-write suppression, unchanged discovery lists, lease expiry
without a new mutation, and revocation. Finite watch state coalesces writes; expiry
deadlines schedule re-reads without periodic polling. Catalog-list observation requires
its own server-target grant. The declaration now exports all 32 compliance statuses
from the required checklist syntax, replacing an unparsed table.

The grouped Knowledge/domain-contract suites and ten Store tests pass, including
malformed denied records, source pagination and completion lookahead. Final HTTP and
contract checks, contract-only compilation, affected all-target strict Clippy and
SurrealDB 3.3 query validation pass. The installed ignored test is not claimed for this
unpublished change. Completion/subscription gateway exposure will activate with the
next service image, batched with indexing audit aggregation. Nodes and BuildKit stay
stopped during this development batch; about 152 GiB remains free.

Indexing audit checkpoint (2026-10-01): admitted resource reads now commit mutable
five-minute collection counters and retry receipts before delivery. Windows separate
service actors and authorization contexts; denials keep individual records. The database
clock selects each interval. A worker recovers elapsed windows, constructs checked
summaries and atomically appends them to the immutable audit log. The existing sealer
then includes them in signed blocks. Member URI/revision hashes form an ordered SHA-256
chain. Open windows survive shutdown; expired retry receipts are removed in limited
batches. The unused aggregate shape receives a hard cut with regenerated Console types.

Native qualification covers duplicate reads racing across replicas, changed-identity
rollback, outcome counts, digest reproduction, separate authority groups, concurrent
finalization and receipt cleanup. Writer replacement preserves acknowledged reads and
seals one summary. Gateway tests prove committed accumulation before successful delivery
and individual denial records. Existing audit service and Knowledge Store checks pass.
Console TypeScript compilation, affected all-target strict Clippy, SurrealDB 3.3 query
validation and documentation-link checks pass. This batch still requires publication and
installed acceptance together with the catalog completion/subscription exposure. Apply
the additive Store migration, stop indexing, drain all gateway replicas and update the
gateway, Console and native audit readers together before restarting indexing. Mixed
gateway versions are outside this coordinated hard-cut profile.

Catalog rollout preparation (2026-10-01): the reference registration enables completion,
resource subscriptions and resource-list changes. Caller profiles expose completion;
the shared caller policy grants completion and subscription actions under Knowledge
read scope. Its policy revision and complete public bundle digest change together.
The installed harness now compares completion with the visible catalog, reads statistics
and receives initial collection and resource-list notifications before retrieval.
The four native HTTP checks and Helm configuration smoke pass. Deployment and the
expanded ignored installed check remain pending; the cluster stays stopped while images
are prepared.

Catalog activation inputs (2026-10-01): platform and UAV images and both OCI charts
published from `1a92901d`; the hosted Knowledge repair published from `97921a7b`.
The reference pins select those manifests, including the managed agent template and
the recording forwarder. Existing Computers guest images are preserved. Ten rollout
checks and final Helm configuration smoke pass. With root reconciliation and the
platform Helm release suspended, the installation stopped its Knowledge worker and
drained every gateway and Console pod before activating these inputs. The bootstrap
Job applies Store migrations using the matching gateway image. Installed catalog and
indexing-window acceptance follow reconciliation.

Installed catalog and audit checkpoint (2026-10-01): `07542b24` converges with both
Helm releases at revision 3 and all 24 changed Deployments Ready. The expanded public
Knowledge check passes sixteen approved collections, their statistics and completion
values, initial catalog notifications, eleven search-result/source-revision links and
the embedding tool. The embedding container reports CUDA 13.0 on an RTX 4090. Its
active generation matches the generation from before this deployment. An initial
retrieval assertion failed during source reconciliation; the subsequent check passes,
and failure diagnostics now name the source URI, expected generation and returned
members. Source mutation and fault-recovery probes remain separate acceptance work.

The installed gateway finalized sixteen collection windows for 22:30–22:35 UTC,
covering 144 reads, 93 unchanged responses and two failed source reads during rollout.
Each window has a member digest and a distinct collection/actor/authority identity.
The native audit CLI verifies 890 signed blocks containing 2,473 tenant records with
zero clock findings. A sealed export through block 911 contains all sixteen observed
window identities. The reference installation check also passes declared deployment
readiness, GPU capacity, public Console and authorization endpoints, and full, HEAD
and ranged Artifact delivery. This proves installed finalization and chain integrity;
native race and retry cases supply the separate aggregation correctness checks.
The temporary caller token is removed and the cluster is stopped after acceptance.

Hosted conformance repair (2026-10-01): the Knowledge HTTP suite now runs the shared
hosted checker. It found two delivery defects: the embedding request schema omitted
its root object declaration, and `knowledge://contract` had no read handler. Both
paths are corrected. The checker reports 31 passes; K07 and K08 do not apply to the
immutable documentation collection. K09 and K10 are owner review obligations: document
provenance and revisions come from the shared build-time document provider.
All five native HTTP checks and four contract tests pass, along with contract-only
compilation, strict all-target Clippy and document links. An earlier HTTP run exposed
an extra test assertion and Docker cleanup deadlines under publication I/O. The
corrected suite passes after I/O eased, with owned fixture cleanup complete. This
repair still requires its own image publication before deployment.

During this publication, scoped cleanup removed 100 unlinked Rust executable copies
older than 24 hours, reclaiming 64.26 GiB. It retained two newer copies per name, running
executables, all dependency libraries and every incremental cache. The cluster stays
stopped while BuildKit publishes the platform, charts and UAV images. Registry cleanup
waits until publication and installed acceptance establish the retained image set.

## Phase 9: Reusable Reason Analyses

Finding admission checkpoint (2026-10-01): Reason's library selects successful Tasks
and their readable result Artifacts in one SQL query, using Artifact's extracted
predicate and typed caller bindings. Current result provenance must match the analysis.
The query applies retention, clearance, current grants and selected-context access
before decoding or its 101-row page limit. Task control keeps its owner checks.
The new native case exercises 105 readable findings behind 120 malformed denied
outputs, mismatched provenance, unsuccessful and expired Tasks, grant expiry and
revocation, and cross-replica pagination. The complete run passes with fixture cleanup
after image publication stopped. Artifact's native service admission regression passes,
as do query validation on SurrealDB 3.3.0 and strict all-target Store and Reason Clippy.

Finding source implementation (2026-10-01): the hosted surface declares
`reason.analyses` and `reason.results`, with collection-bound cursor types, SQL ID
completion and bounded `FindingSummary` members. Reason's vocabulary is analyses,
findings and results; documents name actual server documentation. The publisher and
reader share typed Artifact provenance, including a closed reasoning-kind enum.
Artifact's lightweight `knowledge` feature owns access-descriptor conversion without
bringing its service or transport into consumers.

Member reads check stored result provenance and recheck Task and Artifact access after
metadata I/O, including conditional reads. Revisions cover both summary bytes and access.
A process-wide Store observer wakes listener-specific SQL fingerprints over every
admitted finding, including members outside the first page. Grant and retention
deadlines participate in wakeups. These handlers require hosted stream qualification,
reference installation approvals and deployment, K01–K10 acceptance, and retrieval
evaluation before Phase 9 is accepted.

Native qualification passes 72 Reason and Artifact cases. The added cases exercise
URI and cursor admission, bounded Unicode and event excerpts, checked summary JSON,
source-provenance disagreement, conditional reads, access-driven revisions, live Store
wakeups, grants beyond the first page and hidden-row stability. The independent consumer
passes all 22 Reason contract cases; its 142-package graph excludes MCP, Store,
asynchronous runtimes, HTTP clients and Rerun. Strict all-target Clippy, formatting,
document links and the composed SurrealDB 3.3.0 queries pass. These fixtures perform no
inference and do not qualify the installed source rollout.

Hosted source qualification (2026-10-01): the production Reason HTTP router and the
real Artifact service run against an isolated SurrealDB fixture with generated signing
keys. The generic checker reports 41 passes, including K01–K07 for populated findings
and both collection change/restart probes. Its two skips are GPU readiness, which this
stored-result fixture does not exercise, and K08, because Reason declares no knowledge
search tool. Owner assertions cover cross-context result sharing, denied Task control
and annotation reads, conditional denial after revocation, and expiry-driven invalidation
without a database write. Revocation initially closed a mixed member/root subscription
before its invalidations; the handler now sends those invalidations before closing and
rejects initially inaccessible members. Restart probes gracefully close the service and
recreate its application state at the same endpoint.

K09/K10 review: observations derive from stored Task and result provenance plus the
Artifact service's current access snapshot. Callers provide no provenance fields.
Revisions cover both the returned summary and access descriptor, and grant mutations
change them through the public HTTP path. The source listens to Task, occurrence and
grant tables; the gateway owns caller authentication and the source checks token expiry.
Reference approvals, installed source acceptance and retrieval evaluation remain open.

Bounded finding publication (2026-10-01): `FindingData` captures the task, truncated
answer or events, recording grounding and model provenance before Artifact publication.
Successful Task outputs require that checked value and derive model and pipeline
addresses from it. Missing findings or conflicting identities fail decoding; no old
output adapter is provided. Summary reads use the stored finding and current Artifact
access snapshots without downloading result bytes. The hosted fixture reads both
collections for a 1.2 MB result above its 1 MiB inline limit and asserts zero content
downloads across conformance, sharing, revocation, expiry and large-result reads.
The owning design declares the coordinated Reason admission/indexing drain and reset
of disposable Task/index state before installing the required output field. All 62
Reason tests and the isolated consumer's 24 contract cases pass.

Reference source rollout inputs (2026-10-01): Reason is published from `0b7407bf`
with runnable digest `sha256:dd9113d4236178afea95346372fb4f9d1dfc1680408cb7ae87e82c96e087d446`.
Release qualification preserves the staged runnable digest and records SBOM and
provenance. The reference approves Reason analyses, results and documentation, taking
its catalog to nineteen collections. The indexing profile keeps resource-only access
and Operations viewer membership; composition and Helm checks pass. With gateway,
indexing and Reason pods drained, the reset removes two completed Reason Tasks and
their two gateway routes. No analysis was unfinished and no agent awaited those Tasks.
Full result Artifacts and recordings keep their current formats. Installed convergence,
fresh GPU publication and retrieval qualification are still required.
The first installed catalog read reaches all nineteen collections, but its search gate
finds Reason links rewritten to `knowledge://`: the installation omitted Reason from
Knowledge's declared reference schemes. The corrected bundle preserves those owner
addresses, and the composition check now requires every approved source scheme there.
The initial image/configuration converges at `b63ae49f`; installation-wide readiness,
GPU allocation and public full/HEAD/range Artifact delivery pass at that revision.

Installed finding checkpoint (2026-10-01): the corrected configuration converges at
`af73f95b`, and catalog, completion, subscriptions, source-linked search and embedding
pass for all nineteen collections. A fresh analysis samples six recorded frames and
completes inference in 101,953 ms. Both its analysis and result summary rank first for
the focused aerial-terrain query. The installed harness compares every summary field
with the completed output, validates conditional reads and matches indexed revisions
to fresh source observations. The same checks pass after separate Reason and Knowledge
restarts. This single-finding query establishes delivery; corpus recall and GPU load
evaluation remain required.

The installed access test stops before granting access because the gateway wraps a
source's typed MCP rejection in an internal error. The shared forwarding correction
preserves upstream protocol code, message and data while transport failures use internal
errors. Native qualification covers denial forwarding and its completion audit. The
corrected gateway must be published before completing installed grant/revocation checks.

The forwarding fix is published from `db6cec6f` with runnable digest
`sha256:7023201f251b260236334867a990914a548c2cf3507f8b963d483301b8c764c3`.
All fifty native MCP forwarding cases and strict library Clippy pass. Staging takes
124 seconds; release qualification adds SBOM and provenance in ten seconds without
changing the runnable digest. The reference lock selects that image for installed
access qualification.

The gateway converges at `dc1c67db` and preserves typed source rejections. Finding
retrieval still passes. The access harness also exposes a cold discovery race:
ordinary profiles do not consume a declaring source's initial catalog notification
before fetching, so it can invalidate that fetch. Catalog watches now apply K07
readiness to every caller of a declaring source. External servers without the
extension keep their declared protocol behavior. The delayed-source fixture covers
ordinary and indexing callers, early closure, and a non-declaring external server.
All 51 native forwarding checks pass. The corrected watch is published from `ce4baba4`
with runnable digest `sha256:aa88a9ca796053ae5edae8b0d98aa8f6c85b4cde1729865e1847169c94df37b7`.
Staging reuses dependency layers and takes 67 seconds; release qualification takes ten
seconds and preserves the runnable digest. Installed qualification remains open.

The reference converges at `790e3f86`, but the subsequent acceptance batch fails.
The access fixture now grants the authenticated reviewer principal; its selected Work
Context does not assert membership in an identity-provider group. The corrected fixture
encounters a Knowledge upstream connection failure during source recovery and removes
its temporary grant. Follow-up checks find a catalog that differs from installation
approvals and cannot retrieve the completed analysis among the first ten results.
These failures require diagnosis before accepting this deployment. The earlier finding
and restart passes do not qualify the current state. Knowledge is drained, temporary
tokens are removed, and the cluster is stopped for development; BuildKit stays stopped.

Recovery diagnosis: Reason's source subscriptions end when the gateway's 60-second
request assertion expires, restarting the indexer. Separately, every ordinary index
update withdraws Knowledge readiness while clients await its catalog notification.
The repair gives typed subscription POSTs a maximum 15-minute assertion bounded by the
existing caller token. Ordinary and delegated Artifact-read assertions keep 60 seconds.
Knowledge distinguishes updates to a serving generation from initial synchronization;
SQL still hides invalidated members during those updates. Source loss and failed
workers still withdraw readiness. The catalog acceptance credential also omitted
`map:feature:read`; the next exchange must use installation-target scopes. All 41
gateway MCP tests and nine Knowledge coordination, HTTP and machine-host tests pass.
The source-loss, current-revocation and stale-member checks keep their denial behavior.
Strict Clippy and documentation checks pass. Publication and installed qualification
remain required.

The two recovery images are published from `e94f6ed5`. The gateway runnable digest is
`sha256:bea72699273075db598dd31d2138b3fb15774e20f7c8fbdd172b380e4b5da7d2`;
Knowledge is `sha256:434c55b6a52302eecb4bf5057f07ca1dbec9c38825df0fc5956da200886b73d5`.
Staging both images takes 128 seconds, and release qualification adds SBOM and provenance
in nine seconds with unchanged runnable digests. The reference selects both for the
installed recovery and result-grant batch.

Installed recovery checkpoint: both Helm releases and the four selected Deployments
converge at `391a4516`. The maintained Reason access harness completes its grant/revoke
cycle in 23.5 seconds. The reviewer can read the result and both indexed finding
collections while the grant is present; public Task get/cancel and the annotations
Artifact stay denied. Revocation changes source revisions, removes source and search
access, and the harness confirms its temporary grant is absent before returning.
The reviewer uses the same machine principal in another selected Work Context; the
hosted fixture separately qualifies a different principal.

Finding retrieval then passes in two seconds with both source revisions matching the
index and both summaries ranked first. The public catalog, completion, statistics,
subscriptions, embedding and thirteen linked source observations pass across all
nineteen approved collections in thirteen seconds. These checks run more than a minute
after the indexer's source listeners open. Captured logs contain no renewed source
failure or expired finding-authority error in that interval. The active generation is
preserved. Corpus recall, model comparison and the remaining installed conformance
probes still require qualification.
