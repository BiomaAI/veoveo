# Durable Task Runtime

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28`, Tasks SEP-2663 | Official RMCP Task projection; internal recovery classes introduce no new MCP status or method |
| SurrealDB / SurrealQL `3.3.0` | Shared durable Task records, lease compare-and-set, transactional outbox and additive schema migrations; the Python port uses the pinned SurrealDB Python SDK `2.0.0` |
| Veoveo Work Context | Canonical TaskOwner/InvocationAuthority, tenant and server ownership, retained result pins |
| Internal recovery-class vocabulary | `resume`, `webhook_wait`, `provider_wait`, `interrupted_indeterminate`; domain-qualified completion semantics |
| Native Task identity | `veoveo_types::TaskId` carries UUID identity; external runtime lookups require UUIDv7. MCP opaque handles have their own protocol profile. |
| Internal Task result and event format | Store results have one required `payload` field. Task outbox schema 3 preserves absent and JSON-null results as distinct states. |

This library is the shared Task authority used by hosted domain services. Public
handlers delegate protocol projection to the official RMCP types and the shared
service adapter. The library does not authorize provider side effects by itself.

## Ownership

`runtime` owns creation, state transitions, subscriptions and retention. `leases`
owns execution and observation claims. `recovery` applies each declared restart
profile. `types` validates durable envelopes; `mcp` and `service` project them into
the hosted server contract. Provider SDKs and resource-specific fences belong to
the owning domain.
`provider_transaction` composes a domain journal write with the exact shared
observation-lease receipt in the same database transaction.
`provider_resume` composes explicit domain recovery with a Task status transition.
`admission` composes a domain admission with the exact queued, unclaimed Task.
`runtime/owner_query` owns `OwnerTaskQuery`, built by `TaskRuntime::for_owner`.
The builder holds the current owner, an optional checked Work Context selection,
and an optional selection of 1–32 validated `TaskTypeName` values. Omitting the
operation selection admits every operation hosted by the
runtime server; an empty explicit selection is an error. `get`, `page` and `subscribe`
apply the same selection in SQL before decoding. Subscription updates and Store
reconnect baselines preserve it. Shared code contains no server operation variants.
`runtime/context_scope` owns the Work Context predicates and typed scalar bindings
shared by Task observation and linked usage reads.
`runtime/owner_reads` owns SQL Task-owner selection and bindings;
`runtime/owner_subscriptions` delivers the selected current state to public listeners.
`resource_subscriptions` maps that authorized Task stream to requested Task status
and resource invalidations using domain-owned `TaskResourceAddress` implementations.

Consumers import native `TaskId` directly from `veoveo-types`. Store's `task_record_id`
function performs the database conversion at bindings. TaskRuntime owns Task lifecycle
and external lookup admission; public identity consumers do not depend on this runtime
or Store. UUID generation, serialization, persisted UUID keys and admission profiles
are unchanged by this ownership split. It requires no data conversion or deployment drain.

Subscription baselines read the newest available outbox sequence through the
sequence index in reverse order. The available-time index would scan and sort
historical events before applying the limit. A native query-plan regression checks
the reverse scan and exclusion of future events against the pinned database.

## Task Operation Identity

`CreateTask`, `TaskSnapshot` and Store's `TaskRecord` carry `TaskTypeName` from admission
through durable reads and outbox delivery. Server contract libraries own the closed
enums that implement `TaskTypeDefinition`. Shared lifecycle code accepts their names
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

## Result Persistence And Installation

A present Task result uses Store's `TaskResultRecord` with one required `payload`
field. Domain JSON stays inside that field, including scalars, arrays and JSON null.
SQL readers and indexes address domain fields beneath `result.payload`. An absent
result is database `NONE`; a completed JSON null is `{payload: NULL}`. Envelope
validation rejects missing or additional fields before returning a result.

Task outbox events use the shared `TASK_EVENT_SCHEMA_VERSION`, currently 3. Snapshot
JSON omits an absent result and includes a present result even when its value is null.
Replay deserialization preserves that distinction. The official MCP adapter projects
object results directly and wraps other JSON values as `{"value": payload}` because the Task
protocol requires an object. That protocol projection does not alter stored results.

The Python SDK represents a present result with `TaskResult`; its payload can be JSON
null. The Store adapter binds that null with the driver's CBOR null value because the
pinned driver maps ordinary Python `None` to database `NONE`. Unsigned integers above
the signed 64-bit range use SurrealDB decimals, matching Rust's Store representation.
Result reads and event replay restore those decimals to JSON numbers. Both languages
write the same result envelope and event version. Python's native checks cover stored
shape, independent-connection reads, replay and official Tasks projection.

Store schema 99 requires an empty Task table and no Task outbox events before installing the result format.
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
bindings; predicate composition accepts only repository-owned fragments. This
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
helper uses this stream. Outbox pages select only event sequence and Task identity;
the database then selects current visible Tasks. Historical event payloads never
supply public results or authorization. Pages advance past denied events without
decoding their payloads. Intermediate states may coalesce because Tasks subscriptions
observe current state; callers that require every durable transition use the trusted
internal event APIs. This API does not replace `tasks/get` as the correctness path.

`subscribe_authorized_snapshots` applies the hosted protocol's 256-handle admission
and returns the selected snapshot stream before MCP projection. Domains use it with
`authorized_snapshot` to validate retained requests and results under one SQL selection.
`subscribe_durable_tasks` composes that same stream with the shared projection. Domain
validators belong to the owning server; the runtime imports no domain result model.

The shared wake source tracks connection generations separately from write activity.
On a new LIVE connection, each public listener rereads its admitted Task identities
under current SQL owner predicates and advances to the current event tail without
rewinding its cursor. This restores current state even when event history expired
during the gap. Denied rows remain outside decoding. The 15-second recovery timer
checks retained activity but does not emit unchanged state on an idle connection.

`tests/support/context_query_cases.rs` exercises mismatched indexed and envelope
contexts with malformed payloads, page limits, protocol mutation admission, and
notification recovery after events expire. Each case uses disposable Store clients
and a 60-second deadline, extended to 90 seconds for connection fault injection.

The change preserves Task storage and MCP wire models. Replace hosted replicas
together to establish the SQL selection guarantee across an installation. Preflight
retained indexed/envelope owner and profile agreement; rejected records stay intact
for operator review. Rolling back to a version that checks historical event authority
does not preserve the current-owner notification guarantee. Installed rollout and
cross-replica qualification are tracked in the foundations plan.

The Python SDK exposes the same owner-query composition through
`TaskRuntime.for_owner`, with native UUIDv7 inputs, `TaskTypeName` selections and
typed creation-time/ID page cursors. Its cancellation and input-response transactions
repeat the caller predicates. Pending-input projection uses the owner query too.
Python subscriptions use their owned LIVE reader for wakeups and reread admitted IDs
every 15 seconds to cover expired events; this reconciliation may repeat unchanged
state. A terminated LIVE source ends that reader. A replacement subscription admits
the requested IDs again and selects a fresh baseline. The MCP adapter closes readers
when acknowledgement or delivery fails, including before their first iteration.
The [Python SDK guide](../../sdk/python/README.md) documents public and trusted APIs.

## Task-Backed Resource Observation

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

The existing outbox sequence, LIVE wake, 15-second reconciliation and reconnect
baseline supply recovery. No domain-local broadcaster participates. Reconnection
invalidates each admitted resource from current Task state, including a completed
Task. Intermediate transitions can coalesce. Cancellation or a query error drops
the request's watch; the shared LIVE source stops after its last listener leaves.
This source covers Task-backed resources only. Other domain change sources keep their
declared observation contracts. Phase 5 owns the outbox-to-change-feed migration.

`tests/task_resources.rs` qualifies independent Store clients, mixed and resource-only
filters, reconnect baselines, a broken TCP connection with deleted event history,
SQL exclusion of malformed revoked rows, and official
RMCP notification and cancellation behavior. The RMCP fixture uses an in-memory
transport and establishes no installed HTTP or GPU qualification.

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

Native Task subscription baselines select only their requested record IDs.
Outbox replay filters those aggregate identities and the hosted server in
pages of 256. One projected LIVE source per runtime wakes all listeners; it carries only
a sequence. Sources close after the last listener leaves. A new source wakes every
reader after establishment, covering the baseline and reconnection races. Each
listener retains its own durable cursor. Fifteen-second reconciliation covers a
missed wake and never invokes a provider or model. General server-owned consumers
retain the complete durable stream APIs.

`tests/subscriptions.rs` owns a disposable real Store and separate connections. It
qualifies concurrent listeners, cross-replica completion, reconnection, excluded
unauthorized IDs and an unrelated malformed envelope that must not be decoded.
It also qualifies exact owner reads, indexed/envelope disagreement, optional tenants,
revocation after admission and a full replay page of denied malformed events.
`tests/support/owner_query_cases.rs` adds owner-visible malformed rows of excluded
operation types. It checks exact reads, limits, multiple selected types, subscription
admission, operation changes during delivery and Store reconnect after event removal.
