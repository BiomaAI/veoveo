# Platform Foundations Plan

Status: Phase 0 accepted and published on 2026-09-27 at `1177185f`; documentation
checks and reference GitOps convergence passed. Phases 1–3 are in progress.
Phases 4–9 have not started. Phase 1 images and charts are published at `faa1fed3`;
the old reference node and its five owned volumes have been removed. Cleanup
reclaimed about 416 GiB, leaving 570 GiB free on 2026-09-27. The reference workloads
are stopped at the user's request; native acceptance, rebuild, and installed
qualification are pending. The Phase 2 catalog fixture
at `883a09ba` passed native SDK reads (16 rows) and grant renewal on 2026-09-27;
reference installation acceptance is pending. Phase 3 Reason pagination
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
same transaction as the operation and outbox event. Four native database cases pass:
direct isolation and concurrent replay, Task authority changes, additive migration over
historical records, and rollback/retry after injected event-publication failure. The
UUIDv7 storage-admission case also passes. The new authority v1 profile leaves retained
v0 records unchanged and inaccessible through public resource reads; the owning design
declares the coordinated drain, export and recovery requirements. Operation addresses
use the foundational builder, and checked provenance construction/decoding enforces
ID/URI agreement without changing existing schemas. Sixteen independent contract
consumer cases pass without runtime dependencies, and six compile-fail cases pass. The
native MCP smoke passes direct and Task-backed operation reads and denial under a
different principal, tenant or profile. Store regressions and strict workspace Clippy
pass. Installed operation acceptance is pending.
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
Gateway completion and template discovery now use `PolicyTarget::ResourceTemplate`.
The shared evaluator preserves lexical selectors, rejects declaration targets for reads,
and keeps ownership, scope and deny checks in force. New policy audit writes carry the
v2 format marker. A read-only v1 adapter converts historical targets according to their
recorded action, including literal-only templates, without changing stored rows or decisions.
Native checks cover both formats through separate Store connections and reject invalid
new events before writing. Contract, policy, gateway library and gateway integration
checks pass 280 cases, plus five compile-fail examples; strict workspace Clippy passes.
The gateway audit design declares the coordinated drain,
snapshot rollback and adapter retirement; installed transition qualification is pending.
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
The full Rust enforcer passed at `ab61a602`; default-feature workspace acceptance
and reference installation qualification are pending.

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
- Read `AGENTS.md` and `docs/CODEMAP.md` before each phase.
- Work on `main` in small commits, one concern each. Run the native checks each commit
  touches, and run `cargo xtask enforce docs` for every documentation change.
- Internal names and formats change by hard cut. Do not add aliases, fallbacks, or
  readers for old identifiers.
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
| Public payload schemas | about 12 | `live-view/v4` (`mcp/contract/src/live_view.rs:12`), `hosted-mcp/v3` (`mcp/contract/src/lib.rs:9`), conformance profile and report (`mcp/conformance/src/profile.rs:10`, `report.rs:7`), recording catalog, projection, and playback tags, `map-route-handoff/v1` (map and `servers/uav-sim-mcp/src/contract.rs:388`), and the optimization problem tags (`servers/optimization-mcp/src/domain/mod.rs:18-21`) |
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
| Qualification | Demonstrate valid construction and rejection of invalid combinations, plus the affected domain behavior. Prove contract feature isolation with an independent consumer; qualify published wire and persisted data transitions before claiming completion. |

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
   semantics. Published protocol and persisted format changes require their declared
   transition and rollback qualification.
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

All 15 Rust MCP server packages under `servers/` have library targets. Time, Map, and
Frames define the required `contract` feature and pass independently resolved consumer
checks. The other 12 packages still need feature isolation. Existing libraries remain the
default owner; the inventory must not become a central domain-type registry.

| Surface | Current dependency or representation gap | Next owning change |
|---|---|---|
| Foundational primitives | `ScopeName`, `ResourceScheme`, `ResourceUri`, and `IdentifierError` are extracted into `platform/types`; direct callers use that crate; wire/schema preservation and independent dependency isolation pass | Resolve concrete/template resource references before tightening URI validation |
| Independent extension traits | `ScopeDefinition` and `ResourceAddress` are public and contain no domain variants; `McpServerContract` associates server-owned types with descriptors and documents. Time consumes checked setup; the independently owned fixture passes hosted conformance, typed access/denial and contract-only consumption | Adopt checked setup across the remaining servers and templates; preserve domain-owned authorization |
| Scope declarations | `scope_enum!` generates conversions and schemas from server-owned spellings, with compile-time rejection of invalid or duplicate declarations | Adopt it across server libraries while keeping each domain's vocabulary local |
| Concrete URI components | `ResourceUriParts` validates the concrete profile with URL 2.5.8; `ResourceUriBuilder`, `UriAuthority`, and percent-encoding 2.3.2 encode typed scheme/authority and path/query components, preserve segment identity, and reject duplicate query names | Adopt through domain constructors with specific ID types; qualify each family's spelling and parameters |
| Resource templates | `ResourceTemplateUri` uses iri-string 0.7.14 with guards for RFC prefix bounds and dotted variable names; `McpResourceTemplate` prevents descriptor mutation. Time and the independent fixture consume checked template declarations. Native and isolated-consumer cases qualify syntax, expansion, existing addresses and error redaction | Extend checked declarations and domain-builder agreement across remaining servers |
| Gateway completion and audit targets | Completion and template discovery use `PolicyTarget::ResourceTemplate`; shared policy keeps the existing lexical selectors. New stored events use v2; the read-only v1 DTO adapter separates historical targets using their actions. Native qualification preserves old rows through separate Store connections | Qualify the declared installation drain and rollback; tighten the opaque resource validator after remaining URI families are inventoried |
| Platform identity and attribution | Principal, tenant, group, role, Work Context, delegation, data-label, and policy-version types, access subjects, and invocation provenance are extracted into `platform/types`; consumers import them directly; eleven baseline schemas, wire/profile tests, independent consumer tests, and strict workspace Clippy pass | Preserve these contracts during domain extraction; qualify installed identity and policy behavior with the affected services |
| Map | `MapScope` owns handler, Task, and default administrative scope spellings; authoring metadata requests and cursors use typed IDs and shared URI components; identity, Artifact metadata, and geodetic IDs now come from their owning contract libraries; the contract feature excludes runtime dependencies | Migrate remaining addresses and Store query IDs; qualify installed behavior |
| Coordinate vocabulary | Map owns geodetic IDs; Frames owns worlds, conversions, and typed world/revision/frame addresses; RRD owns recorded frame/geofence metadata. Shared MCP coordinates are removed. Independent contract consumption and schema compatibility pass | Qualify installed behavior; enforce the documented retained-data preflight and coordinated drain for relative frame-ID rejection |
| Map identity admission | Domain IDs accept UUIDv5/v7 spellings through the UUID library; Store authoring keys check only a prefix, byte bound, and slash exclusion | Qualify persisted spellings and establish one domain-owned admission profile when moving IDs into query APIs |
| Time | The contract feature excludes runtime dependencies; handlers, Tasks, and configuration defaults use `TimeScope`; `TimeResource` owns every URI family and the three collection cursor types. Checked server setup supplies startup, discovery and scope membership. Private runtime persistence owns SQL, mutation drafts and driver records; catalog calls retain domain IDs, versions, completion parents and cursors until driver conversion. Checked catalog decoding binds JSON identity, versions and indexed fields to the stored row; native corruption and immutable-acquisition checks pass | Complete broader DTO field types; qualify installed hosted behavior, retained-metadata preflight/rollback and the zone-template upgrade drain |
| Time identity admission | Time owns both profiles: public IDs accept bounded prefixed names, including bootstrap authority references, while stored catalog keys require UUIDv7 suffixes. Persistence validates the stored profile without narrowing public provenance; Store has no Time query or draft API and owns the shared connection and migrations | Qualify installed and retained-data behavior; use the declared profiles when strengthening public metadata construction |
| Time scalar admission | Clock policies have a checked builder. Metadata versions use `TimeVersion`, with a distinct zero-only source-creation input and private retained-body decoding. `SubsecondNanoseconds` covers instants, expressions, cursors and persistence drafts. Total-coordinate conversion and NTP/UTC epoch arithmetic check seconds overflow; native boundary, transition and retained-row cases pass. JSON keeps its numeric shape. Requests and persistence keep typed guards; exhaustion checks preserve rows and atomically roll back failed retirement | Qualify installed numeric admission and the documented preflight/drain/rollback; finish remaining expression/projection scalar types and acquisition-state relationships |
| Time intervals | `TimeWindow` checks increasing bounds and a common authority at construction and decoding; accessors preserve these invariants. Algebra keeps endpoint uncertainty, selecting the maximum at tied coordinates. Schedule expansion clips to the horizon while preserving recurrence limits and labels. Native membership, metadata, authority and clipping cases pass; independent contract consumption qualifies the unchanged valid wire shape | Qualify installed schedule Tasks, including the pending-Task drain, recovery and rollback in Time's design |
| Time active-pointer admission | One SQL statement resolves each visible pointer to an active release with matching tenant, family and key. Pointer identity, history and versions are checked; activation rechecks parent and lifecycle relationships in the transaction. Native corruption, SQL payload exclusion and ten interleaved-mutation rollback cases pass | Qualify installed parent admission, retained-data preflight and rollback |
| Time authority contexts | Each engine request validates the joined active selection and provenance before reusing loaded files. Cache keys use the Store tenant type, engine epoch maps are independent, Store signals evict contexts and failures remove cached values. Native restart, isolation, disconnected-observation, provenance and file-recovery cases pass | Qualify installed restart/replica behavior and the declared coordinated upgrade |
| Time authority metadata | Checked references derive release identity from typed URIs. Effective pairs require the correct dataset roles and distinct IDs, and derive instant bindings. Wire adapters preserve valid fields; the runtime context keeps metadata and bindings private. Native retained-row and independent-consumer admission checks pass | Qualify retained binding preflight and installed startup |
| Time resolution metadata | `ResolveTimeOutput` admits matching instant/release pairs and protects them with read-only accessors; its wire adapter preserves flat projection fields. Engine epoch keys remain typed, relative calculations reject foreign authority and preserve uncertainty, and additional uncertainty checks overflow. Native and independent-consumer cases qualify these relationships | Qualify installed resolve/convert decoding and epoch behavior after authority activation; computed representations remain the engine's responsibility |
| Time activation preflight | A private draft carries the candidate and both admitted active families through file loading to SQL commit. A shared tenant write fences different-family decisions on 3.2.4; the transaction compares the observed metadata and rolls back every mutation on conflict. Native cases cover RocksDB contention, retained schema upgrades, stale inputs and failed file loads | Complete installed coordinated-upgrade/rollback acceptance; qualify 3.3 locked reads before retiring the fence |
| Digest wire profiles | Time's `AuthoritySourceDigest` preserves bare hexadecimal spelling through metadata, requests and typed persistence drafts; canonical content comparison and shared provenance use the foundational `sha256:` value. Native cases preserve uppercase retained data and idempotency while rejecting malformed matching rows. View still uses bare hexadecimal text | Qualify Time's installed retained-data admission and rollback; migrate each remaining owner through its declared wire profile |
| Computers and Speech | Existing domain contract crates separate some types from the server runtime; Speech imports Artifact metadata directly and its independently resolved contract excludes MCP and service dependencies | Qualify the Computers dependency closure and expose both server libraries' contract features without duplicating types |
| Artifact plane model | `platform/artifacts/contract` owns occurrence identity, metadata, compliance, provenance, release state, and byte handoffs; the separate plane service/client prevents placing their common model in the MCP server package without a Cargo cycle; schema/wire preservation, independent consumption, and native checks pass | Typed addresses now flow through metadata, service/client resolution, and the migrated domain/Store consumers; qualify installed reads and separate remaining access/service request contracts |
| Artifact identity and URI admission | `ArtifactId` checks version and RFC variant; `ArtifactUri` owns neutral/presented variants, preserves accepted URI spelling, and builds from typed IDs and schemes; metadata checks wire ID/URI agreement | Contract, HTTP, native Store, independent-consumer, and strict workspace Clippy checks pass; qualify installed consumption; retain the documented preflight and recovery requirements for existing data |
| Remaining Artifact references | Download URLs, Reason grounding references, some Store DTOs, and other domain URI fields still use broader string profiles | Migrate with each owning contract; distinguish Artifact identities from external fetch locations and declare persisted/profile changes |
| Artifact attribution construction | `ArtifactProvenance` uses foundational `InvocationProvenance`; a private wire adapter preserves valid flat metadata and requires each mode's identities in both decoding and schemas | Native publication/readback, independent consumption, schema/decoder parity, and compile-fail qualification pass; qualify installed metadata consumption during reference acceptance |
| Artifact MCP, Media, Optimization | Public operation types still depend on shared access/provider contracts and server runtime modules | Assign each contract to its domain owner and gate runtime dependencies |
| Frames world reads | Frames owns typed reads over the existing Store client; SQL applies visibility and parent checks, world catalogs use typed keyset pages, and completion binds parents and matches before limits. Six isolated native cases pass. Discovery is static; private driver records and mutations also belong to Frames | Qualify installed paging and completion; enforce the documented client/server coordinated upgrade |
| Frames mutation inputs | Frames owns typed mutations, private driver records and world-event vocabulary. World publication checks owner, current labels and head agreement in the transaction; repeated writes settle from authorized matching state. Store has no world draft API | Qualify installed publication, concurrent replay and the coordinated writer upgrade |
| Frames world metadata construction | Checked summaries, immutable revisions, and source references derive their repeated identities from typed URIs. The contract owns complete-tree validation and hashing; Store reads and UAV use it. Native corruption/visibility and independent schema/consumer checks pass | Qualify installed consumers and the stricter retained-metadata preflight |
| Frames operation references | Operation addresses use typed component builders; checked provenance derives its ID from the URI and rejects conflicting wire identity. Existing schema snapshots and independent contract consumption pass | Qualify installed consumption and the documented retained-provenance preflight |
| Frames stream references | `FrameStreamUri` applies the shared concrete URI profile; `FrameEntityPath` checks bounded producer selectors. Independent consumption, schema compatibility and native retained-node qualification pass. Frames preserves source spelling and leaves route vocabulary with producers | Qualify installed behavior and retained-data preflight; complete each producer's own typed builders in its migration |
| Frames usage visibility and pages | TaskRuntime applies current Task owner policy and linked-record agreement in SQL before grouping and limiting usage. Frames owns checked pages and typed Task cursors/URIs in its isolated contract feature; native denied-row and cursor cases pass | Qualify installed reads and subscriptions; enforce the documented coordinated catalog upgrade and retained-reference preflight |
| Frames operation visibility | Frames owns SQL-scoped operation reads and transactional authority/immutable replay checks; native caller, parent, migration and event-rollback cases pass. Version 1 stores profile authority; historical version 0 records are preserved without public access | Qualify installed direct/Task operation reads and execute the documented coordinated upgrade |
| Timeseries usage | The library delegates pages and exact reads to TaskRuntime's SQL owner policy; typed usage URIs, cursors and checked pages belong to its isolated contract feature. Valid version 1 cursor bytes and response fields are preserved. Native denied-row, continuation, label-change and parent-metadata cases pass; the independent 75-package consumer and strict runtime/workspace Clippy pass | Complete other resource builders and checked MCP setup; qualify the coordinated replica replacement and installed reads |
| DuckDB usage and discovery | The library uses TaskRuntime SQL visibility for 100-entry usage pages and exact reads. Its isolated contract owns usage addresses, collection-bound cursors and checked pages; discovery declares roots/templates without scanning records. The unused unbounded Store usage catalog API is removed. Native reads and Spatial, the independent 74-package contract consumer, and strict runtime/workspace Clippy pass. Workbench cursor construction uses the browser URL API; headless navigation and reserved-character cases pass | Qualify the coordinated array-to-page transition, installed reads and headed hardware Workbench acceptance |
| Other usage query owners | Optimization retains a separate usage selection path whose policy additionally compares Work Context identity | Apply its full policy in SQL before limits and qualify context-aware selection |
| Media usage and prediction reads | Media uses its own ledger; usage catalogs and prediction discovery enumerate rows before checking Task owners, and exact reads check ownership separately from selection | Move current owner and linked-record checks into Media's SQL, expose typed bounded catalogs, replace dynamic discovery with roots/templates, and qualify subscriptions under the same policy |
| Native Task identity | `veoveo-types` owns `TaskId`; consumers import it directly and Store's `task_record_id` performs database conversion. Native lifecycle, wire preservation, compile-fail, and independent consumption checks pass. Frames uses it in usage cursors without runtime dependencies; runtime external lookups still require v7 and opaque MCP handles keep their own profile | Migrate remaining string-based runtime lookup APIs with their owning admission contract |
| DuckDB source contract | DuckDB owns its public source vocabulary and read SQL helpers through an isolated `contract` feature; Timeseries imports that contract directly, and the agent kernel consumes its SQL quoting. MCP core has no source types or re-exports. Source wire, SQL-fragment and 17-schema checks pass without MCP, engine or service dependencies; native DuckDB, Timeseries, MCP and kernel memory checks pass, including Spatial execution and the unchanged Timeseries forecast schema | Complete owner database catalog paging, remaining typed resource builders and checked MCP setup; qualify installed source consumption |
| Reason, Recording, Stream, UAV, View | Existing contract modules have no isolated server library feature | Audit module dependencies, add feature gates, and migrate domain scopes and resource construction |
| Shared consumers | Gateway, policy, Console BFF, Computers, conformance, smoke, and integration tests import foundational names | Keep imports direct and preserve authorization, identity serialization, and schemas |
| SDKs, clients, templates, and showcase servers | Cross-language builders and extension qualification are not yet inventoried completely; the independent Rust hosted fixture and its isolated consumer pass | Complete owner-local adoption and template guidance |

The initial extraction preserves `ResourceUri`'s current opaque lexical profile and
wire strings. Concrete validation is a separate step through `ResourceUriParts`, also
available from `ResourceUri::components`. This lets domain contracts validate their
addresses while the gateway's explicit v1 adapter reads historical completion and audit
references as text before classifying their target kinds.
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
An explicit Time completion v1 adapter keeps the old reference spelling during the
0.1.x support window. Its owner and removal gate live in
[Time's compatibility section](../servers/time-mcp/DESIGN.md#zone-completion-template-compatibility).
The reference rebuild must qualify a drained Time transition and discovery refresh;
mixed v1-only/current replicas are unsupported. Installed qualification is pending.

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
   The 2026-09-27 release check returned `v3.3.0` from the upstream download endpoint,
   while GitHub's latest-release API returned `v3.2.4` and the `v3.3.0` release page
   returned 404. Resolve the release artifact and SDK provenance before changing pins.
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
| Phase 1 reference reset | `examples/bioma/README.md` | Finish native acceptance, rebuild from the published platform and UAV locks, then qualify the reference installation | Node and volume cleanup is complete. Reference workloads are stopped at the user's request after disk pressure; local qualification must finish before reactivation |
