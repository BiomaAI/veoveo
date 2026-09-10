# Durable Task Runtime

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28`, Tasks SEP-2663 | Official RMCP Task projection; internal recovery classes introduce no new MCP status or method |
| SurrealDB / SurrealQL `3.2.4` | Shared durable Task records, lease compare-and-set, transactional outbox and additive schema migrations |
| Veoveo Work Context | Canonical TaskOwner/InvocationAuthority, tenant and server ownership, retained result pins |
| Internal recovery-class vocabulary | `resume`, `webhook_wait`, `provider_wait`, `interrupted_indeterminate`; domain-qualified completion semantics |

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
silently skips these tests. The older `surreal_integration` suite retains its explicit
environment gate and must be reported separately when it is not enabled.

A current `provider_wait` observer may update a Waiting message without changing its
status. This preserves visible recovery progress and its lease instead of inventing
an execution restart. Other recovery profiles keep their existing transition rules.

`release_observation` relinquishes an exact current receipt after its local provider
future ends. It preserves Task progress, cancellation and retention. A stale receipt
cannot release a renewed or successor lease. This avoids a full lease-expiry delay
between bounded observations handled by different replicas.
