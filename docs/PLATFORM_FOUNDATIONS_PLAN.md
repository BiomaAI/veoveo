# Platform Foundations Plan

Current direction: every contract change in this plan is a coordinated hard cut.
Historical data requires no support or migration. Remove compatibility adapters
introduced during earlier checkpoints; those checkpoint descriptions are not
requirements to preserve them. Current-format restart and failure recovery still apply.

Status: Phase 0 accepted and published on 2026-09-27 at `1177185f`; documentation
checks and reference GitOps convergence passed. Phases 1–3 are in progress.
Phases 4–9 have not started. The platform and UAV images and charts are published at
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
expiry, overflow and injected rollback cases pass. Store migration 0097 adds the composite
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
hand-built mechanisms into the database. The sixth makes every server a knowledge
source and adds the `knowledge-mcp` catalog and index.

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

## Standards And Protocols

| Standard or protocol | Role in this plan |
|---|---|
| MCP `2026-07-28` extensions and `_meta` key rules | Identifier forms and the `ai.veoveo/knowledge-source` extension |
| RFC 6570, iri-string 0.7.14 | Foundational ASCII resource templates, typed MCP descriptors and expansion qualified against domain builders |
| RFC 9110, RFC 9111, RFC 8246 | Revision, freshness, and immutability semantics for knowledge reads |
| W3C DCAT 3 | Catalog model in `knowledge-mcp` |
| SurrealDB 3.3 | Audit records, change feeds, LIVE queries, table views, record references, catalog, `FULLTEXT` BM25, and `HNSW` indexes |
| vLLM 0.30.0 pooling runner, OpenAI Embeddings API, `Qwen/Qwen3-Embedding-0.6B` | Embedding runtime on a hardware GPU |
| `veoveo.ai/installation-target/v1` | Installation input for installed smoke scenarios |
| OCSF 1.9.0, W3C Trace Context, RFC 9162, RFC 8785, S3 Object Lock | Audit record export, correlation, sealing, and write-once retention |


## Working Rules

- Work autonomously. Do not stop to ask the user questions. When a choice is open,
  take the option closest to the designs this plan links, or the best-supported guess,
  and record the choice in the commit message.
- Define a complete behavior or contract change before editing, including its direct
  consumers, fixtures and documentation. Finish those related edits before compiling.
  An individual helper, query or ID wrapper is not a validation checkpoint. Use reads,
  formatting and query parsing during implementation; run an early test only to resolve
  a concrete uncertainty that would change the implementation.
- Select the affected packages, targets and features once per batch. Inspect development
  dependencies and preserve the qualified Cargo feature graph, build directory and
  environment. A stable compilation graph does not require executing every test in it.
  Keep one Cargo pipeline. For broad type migrations, use `cargo build --keep-going`
  with the selected test targets to collect independent compiler errors, fix them
  together, then proceed to tests using the same graph. Avoid a separate `cargo check`
  pass when the required test build provides the same compiler feedback.
- Validate in three checkpoints: focused behavioral regressions after implementation,
  affected component and consumer suites before committing the completed batch, and
  workspace/deployment acceptance at an integration milestone. Select regression cases
  by the changed behavior and consolidate failures before editing again. Use
  `--no-fail-fast` for independent test targets; it does not aggregate compiler errors.
  Passing focused cases count toward the batch's qualification; exclude them from the
  remaining run while their source, dependency inputs and environment are unchanged.
  A failure or follow-up edit repeats only checks whose inputs or behavior changed.
  Reuse current native executables for fixture-only retries. Rebuild after source or
  dependency changes. Test-name filters narrow execution but still compile selected
  targets; choose targets first and avoid blanket `--workspace --lib` feedback runs.
- Include direct consumers when a shared contract changes. Broaden qualification when
  the affected dependency closure requires it, rather than after every local edit.
  Deployment configuration uses the normal harness binary and `helm-config`; gateway
  integration tests run when their integration changes. Publish and perform installed
  acceptance once the composed batch passes local qualification. A documentation-only
  edit runs the documentation check and does not repeat native behavioral suites.
- Evaluate contract adequacy during each change. Improve outdated or inexpressive
  contracts when the accepted architecture requires it, without waiting for the user
  to identify the problem. Explain material tradeoffs, update the owning design and
  this plan, and record ownership or repository-wide decisions in
  `docs/CONTRACT_EVOLUTION.md`. Qualify the replacement before advertising it.
- Apply the [modular type architecture](#modular-types-and-server-contracts) in every
  phase. Keep domain vocabulary with its owner, preserve specific types through
  internal APIs, and provide focused builders where construction needs validation.
  Phase 3 establishes the shared interfaces; later work must use and extend them
  without adding server-specific dependencies to core.
- Do not let a blocker stop progress. When one step cannot finish, work around it,
  leave a `TODO(foundations): <what remains and why>` comment at the exact code path,
  add a row to [Deferred Work](#deferred-work), and continue with the next step. Defer
  only what later work does not depend on.
- A deferral never weakens a requirement. Do not replace a GPU path with a CPU path,
  skip an audit record that must commit, relax authorization, or mark a failing check as
  passing. Leave that work deferred and visible instead.
- The reference installation at veoveo.bioma.ai holds no data to preserve. Stop,
  wipe, and rebuild its cluster whenever a phase needs it, without backups, data
  migration, backward compatibility, or approval. Deploy each phase there once its
  local checks pass, and run its installed acceptance there.
- Keep the reference cluster stopped during editing, compilation, and image builds.
  Start only the isolated services a focused check requires. Start the full reference
  cluster for installed acceptance and stop it when those checks finish. Use node
  stop/start for routine development; delete and rebuild when the phase requires a
  reset. Check free disk space and expected build growth before large builds, because
  stopping nodes does not reclaim their volumes or build caches. Required installed
  and hardware GPU acceptance still runs against the complete deployment.
- During this plan, clean up completed experiment containers, verified-empty unused
  volumes, and superseded build outputs between steps. Preserve useful Rust and
  BuildKit caches. Before retiring Docker or OCI registry images, protect current
  installation and rollback references, reusable dependency images, and their full
  manifest and layer closure. Old simulation experiments may be removed. This
  resource discipline applies to this plan; it adds no repository-wide development
  rule or background cleanup service.
- Before a fresh reference bootstrap, prepare the published images in the node cache
  sequentially before enabling application reconciliation. Include the rendered
  workload images and the runtime images selected through the installation locks.
  Confirm every reference in the node's CRI inventory before starting the workloads.
  Cold image extraction must finish before database provisioning and application
  startup compete for the same disk. Preserve the failed attempt's diagnostics and
  clear its owned workloads before retrying.
- Read `AGENTS.md` and `docs/CODEMAP.md` before each phase.
- Work on `main` in coherent commits, one completed concern each. Commit after the
  batch's affected checks pass; commit size alone does not trigger another validation
  cycle. Run `cargo xtask enforce docs` for every documentation change.
- Internal names and formats change by hard cut. Do not add aliases, fallbacks, or
  readers for old identifiers. This applies to stored data and server/client contracts
  throughout this plan. Keep one current format, update its callers together and reset
  disposable reference state as needed. Do not add historical-data audits, conversion
  tools, dual readers, support windows or compatibility-only rollback qualification.
  Remove such work already introduced. Current-format recovery and transactional
  rollback remain required. This rule supersedes historical-data transition tasks
  recorded in earlier checkpoints and linked component designs for this plan.
- Re-verify the latest stable release of every new dependency at the moment you add it.
  Update the pin and this plan if it has moved.
- Delete superseded plans as you go, following Phase 0.
- GPU workloads request their device and fail closed. The embedding path has no CPU
  mode.
- Keep the standards registers current. Add a standard to them in the change that
  implements it, never earlier, because each register lists what Veoveo implements.
- Update `docs/CODEMAP.md` in the same change that adds, moves, or removes a crate,
  component, or document.
- Update the Status line at the top of this plan when a phase lands, naming the phase
  and its deployed revision.

### Current Computers Validation Batches

The last broad selection ran 41 executables across seven packages. Compilation took
2 minutes 45 seconds and reported test execution totaled 6 minutes 2 seconds, excluding
the filesystem flush and orchestration. Its affected six-target follow-up compiled in
30 seconds and reported 77 seconds of tests. Those measurements support selecting
execution separately from the stable compilation graph; they are not runtime budgets.

| Batch | Implementation checkpoint | Qualification checkpoint |
|---|---|---|
| Computers ownership and public identities | Complete maintenance SQL admission, accepted-request and worker lookup review, lifecycle/access IDs and their direct consumers together | First run SQL denial, replay/conflict and identity regressions. Then run the affected domain, service, gateway/BFF and generated-schema checks once for the completed batch |
| Provider identities | Trace private runtime and retained-instance identity requirements, then update the owning types and all transport consumers together | Native runtime and provider fixtures, including current-format recovery; preserve required GPU acceptance |
| Composed Phase 3 delivery | Finish remaining Map/template setup and cross-component contract work | Run the shared contract closure and installed acceptance against the complete deployment, then stop the cluster |

These checkpoints guide this plan's work. They do not add a repository-wide testing
rule or remove required acceptance. Report implementation completed since the last
checkpoint separately from checks still pending.

## Prerequisites And Deployment

The agent runs on the reference installation's GPU host, where the `k3d-veoveo-bioma`
Kubernetes context, the local registry, and a headed browser with hardware-backed
graphics are available. Without them, deployment and installed acceptance steps go to
[Deferred Work](#deferred-work) while the local work continues.

Deploy through the reference installation's own runbook: the Release publication
section of [`examples/bioma/README.md`](../examples/bioma/README.md) builds and pushes
images to the local registry, updates the release locks, and observes the rollout with
`cargo xtask smoke gitops-converge`. Flux reconciles veoveo.bioma.ai from `main` on
`origin`, so pushing `main` to `origin` is part of each deployment and is authorized
by this plan. Use [`docs/DEVELOPMENT_ITERATION.md`](DEVELOPMENT_ITERATION.md) for
affected-image staging. The "Create the local platform" section of the same README
rebuilds the cluster from scratch when a phase needs a clean installation.

## Standards Registers

Four documents list the standards and protocols Veoveo implements. Owning designs keep
their own Standards And Protocols sections, and these registers summarize them:

| Register | Scope |
|---|---|
| `README.md` § Standards And Protocols | Product-level areas |
| `docs/TECH_DESIGN.md` § Standards And Protocols | Cross-component standards and their supported subsets |
| `docs/ARCHITECTURE_DECISIONS.md` § Standards And Protocols | Architecture boundaries |
| `docs/architecture/catalogs/interfaces-and-protocols.csv` | Interface model; run `render.py` and `validate.py` after editing, as `docs/architecture/README.md` describes |

Each phase below names the register updates it owns.

## Phase 0: Retire Finished Plans

A plan is finished when its status says delivered, complete, or implemented. Before
deleting one, move each still-open item into the owning component: a `DESIGN.md`
status or limits section, or a `pending` entry with a reason in the server's
`AGENTS.md` Contract Compliance section. Then replace every link to the plan with a
link to the owning design, and remove its rows from `docs/CODEMAP.md` and
`docs/README.md`. Dated measurements disappear with the plan and stay in git history.

| Plan | Verdict | Open items go to |
|---|---|---|
| `RECORDING_CATALOG_HARD_CUT_PLAN.md` | Retired | `docs/RECORDINGS.md`, `servers/recording-mcp/DESIGN.md` |
| `RMCP_3_MIGRATION.md` | Retired | `mcp/contract/DESIGN.md` |
| `REACTIVE_UX_PLAN.md` | Retired | `apps/console/web` and `apps/workspace/DESIGN.md` |
| `SPEECH_PLAN.md` | Retired | `servers/speech-mcp/DESIGN.md` |
| `WORKSPACE_PLAN.md` | Retired | `apps/workspace/DESIGN.md` |
| `ARTIFACT_UPLOAD_PLAN.md` | Retired | `platform/artifacts/service/DESIGN.md`; replace the `docs/README.md` guide link with that design and `apps/console/web/src/uploads/DESIGN.md` |
| `AGENT_MANAGEMENT_PLAN.md` | Retired | `agents/manager/DESIGN.md` |
| `FORK_DEVELOPMENT_PLAN.md` | Retired | `docs/FORK_DEVELOPMENT.md`; drop the pointer at the top of `CONTRACT_EVOLUTION.md` |
| `COMPUTERS_PLAN.md` | Retired | `platform/computers/DESIGN.md`; rewrite the `CONTRACT_EVOLUTION.md` sentences that cite it |
| `PLATFORM_IMPROVEMENTS_PLAN.md` | Kept open cycle only | owning designs named in each cycle |
| `REPOSITORY_HARDENING_PLAN.md` | Kept open work; delivered sections retired | `tools/xtask`, `testing/` designs |
| `CAPABILITY_ADOPTION_PLAN.md` | Keep; its proposals are unapproved | none |

Acceptance: `cargo xtask enforce docs` passes, and `git grep` finds no link to a deleted
plan.

## Phase 1: Identifier Hard Cut

### Target forms

| Kind | Form | Example |
|---|---|---|
| MCP `_meta` keys and extension identifiers | `ai.veoveo/<kebab-name>` | `ai.veoveo/agent-message-targets` |
| OCI image labels | `ai.veoveo.<group>.<name>` | `ai.veoveo.build.mode` |
| Schema, format, evidence, and version tags | `veoveo.ai/<name>/v<N>` | `veoveo.ai/live-view/v4` |
| Kubernetes labels and annotations | `veoveo.ai/<name>` | `veoveo.ai/cancellation-phase` |
| JSON Schema `$id` | `https://veoveo.ai/contracts/<name>/v<N>` | `https://veoveo.ai/contracts/computer-storage/v1` |

Version numbers stay the same. Only the namespace changes.

### Inventory

`git grep -nE 'io\.veoveo|veoveo\.io|ai\.bioma\.veoveo'` finds 145 distinct
identifiers in 177 tracked files when this plan was written. Kubernetes chart labels,
selectors, and finalizers already use `veoveo.ai/`.

| Group | Count | Examples and defining locations |
|---|---|---|
| MCP `_meta` keys | 5 | `io.veoveo/agent-message-targets` (`mcp/apps-extension/src/models.rs:14`); `io.veoveo/app-resource-dependencies` and `io.veoveo/app-tool-dependencies` (`mcp/contract/src/gateway/server_config.rs:45-46`); `veoveo.io/gateway-discovery-degradation` (`mcp/contract/src/catalog.rs:7`) becomes `ai.veoveo/gateway-discovery-degradation`; `ai.bioma.veoveo/taskRetentionPin` (`platform/task-runtime/src/service.rs:19`, `servers/timeseries-mcp/src/bin/server.rs:95`, `sdk/python/src/veoveo_mcp/task_extension/models.py:23`) becomes `ai.veoveo/task-retention-pin` |
| Public payload schemas | about 12 | `live-view/v4` (`servers/uav-sim-mcp/src/contract/live_view.rs:12`), `hosted-mcp/v3` (`mcp/contract/src/lib.rs:9`), conformance profile and report (`mcp/conformance/src/profile.rs:10`, `report.rs:7`), recording catalog, projection, and playback tags, `map-route-handoff/v1` (Map-owned; UAV imports the contract), and the optimization problem tags (`servers/optimization-mcp/src/contract/mod.rs`) |
| Internal protocols | 4 | `cuopt-executor/v1` (Rust and Python), `uav-runtime-event/v2` (Rust and Python), `computer-storage/v1` with its `$id` (`platform/runtimes/computers/protocol/storage.json:3`), `computer-host/v1` |
| Persisted formats | about 9 | `travel-model-artifact/v1`, `recording-manifest/v9`, `retained-storage-host/v1`, `retained-home/v1`, `recording-journal-quarantine/v1`, `replacement-policy`, `computer-persistent-home/v1`, `computers-unconfigured/v1`, and the `simulation-view-desired-digest` values in migrations `0031` and `0032` |
| Deployment and installation formats | about 14 | `deployment/v8`, `deployment-lock/v8`, `local-registry/v1`, `image-release-evidence/v3`, `component-*`, `installed-deployment-unit/v1`, `computers-service/v3`, `computer-host/v1` config, and simulation lock and evidence tags |
| Evidence and report tags | about 55 | `testing/browser-smoke/src/main.rs:31`, `testing/deployment-smoke/src/gitops.rs:15`, `tools/xtask/src/commands/image.rs:43`, and siblings |
| OCI image labels | 19 | `io.veoveo.build.*` (`tools/xtask/src/commands/image.rs:44-48`, `image/normalized.rs:23-24`, `docker-bake.hcl`), `io.veoveo.simulation.*`, `io.veoveo.certification.source`, `io.veoveo.workload.role`, `io.veoveo.artifact.kind`, and the `veoveo.io/*` version labels in `platform/runtimes/simulation/Dockerfile:153-158`, which become `ai.veoveo.simulation.*` |
| Kubernetes annotation | 1 | `veoveo.io/cancellation-phase` (`testing/deployment-smoke/src/flux_cancellation/witness.yaml`) |
| Documentation only | 21 | historical names in docs; rename them or delete them with their plan |

### Consequences accepted by the cut

These changes alter stored or derived identities. The reference installation resets
its data, so none of them needs a migration:

- `computer-persistent-home/v1` feeds Computer template fingerprints stored in the
  store and in `examples/bioma/computers/computers.json`. Regenerate the fingerprints
  in that file.
- `computers-unconfigured/v1` derives a stored provider instance ID.
- Deployment-contract tags change every deployment digest. The next installation is
  a full rollout, and local receipts under `.git/veoveo-deployment/` become stale.
  Delete them.
- `map/source-feature-query/v2` invalidates outstanding pagination cursors.
- OCI label changes rebuild every image. Regenerate
  `platform/runtimes/simulation/simulation-runtime.lock.json`, the overlay
  `identity.json` files, and `deploy/local/k3d/registry.json`.
- Migrations `0031` and `0032` change in place. Their checksums change, so every
  existing store, including local development stores, must be recreated.

### Work

1. Rename every identifier in code, fixtures, tests, Helm templates, Dockerfiles,
   `docker-bake.hcl`, Python packages, and current-state docs in one commit series that
   builds at each step. Rename negative tests too, so they still reject an older
   version: `deploy/contract/tests/fork_installation.rs:270`,
   `deploy/contract/src/decoding.rs:34`,
   `apps/console/bff/src/recording_playback.rs:622`, and
   `servers/computers-mcp/tests/service.rs:383`.
2. Update committed fixtures: `deploy/contract/tests/fixtures/deployment-lock.json`,
   `showcase/sumo/deploy/deployment.json`,
   `testing/fixtures/{fork-installation,platform-selection}/deployment.json`,
   `examples/bioma/computers/{computers,host}.json`, and
   `mcp/conformance/profiles/hosted-server.example.json`.
3. Delete the unused `PREPARED_PROBLEM_VERSION` in
   `servers/optimization-mcp/src/problem_store.rs:18`.
4. Rename the architecture catalog entries in `docs/architecture/catalogs/*.csv`, then
   run `docs/architecture/tools/render.py` and `validate.py`. Render the PDF in a headed
   browser with hardware-backed graphics, following the GPU rules in `AGENTS.md`.
5. Add `cargo xtask enforce identifiers`. It scans tracked text files and fails on
   `io.veoveo`, `veoveo.io`, and `ai.bioma`. Its allowlist names only the documents
   that describe the cut: the Naming section of `AGENTS.md`, CE-10 in
   `docs/CONTRACT_EVOLUTION.md`, and this plan until its deletion. Historical records
   that stay in the repository are renamed like everything else. Add the target to the
   xtask surface listed in `AGENTS.md`.
6. Update the identifier rows in `mcp/contract/DESIGN.md`,
   `mcp/apps-extension/DESIGN.md`, and `deploy/contract/DESIGN.md`.
7. Rename the identifiers in the standards registers: the Optimization row in
   `README.md`, the cuOpt row in `docs/TECH_DESIGN.md`, the travel-model and executor
   references in `docs/ARCHITECTURE_DECISIONS.md`, and interfaces `VV-IF-041` and
   `VV-IF-042` in `interfaces-and-protocols.csv`.

### Reference installation reset

Write the reset procedure into `examples/bioma/README.md` using its existing runbook
commands. It stops the platform, deletes the SurrealDB volume, the Artifact object
storage, recording hub journals, and Computers retained homes and host journals, then
reinstalls from the new lock through GitOps. Run it without waiting for approval.

Acceptance:

- `cargo xtask enforce identifiers` passes, and the grep above finds matches only in its
  allowlist.
- `cargo test --workspace`, the Python SDK tests, and `cargo xtask enforce rust|python|docs` pass.
- The reference installation passes `installation-verify` from Phase 2 and live
  conformance certification after the reset.

## Phase 2: Installation Targets For Installed Smoke

Installed smoke scenarios default to the reference installation's cluster, ports, and
URL, and they hardcode its signing key ID, identity provider, tenant, deployment list,
and GPU count. A user with their own domain cannot run them. This phase makes the
installation an explicit input. The reference installation keeps running the same
checks from its own target file.

Work:

1. Implement `veoveo.ai/installation-target/v1` in `deploy/contract`, as specified in
   [its design](../deploy/contract/DESIGN.md#installation-target).
2. Add `examples/bioma/installation-target.json` with today's values: context
   `k3d-veoveo-bioma`, local base URL `http://127.0.0.1:8781`, public base URL
   `https://veoveo.bioma.ai`, the deployment list in
   `testing/smoke/src/bin/smoke/scenarios/bioma.rs:23`, its GPU minimum, its operator
   scopes, and its control-plane path.
3. Rename the scenario `bioma-verify` to `installation-verify` and the module
   `scenarios/bioma.rs` to `scenarios/installation.rs`. Each installed scenario takes
   a required `--installation <file>` and drops its default context and URLs
   (`testing/smoke/src/bin/smoke.rs:87-108`, `:538-540`, `:605-611`). Update
   `tools/xtask/src/commands/smoke.rs:266`.
4. Read the access-token key ID, the identity provider's authorization endpoint, and
   the tenant from the control plane named by the target. That replaces the
   `veoveo-bioma-2026-07` key ID, the Microsoft Entra host check, and the `"bioma"`
   tenant (`scenarios/artifact_consumers/python.rs:82`). Read deployments, GPU minimum,
   and operator scopes from the target.
5. In `scenarios/recording_catalog_sdk.rs`, remove the context check at line 17 and take
   the host mapping and Rerun endpoint from the target.
6. Make the generic chart assertions in `testing/deployment-smoke/src/helm_config.rs`
   run against `testing/fixtures/fork-installation` and
   `testing/fixtures/platform-selection`. Keep the reference-installation assertions as
   a separate module that renders `examples/bioma`.
7. Update the smoke command in `examples/bioma/README.md` (line 593) and any guide that
   names `bioma-verify`.

Acceptance:

- An installation target for the fork-installation fixture parses and validates.
- `installation-verify` passes against the reference installation with its target
  file.
- The recording catalog SDK smoke runs against an installation whose hostname and
  tenant differ from the reference, without editing Veoveo source. A disposable local
  profile from [`LOCAL_DEPLOYMENT_PROFILES.md`](LOCAL_DEPLOYMENT_PROFILES.md) serves as
  that installation.

## Phase 3: Resource Contract Corrections

The survey found existing violations of rules C04, C27, and C28. Fix each in its owning
server with a test that fails before the fix.

| Server | Violation | Fix |
|---|---|---|
| frames | `DESIGN.md` promises resource notifications, but `listen` passes no hub (`servers/frames-mcp/src/bin/server.rs:400`) | Emit world and list changes from a Store LIVE source through `SubscriptionHub` |
| duckdb | `listen` accepts resource filters that never fire (`server.rs:361`) | Reject resource filters and declare no resource subscriptions until a source exists |
| timeseries | same as duckdb (`server.rs:290-292`) | Same as duckdb |
| media | usage URIs are notified (`usage.rs:155`) but rejected by `listen` (`server.rs:809-830`) | Make usage URIs subscribable, or stop notifying them |
| map | spatial and raster derivations are notified but not subscribable (`mcp.rs:2487`) | Add them to the subscribable set |
| recording | the catalog URI is notified (`server.rs:131`) but not subscribable | Make it subscribable |
| map, media, optimization | per-process broadcast hubs do not survive restarts or reach other replicas | Feed their hubs from Store LIVE queries with change-feed recovery, as Time and Recording do; do not add outbox consumers |
| reason | the analyses index scans every analysis (`server.rs:624`) | Bounded cursor pages at the store |
| stream | run and session lists are unbounded | Bounded cursor pages |
| time | calendar, epoch, and event collections are unbounded (`catalog.rs:382,487`) | Bounded cursor pages |
| recording | the catalog caps at 500 with no cursor (`service.rs:284`) | Bounded cursor pages |
| uav-sim | `list_resources` enumerates every grant, plan, and task | List roots and templates only, with paged collections |
| speech | docs, contract, and artifact resources omit MIME types (`mcp.rs:143-148`, `:171-173`) | Declare MIME types |
| mcp/contract | `CHECKLIST_IDS` stops at C30 (`src/docs.rs:34`) while the checklist has C31 | Add C31, and declare it in every server's `AGENTS.md` |
| gateway | every resource read calls the upstream `list_all_resources` (`platform/gateway/src/mcp/resources.rs:372-400`) | Resolve the owning server from the cached discovery surface |

Acceptance: each server's tests cover its fix, and each server's `AGENTS.md` updates
the affected compliance entries.

### Modular Types And Server Contracts

[CE-13](CONTRACT_EVOLUTION.md#ce-13-modular-types-and-server-owned-contracts) accepts
this architecture across Veoveo. Build the foundation before continuing scope and
URI adoption. This is required Phase 3 work; the first converted servers establish
the pattern, and do not establish repository-wide completion.

| Owner | Target responsibility |
|---|---|
| `platform/types`, crate `veoveo-types` | Protocol-independent platform identity and attribution, validated names and resource URIs, focused builders, and public extension traits such as `ScopeDefinition` and `ResourceAddress` |
| Each server library's public `contract` module | Its scope enum, domain IDs, resource address variants, and request/response types |
| `mcp/contract` | MCP-specific traits that consume the foundational types, descriptors, discovery integration, protocol conversions, and hosted-server setup requirements |
| Server runtime and policy owners | Domain operations, current authorization, persistence, and protocol handlers |

The foundational crate has no dependency on RMCP, server implementations, database
clients, GPU libraries, or asynchronous runtimes. Keep domain vocabulary in its owning
library. The gateway accepts validated scope names and registration data without
exhaustive matches on server-specific scopes or resources.

Rust cross-server consumers depend on the owning library with
`default-features = false, features = ["contract"]`. That feature exposes the public
contract with foundational types and the required value, validation, and
serialization/schema support.
MCP integration and runtime modules have separate feature gates, and their Cargo
dependencies are optional. Binary targets require their runtime features. Adding a
feature name without removing runtime dependencies does not meet this requirement.
A separate contract crate needs a concrete dependency or independent release reason.

#### Review Each Contract Change

Review the affected contract before extending its implementation. Trace a representative
caller through construction, authorization, and persistence to identify where an API
loses domain types or requires knowledge of another component's internals. Apply the
following criteria throughout this plan, including audit, knowledge, and store work:

| Concern | Required result |
|---|---|
| Ownership | Name the library that owns the vocabulary and inspect the dependency direction. A cross-server consumer imports that owner's public contract; adding the domain must not add a dependency or registry entry to MCP core. |
| Typed interfaces | Keep specific scope, ID, resource, and query types through public and internal calls. Validate external values at admission and convert them to wire or driver representations at those adapters. |
| Construction | Prefer a typed constructor for a simple value. Add a focused builder when options or relationships make construction difficult. Required inputs retain their types; validation checks relationships before exposing a usable value. Use typestate only when it eliminates a concrete invalid sequence. |
| Contract adequacy | When the existing model cannot express the required invariant, change the owning contract and migrate its callers. Record the reason and material tradeoff in the owning design and this plan; update CE-13 when the ownership architecture changes. |
| Qualification | Demonstrate valid construction and rejection of invalid combinations, plus the affected domain behavior. Prove contract feature isolation with an independent consumer; qualify the current wire and stored formats with their consumers before claiming completion. |

Make these decisions within the accepted product architecture without waiting for the
user to identify each gap. Explain material decisions as work progresses. A new wrapper,
builder, or trait is complete only when the affected callers use it and its promised
invariants are checked. Record remaining adoption explicitly in the inventory below.

#### Implementation

1. Inventory current type ownership and Cargo dependency paths across servers,
   shared contracts, gateway, policy, SDKs, tests, and CLI tools. Record the scope and
   resource interfaces to migrate and their owning libraries. Identify contract
   modules that currently import runtime types, and resolve dependency cycles before
   extracting the foundation. Add its owning `DESIGN.md` and update the code map.
2. Extract `ScopeName`, `ResourceUri`, and the supporting types required by their
   actual consumers into `veoveo-types`. Keep the crate small; a shared use site alone
   does not transfer domain ownership. Move internal imports in a hard cut without
   aliases or duplicate definitions. Preserve wire spellings and current policy
   semantics. Change affected protocol and persisted formats by coordinated hard cut;
   qualify the current format with current consumers on a fresh installation.
3. Define small public, unsealed traits for scope-name conversion and typed resource
   address parsing/serialization. Each server implements them with its own enums
   and ID types. Define MCP-specific associations in `mcp/contract` using those
   traits and existing RMCP handlers. Shared constructors consume these interfaces;
   adding a server cannot require a core domain registry or a new match arm.
4. Use one server-owned declaration of each scope's wire spelling for authorization,
   serialization, configuration defaults, and advertised vocabulary where applicable.
   Domain helpers take the owning enum. Generic policy consumes validated names and
   continues to admit installation-defined scopes. A caller's full grant set may
   contain scopes outside a particular server's vocabulary.
5. Provide ergonomic builders that preserve the types of required IDs and query
   fields and validate field combinations before construction. Use typestate when it
   prevents a concrete invalid construction sequence. Resource variants own route
   shapes and supported parameters. The shared builder delegates component parsing
   and encoding to a maintained URI library; begin with the existing pinned `url`
   implementation and qualify its supported custom-scheme profile. Keep strings at
   serialization and driver bindings. Typed query APIs still apply current tenant,
   owner, context, labels, parent relationships, and filters in SQL before limits.
6. Add and qualify contract-only library features, then migrate all owned scope and
   resource paths in small commits. Map and Time are the first consumers. The known
   inventory also includes View and UAV scope helpers, configurable administrative
   middleware, gateway and policy literals, and Map URI builders and Store record-key
   APIs that erase domain types. Complete the inventory through other servers,
   clients, SDKs, templates, and CLI tools. Other languages use their native types and
   builders with the same ownership and wire contracts.
7. Extend shared conformance and template guidance. Reuse ordinary server libraries
   for domain tests and CLI operations. Hosted checks consume a profile and the
   discovered surface without importing server implementations. Traits establish
   API requirements; no marker trait claims behavioral compliance.

#### Acceptance

- An independently defined fixture server adds a new scope and resource family,
  implements the public traits, and passes applicable hosted conformance with only
  registration/configuration changes. Its implementation is not added to either
  foundational or MCP core source. A separate consumer imports only its public
  contract and constructs typed resource addresses.
- Isolated contract-only builds and Cargo dependency inspection prove that runtime,
  MCP integration, database, GPU, and provider dependencies are excluded. Run these
  checks outside a workspace feature-unified build that could conceal missing gates.
  Run supported runtime configurations separately to prove the gates compose.
- Compile-fail cases reject raw strings at domain authorization APIs, wrong-domain
  scope and ID types, and invalid required builder states. Runtime cases reject
  unknown domain scope spellings while allowing unrelated validated names in the
  caller's grant set. Grant decisions match the existing policy.
- URI tests cover build/parse round trips, existing published wire forms, custom
  schemes, reserved characters, percent encoding, repeated or unsupported parameters,
  fragments, normalization, and malformed IDs. Discovery templates agree with typed
  builders. Database tests reject incorrect parent combinations and apply visibility
  before limits; a URI's type alone grants no access.
- Applicable hosted conformance and owner-local lifecycle tests pass. Qualification
  keeps authentication, authorization, recovery, SQL selection, and required hardware
  checks distinct from compile-time structure. Update owning designs and compliance
  declarations with their actual implementation and qualification status.

#### Migration Inventory And Status

All 15 Rust MCP server packages under `servers/` have library targets and define the
`contract` feature. Independent consumer qualification is recorded in each owning row.
Artifact, Computers and Speech also pass one separately resolved consumer and grouped
native default-feature tests; their runtime-only libraries compile. Existing libraries remain the
default owner; the inventory must not become a central domain-type registry.

| Surface | Current dependency or representation gap | Next owning change |
|---|---|---|
| Foundational primitives | `ScopeName`, `ResourceScheme`, `ResourceUri`, and `IdentifierError` are extracted into `platform/types`; direct callers use that crate; wire/schema preservation and independent dependency isolation pass | Resolve concrete/template resource references before tightening URI validation |
| Independent extension traits | `ScopeDefinition`, `ResourceAddress` and `TaskResourceAddress` are public and contain no domain variants; `McpServerContract` associates server-owned types with descriptors and documents. Artifact, Computers, Speech, Frames, Timeseries, Media, Time, UAV, Reason, DuckDB, Optimization, Recording, View and Stream consume checked setup. The independently owned fixture passes hosted conformance, typed access/denial and contract-only consumption | Adopt checked setup in Map and templates; preserve domain-owned authorization |
| Scope declarations | `scope_enum!` generates conversions and schemas from server-owned spellings, with compile-time rejection of invalid or duplicate declarations | Adopt it across server libraries while keeping each domain's vocabulary local |
| Resolved invocation authority | Capability and Work Context membership levels, invocation authority and output defaults belong to `veoveo-types`; callers import them directly. Five schemas, serialized authority bytes, nested identity admission and level ordering pass native and independent-consumer checks. MCP retains configuration and membership matching | Preserve complete authority when extracting domain contracts; qualify installed policy and composition consumers |
| Concrete URI components | `ResourceUriParts` validates the concrete profile with URL 2.5.8; `ResourceUriBuilder`, `UriAuthority`, and percent-encoding 2.3.2 encode typed scheme/authority and path/query components, preserve segment identity, and reject duplicate query names | Adopt through domain constructors with specific ID types; qualify each family's spelling and parameters |
| HTTPS network addresses | `HttpsUrl` owns canonical ASCII HTTPS syntax, immutable parsed access and redacted diagnostics. Source URLs retain signed query bytes and permit nondefault ports and repeated query names. DuckDB, Map and Timeseries preserve the type into the shared download runtime; host/DNS/redirect policy still controls access | Adopt this profile where other domain contracts require HTTPS; qualify installed source consumers |
| Resource templates | `ResourceTemplateUri` uses iri-string 0.7.14 with guards for RFC prefix bounds and dotted variable names; `McpResourceTemplate` prevents descriptor mutation. Time, UAV, Reason, DuckDB and the independent fixture consume checked template declarations. Native and isolated-consumer cases qualify syntax, expansion, existing addresses and error redaction | Extend checked declarations and domain-builder agreement across remaining servers |
| Gateway completion and audit targets | Completion and template discovery use `PolicyTarget::ResourceTemplate`. Stored policy events require the current v2 marker and typed event; the historical DTO adapter is removed. Native cross-connection reads and invalid-write checks pass | Qualify installed authorization and current-format audit reads; tighten the opaque resource validator after remaining URI families are inventoried |
| Platform identity and attribution | Principal, tenant, group, role, Work Context, delegation, data-label, and policy-version types, access subjects, and invocation provenance are extracted into `platform/types`; consumers import them directly; eleven baseline schemas, wire/profile tests, independent consumer tests, and strict workspace Clippy pass | Preserve these contracts during domain extraction; qualify installed identity and policy behavior with the affected services |
| Map | `MapScope` owns handler, Task, and default administrative scope spellings; authoring metadata requests and cursors use typed IDs and shared URI components; identity, Artifact metadata, and geodetic IDs now come from their owning contract libraries; the contract feature excludes runtime dependencies | Migrate remaining addresses and Store query IDs; qualify installed behavior |
| Coordinate vocabulary | Map owns geodetic IDs; Frames owns worlds, conversions, and typed world/revision/frame addresses; RRD owns recorded frame/geofence metadata. Shared MCP coordinates are removed. Independent contract consumption and schema compatibility pass | Qualify installed consumers with the current absolute frame-ID profile |
| Map identity admission | Source, restriction, mobility, travel-model and six product-address families use canonical RFC UUIDv5/v7 IDs. Other domain IDs accept broader UUID-library spellings; Store authoring keys check only a prefix, byte bound, and slash exclusion | Apply the owner admission profile to remaining IDs and Store query APIs; qualify current-format installed consumption |
| Time | The contract feature excludes runtime dependencies; handlers, Tasks, and configuration defaults use `TimeScope`; `TimeResource` owns every URI family and the three collection cursor types. Checked server setup supplies startup, discovery and scope membership. Private runtime persistence owns SQL, mutation drafts and driver records; catalog calls retain domain IDs, versions, completion parents and cursors until driver conversion. Checked catalog decoding binds JSON identity, versions and indexed fields to the stored row; native corruption and immutable-acquisition checks pass | Complete broader DTO types and qualify current-format installed behavior; completion now requires the advertised reserved-expansion zone template |
| Time identity admission | Time owns both profiles: public IDs accept bounded prefixed names, including bootstrap authority references, while stored catalog keys require UUIDv7 suffixes. Persistence validates the stored profile without narrowing public provenance; Store has no Time query or draft API and owns the shared connection and migrations | Qualify installed admission; use the distinct current public and stored ID profiles when strengthening metadata construction |
| Time scalar admission | Clock policies have a checked builder. Metadata versions use `TimeVersion`, with a distinct zero-only source-creation input and private retained-body decoding. `SubsecondNanoseconds` covers instants, expressions, cursors and persistence drafts. Total-coordinate conversion and NTP/UTC epoch arithmetic check seconds overflow; native boundary, transition and retained-row cases pass. JSON keeps its numeric shape. Requests and persistence keep typed guards; exhaustion checks preserve rows and atomically roll back failed retirement | Qualify installed numeric admission; finish remaining expression/projection scalar types and acquisition-state relationships |
| Time intervals | `TimeWindow` checks increasing bounds and a common authority at construction and decoding; accessors preserve these invariants. Algebra keeps endpoint uncertainty, selecting the maximum at tied coordinates. Schedule expansion clips to the horizon while preserving recurrence limits and labels. Native membership, metadata, authority and clipping cases pass; independent contract consumption qualifies the unchanged valid wire shape | Qualify current-format installed schedule Tasks and restart recovery |
| Time active-pointer admission | One SQL statement resolves each visible pointer to an active release with matching tenant, family and key. Pointer identity, history and versions are checked; activation rechecks parent and lifecycle relationships in the transaction. Native corruption, SQL payload exclusion and ten interleaved-mutation rollback cases pass | Qualify installed parent admission and transactional conflict rollback |
| Time authority contexts | Each engine request validates the joined active selection and provenance before reusing loaded files. Cache keys use the Store tenant type, engine epoch maps are independent, Store signals evict contexts and failures remove cached values. Native restart, isolation, disconnected-observation, provenance and file-recovery cases pass | Qualify installed restart/replica behavior and the declared coordinated upgrade |
| Time authority metadata | Checked references derive release identity from typed URIs. Effective pairs require the correct dataset roles and distinct IDs, and derive instant bindings. Wire adapters preserve valid fields; the runtime context keeps metadata and bindings private. Native retained-row and independent-consumer admission checks pass | Qualify current binding validation and installed startup |
| Time resolution metadata | `ResolveTimeOutput` admits matching instant/release pairs and protects them with read-only accessors; its wire adapter preserves flat projection fields. Engine epoch keys remain typed, relative calculations reject foreign authority and preserve uncertainty, and additional uncertainty checks overflow. Native and independent-consumer cases qualify these relationships | Qualify installed resolve/convert decoding and epoch behavior after authority activation; computed representations remain the engine's responsibility |
| Time activation preflight | A private draft carries the candidate and both admitted active families through file loading to SQL commit. A shared tenant write fences different-family decisions on 3.2.4; the transaction compares the observed metadata and rolls back every mutation on conflict. Native cases cover RocksDB contention, retained schema upgrades, stale inputs and failed file loads | Qualify installed activation; qualify 3.3 locked reads before retiring the concurrency fence |
| Digest wire profiles | Time's `AuthoritySourceDigest` preserves bare hexadecimal spelling through metadata, requests and typed persistence drafts; canonical content comparison and shared provenance use the foundational `sha256:` value. Native cases preserve uppercase retained data and idempotency while rejecting malformed matching rows. View still uses bare hexadecimal text | Qualify current digest admission in installed Time; migrate remaining owners and callers by hard cut |
| Computers | The isolated contract owns distinct Computer, execution, file-transfer and automation-grant IDs, the complete resource vocabulary and an empty scope enum. Domain APIs, gateway routes, relays and generated browser schemas adopt those types. Checked hosted setup supplies startup and discovery. Owned and granted Computer reads admit identity, clearance and current policy before decoding; final SQL rechecks authority before public ordering and limits. Lifecycle and command/file Tasks resolve scoped policy inputs before reading private state. Browser/CLI grant reads bind credentials, sessions, parents and providers in SQL | Complete maintenance SQL admission and audit accepted-request receipts and internal worker lookups; remaining lifecycle/access/provider identities and DTO relationships; installed qualification |
| Speech | The isolated contract owns distinct transcription/dictation IDs, every public resource family and an empty scope vocabulary. Checked hosted setup supplies initialization and discovery. Application execution, resource subscriptions, Gateway targets and Console routes retain the owner types; receipt decoding and completed output reads check parent identity. Independent consumers, compile-fail cases and native callers pass; generated browser schemas use the qualified shared converter | Qualify current-profile CUDA transcription/dictation and installed delivery; strengthen remaining transcript result relationships |
| Artifact plane model | `platform/artifacts/contract` owns occurrence identity, metadata, compliance, provenance, release state, grants, share-link values and byte handoffs. The separate plane service/client prevents placing their common model in the MCP server package without a Cargo cycle. Its server library exposes tool DTOs and typed `ArtifactResource` families through an isolated contract feature. Shared URI components build addresses from occurrence IDs or closed document variants. Checked hosted setup includes the Library App, index and embedded documents; both registrations declare revision 3. Independent consumption, direct consumer tests, and wire/schema qualification pass | Qualify installed reads and sharing at the next integration checkpoint; separate remaining access/service request contracts and finish index cursor addresses |
| Artifact identity and URI admission | `ArtifactId` checks version and RFC variant; `ArtifactUri` owns neutral/presented variants, preserves accepted URI spelling, and builds from typed IDs and schemes; metadata checks wire ID/URI agreement | Qualify installed consumption of the current identity and URI contract |
| Remaining Artifact references | Download URLs, some Store DTOs, and other domain URI fields still use broader string profiles | Migrate with each owning contract; distinguish Artifact identities from external fetch locations and declare persisted/profile changes |
| Artifact attribution construction | `ArtifactProvenance` uses foundational `InvocationProvenance`; a private wire adapter preserves valid flat metadata and requires each mode's identities in both decoding and schemas | Native publication/readback, independent consumption, schema/decoder parity, and compile-fail qualification pass; qualify installed metadata consumption during reference acceptance |
| Media public contract and hosted setup | The isolated library owns public requests and responses, registry entries, typed model/prediction identities, all hosted resource variants and an empty scope vocabulary. Checked MCP setup supplies startup and discovery; resource reads and subscription admission share the owner parser. Model IDs remain typed through provider submission and output attribution. Opaque prediction builders share the declared template encoding; model routes use reserved expansion. Provider HTTP URLs use component builders | Complete remaining DTO relationships and model catalog paging; qualify registration, installed behavior and provider recovery budgets |
| Frames hosted setup | The isolated server contract composes world, operation, usage, Artifact and document routes into `FramesResource`. Startup and discovery consume checked setup; reads and mutable subscription admission use owner parsing. No domain scopes are added | Qualify current resource admission and subscriptions through installed clients |
| Frames world reads | Frames owns typed reads over the existing Store client; SQL applies visibility and parent checks, world catalogs use typed keyset pages, and completion binds parents and matches before limits. Six isolated native cases pass. Discovery is static; private driver records and mutations also belong to Frames | Qualify installed paging and completion with current catalog consumers |
| Frames mutation inputs | Frames owns typed mutations, private driver records and world-event vocabulary. World publication checks owner, current labels and head agreement in the transaction; repeated writes settle from authorized matching state. Store has no world draft API | Qualify installed publication and concurrent replay under current writer policy |
| Frames world metadata construction | The shared domain crate below Frames and Recording runtimes owns the public model; the MCP contract feature re-exports it. Checked summaries, immutable revisions, and source references derive their repeated identities from typed URIs. The contract owns complete-tree validation and hashing; Store reads and UAV use it. Native corruption/visibility and independent schema/consumer checks pass | Qualify current world metadata with installed consumers |
| Frames operation references | Operation addresses use typed component builders; checked provenance derives its ID from the URI and rejects conflicting wire identity. Existing schema snapshots and independent contract consumption pass | Qualify installed consumption and current provenance checks |
| Frames stream references | `FrameStreamUri` applies the shared concrete URI profile; `FrameEntityPath` checks bounded producer selectors. Independent consumption, current schemas and native node qualification pass. Frames preserves source spelling and leaves route vocabulary with producers | Qualify installed behavior; complete each producer's typed builders |
| Frames usage visibility and pages | TaskRuntime applies current Task owner policy and linked-record agreement in SQL before grouping and limiting usage. Frames owns checked pages and typed Task cursors/URIs in its isolated contract feature; native denied-row and cursor cases pass | Qualify installed reads and subscriptions with current catalog consumers |
| Frames operation visibility | Frames owns SQL-scoped operation reads and transactional authority/immutable replay checks. Rust and Store require the authority object and profile; native caller, parent, schema and event-rollback cases qualify the current format | Qualify installed direct/Task operation reads and current-format recovery |
| Timeseries resource admission | The isolated library owns `TimeseriesResource`, closed document identities and typed Artifact handoff construction. Resource reads parse once; startup and discovery consume checked setup with unchanged task-only subscriptions. Parsers reject malformed route components and unsupported queries | Qualify current Artifact handoffs and installed resource consumption |
| Timeseries usage | The library delegates pages and exact reads to TaskRuntime's SQL owner policy; typed usage URIs, cursors and checked pages belong to its isolated contract feature. Valid version 1 cursor bytes and response fields are preserved. Native denied-row, continuation, label-change and parent-metadata cases pass; the independent 60-package consumer and strict runtime/workspace Clippy pass | Qualify the coordinated replica replacement and installed reads |
| DuckDB usage and discovery | The library uses TaskRuntime SQL visibility for 100-entry usage pages and exact reads. Its isolated contract owns usage addresses, collection-bound cursors and checked pages; discovery declares roots/templates without scanning records. The unused unbounded Store usage catalog API is removed. Native reads and Spatial, the independent 59-package contract consumer, and strict runtime/workspace Clippy pass. Workbench cursor construction uses the browser URL API; headless navigation and reserved-character cases pass | Qualify installed page consumers and headed hardware Workbench acceptance |
| Optimization usage and contract | The library selects explicit owner-plus-Work-Context policy in TaskRuntime SQL before grouping and limits. Context records and stored authority must agree. Contract-only types preserve version 1 cursor bytes and report fields. Unscoped Store usage reads and Rust post-filter helpers are removed. Native owner/context selection and current-authority checks pass. The independent contract consumer preserves eleven solver schemas and excludes service dependencies | Qualify the coordinated control/executor replacement and installed reads |
| Optimization catalogs | Domain-owned readers match indexed and retained owner/context fields in SQL before pagination, exact lookup and completion. Optional tenants remain distinct; selected malformed records fail explicitly. Typed collection builders preserve version 1 cursor bytes. Native database checks, independent contract consumption and strict Clippy pass | Qualify current catalog permissions and installed consumers |
| Optimization resource admission | Typed domain builders, canonical output IDs and exhaustive resource dispatch replace string construction and prefix parsing for every hosted family. Checked MCP setup preserves discovery metadata and public schemas. The server declares no domain scopes and preserves gateway operation policy. Native template, setup and isolated-consumer checks pass; Map travel-model references use Map's contract feature | Qualify installed readiness and resource admission; finish DTO relationship checks |
| Map travel-model reads and shared references | Map owns typed model addresses and version 1 native Task cursors. Optimization imports the owner library's contract feature. SQL selects caller-owned successful results before limits and grouping, checking stored owner/request/result agreement. Native paging, clearance, malformed-row and isolated-consumer checks pass | Qualify installed page traversal with current consumers; complete DTO relationship admission |
| Map restriction reads | Map owns typed addresses, collection-bound cursors and checked compact pages. SQL applies tenant visibility before limits and operational time/family/withdrawal selection. Exact reads and pages reject indexed/document disagreement. Native page, validity, corruption, overflow and independent consumer checks pass; the unbounded Store list is removed | Qualify installed summary pages with current consumers; finish broader restriction DTO admission |
| Map product addresses | The contract owns typed release, source feature, raster, raster derivation, spatial derivation and route URIs. Builders and templates agree; exact reads and route subscriptions use owner parsing. View imports those types and checks source-feature release membership. Contract, native and independent-consumer checks pass | Type the remaining product DTO references and parent relationships; qualify current installed consumption |
| Map routing authority | One domain-owned SQL statement selects compatible enabled sources through tenant/dataset-matching active pointers and valid active releases. Typed release/family sets replace full-catalog scans and per-release lookups. Native tenant, lifecycle, parent, boundary-time and retained-document cases pass | Qualify installed routing; finish other internal release selections |
| Map source catalog | Map owns typed addresses, collection-bound cursors and checked public summaries. Exact reads, pages and completion select the tenant in SQL before limits; selected documents must agree with indexed metadata. Native isolation, continuation, redaction and corruption cases, isolated contract consumption and strict Clippy pass; Map Explorer walks the page envelope and the unbounded Store list is removed | Qualify current source-ID and document admission and installed page traversal |
| Map mobility catalogs | Map owns checked profile versions, exact and collection addresses, composite cursors and complete-profile pages. SQL selects tenant, parent, ID and numeric version before limits. Native isolation, ordering, completion and corruption cases, isolated contract consumption and strict Clippy pass; Map Explorer traverses pages and the unbounded Store read is removed | Qualify installed traversal with current consumers; UAV grants/handoff and the flight harness consume Map-owned addresses |
| UAV Map admission | Map-owned profile URIs and handoffs replace copied DTOs and manual parsing. SQL applies profile/advisory grants before selection and rechecks them during execution admission. Selected plans validate indexed metadata and physical identity; focused native policy, corruption and revocation checks pass | Qualify current adapter/Task restart recovery and installed grant/mission behavior |
| UAV execution exclusion | Admission guards a retained queued Task and writes its exact link, vehicle lease and plan in one transaction. Mission reads follow that link; unresolved outcomes retain their Task pin. Native correlation, cancellation/rollback and SQL cleanup-page tests cover the current admission model alongside contention and HTTP interruption | Retain complete observations across process loss; qualify current-format installed recovery and operator reconciliation |
| Media usage and prediction reads | Media owns current-owner and linked-record SQL selection before limits, typed 100-entry catalogs, static discovery and query-backed subscriptions. Billing selects unsettled jobs in SQL with Task/tenant/provider correlation. Native database and isolated contract checks pass; the prediction summary schema is preserved | Qualify current catalog consumers and installed subscription recovery |
| Media generation result | The isolated contract owns checked current generation results, typed addresses and output attribution. SQL selects successful linked Tasks under current owner policy. The old decoder, result conversion and rollback-only smoke option are removed. CLI downloads use the current result and verify resource equality | Qualify current-format installed delivery, caller isolation and restart behavior |
| Native Task identity | `veoveo-types` owns `TaskId`; consumers import it directly and Store's `task_record_id` performs database conversion. Native lifecycle, wire preservation, compile-fail, and independent consumption checks pass. Frames uses it in usage cursors without runtime dependencies; runtime external lookups still require v7 and opaque MCP handles keep their own profile | Migrate remaining string-based runtime lookup APIs with their owning admission contract |
| Shared public Task reads and notifications | Exact owner reads, subscription baselines and current-state delivery apply SQL visibility before decoding. Typed native IDs reach driver bindings. Public delivery uses event identities to select current Tasks; trusted internal replay preserves historical transitions. Native lifecycle, revocation, malformed-row and denied-page cases pass | Qualify current-format installed delivery; audit domain-specific Task adapters for their additional policy |
| DuckDB source contract | The isolated `contract` feature owns source vocabulary, read SQL helpers, checked resources/pages, Artifact origins and operation-usage metadata. Timeseries and the agent kernel consume that owner directly; MCP core has no domain dependency. Checked MCP setup owns startup and discovery. Database catalogs page over the current owner directory; owner keys distinguish optional tenant values. Execution keeps native Task IDs, direct calls carry no Task association, and publication checks capability/origin agreement. Export variants enforce selection/format agreement; query-result builders check row shape, count and output mode. Source URLs retain foundational HTTPS types through materialization; nonempty lists and neutral Artifact addresses are admitted before execution. Request builders check attachments, positive limits and output relationships; SQL/table text and reader options stay typed through execution. Fifty-five native and 26 independent-consumer cases over 36 schemas pass, with eleven compile-fail examples, 30 shared result cases and 75 shared request decoder/schema cases | Qualify installed source consumption, catalog pages, the fresh owner-directory/metadata formats and Artifact publication/recovery; correct the Timeseries Artifact-source profile |
| UAV contract | Contract-only imports expose public mission, grant and live-view v4 types without service dependencies. MCP core has no live-view exports; gateway owner conversion stays in the authenticated adapter. The independent consumer qualifies 96 schemas. Atomic Recording catalog state and result references use the shared Recording owner; native tests preserve physical settlement on invalid recording metadata. Checked MCP setup supplies startup, configuration, documents, templates and typed scope membership; native discovery and all 22 template expansions pass. Both gateway registrations declare revision 3 | Complete broader DTO relationships and cross-language construction; qualify current installed consumers, URI admission and recovery |
| UAV scopes | `UavScope` owns the four scope spellings and shared permission guards; tools and Tasks apply the same admission requirement, and flight preflight checks the installation-selected scopes through the owning vocabulary. Native routing, grant combinations, contract-only consumption and strict workspace Clippy pass | Qualify current scope enforcement on every installed UAV replica |
| Reason | Its isolated contract owns distinct catalog and analysis IDs, typed resources, cursors and an empty scope vocabulary. Checked MCP setup, SQL owner reads and Task-backed notifications are implemented. Current v1 results serve completion, Task reads and subscriptions. Grounding imports Stream contracts and carries input labels into output capabilities. Recording addresses in selections, results and analysis views use the Recording owner’s contract type | Complete broader result/reference typing; qualify current-format installed delivery, restart recovery and GPU behavior |
| Task-backed resource notifications | Domain-owned `TaskResourceAddress` implementations feed the shared `TaskResourceSubscriptions` adapter. One authorized Task subscription supplies explicit Task status and resource invalidations. Native independent-client, reconnect, revocation and official MCP cancellation cases pass. LIVE connection generations trigger a current-owner SQL baseline even after retained events expire; a TCP outage regression fails against the old watch. The adapter reuses the Task stream; Phase 5 still owns outbox replacement | Adopt for other Task-backed domains while preserving their additional admission policy; qualify installed cross-replica delivery and coordinated replacement |
| Stream | Its isolated contract owns IDs, resources, cursors and response builders that check repeated identities and parent/output agreement. Checked MCP setup supplies static discovery; all 11 templates match builders, and both registrations declare revision 3 without list-change notifications. Run reads and subscription admission select caller-owned Tasks in SQL; isolated contracts qualify all 37 schemas. Recording addresses in selections, results and run views use the Recording owner’s contract type | Strengthen remaining result relationships; qualify canonical result reads, mixed-source notifications, Task delivery and GPU behavior on the reference installation |
| Shared recorded video | `contract` owns selectors, timeline kinds and immutable source/snapshot builders with typed Recording IDs, positive byte counts, typed digests and checked layer/part relationships. JSON decoding applies the builders. The ordered source hash preserves its declared bytes. Materialization admits the public snapshot before extraction; Reason/Stream executors and Stream result validation check its selected Recording. Independent consumption excludes MCP, Store, Rerun and async dependencies | Finish selector construction and broader result relationships; qualify installed snapshot digests and consumers |
| View | Its isolated contract owns public scene types, scopes, Task kinds and typed resource addresses. Governed references import Map, Frames, Recording and Artifact URI types, and source features must belong to a declared Map release. Checked records validate parents, cameras, geometry and output bytes. Capture admission checks request revision and principal/tenant/Work Context before claiming. Task operations apply Work Context and operation selection in SQL; completed reads and subscription delivery validate saved requests, metadata, bytes and attribution before projection | Qualify installed consumers, cross-context Task delivery and GPU behavior, including GPU JPEG encoding |
| Recording | The shared domain crate below Hub and the MCP server owns public models, RFC UUIDv7 IDs, resource builders/parsers and catalog positions. The MCP library exposes the same types through its isolated contract feature. Hub ingest responses, Gateway policy targets, hosted reads/subscriptions/prompts and Video/Reason/Stream/UAV/View references use those types. Smoke clients reuse the owner for CLI admission, replay construction and capture results. SQL cursor conversion stays at the query call; MCP core imports neither package. Checked MCP setup owns static discovery and both registrations declare revision 3; typed `recording:seal` admission preserves gateway administrator restrictions. Public DTOs keep distinct Recording/dataset/layer/grant/projection IDs; catalog selections have a checked constructor, and Console imports shared playback DTOs. Projection reads, reservations and lifecycle transitions select current caller authority, source visibility and grant relationships in SQL before receipt decoding; writes admit and change state in one transaction. Typed requests bind idempotency to context, policy and inputs; native memory/RocksDB races and rollback pass. Grant creation admits every parent in its transaction; reuse checks current caller authority including Work Context in SQL. Redap grant classes are selected before decoding. Sealed playback manifests share typed lifecycle, time, Blueprint integrity and parent checks between the producer and Console; Rerun origins use typed URL components. Sealed query/request builders own projection bounds and metadata checks; RRD prepares upstream selectors before source materialization, and consumers share the domain model. Sealed result handles check request agreement and typed integrity facts; reuse verifies receipts and payloads. Scratch quotas include bounded metadata, preserve valid pairs across restarts and reclaim expired completed pairs on reservation. Typed Redap entry and segment addresses check parent identities; catalog responses use a checked immutable builder, and the supported address profile is qualified against Rerun. Public catalog, layer and seal models import Artifact owner references and typed integrity facts through immutable builders. Fallible Store conversion and seal ordering reject malformed metadata before lifecycle advancement or retry cleanup; SQL corruption cases pass. Projection coordinate references use the Frames owner’s revision-scoped type with bounded unique selection and request/result agreement. Playback plans, Blueprint validation, reader snapshots and cache APIs carry typed SHA-256 values; native byte, authorization and source-identity checks pass | Complete remaining Recording/RRD identity and publication adapters; qualify installed behavior |
| Shared consumers | Gateway, policy, Console BFF, Computers, conformance, smoke, and integration tests import foundational names | Keep imports direct and preserve authorization, identity serialization, and schemas |
| SDKs, clients, templates, and showcase servers | Python Task results use the shared result envelope and event version 3. `OwnerTaskQuery` composes typed operation names, UUIDv7 identities and optional Work Context predicates with current owner SQL selection. Exact reads, bounded pages, pending inputs and public notifications select before decoding; cancellation and input-response transactions recheck authority. Datasheet adopts that query for Tasks and usage. Report and usage catalogs have checked 100-item pages and distinct cursor types; exact usage and prefix completion select in SQL. Static discovery has no Task scans. URI variants use standard-library builders; Workbench follows page URIs. Both registrations declare revision 3. Identity reuse checks current security fields transactionally and preserves display names. RFC 9562 Task generation uses `uuid-utils` 1.0.0 and native UUID values. Native SDK/template checks pass 193 cases and the fork fixture passes eight; headless Workbench page navigation, Helm configuration and native Datasheet smoke pass. Installed Datasheet passes all 26 hosted checks and public Task, catalog and usage reads | Complete wider owner-local SDK types and the remaining installed multi-page consumers |

The initial extraction preserves `ResourceUri`'s current opaque lexical profile and
wire strings. Concrete validation is a separate step through `ResourceUriParts`, also
available from `ResourceUri::components`. This lets domain contracts validate their
addresses. The gateway requires the current typed completion and audit target format;
its historical v1 reader is removed.
Do not pass a template through the concrete parser or claim the opaque constructor
establishes route safety. Gateway completion authorization and stored target decoding
now have native qualification; the installation transition remains pending. Its
simple policy selector matcher is not a general RFC 6570 template parser.
Foundation tests cover wire strings, schema descriptions, lexical rejection, independent
trait implementations, and compile-fail examples. A separately resolved consumer builds
without MCP, runtime, database, GPU, or provider dependencies. The shared MCP contract,
policy, and gateway library suites pass, as does strict workspace Clippy across all
targets and features. These checks qualify the extraction. Remaining server contract-only
features, broader checked-setup adoption, and repository-wide builders are pending.
Time's independent consumer passes its public-contract tests with only the Time
library, foundational types and URI libraries, serialization/schema support, and Chrono's
date/time types. The resolved graph excludes the clock feature as well as server, database,
GPU, and acquisition dependencies. Its runtime feature passes strict Clippy separately.
The default feature build, scope admission and configuration checks, public-ID
deserialization cases, and native catalog SQL tests pass. Shared-digest contract tests
pass with unchanged wire values. Scope schemas from independent modules stay distinct
even when their enums have the same name. Installed conformance is still pending.

The URI component tests cover all printable ASCII characters and Unicode, encoded
separators, malformed escapes and UTF-8, duplicate decoded query names, normalization,
and redacted validation errors. A regression exposed unescaped brackets from the URL
custom-scheme path setter; the percent-encoding step now covers its remaining non-URL
ASCII characters. Time release tests preserve supported ID spellings and string schemas,
reject wrong routes, IDs, queries, and encoded aliases, and prove the public trait round
trip. URL 2.5.8 and percent-encoding
2.3.2 were verified against their upstream stable release listings before adoption;
both versions were already present in the workspace lockfile.
The concrete profile requires unescaped authorities because URL's opaque-host
parser permits malformed percent escapes there. Current Time authorities and Artifact
UUID authorities fit that profile; encoded authority use must be inventoried before
wider adoption. Dynamic path and query components retain library encoding.

Time's resource migration uses one enum for parsing, building, and subscription
eligibility. Calendar, epoch, and event cursors preserve their v1 wire payloads and
carry distinct domain ID types. Catalog and admin APIs consume those cursor types;
event recovery follows the returned cursor directly. Native SQL cases round-trip the
emitted tokens before following pages. The low-level Store DTOs still erase IDs to
strings, and Time DTO zone fields outside resource addresses still need review.
These are remaining type-adoption work, not a completed Store boundary.

Authority-only root tests exposed an incorrect path-segment assumption in the
foundation helper. Roots now yield no segments; an explicit slash is distinct. Time
resource tests cover every family, malformed IDs and queries, cursor family mismatch,
version bounds, subscriptions, schemas, and compile-fail construction. Discovery's
zone template uses RFC 6570 reserved expansion to preserve slash-separated keys.
Time completion accepts only the advertised current zone template. The old adapter
and its support window are removed. Native discovery and typed-address checks pass;
qualify installed discovery on the rebuilt reference installation.

## Phase 4: Unified Audit Log

The survey behind [the audit design](AUDIT.md) found these paths and volumes. Each
figure counts audit rows, and every row also wrote an outbox row until this phase.

| Path | Today | Target |
|---|---|---|
| Gateway authentication (`platform/gateway/src/bin/gateway/auth.rs:98-380`) | One row per HTTP request, including every poll | Part of the request record; token lifecycle and credential denials only |
| Gateway policy (`platform/gateway/src/mcp/authorization.rs:329-367`) | One row per call, and one per item for discovery, repeated for every page and on each 5 s cache expiry; prompts are not cached (`prompts.rs:35`) | One record per request; one per list |
| Gateway tool call (`platform/gateway/src/mcp/tools.rs:410-432`) | Written after the effect, and a failed write errors the response | Completion record, retried, never fails the response |
| Admin outcome (`record_admin_operation_audit` in `platform/gateway/src/bin/gateway/audit.rs:236`) | Extra policy row with free-form metadata | Completion record |
| Artifact service (`platform/artifacts/service/src/service.rs:146-199`) | One row per authorization, including each range request; no trace ID; never deleted | Download windows; trace from the request context; retention |
| Upload completion (`platform/store/src/artifact_uploads/publication.rs:28-84`) | Same transaction as publication | Transactional writer |
| Live views (`servers/uav-sim-mcp/src/server/live_view_audit.rs`, `platform/store/src/live_views.rs`) | Best effort; failures only logged; never deleted | Issuance requires its record; close and expiry retried; retention |
| Refresh rotation (`platform/store/src/gateway_runtime.rs:504-620`) | Transactional | Transactional writer, `authentication` class |
| Speech (`platform/gateway/src/bin/gateway/speech/authority.rs:44-87`) | Authentication and policy rows per 1 s chunk, about 124 per minute | Session open, summary, and denials |
| Recording ingest denials (`platform/gateway/src/bin/gateway/recording_ingest.rs:449`) | Stored as authentication rows with a hard-coded `BearerJwt` method | Typed denial records |

| Action | Rows today | Records after |
|---|---|---|
| List tools with a cold cache | 1 + one per visible tool, above 100 | 1 |
| One tool call | 3 | 2 |
| One minute of dictation | about 124 | 2, plus denials |
| Video playback with 100 to 300 range requests | 300 to 900 | 1 per five-minute window |
| Agent episode with 10 turns and 8 tool calls | about 42, plus discovery on each connection rotation | one per request, plus one completion per tool call |

Other findings this phase fixes: `source_ip` is never written; trace IDs come in three
formats, so a request's authentication and policy rows never correlate; `request_id`
holds the event ID; rows without a tenant are invisible in the Console; the Console
stream scans the global changefeed and filters tenants in memory; CLI summaries read
every row without a tenant filter; `configs/deployments.json` `audit_event_days` is not
wired to the gateway's retention flag; and no code deletes `outbox_event` rows.

Work:

1. Upgrade SurrealDB from 3.2.4 to the latest 3.3 patch release in its own commit,
   before any audit change. Update the Helm image pin, every `surrealdb` crate pin, and
   the tests and documents that name the version. Check the store code against the 3.3
   behavior changes: locked reads through `SELECT … FOR UPDATE`, and `UPDATE` and
   `UPSERT` now evaluating `WHERE` before their data clauses. Qualify with the store and
   gateway test suites, all migrations on a fresh store, and the installed smoke
   scenarios.
   Time currently uses a tenant fence because the qualified server is 3.2.4.
   Qualify `FOR UPDATE` on both exact pointer IDs, including absent records, and
   their releases before replacing that fence; preserve the preflight snapshot checks.
   GitHub's latest-release API confirmed [SurrealDB 3.3.0](https://github.com/surrealdb/surrealdb/releases/tag/v3.3.0)
   on 2026-09-28, published at 11:40:24 UTC. Resolve the image digest and SDK provenance
   and qualify the release before changing pins.
2. Add request timing before changing audit. Each request reports policy evaluation,
   audit commit, and upstream time in its trace, and the gateway exports them as
   histograms. Record a baseline for catalog lists, resource reads, and tool calls on the
   reference installation.
3. Define the record types in `mcp/contract/src/audit.rs`, and delete `AuditEvent`,
   `AuthAuditEvent`, and their metadata maps.
4. Add a migration that removes `audit_event` and creates `audit_record` with compound
   record IDs `[partition, uuidv7]`, record links for platform targets, `READONLY`
   fields, and a change feed without `INCLUDE ORIGINAL`; `audit_block`; and the
   `audit_daily` table view. Add only the secondary indexes the bounded queries use
   beyond the ID range. Existing audit rows are discarded, as CE-12 records.
5. Create `platform/audit` with the writer's transactional and group-commit modes, the
   sealer that follows the `audit_record` change feed under a store lease, block-based
   retention, the OCSF and OpenTelemetry exporters, and verification. The gateway hosts
   the sealer, exporter, and retention worker.
   Add the dedicated audit signing key to the installation secrets.
6. In the gateway, assign request IDs, establish W3C trace context, carry both in the
   signed request context to upstream servers, record source IP addresses, write one
   record per request, aggregate discovery lists, write tool and admin completion
   records, write token lifecycle records, type recording ingest denials, and add
   `gateway audit verify`. Write no per-request record for allowed artifact range
   requests and dictation chunks, because their window and session records own them. Make audit retention a required
   installation value, wired from Helm, and remove the 365-day default.
7. Cache discovery decisions by caller authority, policy revision, and catalog
   generation, and invalidate them on change events instead of the 5 s expiry. Cache
   prompt decisions the same way.
8. Move the Artifact service, Computers lifecycle changes, Work Context transitions, and
   live views onto the writer. Live-view issuance fails when its record cannot commit;
   close and expiry records are retried. For upload publication, evaluate a synchronous
   `DEFINE EVENT` that writes the record in the publication transaction. Keep it only if
   it is simpler than the writer call and passes the record-type tests, and record the
   result in the audit design.
9. Replace per-chunk speech records with session records.
10. Stop writing outbox events for audit records. Phase 5 removes the outbox itself.
11. Rebuild the Console's audit views on the paged, partition-scoped range query, the
    `audit_daily` view, and a LIVE query per partition with change-feed recovery after a
    reconnect. Remove the global change-feed scan in
    `platform/gateway/src/bin/gateway/admin/console/stream.rs`. Add server-side export,
    show the `installation` partition to installation administrators and auditors, and
    record audit-view access. Bound and scope the CLI summaries, and update smoke
    assertions that counted old rows.
12. Update the documents that describe the old behavior: the tool-call and discovery
    paragraphs in `mcp/contract/DESIGN.md`, the audit section of
    `servers/uav-sim-mcp/DESIGN.md`, `platform/gateway/src/bin/gateway/speech/DESIGN.md`,
    the audit retention section of `docs/TECH_DESIGN.md`, and gap G4 with its stale
    retention reference in `docs/REGULATED_READINESS.md`. Replace the 500 ms catalog
    target in `docs/DEVELOPMENT_ITERATION.md` with a pointer to the audit design's
    measurement section.
13. Update the standards registers: SurrealDB 3.3, OCSF export, and W3C Trace Context in
    `README.md`; OCSF 1.9.0, RFC 9162, RFC 8785, and S3 Object Lock in
    `docs/TECH_DESIGN.md`; and an audit export interface in
    `interfaces-and-protocols.csv`.

Acceptance:

- The SurrealDB 3.3 upgrade passes its qualification before any audit commit lands.
- Tests count records for each action in the table above and match the target column,
  and a live-view renewal and an indexing window each produce the record the audit
  design specifies.
- A request fails when its record cannot commit, and a live-view authorization is not
  issued while the audit store is unavailable. An issued authorization keeps working.
- `gateway audit verify` detects an updated and a deleted sealed record, a back-dated
  inserted record, a removed block, and a bad signature, each made with database root
  credentials.
- A record committed after a later-stamped record is still sealed, because the sealer
  follows commit order.
- The gateway refuses to start without a retention value. Retention deletes every
  class, and with export configured it deletes only exported blocks.
- The exporter writes OCSF JSON Lines to the bundled S3-compatible store. Object Lock
  acceptance runs against a store that supports compliance mode, because the bundled
  RustFS does not (`docs/REGULATED_READINESS.md` gap G9).
- A repository test rejects any string field in a `detail` variant outside the
  identifier and reason-code allowlist.
- A warm catalog list evaluates no policy and writes one record.
- The Console's live audit view receives new records through its LIVE query without
  polling, and recovers missed records from the change feed after a reconnect.
- The before and after timing measurements are recorded in the audit design. Audit
  commit is no longer the dominant cost of catalog and read latency. This phase sets no
  fixed millisecond target.

## Phase 5: Store Simplification

This phase applies [Database First](../AGENTS.md#database-first) to the platform store.
Veoveo keeps two event-delivery mechanisms today. The outbox writes a second row for each
domain change and numbers it from `platform_outbox_sequence`, defined with `BATCH 1`, so
every outbox row writes the same sequence key. Change feeds already record every table
change in commit order, and `platform/store/src/changefeed.rs` reads them. The outbox's
`available_at` delay is unused: every writer sets it to the current time, and only a
test sets a future time.

Work:

1. Measure first. Record transaction conflicts, retries, and commit latency for outbox
   writers, and confirm how much of that cost the shared sequence causes.
2. Inventory every outbox writer and consumer with `git grep -n "outbox"`. Writers
   include `agents/runtime/src/runtime.rs`, `platform/store/src/artifact_access_requests.rs`,
   `map_authoring.rs`, `map_presentations.rs`,
   `servers/frames-mcp/src/state/worlds/`, and
   `servers/frames-mcp/src/state/operations/record.surql`.
   Consumers include the agent manager, the agent runtime, gateway agent events,
   `platform/task-runtime/src/runtime/subscriptions.rs`,
   `servers/artifact-mcp/src/bin/server/subscriptions.rs`,
   `servers/computers-mcp/src/protocol/subscriptions.rs`,
   `servers/media-mcp/src/bin/server/app_state.rs`, and the Map changeset projection.
3. Move each consumer to change feeds of the tables whose changes it needs. A consumer
   holds a LIVE query for push delivery and a persisted change-feed versionstamp cursor
   for recovery. Typed decoders in `platform/store` turn table changes into the events
   consumers act on, so each table's event vocabulary has one owner. A consumer whose
   cursor falls behind the change-feed retention reconciles from current table state.
   Migrate one consumer per commit, and keep its existing reactive tests passing.
4. Deliver delayed agent wakes from `wake.available_at` with a timer armed for the next
   due wake and re-armed by LIVE changes on `wake`. Do not poll.
5. Delete `outbox_event`, `outbox_checkpoint`, `platform_outbox_sequence`,
   `OutboxDraft`, and `platform/store/src/outbox.rs` once no consumer remains.
6. Review the change feed on each of the tables that set `INCLUDE ORIGINAL`, about 40
   of them. Keep it only where a consumer needs the prior state of a change, and record
   the reason in the owning design.
7. Survey code that deletes or repairs related records by hand, such as grants, shares,
   and relation edges. Replace each case with `REFERENCE` fields and `ON DELETE` rules
   where the database can enforce the same behavior, with a test per relationship.
8. Evaluate `DEFINE EVENT … ASYNC` with `RETRY` for projections that stay inside the
   database, starting with the Map changeset projection. Work that calls another
   service stays in that service.
9. Update `platform/store`, `platform/task-runtime`, `agents/runtime`, the Durable
   Platform Store section of `docs/TECH_DESIGN.md`, every other affected design, and
   the CODEMAP rows for `outbox.rs` and `changefeed.rs`.

Acceptance:

- `git grep outbox_event` returns nothing, and every former consumer passes its
  reactive tests.
- No consumer polls. Review confirms each wait is a LIVE query, a change-feed read after
  a reconnect, or a timer for a known due time.
- Write amplification, conflicts, and commit latency are measured before and after and
  recorded in the platform store design.
- Each `REFERENCE` adoption has a test for its `ON DELETE` behavior, and each rejected
  candidate has its reason recorded.

## Phase 6: Extension Crate And Shared Plumbing

1. Create `mcp/knowledge-extension` as a workspace crate with the models, server and
   client helpers, and docs collection listed in its design's implementation map.
2. Build the `{slug}.docs` collection into `veoveo_mcp_contract::docs`, so every Rust
   server that uses `server_docs!` declares it, and into `veoveo_mcp.contract.docs` in
   `sdk/python`, so `datasheet-mcp` in `templates/python-mcp` and fork Python servers
   declare it. Document revisions are SHA-256 digests computed at build time.
3. Add C32 to `CHECKLIST_IDS` and declare it in every server's `AGENTS.md`.
4. Add checks K01 through K08 to the conformance client and run them in certification
   for every server that declares the extension.
5. Add `platform/store/src/knowledge.rs` and the next ordered migration for catalog,
   chunk, and index-generation records.
6. In the gateway read path, declare the extension on upstream reads to declaring
   servers, attach the observation and read outcome to the read's audit record as a
   `detail` variant of the Phase 4 record type, commit that record before returning, and forward the observation only to declaring
   callers.
7. In `agents/kernel/src/resource.rs`, declare the extension on resource reads, keep
   the observation beside each admitted item, and render one provenance line per item
   inside the existing budgets.
8. Update the standards registers. Add `ai.veoveo/knowledge-source` to the agent and
   app interfaces row in `README.md`. Add a `docs/TECH_DESIGN.md` row for the extension
   with its RFC 9110 validator, RFC 9111 freshness, and RFC 8246 immutability
   semantics. Add the extension to the MCP row in `docs/ARCHITECTURE_DECISIONS.md`.
   Add an interface row for audited knowledge reads to
   `interfaces-and-protocols.csv`.

Acceptance:

- Gateway tests prove that a declared read's audit record carries its observation and
  outcome and commits before the result returns.
- A kernel test shows provenance lines within the byte budget.
- Every Rust server and `datasheet-mcp` pass K01 through K08 for their docs
  collections.

## Phase 7: First Adoption Wave

Each server below declares its collections, fills observations from existing records,
and makes its search results resource links. When a domain has no revision, use the
content digest as the revision.

| Server | Collections | Existing provenance | Gaps to close |
|---|---|---|---|
| chart | `charts.docs` only | none | Implement the docs declaration in `server.mjs` |
| time | events, calendar versions, epochs, authority releases | `record_version`, calendar `version`, `source_digest`, admin timestamps; owner only in the store | Surface owner and Work Context in observations; paging from Phase 3 |
| optimization | problems and solutions (immutable), runs | `digest_sha256`, `authority`, timestamps, engine digest | Surface labels in observations; restart-safe hub from Phase 3 |
| artifact | artifact metadata (`artifact://metadata/{id}`), never the bytes | compliance metadata: tenant, owner, Work Context, labels, provenance | Cursor paging for `artifact://index`; `modifiedBy` from the occurrence record |
| map | feature layers, features, publications, locations, facilities, dataset releases | layer and feature revisions, `created_by`, Work Context, labels, changeset sequence, source digests | Return resource links from `search_locations`; declare the other collections from its templates |

Acceptance: each server passes K01 through K10 review and conformance, and the audit
log records the observed revision for reads of each collection.

## Phase 8: Knowledge Service

1. Deliver the shared [embedding runtime](../platform/runtimes/embedding/DESIGN.md)
   before the knowledge service consumes it. Re-verify the latest stable vLLM release
   first, and keep one vLLM pin shared with `reason-mcp`. Do not add candle, fastembed,
   `ort`, or a Hugging Face client to any Veoveo service.
   1. Add the runtime to `deploy/helm/veoveo` following the `reason` values pattern:
      the official `vllm/vllm-openai` image run with `--runner pooling` and
      `--scheduling-policy priority`, a model-cache volume with the
      installation-supplied checkpoint at revision
      `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3`, an init container that checks
      `platform/runtimes/embedding/checkpoint.sha256`, `HF_HUB_OFFLINE=1`, the `nvidia`
      runtime class, a `nvidia.com/gpu` request, an installation-set
      `--gpu-memory-utilization`, readiness on `/health`, the installation embedding
      API key Secret mounted into the platform workloads that use it, and a
      NetworkPolicy that admits platform-namespace pods except the `computer-host`
      component. Stage the checkpoint on the reference installation the way Reason's
      world-model checkpoint is staged, and add the runtime's GPU share and Deployment
      to `examples/bioma/installation-target.json`. Document it in
      `deploy/helm/veoveo/DESIGN.md`.
   2. Confirm that vLLM applies last-token pooling with L2 normalization for this
      checkpoint, and set the pooler configuration explicitly if it does not. Confirm
      that the embeddings route honors request priority on the pinned version, and
      record the result in the runtime design.
   3. Create `platform/runtimes/embedding/client` as the `veoveo-embedding-client`
      workspace crate: `embed_documents`, `embed_query`, priorities, request bounds,
      response validation, and `EmbeddingSpace` read from `/v1/models`.
   4. Generate the reference vectors with the model card's `transformers` recipe
      through `uv run`, commit them as a fixture, and make the reference test pass
      against the runtime on a hardware GPU.
   5. Run the load test that shows interactive requests completing ahead of bulk work,
      and the network test that shows pods outside the platform namespace cannot
      connect.
2. Create `servers/knowledge-mcp` with `DESIGN.md` and `AGENTS.md`. Move the service
   sections of [`KNOWLEDGE.md`](KNOWLEDGE.md) into its design, keeping the
   cross-component flow in `KNOWLEDGE.md`.
3. Add typed knowledge approval entries to the control-plane contract in
   `mcp/contract/src/gateway/server_config.rs`, with validation. Register the service's
   machine client, grant it read access to approved collections only, and mark it as an
   indexing client so the gateway records its reads by collection window, as
   [the audit design](AUDIT.md#event-selection) specifies.
4. Implement discovery, the catalog resources, enumeration, change subscriptions,
   reconciliation, chunking, and index generations keyed by embedding space, with
   indexing at bulk priority. Add the `embed` tool for agents and external MCP hosts.
5. Implement `search` with BM25, HNSW, reciprocal rank fusion, and query embedding at
   interactive priority. Narrow candidates inside the SurrealDB query by tenant, the
   caller's Work Contexts and grant subjects, and clearance labels, over-fetch, and
   decide each candidate with `veoveo_mcp_contract::access::decide` from
   `mcp/contract/src/access.rs`, the predicate the Artifact service uses. Do not copy
   it.
6. Add the `knowledge-mcp` Helm chart, gateway registration, and offline image entries
   for it and the embedding runtime. `knowledge-mcp` requests no GPU.
7. Build an evaluation set from the Phase 7 collections, and record recall at 10 for
   the chosen chunk settings in the index generation. Measure indexing throughput with
   concurrent searches, then qualify `Qwen3-Embedding-4B` and `8B` against 0.6B as the
   runtime's [Model Selection](../platform/runtimes/embedding/DESIGN.md#model-selection)
   describes, and record the choice in the runtime design.
8. Update the standards registers. Add a knowledge area to `README.md` naming W3C DCAT
   3 and `Qwen/Qwen3-Embedding-0.6B` served by vLLM. Add `docs/TECH_DESIGN.md`
   rows for DCAT 3, SurrealDB `FULLTEXT` and `HNSW` indexes, and the vLLM embedding
   runtime with its internal OpenAI Embeddings API profile. Add the embedding runtime
   to `software-components.csv` with its internal interface. Add `knowledge-mcp` to `software-components.csv` and its search,
   catalog, and source-read interfaces to `interfaces-and-protocols.csv`.

Acceptance:

- K01 through K10 pass for `knowledge.docs`.
- Access tests prove that no result, title, or snippet escapes the caller's effective
  access.
- The embedding runtime passes its verification on a hardware GPU: readiness, digest
  and CUDA refusal, reference vectors at cosine similarity of at least 0.999, priority
  under load, and network isolation.
- Indexing throughput with concurrent searches and the 0.6B, 4B, and 8B comparison are
  recorded.
- The `embed` tool is authorized and audited through the gateway, and an agent's
  episode budget counts it.
- Invalidation and reconciliation tests pass.
- The reference installation indexes the approved Phase 7 collections, and a search
  returns links an agent can read.

## Phase 9: Second Adoption Wave

| Server | Collections | Prerequisite |
|---|---|---|
| frames | worlds, immutable revisions | Stamp Work Context on worlds from invocation authority; today owner and labels live only in the store |
| recording | recordings, layers | Recording URIs in catalog entries; owner in observations |
| reason | analyses and results; pipelines and models | Owner and labels in observations; link result artifacts instead of inlining them |
| stream | runs and results | Owner in observations |
| speech | transcripts | Transcript index and completion |
| view | compositions | Frames and tiles are not knowledge |
| uav-sim | control grants, mission plans | Live simulation state is not knowledge |
| duckdb | database schemas with `indexing: metadata` | Table contents stay behind the `query` tool |

Media and timeseries declare only their docs collection until they hold records worth
sharing. `showcase/sumo` has no well-known surface and is not part of the reference
installation, so this plan leaves it out.

Acceptance: each server passes K01 through K10, and `knowledge-mcp` indexes its
approved collections.

## Accepted Risks

The user accepted these risks on 2026-09-26. They do not block completion, and they
are recorded in the owning designs instead of Deferred Work:

| Risk | Handling |
|---|---|
| The bundled RustFS store has no Object Lock, so compliance-mode export cannot be qualified on the reference installation (`docs/REGULATED_READINESS.md` gap G9) | Export to the bundled store is still required. When no compliance-mode store is available, record the unqualified Object Lock path in the audit design's status and in gap G9 |
| vLLM may not honor request priority on the embeddings route | The client caps bulk requests in flight, as the embedding runtime design specifies, and the runtime design records which mechanism shipped |
| The plan spans ten phases | The Status line and Deferred Work record progress; phases land and deploy one at a time |

## Deferred Work

The implementing agent adds a row for every step it defers, and removes the row when
the step lands. Each row matches a `TODO(foundations)` comment in the code. The plan is
not complete while a row remains.

| Phase and step | Code path | What remains | Why it was deferred |
|---|---|---|---|
| Phase 3 Task result installation | `platform/task-runtime/DESIGN.md` | Qualify cross-replica delivery of the installed shared result envelope and event schema 3 | The fresh Store and linked Stream/Reason result acceptance pass; multiple installed replicas still need qualification |
| Phase 3 final server library gates | `servers/artifact-mcp/DESIGN.md` | Publish and qualify Artifact reads/sharing, Computers hosted feature builds and Speech's typed identity profile through CUDA transcription/dictation at the next integration checkpoint | Contract and direct consumer qualification stays separate from GPU workload acceptance; publication and installed runs are grouped with the next Phase 3 batch |
| Phase 1 reference reset | `testing/flight-smoke/src/domain.rs` | Finish composed flight, timing and installed workload acceptance with the corrected shared recording path | Map recovery, route and mission execution, live Stream, and independent live-part Stream replay and grounded Reason pass. The composed run has not yet exercised its corrected recording stage or reached final acceptance |
| Phase 3 Stream C27 installation | `servers/stream-mcp/src/bin/server/subscriptions.rs` | Qualify cross-replica run invalidations and live-session notifications on the rebuilt reference installation | Native mixed-source, reconnect and MCP cancellation checks pass; reference workloads are stopped during development |
