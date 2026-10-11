# Time MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 4.

## Purpose

One temporal authority for civil time, military date time groups, GNSS time,
mission epochs, operational calendars, clock quality, and temporal events.
Every resolved instant carries the authority releases and uncertainty used to
interpret it, so other servers consume time without reconstructing timezone or
leap second assumptions.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `TimeMcp` implements
  `DomainServer`, `TimeSubscriptions` implements `ResourceSubscriptions`, and the
  administration router joins the host's admin routes behind the admin scope check.
  `/time/readyz` reports ready while the clock is observed. Do not add a
  `ServerHandler`, router, host check or authentication middleware here.
- Owns the `time://` URI scheme. Identity: slug `time`, MCP `/time/mcp`,
  admin REST `/time/admin`, port 8800.
- The canonical instant is `TimeInstant`: integral TAI seconds plus nanosecond,
  uncertainty, and the TZDB and leap second release ids. Never emit an instant
  without its authority binding. Deterministic results also emit both exact
  release URIs, version labels, canonical SHA-256 digests, and explicit
  bootstrap or acquisition provenance. Intervals are half open `[start, end)`.
- Deterministic resolution and conversion do not contain a live clock
  observation. `time://clock/current` carries the effective clock policy and
  measured quality, including holdover evidence.
- Durable state lives in its SurrealDB owner tables (`time_*`) and the
  authority release volume under `/var/lib/veoveo/time`. The server never
  applies migrations. Tenant engine caches are derived and rebuilt from the
  active release pair.
- Authority activation is atomic: optimistic versions plus a full preflight
  load of the prospective TZDB and leap second pair; one active release per
  family per tenant. Acquisition downloads run under fixed host, media,
  digest, size, and time policy with archive traversal rejected.
- `GET /active-authorities` returns `AdminPage<ActiveAuthoritySelection>` with
  closed `{pointerVersion, release}` entries. Use `selection.write_guard()` for
  activation; the persisted pointer version differs from `release.recordVersion`.
  Preserve the joined tenant/parent/body admission and reject non-active releases.
  An absent family uses `TimeWriteGuard::Absent`; packaged effective references
  never supply a persisted pointer version. Administrative clients and Time use
  this response together in a coordinated reference upgrade, without old-shape aliases.
- Packaged bootstrap references bind the authority family and source-file SHA-256.
  Their immutable resource IDs must change with the bytes, never with a process
  restart or path. `server/bootstrap.rs` owns construction and qualification.
- `expand_schedule` and `validate_timeline` run only through the final Task
  API extension on `veoveo-task-runtime`; a direct call returns an instruction
  to use the task form. Hosted calculations use Tokio's blocking pool. Retain the
  original job outside the cancellable waiter and await it on normal worker exit,
  including heartbeat failure. Preserve cooperative stop checks; sorting and
  serialization between checks do not supply a hard abort or drain-time guarantee.
  Public engine methods stay synchronous for library callers.
- Civil fold and gap resolution defaults to `reject`; military zone `J` is
  rejected by the DTG parser. A positive leap second keeps its `:60`
  representation.

## Build And Test

- `cargo check -p veoveo-time-mcp`
- `cargo test -p veoveo-time-mcp --test gateway_consumers` runs local fixture and
  receipt controls. Its ignored public-gateway case uses explicit read-only fixtures;
  `cargo xtask smoke time-installed-consumers --help` lists prerequisites. Follow
  [the installed input contract](../../testing/installed/DESIGN.md#time-consumers).
  The same harness owns `time-installed-schedule-task`, whose private input requires
  explicit `complete`, `cancel` or `recover` mode, a typed selected schedule request
  and independent expected output. Cancel and recover require an acknowledged exact-ID
  Working delivery and a current Working read. Recover admits a selected single-replica
  `time-mcp` Rust container before Task creation. Ops may signal after the private
  armed marker, or explicitly admit `recovery.runtimeSignal` for the fixed k3d
  ancestor-containerd signal. That profile verifies node/container/PID/start-time
  identity before the final authenticated same-Task Working read, persists signal
  intent and dispatches once within the original deadline. An ambiguous dispatch
  never permits replay. Do not run an external signal driver with that profile.
  The same Task must still be Working after replacement.
  Complete/recover verify delivered Completed and current official result agreement;
  cancel requires delivered and current Cancelled without successful output. The
  operation deadline starts once before connections: 120 seconds for complete/cancel,
  300 for recover. The scenario allows 360 seconds plus one 30-second owner cleanup
  grace, retaining original consuming close futures. Recurrence count may reach
  1,000,000 while output is capped at 512 and the horizon at 31 days. Historical UTC
  recurrence scanning can produce a small result after substantial real computation;
  its duration is not guaranteed. Early completion fails cancellation/recovery
  qualification. The private append-only receipt retains intents, opaque Gateway
  Task identity, replacement progress and unresolved cleanup. See the installed input
  contract for selected crash fields and external operator coordination.
- `cargo xtask smoke time-installed-completed-task-handoff --help` describes the
  retained completed-Task profile in the same harness. Its private input selects the
  original complete-mode input, successful receipt and SHA-256, opaque Task ID and
  creation time, and a typed physical A/B setup. Ops owns every routing and Deployment
  mutation. Require A-only reads before B exists, A's observed process exit and Pod
  absence during B-only reads, then A restoration before selector restoration and B
  retirement. Fresh acknowledged exact-ID delivery is an initial completed snapshot.
  Keep unfinished cross-replica recovery and later mutation delivery separate.
  The operation uses one 300-second deadline and the existing 30-second cleanup grace.
- `cargo xtask smoke time-installed-authorities --help` describes the separate
  ignored `authority::isolated_authority_acquisition_activation_and_retained_epoch_through_gateway`
  profile in the existing harness. `VEOVEO_TIME_AUTHORITY_INPUT` selects a private
  fixture with an Ops-attested isolated namespace, tenant, context and verified
  reader/admin principal identities; normal private OAuth token files; typed TZDB
  and leap sources; independent product digests/version labels; initial bootstrap
  references; and independent epoch physical/UTC expectations. Persisted pointers
  and the source catalog must initially be empty. Never run this profile against
  reference authorities. It creates four acquisition jobs, activates both families,
  checks one concurrent winner and typed conflict, refuses a stale pointer guard,
  and verifies immutable epoch v1 plus explicitly rebound v2. Resource delivery
  must acknowledge the requested filter and accompany a changed current authority
  after activation; initial snapshot delivery is recorded separately.
  One original 900-second operation deadline caps all requests and joins. Each
  acquisition permits at most 66 correlated status GETs separated by five seconds,
  with bounded initial/final reads. The scenario allows 960 seconds and the existing
  30-second owner cleanup grace. The private append-only journal records mutation
  intents, acknowledged and unresolved identities, safe status/code/digests and
  consuming SDK close futures under their original caps. It does not roll back
  published authorities or delete retained state; Ops owns isolated retirement.
  Acquisition interruption, authority restart/replica delivery and coordinated
  installed upgrades require separate qualification. Local admission, pointer guard
  and subscription counterexamples live in `gateway_consumers/authority/tests.rs`;
  the shared owner close control is now
  `cleanup::tests::interrupted_close_retains_original_future_and_deadline`.
- `cargo test -p veoveo-time-mcp --lib server::tasks::tests` qualifies durable
  cancellation at calculation checkpoints and final settlement using distinct
  workers on the existing isolated Store fixture. Keep the cancellation/completion
  CAS interleave deterministic; do not manufacture a slow installed schedule.
  Preserve completion that committed first, genuine calculation failures and the
  executing worker's current lease. Local shutdown without durable cancel intent
  stops success publication before shared transition dispatch and leaves the request
  for Resume recovery. Keep token checks at both selected-snapshot dispatch seams;
  a transition already entered may settle despite a later local stop. Installed cancellation and unfinished restart
  still require their own observable qualification.
- `cargo test -p veoveo-time-mcp --lib server::tasks::calculation::tests`
  checks async responsiveness during a retained blocking job and joins the original
  job after interruption. `cargo test -p veoveo-time-mcp --lib cooperative_stop`
  checks engine stop handling separately from calculation failures.
- `cargo test -p veoveo-time-mcp --test metadata_versions active_selection_exposes_pointer_guard_and_rejects_unadmitted_wire`
  checks the closed active-selection response and schema. The focused hosted
  `registry::tests::active_authorities::operator_reads_pointer_guard_for_next_activation`
  library control reads the real pointer guard for two successive activations;
  preserve the existing native active-pointer corruption checks.
- `cargo test -p veoveo-time-mcp`
- `tests/gateway_source_conformance.rs` supplies two disposable scheduled events to
  the shared source checker. It cancels one before a Time Deployment restart and one
  afterwards, then reconciles both to cancelled state. Prerequisites and input fields
  follow [the installed harness contract](../../testing/installed/DESIGN.md).
- `cargo test -p veoveo-time-mcp --no-default-features --features contract` checks
  public contract validation and compile-fail examples. Qualify dependency isolation
  with a separately resolved consumer workspace; a workspace-wide build may enable
  dependencies through other packages.
- `cargo clippy -p veoveo-time-mcp --no-default-features --features runtime --all-targets -- -D warnings`
  checks runtime composition independently from the MCP feature.
- Time owns its private `src/persistence/` queries, driver records and mutation
  validation. Its `src/schema/` declaration owns the selected Time migration lane.
  Activation uses exact-ID locked reads
  of both pointers, including absence, and their preflight releases. Preserve full
  snapshot comparison and qualify cross-family contention on RocksDB.
  Runtime library tests use the shared
  isolated SurrealDB fixture with the Time lane selected; schema changes require
  fresh lane admission and persistence checks.
- `node --test tests/workbench-pagination.test.mjs` in `apps/console/web` checks
  page navigation behavior headlessly; it provides no visual or GPU acceptance.
- The container builds from `servers/time-mcp/Dockerfile` (needs Docker);
  Helm material is the `time-mcp` domain service in `deploy/helm/veoveo`.
  No GPU requirement.
- `src/catalog/tests.rs` qualifies SQL tenant and owner predicates, latest epoch
  selection, event transition isolation, page boundaries, requested epoch batches,
  and bounded distinct completion against the
  pinned disposable SurrealDB fixture.
- `src/catalog/tests/metadata.rs` qualifies retained body/key agreement, indexed
  ordering fields, redacted diagnostics and immutable acquisition metadata. Preserve
  the lifecycle-column rules and upgrade requirements in the design's Retained Catalog
  Metadata section; stale release-state bodies can represent valid retired records.
- `src/contract/clock_policy.rs` owns the checked policy builder and JSON bounds.
  Positive `TimeVersion` guards and optional-row `TimeWriteGuard` guards stay typed
  through writes; numeric zero means absence only for clock and active-pointer writes.
  All public metadata and immutable calendar/epoch versions use `TimeVersion`.
  Source creation takes `NewTimeSource` with the zero-only `SourceCreationVersion`.
  Check every increment; avoid converting a metadata version back through a raw integer.
- Catalog bodies decode through the current public metadata types. Lifecycle columns
  supply current state and version; zero and overflowing body versions are rejected.
  Historical formats have no adapter. The foundations installation drains and resets.
- `catalog/knowledge.rs` binds observations to SQL-admitted records and stored creation
  provenance. Calendar/epoch/release reads are tenant-shared; events are owner-only.
  Keep packaged bootstrap references separate from tenant-acquired releases, and keep
  epoch version URIs distinct in enumeration. Source access and content both affect revisions.
- `contract/instant.rs` owns subsecond admission and checked total-coordinate
  construction. Keep `SubsecondNanoseconds` through requests, cursors and persistence
  drafts; convert at protocol-library and database-driver adapters. Durations have
  separate ranges. `tests/instant_contract.rs` checks wire/schema bounds, defaults,
  negative fractions and both signed-seconds endpoints. Native metadata checks reject
  matching invalid body/column fractions without rewriting stored rows.
- `contract/authority.rs` owns checked release references, family pairs and instant
  bindings. Derive repeated identity from the typed release URI and derive bindings
  from the effective pair. Keep their fields and the runtime `AuthorityContext`
  private; callers consume accessors. Pair construction checks dataset roles and
  distinct release IDs. `tests/authority_contract.rs` qualifies wire admission and
  schemas, while native metadata tests qualify stricter retained-binding admission.
- `contract/window.rs` owns checked nonempty half-open intervals and intersection.
  Keep bounds private and check the active engine authority at runtime. Algebra and
  schedule clipping preserve selected endpoint uncertainty, taking the maximum at
  tied coordinates without moving the bound. `tests/window_contract.rs` checks wire
  admission; `engine/windows.rs` and engine schedule tests qualify runtime behavior.
- `contract/resolution.rs` binds each resolved instant to matching effective release
  metadata, with read-only accessors and the existing flat projection fields. The
  engine calculates the projections and rejects foreign epoch authorities. Keep
  epoch IDs typed and carry their uncertainty through checked addition. Authority
  contract tests and native engine cases qualify decoding and relative resolution.
- `AuthoritySourceDigest` preserves the admin bare-hexadecimal spelling and idempotency
  equality. Download checks compare its canonical digest values; provenance uses the
  foundational `sha256:` representation. Keep digests typed through persistence drafts.
  `tests/digest_contract.rs` and `src/registry/tests/digest.rs` qualify the wire adapter,
  uppercase retained records and malformed matching body/column values.
- `src/catalog/tests/numeric.rs` qualifies clock scalar rejection, optimistic clock
  writes and exhausted versions without row mutation. Authority qualification verifies
  that retirement exhaustion rolls back both the candidate and pointer.
- `src/persistence/active_tests.rs` qualifies joined pointer/release selection,
  inconsistent retained records and relationship changes inside activation. Keep
  relationship predicates in SQL and reject a visible dangling pointer. Bootstrap
  selection during reload requires an absent pointer.
- `src/registry/tests.rs` uses Linux `/usr/share/zoneinfo/UTC`, owned temporary files
  and the shared isolated database fixture. Engine requests validate the active pair
  and provenance before cache reuse. Each returned engine owns its epoch map. Cache
  eviction follows every Store invalidation; delivery never substitutes for a read.
  Authority loading and preflight each have a 30-second deadline.
- `src/registry/tests/activation.rs` qualifies cross-family conflicts on disposable
  RocksDB, absent and present pointers, concurrent release repairs and stale preflight
  metadata. Production activation goes through the registry and consumes its observed
  draft. Both exact pointer IDs and all preflight release IDs participate in commit
  conflict checks. Keep the fresh-store and drain requirements in DESIGN.
- The optional ntpd-rs observation socket is a deployment concern; unit tests
  use bounded fake observations.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met
- C02: met
- C03: met
- C04: met — stable discovery, SQL-scoped pages of 100 records, exact reads, and bounded completions; native tests cover tenant/owner isolation and cursor ordering
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: met
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met — static discovery advertises no list-change capability; Store observations invalidate resource contents
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — typed docs and five domain collections are implemented; source-policy, provenance, URI and paging checks pass natively; installed K01–K08 qualification and event change/restart probes remain open
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->

### Installed Future Events

`cargo xtask smoke time-installed-events` selects
`events::future_temporal_events_through_public_gateway` in the existing
`gateway_consumers` target. `VEOVEO_TIME_EVENTS_INPUT` points to a private closed
fixture with `installation`, `authority`, and `selectedPod`. Select a disposable
single-replica Time Deployment and the same normal OAuth principal for every
connection. Its profile must admit its required scopes plus `time:read` and
`time:event:write`; an operator-created event is invisible to another agent owner.

The original 300-second interval includes discovery, exact event-root listener
acknowledgement, current-clock reads, four creations, guarded refusals, one fenced
restart and explicit reconnects. Due coordinates preserve the current authority
and uncertainty while adding checked TAI offsets of 20, 80, 180 and 240 seconds.
The first and third events require delivered invalidation plus an uncached Due
read. The second is cancelled before its due instant. The fourth becomes due
while disconnected and qualifies current-state reconciliation after reconnect.
An initial snapshot does not establish notification replay. Early Due refuses the
required Scheduled checkpoint without renewing any deadline.

The 30-second cleanup reconciles only acknowledged owned events still Scheduled,
then closes every retained SDK generation. Creation intents and acknowledged IDs
survive assertion and journal failures; an unknown create outcome is not retried
or deleted. Due and cancelled records remain available for inspection. Ops owns
retirement of isolated state. This profile does not replace calendar Task recovery.
