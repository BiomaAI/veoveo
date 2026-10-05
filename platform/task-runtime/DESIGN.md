# Durable Task Runtime

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28`, Tasks SEP-2663 | Official RMCP Task projection; internal recovery classes introduce no new MCP status or method |
| SurrealDB / SurrealQL `3.3.0` | Shared durable Task records, lease compare-and-set, native changefeed observation and schema migrations; the Python port uses the pinned SurrealDB Python SDK `2.0.0` |
| Veoveo Work Context | Canonical TaskOwner/InvocationAuthority, tenant and server ownership, retained result pins |
| Internal recovery-class vocabulary | `resume`, `webhook_wait`, `provider_wait`, `interrupted_indeterminate`; domain-qualified completion semantics |
| Native Task identity | `veoveo_types::TaskId` carries UUID identity; external runtime lookups require UUIDv7. MCP opaque handles have their own protocol profile. |
| Internal Task result format | Store results have one required `payload` field and a separate optional typed product URI. Native changefeed replay preserves absent and JSON-null results as distinct states. |

This library is the shared Task authority used by hosted domain services. Public
handlers delegate protocol projection to the official RMCP types and the shared
service adapter. The library does not authorize provider side effects by itself.

## Stored Task Controls

Store owns `TaskOwnerRecord` and `TaskRequestRecord`. The required `owner_context`
column carries the admitted caller snapshot; `task.owner` keeps its principal record
link. Runtime decoding compares the snapshot with the indexed tenant, principal,
profile, context, initiator and the independent stored authority. Caller clearance
labels and output-policy labels have separate meanings and can differ.
Current reads compare the caller's clearance with the stored snapshot. A clearance
change does not rewrite the Task's submission identity or its owner contribution.

The request object declares `input`, `status_message`, `ttl_ms` and
`poll_interval_ms`. Only `input` accepts arbitrary JSON, including null, scalars,
arrays and nested provider fields. The driver preserves unsigned 64-bit integers
through SurrealDB decimal values. Optional controls use explicit JSON null, policy
arrays use their declared defaults, and label sets have stable ordering. Request
mutations compare both the retained request and owner snapshot in the same
transaction. These storage rules apply to a fresh coordinated installation.

Stored timing controls preserve the full unsigned 64-bit range. Creation admits a
TTL only when its deadline fits the shared RFC 3339 four-digit-year range through
9999; Runtime rejects overflow before identity or Task writes. An omitted TTL uses
the seven-day retention default, and zero produces an immediate deadline.

The database derives `created_at_exact` and `updated_at_exact` from their
nanosecond datetime fields on every write. These private driver tokens let clients
whose datetime decoder keeps only microseconds retain complete timestamps for
keyset paging, compare-and-set operations and changefeed replay. Rust checks each
token against its datetime; the public Task model exposes the original timestamps
without extra fields.

## Ownership

`runtime` owns creation, state transitions, subscriptions and retention. `leases`
owns execution and observation claims. `recovery` applies each declared restart
profile. `types` validates durable envelopes; `mcp` and `service` project them into
the hosted server contract. Provider SDKs and resource-specific fences belong to
the owning domain.
`provider_transaction` composes a domain journal write with the exact shared
observation-lease receipt in the same database transaction.
`provider_resume` composes explicit domain recovery with a Task status transition.
`runtime/webhooks` owns the provider journal for `WebhookWait`, including dispatch
receipts, job associations and authenticated terminal observations. Domains verify
provider signatures and callback correlation before calling this trusted API.
`admission` composes a domain admission with the exact queued, unclaimed Task.
`runtime/owner_query` owns `OwnerTaskQuery`, built by `TaskRuntime::for_owner`.
The builder holds the current owner, an optional checked Work Context selection,
and an optional selection of 1–32 validated `TaskTypeName` values. Omitting the
operation selection admits every operation hosted by the
runtime server; an empty explicit selection is an error. `get`, `page` and `subscribe`
apply the same selection in SQL before decoding. Subscription updates and Store
reconnect baselines preserve it. Shared code contains no server operation variants.
`cancel` applies that selection to every read and to both cancellation transitions.
The transition transaction checks current profile, owner, clearance and selected
Work Context before updating the Task or writing its event. A denied Task has the
same unknown-ID response as an absent Task. The MCP service adapter uses this method;
domain workers keep the trusted runtime API for their own lifecycle transitions.
`runtime/input_responses` owns trusted and caller-scoped input answers. Public
updates use `OwnerTaskQuery::submit_input_responses`, which repeats the same
selection inside each answer's transaction.
`runtime/context_scope` owns the Work Context predicates and typed scalar bindings
shared by Task observation and linked usage reads. The included policy fragments
in `queries/` guard stored request and authority objects before accessing their
fields and require an array of stored data labels before clearance comparison.
Nested `IF` branches prevent record-valued policy fields from dereferencing foreign
rows; Boolean conjunction evaluation order does not establish shape admission.
`runtime/owner_reads` owns SQL Task-owner selection and bindings;
`runtime/owner_subscriptions` delivers the selected current state to public listeners.
`resource_subscriptions` maps that authorized Task stream to requested Task status
and resource invalidations using domain-owned `TaskResourceAddress` implementations.

Consumers import native `TaskId` directly from `veoveo-types`. Native reads, claims,
transitions, cancellation, input exchanges, retention and worker registration take that
value through Store's `task_record_id` conversion at bindings. Creation and lookup
admit RFC UUIDv7 values, including their variant. The MCP service adapter parses text
handles once before calling the owner query. Public identity consumers do not depend
on this runtime or Store. Domain workers keep their own Task-derived identity types
and use checked conversions at the runtime adapter.

Rust subscription baselines anchor native versionstamp cursors before selecting Task
state. One projected LIVE source per runtime wakes readers and persists its cursor
under the server and worker identity. Separate replicas require distinct worker IDs.
Store completes transaction tails before advancing a cursor. The checkpoint table
has no changefeed, so acknowledgements cannot wake their own consumer.

## Module Task Contributions

Optional modules declare a table through `ModuleOwnership` and bind a pure
`TaskContributions` adapter to validated operation names. `OwnedTaskTable` checks
that declaration's optional layer and table claims and excludes the kernel Task
table. Composition checks global ownership conflicts. A declaration and a bound
adapter establish neither installed schema nor lane readiness.

`TaskContribution` admits only typed object values. Creation supplies an immutable
`identity`; settlement supplies the same identity and a terminal `settlement`.
The runtime constructs the row key from `TaskId` and supplies its protected `task`,
`task_type` and `created_at` columns. An adapter cannot supply SQL, transaction
control, field names or bindings. One adapter owns a table and its declared
operation set; duplicate table or operation bindings fail. A module may explicitly
contribute no row for an operation that needs no index.

Creation writes the Task, optional idempotency claim and contributed row in one
transaction. Existing idempotency winners return without replaying the contribution.
The existing bounded transaction retry preserves the prepared row and native Task
identity. Ordinary terminal transitions and interrupted-indeterminate recovery
write settlement after their respective revision and lease guards succeed.
Settlement requires the matching linked row, creation time, operation and immutable
identity, with no previous settlement. A guard miss executes no contribution.
A missing row or contribution error rolls back the Task mutation. Provider dispatch,
observation, cancellation and recovery permissions keep their existing guards.
Required unbound adapters reject capability use before mutation, including recovery
with an empty Task collection.

Webhook adapters also supply typed dispatch metadata and provider associations.
`TaskDispatch` and `TaskAssociation` carry the same immutable creation identity.
The runtime compares it inside the journal transaction and accepts an existing
dispatch or association only when the new value agrees. Preparing an existing
dispatch does not authorize another provider request. Owner callbacks validate
their fields; shared SQL controls the writes and transaction scope.

Modules link their rows by `record<task>` with `REFERENCE ON DELETE CASCADE`.
Intermediate Task status stays in the kernel; catalog rows do not duplicate it.
This keeps claims, input waits and provider resume independent of catalog refresh.

The creation, transition, recovery and contribution statements live in `queries/`.
Owner, Work Context, usage, transition and input-response choices select complete static
files. Contribution variants include the complete lifecycle transaction.
Three extension templates insert one trusted repository-owned domain SQL body:
`queries/admission/domain_transaction.surql` keeps the queued, unclaimed Task guard;
`queries/provider/journal_transaction.surql` keeps the observation lease guard;
`queries/provider/resume_body.surql` inserts recovery authorization between the
claim/status/cancel checks and the waiting/request update. Domains own these bodies
and bind runtime values. The open domain body API requires these explicit composition
exceptions; reserved parameter namespaces and transaction guards still apply.
`tests/contributions.rs` uses actual declared lanes in isolated native fixtures for
idempotency, rollback, recovery, stale leases and reference cleanup.

`OwnerTaskQuery::get_many` and `get_many_in` select up to 1000 typed Task identities
with the same owner, context and operation policy as exact reads. The latter accepts
the native SurrealDB transaction, allowing an optional module's catalog selection
and typed Task hydration to share one read view. Authorization applies to that view;
a policy change after the snapshot affects the next read.

## Retention

Retention deletes terminal Tasks only after their expiry and the release of every
retention pin. Rust and Python use the same SQL predicate. Reference cascades remove
the Task's idempotency claim and input exchanges in the parent deletion transaction.
A rollback preserves all three records. Provider jobs and observations keep their
separate recovery lifetime; deleting a Task does not discard an unresolved provider
outcome. Task changefeeds keep original rows for Console's tenant-scoped deletes;
input and idempotency feeds need only committed state and deletion identities.

## Task Operation Identity

`CreateTask`, `TaskSnapshot` and Store's `TaskRecord` carry `TaskTypeName` from admission
through durable reads and native changefeed delivery. Server contract libraries own
the closed enums that implement `TaskTypeDefinition`. Shared lifecycle code accepts their names
without importing those libraries. The Console summary preserves the same typed value.
SurrealDB field adapters use the type's Serde validation; stored fields and JSON events
contain a plain string. SQL selectors bind the owning declaration's name and filter
before decoding. A malformed selected name fails decoding. Creating a domain enum or
valid name establishes neither a registered operation nor permission to execute it.

## Queued Task Admission

`commit_admission` accepts a retained Task snapshot, trusted repository SQL and bound
values. The transaction requires the same server, owner, request, recovery class,
retention pins and update timestamp. It rejects cancellation and any execution claim.
A write to the Task record serializes admission against concurrent cancellation, claims
and edits. The domain body guards its own resource state in that same transaction.
Task status, timestamps and recovery behavior are unchanged by a successful admission.

Create and pin the Task before calling this helper. The owning domain persists its
Task link and resource exclusion in the supplied body. The helper does not claim the
Task or authorize a provider effect. It never retries; an uncertain database reply
cannot authorize dispatch or release protection. `_admission_` bindings belong to the
runtime. Domain vocabulary and policy stay in the server that composes the transaction.

`tests/admission.rs` qualifies committed and rolled-back domain writes on the pinned
database, changed requests and owners, cancellation, execution claims and pin removal.
It exercises all four recovery classes without changing their stored profiles.

## Provider Observation

A `provider_wait` Task requires `claim_observation`. The ordinary execution claim
rejects it. An observation claim acquires a bounded durable lease while preserving
status, request, progress, cancellation and retention pins. Competing replicas use
the same compare-and-set transaction and only one obtains the lease. A claim records
when work was first taken by a worker; it does not certify provider dispatch.

Recovery returns expired, nonterminal provider Tasks in `provider_waiting` without
changing their state. Queued Tasks are included because the domain's persisted
intent determines whether a mutation was dispatched. Cancellation requests remain
pending. Lease expiry cannot settle provider cancellation; a current observer must
report its authoritative outcome through the normal transition transaction.

A current `provider_wait` observer may publish a known successful domain outcome
from Queued or Cancel Requested. Observation claims preserve Queued, and a cancellation
request cannot undo an effect that already completed. The qualified domain commits its
result first; this transition retains `cancel_requested_at` when cancellation raced it.
Other recovery profiles retain their existing transition rules. An expired observer
cannot use this path to settle the Task.

The domain persists operation/source/run identity and dispatch stage before side
effects. It accounts for recovery deadlines and request budgets durably. Lost
observations and exhausted budgets retain the domain resource fence. The shared
runtime does not issue provider queries, poll completion, clear a Computer fence,
retry a mutation or infer that cancellation undid an effect. Computers integration
and native fault acceptance remain separate from this shared-runtime checkpoint.

`commit_provider_journal` takes trusted static domain SQL and bound values. It checks
the current server, recovery class, lease owner and exact expiry at database time,
and writes the lease row within the transaction to fence concurrent lease changes.
The guard preserves Task status and timestamps. Dispatch commits reject committed
cancellation; observation commits may settle an already dispatched effect while
cancellation remains pending. Renewal invalidates the old lease receipt.

The domain body must compare its own operation stage and resource fence atomically.
This helper alone cannot establish single dispatch. It never retries a transaction:
a lost database reply preserves uncertainty and cannot authorize a provider effect.
Task model SQL stays in this runtime; the domain supplies only its own journal writes.

`resume_provider_journal` requires the exact current lease, Task status and update
timestamp. Its domain body must authorize a durable idempotent recovery request and
compare the paused operation epoch. A pending cancellation requires acknowledgement
of its exact timestamp. The transaction preserves that timestamp as history, changes
the Task to Waiting and emits `task.recovery_resumed` with the domain journal commit.
A cancellation committed after the read rejects the transaction. A later cancellation
records new pending intent and again prevents dispatch. Ordinary Task transitions
cannot withdraw cancellation. Resumption never supplies a provider mutation ticket.

The helper changes no stored enum or request shape. Existing recovery profiles cannot
use it. Domain adoption must qualify finite observation windows, immutable dispatch
history and request retry semantics before exposing an operator action.

Existing classes keep their behavior. Deterministic Resume work can be reclaimed,
WebhookWait work stays on its qualified webhook path, and interrupted indeterminate
execution produces its declared failure. Media retains its existing profile.

## Webhook Provider Journals

`TaskRuntime::webhooks` binds a provider to the runtime's server. Every journal
transaction checks the Task's tenant, operation, request, owner snapshot and
`WebhookWait` recovery class. The registered owner contribution must agree with
the Task in the same transaction. Provider payloads stay opaque to the runtime.

Before submission, `prepare_dispatch` requires the current execution lease and
rejects cancellation. It stores the owner's dispatch metadata and adds a provider
retention pin atomically. Only `NewlyPrepared` permits the original submission;
`AlreadyPrepared` requires recovery. A lost response cannot establish whether the
provider accepted a request or authorize another send. The domain owns the
callback credential and its verification, including callbacks received before the
submission response supplies an external job identity.

Submission binding and callback receipt enforce one provider job association for
the Task. Event identity, job identity and payload must agree on replay. The first
authenticated terminal event fixes the provider outcome. Concurrent or later
terminal events cannot replace it. A local publication error or Task cancellation
cannot rewrite that outcome; the owner may still need it for billing and recovery.
The journal's terminal-event reference rejects deletion while the job retains it.

Owner SQL reads provider facts through the Tasks-owned observation export. The
export checks job, Task, tenant and provider agreement; a terminal observation also
requires its linked event and whole payload to agree. The runtime returns opaque
provider payloads for domain decoding. Separate Task selection authorizes caller
reads before pagination, while maintenance readers use lifecycle and submission
identity checks without borrowing a caller's authority.

Task settlement runs the owner's contribution in the same transaction. A terminal
Task keeps its settled result when a late callback arrives. The dispatch retention
pin protects the Task and its owner receipt through cancellation, expiry and
unresolved provider outcomes. Release requires completed owner processing of the
provider outcome; expiry of a wait or private execution credential is insufficient.

## Result Persistence And Installation

A present Task result uses Store's `TaskResultRecord` with one required `payload`
field. Domain JSON stays inside that field, including scalars, arrays and JSON null.
Owners publish queried product fields through their declared lookup rows; shared
SQL does not interpret domain fields inside `result.payload`. An absent result
is database `NONE`; a completed JSON null is `{payload: NULL}`. Envelope
validation rejects missing or additional fields before returning a result.

Successful transitions carry an explicit `Option<ResourceUri>` alongside the opaque
result. Rust and Python persist this value atomically with settlement and expose it
through the Task snapshot. The shared runtime does not extract addresses from
domain JSON. At the MCP adapter, `mcp_task_completion` checks that a declared
`result_uri` has exactly one matching resource link. No-product results omit that
field and carry no resource link. An MCP tool error may still return a retained
product; `isError` alone does not determine whether an address exists.

Native changefeed replay decodes the same checked result envelope as direct reads.
Snapshot JSON omits an absent result and includes a present result even when its
value is null. Replay deserialization preserves that distinction. The official MCP
adapter projects object results directly and wraps other JSON values as `{"value": payload}` because the Task
protocol requires an object. That protocol projection does not alter stored results.

The Python SDK represents a present result with `TaskResult`; its payload can be JSON
null. The Store adapter binds that null with the driver's CBOR null value because the
pinned driver maps ordinary Python `None` to database `NONE`. Unsigned integers above
the signed 64-bit range use SurrealDB decimals, matching Rust's Store representation.
Result reads and event replay restore those decimals to JSON numbers. Both languages
write the same result envelope. Python's native checks cover stored
shape, independent-connection reads, replay and official Tasks projection.

Installing the result format requires an empty Task table.
Stop all Task writers and rebuild the platform database for this coordinated release.
The disposable reference installation follows its documented reset runbook. Every
reader and writer must use the same release; mixed execution is unsupported. Saved
ambiguous results are not converted. If bootstrap fails, its transaction leaves the
schema and migration history unchanged; correct the installation state and retry.

## Verification

`tests/provider_wait.rs` runs an isolated exact database with independent clients.
It proves preserved recovery state, observation-claim races, lease-fenced cancellation,
unchanged existing profiles and additive schema expansion. The shared fixture is
owned by `testing/fixtures`; its root credentials remain in process/container
configuration, and runtime clients use a scoped database editor. No environment flag
silently skips these tests. `tests/surreal_integration.rs` uses the same disposable
fixture for lifecycle, recovery, cancellation, idempotency and internal event replay.
Cases have 60-second deadlines, or 90 seconds for the result-shape matrix, and own
their cleanup. `tests/support/result_shape_cases.rs` covers store reads, internal
event replay, current-owner reconnects, JSON null, nested objects, arrays, unsigned
integer precision, malformed envelopes and the populated-store installation guard.

`tests/task_storage.rs` checks opaque inputs and unsigned metadata after reconnect,
atomic rejection of malformed controls, and disagreement between stored authority
and the owner snapshot. The SDK-owned fixture invokes the explicitly ignored
`sdk_task_storage_interop` entry in `tests/surreal_integration.rs`; missing fixture
configuration fails. Each language reads and claims the other language's Tasks,
checking the owner, input and metadata after compare-and-set admission.

A current `provider_wait` observer may update a Waiting message without changing its
status. This preserves visible recovery progress and its lease instead of inventing
an execution restart. Other recovery profiles keep their existing transition rules.

`release_observation` relinquishes an exact current receipt after its local provider
future ends. It preserves Task progress, cancellation and retention. A stale receipt
cannot release a renewed or successor lease. This avoids a full lease-expiry delay
between bounded observations handled by different replicas.

## Filtered Task Observation

Caller-facing Task collections use `TaskRuntime::for_owner(owner).page(...)`. The
query applies owner, operation and label predicates in SQL before decoding and the
page limit. Collection readers advance with the typed Task cursor.

Domains that scope Tasks by Work Context call `in_work_context()` on their owner
query. Construction rejects a caller whose owner tenant disagrees with invocation
authority. SQL requires the indexed Work Context, authority context key, and stored
request authority to agree with the caller's tenant and context. Values use driver
bindings, and finite policy choices select complete static statements. This
selection adds no membership or permission grant. Invocation admission supplies those.

`OwnerTaskQuery::get` takes a native `TaskId` and applies tenant, server, principal,
profile, optional-tenant spelling, selected operation names and label clearance in SQL.
Indexed owner and profile fields must agree with the stored owner envelope. Only a
selected record is decoded. Missing and denied Tasks have the same public response. The shared hosted
helpers accept this same query for Task get, update and cancellation admission,
preserving domain selection through the protocol adapter. Trusted internal `get`
also applies its server predicate in SQL, without adding caller authorization.

`OwnerTaskQuery::subscribe` admits up to 256 typed Task IDs in one SQL baseline and
reapplies the same owner, context and operation predicates during delivery. The shared hosted
helper uses this stream. Native replay supplies Task identities; the database then
selects current visible Tasks under the same SQL admission. Replay payloads never
supply public results or authorization. Denied Tasks are not decoded. Intermediate states may coalesce because Tasks subscriptions
observe current state; callers that require every durable transition use the trusted
internal event APIs. This API does not replace `tasks/get` as the correctness path.

`subscribe_authorized_snapshots` applies the hosted protocol's 256-handle admission
and returns the selected snapshot stream before MCP projection. Domains use it with
`authorized_snapshot` to validate retained requests and results under one SQL selection.
`subscribe_durable_tasks` composes that same stream with the shared projection. Domain
validators belong to the owning server; the runtime imports no domain result model.

The shared wake source tracks connection generations separately from write activity.
On a new LIVE connection, each public listener rereads its admitted Task identities
under current SQL owner predicates and advances its native cursor without rewinding. This restores current state even when event history expired
during the gap. Native replay overlaps that baseline and may repeat a current-state
notification. Denied rows remain outside decoding. Idle Rust listeners issue no
reconciliation queries.

`tests/support/context_query_cases.rs` exercises mismatched indexed and envelope
contexts with malformed payloads, page limits, protocol mutation admission, and
notification recovery after connection loss. Each case uses disposable Store clients
and a 60-second deadline, extended to 90 seconds for connection fault injection.

The change preserves Task storage and MCP wire models. Replace hosted replicas
together to establish the SQL selection guarantee across an installation. Preflight
retained indexed/envelope owner and profile agreement; rejected records stay intact
for operator review. Rolling back to a version that checks historical event authority
does not preserve the current-owner notification guarantee. The
[installed Stream replay](../../testing/smoke/DESIGN.md#stream-cross-replica-acceptance)
qualifies shared Task completion and result delivery through separate hosted replicas.

The Python SDK exposes the same owner-query composition through
`TaskRuntime.for_owner`, with native UUIDv7 inputs, `TaskTypeName` selections and
typed creation-time/ID page cursors. Its cancellation and input-response transactions
repeat the caller predicates. Pending-input projection uses the owner query too.
Python subscriptions register an ID-only Task LIVE source before the current-state
baseline and replay native commit pages after each wake. Every notification reads its
current Task through the owner predicate. A cursor older than six days selects a fresh
baseline on the next wake; idle readers issue no queries. Socket loss ends that reader.
A replacement subscription admits the requested IDs again and selects current state.
Trusted workers can resume native committed states with a checked versionstamp and
repeat the final transaction in full. Python Task and domain-usage mutations write only
their domain records. The MCP adapter closes readers when acknowledgement or delivery
fails, including before their first iteration.
The [Python SDK guide](../../sdk/python/README.md) documents public and trusted APIs.

## Input Responses

Caller input submission selects the Task under the owner query before decoding.
Each answer updates only an unanswered input with the matching Task and request key.
The same transaction updates the parent Task under its current owner, profile,
clearance, selected operation and optional Work Context predicates. It accepts only
Queued, Running or Waiting Tasks. A rejected parent update rolls back the answer.
Cancellation or completion cannot leave a newly accepted answer behind that guard.

Answers commit separately, preserving the protocol's per-input deduplication.
Concurrent submissions accept one answer; duplicates and unknown keys leave the
existing response and Task timestamp unchanged. Recognized transaction failures
use the runtime's retry policy, capped at eight attempts. Store's shared transaction
error selection preserves the rejection cause instead of retrying a guard rejection
hidden by an earlier unexecuted statement. Trusted workers use
`TaskRuntime::submit_input_responses`, which checks server and status without caller
selection. Both paths share the transaction and input-identity checks.

`tests/input_responses.rs` exercises the public owner query and MCP update adapter
against disposable stores. Database events inject authority and status changes
between the input write and parent guard, and assertions check full rollback.
Separate clients compete for one answer on RocksDB. Other cases cover denied
malformed Tasks, cancellation, completion and mismatched input keys or parent links.

## Task-Backed Resource Observation

`listen_durable_subscriptions` combines native Task observation with domain resource
hubs. After Task admission and resource receiver registration, it sends initial
invalidations for the accepted resource URIs and resource catalog filter. Consumers
can then read their baseline while subsequent hub changes remain queued for the listener.

`TaskResourceSubscriptions` checks one request's typed resource addresses and native
Task handles. It admits at most 256 Task handles, 256 resource addresses and 256 distinct
backing Tasks. Duplicate addresses coalesce. A domain address supplies its backing Task
through the foundational `TaskResourceAddress` trait; this library owns no URI routes.
Domains apply additional resource admission before starting the listener.

The listener authenticates through `DurableTaskService` and subscribes once to the
union of explicit Tasks and resource backing Tasks. A resource with a missing or
denied Task rejects the subscription. Unknown explicit Task handles follow the
ordinary Tasks admission policy. Task status notifications require an explicit Task
subscription; a resource-only request receives only resource invalidations. Both
signals use the current-owner SQL watch and the filter-enforcing RMCP sink.

The shared native changefeed cursor, LIVE wake and reconnect baseline supply recovery. No domain-local broadcaster participates. Reconnection
invalidates each admitted resource from current Task state, including a completed
Task. Intermediate transitions can coalesce. Cancellation or a query error drops
the request's watch; the shared LIVE source stops after its last listener leaves.
This source covers Task-backed resources only. Other domain change sources keep their
declared observation contracts.

`tests/task_resources.rs` qualifies independent Store clients, mixed and resource-only
filters, reconnect baselines, a broken TCP connection with deleted event history,
SQL exclusion of malformed revoked rows, and official
RMCP notification and cancellation behavior. The RMCP fixture uses an in-memory
transport and establishes no installed HTTP or GPU qualification.

The installed Stream probe pins dispatch and observation to separate Pods of one image.
It requires a working-state subscription baseline before accepting completion and its
resource invalidations. Completed Task envelopes and typed result reads must agree
across both replicas. A new subscription must recover the completed state, and both
requests must cancel. This case also runs the owning GPU replay and Artifact checks;
other domains keep their provider-specific acceptance requirements.

`runtime/usage` requires an explicit `TaskUsageAccess` policy for usage collections and exact
Task usage reads. Both the usage row and linked Task must match this runtime's server
and the caller's tenant. The Task's principal, profile, optional tenant spelling, and
complete required label set must match the caller's authority. Stored record fields
and owner-envelope identities must agree with the supplied owner. Missing Task parents
provide no access. Task visibility is rechecked on every read.

`usage_page` groups matching usage rows into distinct native Task IDs before
ordering and selecting at most 1,001 IDs. The requested limit is 1–1,000; the extra
ID establishes continuation. `usage` applies the same selection for an exact
Task. `task_visible` can admit a subscription before that Task produces its
first usage row. `TaskUsageAccess::Owner` applies `TaskOwner::allows`.
`TaskUsageAccess::WorkContext` adds agreement between the caller's Work Context,
the Task's indexed context record, retained authority context key, and owner-envelope
context and tenant. It rejects a caller whose authority tenant conflicts with its owner
tenant. Each predicate runs before grouping or limits. Neither policy establishes
Work Context membership or permissions; invocation admission supplies that authority.
Domains choose their policy in their library reader. The shared runtime contains no
server vocabulary.

The runtime owns these authority queries. Domain libraries own their collection
responses, cursor envelopes, and resource address profiles. A cursor is a position
and never grants the originating caller's permissions to its next reader.

`runtime/task_pages` serves caller-owned collections in pages of at most 1,000
Tasks. The database applies server, tenant, principal, profile, data-label and
task-type filters before the limit. The optional tenant in the stored owner must
also match, distinguishing installation-wide authority from an explicit tenant.
Ordering uses creation time and Task ID together, preserving positions when Tasks
share a timestamp. The query reads one extra record to establish continuation.
Each page rechecks the caller's authority; a cursor carries only a position.
Concurrent deletions and retention may remove items between reads, and new Tasks
may appear after that position. The API does not promise a frozen snapshot.
Domains with additional Work Context restrictions own their narrower queries.

`tests/subscriptions.rs` qualifies collection limits against an isolated database,
including excluded malformed envelopes, timestamp ties and cursor reuse by another
caller.

Native Task subscription baselines select only their requested record IDs. Public
updates decode native change identities in Store, then select the current rows with
owner, context and operation predicates in SQL. Intermediate states may coalesce.
One projected LIVE source per runtime wakes all listeners. It closes after the last
listener leaves and supplies a current-state baseline after every source reconnect.

Trusted worker streams replay committed Task states through `runtime/history.rs`.
Their typed cursors carry native versionstamps. Resuming repeats the final transaction,
including every Task in that transaction; consumers must tolerate repeated states.
A cursor older than the six-day recovery safety window requests current state instead.
This stream is internal and does not establish public read permission. Python Task
observation and the remaining event writers are tracked in the [consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md).

`tests/subscriptions.rs` owns a disposable real Store and separate connections. It
qualifies concurrent listeners, cross-replica completion, reconnection, excluded
unauthorized IDs and an unrelated malformed envelope that must not be decoded.
It also qualifies exact owner reads, indexed/envelope disagreement, optional tenants,
revocation after admission and a full replay page of denied malformed events.
`tests/support/owner_query_cases.rs` adds owner-visible malformed rows of excluded
operation types. It checks exact reads, limits, multiple selected types, subscription
admission, operation changes during delivery and Store reconnect after event removal.

## Trusted Lifecycle Reads

The Tasks schema exports `fn::kernel::tasks::lifecycle_v1` for owner maintenance.
It selects an exact Task identity with server, tenant and operation constraints.
The returned object includes lifecycle dates, status, recovery and retention data,
and typed ownership links. A supplied complete result payload yields only a
`result_matches` boolean. This function supplies no caller authorization and
returns no input, result or provider payload. `tests/task_storage.rs` qualifies
wrong identities and whole-result agreement on fresh kernel lanes.

## Controlled Storage Envelopes

Store owns the failure record used by transition, interruption recovery and webhook
settlement. It requires string code and message fields and permits opaque JSON details.
Domain codes remain extensible. An omitted details field differs from an explicit JSON
null through Rust and Python reads and writes. Native decoding rejects database record
identities and timestamps inside JSON payloads.

Input-request storage requires method and parameter-object fields. Construction and
retained decoding apply the same method admission: one to 256 UTF-8 bytes with no control
characters. Parameters and responses preserve their protocol-owned JSON objects.
The result envelope requires its payload while permitting any JSON value, including null.
These envelopes belong to the fresh coordinated schema; all Task writers and readers
must use the current schema together. The SDK-owned native fixture qualifies Rust and
Python storage in both directions without building another database harness.
