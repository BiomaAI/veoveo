# Computers Domain

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo identity and Work Context | Canonical TaskOwner authority, named user/service principals, tenant and context isolation; current implementation admits private ownership |
| SurrealDB / SurrealQL 3.2.4 | Existing qualified platform client/server pin; schema-full records, atomic multi-record admission and outbox, conflict-only bounded transaction retry |
| Veoveo Computers JSON | Public DTOs live in `contract/`; provider identities and persisted authority remain internal |
| Shared Tasks | Lifecycle integration is in progress; reservation does not dispatch a provider mutation |

The domain owns retained Computer identity. Provider transport belongs to
`platform/runtimes/computers`. Native Console and MCP will project these commands
through a single Computers worker service. No provider dependency enters the gateway.

## Admission And Ownership

Private ownership binds tenant, principal identity, profile and Work Context.
Changing a display name does not change identity. User and service principals use
the same admission rules. A gateway-verified TaskOwner is an internal trust boundary;
it is never deserialized from a public Computer request. Current caller data labels
must cover the stored authority labels before a record can be read.

Create reserves one immutable Computer, template fingerprint and provider instance.
The request UUID is scoped to the owner/context; its canonical fingerprint detects
changed-input reuse. Three quota rows serialize owner, tenant and provider admission
across replicas. They count retained Computers, including stopped and unresolved work.
Reducing a quota preserves existing records and allows exact retries to find their
original reservation. Runtime code cannot silently reset these counters.

Owner quota spans profiles and Work Contexts. Quota policy lives in the shared store;
installation administration changes it with compare-and-set. Every new reservation
writes the policy row alongside its counters, so a concurrent limit reduction forces
transaction conflict and re-evaluation. A replica cannot admit against cached limits.

Collection reads use a bounded UUID cursor. They do not expose another owner's rows.
The schema records process identity and an active operation fence for the following
lifecycle checkpoint. The retained Computer UUID is also the home allocation identity.

## Delivery Boundary

The current checkpoint implements private collection admission, operation admission
and shared Task linking. Shared
Task dispatch/recovery, explicit delegation, renewable attachments, deletion with
storage acknowledgement, agent execution and Artifact movement remain active work in
`docs/COMPUTERS_PLAN.md`. Reservation alone is not a usable or deployed Computer.

## Verification

Rust tests run against two independent clients of an isolated SurrealDB 3.2.4 server.
They exercise racing reservations, exact retries, changed inputs, quotas above one,
tenant/context/principal isolation and quota reduction. This evidence establishes
durable admission; native provider and installed journeys require separate checks.

The skill's suggested SQL formatter, `@surrealdb/surql-fmt` 0.1.0-beta.2, has no
stable release and corrupted a CREATE CONTENT expression during adoption. Its output
was rejected by the qualified database validator. Query formatting is reviewed
manually until a formatter passes syntax and semantic preservation checks.

## Operation Journal

`queue_operation` commits the request fingerprint, immutable operation identity, original
provider/resource/run state, Computer fence and audit event in one transaction. An exact
retry returns the original operation; a changed action rejects the reused request ID.
A different accepted request cannot replace an active operation. Create, Start and Stop
require their respective admitted source phases. Mutations require current contributor
membership and clearance for the context output labels before consuming capacity or
creating a fence. The final facades must also enforce their explicit action policy.

`ensure_operation_task` links the accepted journal record to the shared Task whose UUID
matches the operation UUID. This is a recoverable second step, not a cross-component
atomic transaction. A crash between the steps leaves the Computer fenced and retains
all inputs needed to recreate the same Task. The Task carries the `provider_wait`
profile and a retention pin. Concurrent link attempts use the shared Task idempotency
boundary. Actor identity, Work Context and policy provenance remain in the journal;
command text and provider credentials never enter its audit event.

The dispatch worker must repair pending Task links on startup, persist dispatch stage
before provider effects, and settle the domain before projecting a terminal Task.
Those worker paths, recovery budgets and physical home fencing remain in progress.
No journal method in this checkpoint dispatches a provider mutation or clears a fence.
