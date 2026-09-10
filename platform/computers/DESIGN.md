# Computers Domain

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo identity and Work Context | Canonical TaskOwner authority, named user/service principals, tenant and context isolation; current implementation admits private ownership |
| SurrealDB / SurrealQL 3.2.4 | Existing qualified platform client/server pin; schema-full records, atomic multi-record admission and outbox, conflict-only bounded transaction retry |
| Veoveo Computers JSON | Public DTOs live in `contract/`; provider identities and persisted authority remain internal |
| Shared Tasks | Current observation leases guard domain journal transactions; native dispatch and Task result projection belong to `servers/computers-mcp` |
| Veoveo internal `request_context` | Required verified source principal and token metadata for new operation admission; accepted execution evidence contains no bearer secret |
| Veoveo `computer_attach` and session-grant ledger | Resource-scoped interactive authority, one-use browser tickets and bounded renewal; private storage profile introduced by migration 0058 |

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
and shared Task linking, dispatch receipts, durable observation budgets and correlated
domain settlement. The native worker lives in `servers/computers-mcp`. The domain now
retains browser grants and checks their renewal. Public terminal composition, explicit
agent delegation, CLI pairing, deletion with
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

Migration 0056 makes verified execution authority part of every new operation.
`ComputerActor` can only be constructed from a verified Computers identity with
request context. Admission checks the assertion and source-token expiration; the
transaction rechecks its deadline before accepting intent. The journal retains
the actor, profile, invocation and verified source context. Decoding checks that
this evidence agrees with the Task owner. It never reconstructs missing client or
session identity from a Task owner.

The accepted record is evidence for one operation. Its source token may expire
while the worker completes or reconciles accepted work. Dispatch reads current action
policy through the shared evaluator. Browser grants have separate current grant and
family checks, described below. Their transport composition remains implementation work.

No supported installed Computers records precede this migration. Pre-profile
candidate journals with missing context cannot admit new work or infer authority;
they require explicit operator handling with their existing resource fence intact.
The deployed profile must update all readers and writers before admitting Computers.

The dispatch worker repairs pending Task links and projects a terminal Task after
domain settlement. Physical home fencing remains in progress.
Journal methods never call the provider.

## Dispatch And Observation

`authority_snapshot` loads and verifies the current catalog and directory records
once. Dispatch and public control use that same evaluator. A public request obtains
`ControlAuthority`, whose lifetime also ends with its admission token; it has no
dispatch rights. Current and accepted Work Context membership both constrain action
flags. Resource reads use the canonical Computer URI policy and separately enforce
ownership and retained labels. Each new request reads current authority again.
Accepted background execution retains its separate token-lifetime rule below.

`control_session` reads the exact signed browser family from the shared store and uses
the same pure binding predicate as the gateway. Policy, directory and family reads
share one five-second deadline. `ControlAuthority::valid_until` exposes the earliest
of the original assertion/token expiry, known family expiry and current-policy lease.
A missing, revoked or mismatched family denies control. Tokens without a family keep
their admitted request lifetime and gain no session-bound renewal rights. Background
dispatch continues to use its independent accepted execution authority.

## Browser Access Grants

`session_grants` owns browser ticket issuance, one-use redemption, renewal and
revocation. The installation selects `SessionGrantPolicy`: a per-Computer grant limit,
an absolute lifetime and an idle lifetime. A compare-and-set policy transition prevents
an old replica from restoring previous settings. Zero grants closes new admission and
renewal. Active tickets and connections share the limit. Expired tickets and revoked or
expired families consume no admission slot.

Admission requires current contributor membership, `computer_attach` and resource-read
permission on the exact Computer URI. `computer_attach` has no MCP method. Lifecycle
tool permission supplies no interactive access. The caller must own the Computer and
retain clearance for its labels. The Computer must be Ready without an active operation.
The transaction verifies the selected control revision, enabled identities, live family,
current limits and exact provider resource/process before recording the grant and outbox
event. Concurrent admissions serialize through the provider policy and Computer guard.

A ticket contains a grant UUID and 256 random bits. Storage keeps only a domain-separated
SHA-256 hash. Redemption requires the same authenticated owner, profile, OAuth client,
Work Context and browser family. Its transaction consumes the ticket once across replicas.
The ticket expires within thirty seconds. Lost delivery requires a fresh ticket; no
recoverable bearer is retained. A redeemed handle cannot be constructed from public JSON.

The service must obtain a successful `SessionGrantLease` before opening provider I/O.
Each renewal rereads the grant, current installation limits, family, policy and directory.
It also compares the Computer's exact process with the admitted run. Stop/Start requires
a fresh connection. The accepted token may expire while its family and grant remain
valid. Logout, replay revocation or family expiry ends access without stopping processes.
An owner can revoke a grant from another authenticated browser family.

Every grant read has a five-second budget. The resulting monotonic lease lasts at most
thirty seconds from the start of the read, capped by database-relative absolute, idle and
family expiry. The transport renews within ten seconds and closes on failed renewal.
Database latency consumes the lease. Timer ticks and output leave idle expiry unchanged;
accepted terminal input may extend it. Activity cannot extend absolute lifetime or revive
an expired grant. Tightened current limits further restrict an existing grant. Transport
cleanup revokes only its exact consumed connection and changes no Computer lifecycle state.

Migration 0058 adds private schema-full tables and leaves existing Computer and operation
rows intact. Install every policy reader that recognizes `computer_attach` before activating
a revision using it. An older service cannot resume these grants. Rollback drains attachment
admission and closes existing access through the bounded leases before replacing readers;
retain the ledger for revocation and expiry. Grant rollback never removes a retained home.

Six real-store cases use independent clients and synthetic Ready rows. They prove private
redemption, one-use races, quota contention, stale installer rejection, passive idle behavior,
absolute expiry, logout, current action/label/membership changes and process replacement.
They do not establish terminal behavior, public routing, CLI pairing or agent delegation.

## Lifecycle Dispatch

The public Create request resolves its first reservation before selecting a new
installation default. The original template and provider remain attached to that
request. `capacity_for` supplies a current quota hint using the same usage keys as
reservation; its result cannot reserve space or replace transactional admission.

`begin_dispatch` commits a unique dispatch ID before returning a non-cloneable ticket.
Only the queued stage can obtain it. The worker prepares the retained home, then
the domain checks current action authority. It records the original provider/resource/process
identity and a 180-second observation deadline. Losing the ticket or its reply cannot
authorize another dispatch. The active Computer fence remains held.

`current_authority.rs` reads the active immutable control revision, verifies its
normalized typed-document SHA-256, and uses `platform/policy` for the exact lifecycle
tool. Current client, authorization server, profile audience, scopes and invocation
mode must still admit the accepted identity. Current Work Context membership must
permit mutation. Both retained output labels and current output defaults require
clearance. The installation, tenant, source principal and actor must exist with
their recorded bindings and remain enabled. No positive policy or account result is
cached. The complete read has a five-second deadline.

Migration 0057 records the current policy revision and decision with the dispatch
and its outbox event. The journal transaction rechecks the selected policy pointer
and current account enablement under the exact Task lease. The non-cloneable ticket
expires monotonically thirty seconds after the authority read began; database and
policy work consume that same interval. Provider submission must fit within it.
Storage adapters cannot provide or replace an action-policy decision.

Current platform policy and directory state govern this check. Group, role and
assurance claims retain their authenticated source snapshot; an external IdP change
without a platform signal is not a committed platform revocation. Source-token
expiry or browser logout does not cancel an already accepted operation. Neither fact
permits a new attachment. Observing an already dispatched effect also remains allowed
after policy removal, because accurate settlement must retain its original identity.

Queued rows may have no dispatch decision. A dispatched candidate row without one
cannot be read as qualified execution evidence or repaired by inventing a decision.
Update all Computers readers and workers before admission; no supported deployed
Computers rows precede this profile. Prior candidate fences require explicit handling.

`admit_observation` charges each reconciliation read before it can reach the provider.
Eight reads share the original persisted deadline. Exponential delay with deterministic
jitter uses the stored count, and replicas cannot reset that schedule. A ticket's local
remaining time derives from database time, deducts read latency and expires monotonically.
Each reconciliation call gets at most ten seconds. Exhausting either bound records
Recovery Required once and retains the Computer fence. An unavailable observer cannot
produce a terminal failure or silently reopen the operation.

Settlement requires a matching dispatch or observation ticket under a current shared
Task lease. The provider adapter checks full binding labels before producing its internal
outcome. The domain rechecks provider, Computer, template and recorded source identity.
Start requires a new process on the same resource; Stop requires the recorded process.
Operation success, the resulting Computer phase, fence release and audit event commit
together. Shared Task publication follows as a recoverable projection and never precedes
that domain commit.

Conditions that acquire a fence or capacity are predicates on the actual UPDATE/UPSERT.
An earlier SELECT is insufficient: the real-store contention fixture exposed two
different requests passing a preceding read before one overwrote the other's fence.
Quota increments, policy changes, dispatch, read budgets and settlement follow the same
conditional-update rule. Aborted transactions preserve their previous rows and counters.

Migration 0054 adds the journal budget and outcome fields and backfills the read count.
The isolated database tests cover dispatch contention, lost local dispatch receipts,
cross-replica observation, cancellation before dispatch, exhausted time/count budgets,
source epoch rejection and domain-before-Task settlement. These tests do not establish
actual provider dispatch, production home fencing or installed recovery acceptance.

## Worker Delivery And Undispatched Outcomes

Migration 0055 records a known undispatched refusal and a Task projection marker.
Cancellation before dispatch or current action denial restores the previous Computer
phase under the shared lease and queued journal predicate. Capacity stays retained.
Any escaped dispatch ticket permanently excludes this path, including lost replies.

Internal discovery pages operations by provider and UUID. It includes orphaned Task
links and incomplete result delivery. Domain terminal state and the matching terminal
Task permit a durable projection marker; the worker then releases its Task pin.
Discovery repairs a crash between those writes. Once acknowledged, ordinary Task
retention cleanup cannot recreate the old operation. These APIs are private worker
boundaries; public facades use owner-scoped reads.
