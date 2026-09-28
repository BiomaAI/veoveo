# Durable Task Runtime

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28`, Tasks SEP-2663 | Official RMCP Task projection; internal recovery classes introduce no new MCP status or method |
| SurrealDB / SurrealQL `3.2.4` | Shared durable Task records, lease compare-and-set, transactional outbox and additive schema migrations |
| Veoveo Work Context | Canonical TaskOwner/InvocationAuthority, tenant and server ownership, retained result pins |
| Internal recovery-class vocabulary | `resume`, `webhook_wait`, `provider_wait`, `interrupted_indeterminate`; domain-qualified completion semantics |
| Native Task identity | `veoveo_types::TaskId` carries UUID identity; external runtime lookups require UUIDv7. MCP opaque handles have their own protocol profile. |

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
`runtime/owner_reads` owns SQL Task-owner selection and bindings;
`runtime/owner_subscriptions` delivers the selected current state to public listeners.

Consumers import native `TaskId` directly from `veoveo-types`. Store's `task_record_id`
function performs the database conversion at bindings. TaskRuntime owns Task lifecycle
and external lookup admission; public identity consumers do not depend on this runtime
or Store. UUID generation, serialization, persisted UUID keys and admission profiles
are unchanged by this ownership split. It requires no data conversion or deployment drain.

Subscription baselines read the newest available outbox sequence through the
sequence index in reverse order. The available-time index would scan and sort
historical events before applying the limit. A native query-plan regression checks
the reverse scan and exclusion of future events against the pinned database.

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

## Migration And Rollout

Migration 0052 adds `provider_wait` to the stored enum and preserves every existing
row and class. Deploy compatible shared-store readers and Task workers before
admitting the new class. Older readers cannot deserialize it, including on retained
terminal Tasks. A mixed rollout may run old components only while new-class admission
is closed. Computers admission opens after the compatible rollout has converged.

Keep the expanded schema and compatible readers during application rollback. A
rollback to older Task readers requires retiring all new-class records through the
normal retention procedure, including their pins, or an explicit qualified data
migration. Draining active Tasks alone is insufficient. No automatic destructive
downgrade or reclassification is provided.

## Verification

`tests/provider_wait.rs` runs an isolated exact database with independent clients.
It proves preserved recovery state, observation-claim races, lease-fenced cancellation,
unchanged existing profiles and additive schema expansion. The shared fixture is
owned by `testing/fixtures`; its root credentials remain in process/container
configuration, and runtime clients use a scoped database editor. No environment flag
silently skips these tests. `tests/surreal_integration.rs` uses the same disposable
fixture for lifecycle, recovery, cancellation, idempotency and internal event replay.
Each case has a 60-second deadline and owns its cleanup.

A current `provider_wait` observer may update a Waiting message without changing its
status. This preserves visible recovery progress and its lease instead of inventing
an execution restart. Other recovery profiles keep their existing transition rules.

`release_observation` relinquishes an exact current receipt after its local provider
future ends. It preserves Task progress, cancellation and retention. A stale receipt
cannot release a renewed or successor lease. This avoids a full lease-expiry delay
between bounded observations handled by different replicas.

## Filtered Task Observation

`get_for_owner` takes a native `TaskId` and applies tenant, server, principal,
profile, optional-tenant spelling and label clearance in SQL. Indexed owner and
profile fields must agree with the stored owner envelope. Only a selected record
is decoded. Missing and denied Tasks have the same public response. The shared hosted
helpers use this query for Task get, update and cancellation admission. Trusted internal `get`
also applies its server predicate in SQL, without adding caller authorization.

`subscribe_for_owner` admits up to 256 typed Task IDs in one SQL baseline and
reapplies the same current-owner predicates during delivery. The shared hosted
helper uses this stream. Outbox pages select only event sequence and Task identity;
the database then selects current visible Tasks. Historical event payloads never
supply public results or authorization. Pages advance past denied events without
decoding their payloads. Intermediate states may coalesce because Tasks subscriptions
observe current state; callers that require every durable transition use the trusted
internal event APIs. This API does not replace `tasks/get` as the correctness path.

The change preserves Task storage and MCP wire models. Replace hosted replicas
together to establish the SQL selection guarantee across an installation. Preflight
retained indexed/envelope owner and profile agreement; rejected records stay intact
for operator review. Rolling back to a version that checks historical event authority
does not preserve the current-owner notification guarantee. Installed rollout and
cross-replica qualification are tracked in the foundations plan.

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
