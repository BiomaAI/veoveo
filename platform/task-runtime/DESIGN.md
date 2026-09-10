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

The domain persists operation/source/run identity and dispatch stage before side
effects. It accounts for recovery deadlines and request budgets durably. Lost
observations and exhausted budgets retain the domain resource fence. The shared
runtime does not issue provider queries, poll completion, clear a Computer fence,
retry a mutation or infer that cancellation undid an effect. Computers integration
and native fault acceptance remain separate from this shared-runtime checkpoint.

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
