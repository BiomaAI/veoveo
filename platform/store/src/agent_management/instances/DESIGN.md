# Managed Agent Lifecycle Persistence

Status: implementation in progress. This boundary supplies durable admission and
controller fencing for phase 3 of the agent management plan.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| SurrealDB `3.2.4` | Migration 0090, typed bindings, transactions and the platform outbox |
| Veoveo management | Current context authority, UUIDv7 mutation identities, immutable definition revisions and expected-generation updates |
| OAuth 2.0 and private-key JWT | A durable registration binds one instance, service principal, approved scopes and public RSA JWK; private keys remain in Kubernetes Secrets |
| Kubernetes | Recorded resource names precede external provisioning; the controller owns resource-version and generation checks at the API boundary |

## Admission And Ownership

The gateway supplies a trusted provisioning plan after validating the current
installation template, model and deployer's delegated authority. Public request
fields cannot select workload images, namespaces, credential destinations or roles.
The transaction reserves retained instance and storage capacity, creates its service
principal and registration, and records one lifecycle operation. An existing identity
or client ID causes a conflict. Installation migration must use a separate adoption path.

Instance owners and admitted context managers can inspect and control the instance.
Every HTTP mutation also requires its exact gateway action. Current tenant, principal
and context checks use the definition registry's authority fence. An operation receipt
binds the caller and canonical request; retries return its accepted result, including
after a later generation has superseded it.

Archive retains the storage reservation because memory remains allocated. It closes
registration and dispatch immediately. Definition disable is checked when resolving
the registration and before execution, including for retained revisions. Pausing
closes new episode admission while an admitted episode may drain. No lifecycle event
starts a model call.

## Reconciliation And Execution

Desired generation and active generation are separate. An update requests a drain;
the controller activates the new revision only after the old runtime lease is clear.
The operation records each provisioning phase and the exact resource identities.
Controller claims have expiring leases and monotonically increasing fences. A stale
claim cannot publish observations or activate a generation.

The kernel checks its active generation and dispatch epoch before model and tool
dispatch. Stop increments the epoch without changing the admitted revision. A later
episode captures the new epoch. Archive and emergency disable deny both old and new
dispatch. Task completion observation retains its own authority and does not depend
on model execution.

Only public key material enters the registration. Resource observations contain
typed IDs, phases and bounded diagnostics. They cannot carry Kubernetes Secret
payloads, provider keys or arbitrary objects into management responses or outbox
events. The effective identity resolver must reject collisions between static and
managed registrations and recheck this record for already-issued tokens.

Mutation receipts can be recovered before repeating external configuration validation.
Replay still checks the current actor and Work Context and rejects a changed payload.
Management event heads include only instance changes visible to the current owner or
context manager. Definition and lifecycle invalidations share the existing browser stream.
