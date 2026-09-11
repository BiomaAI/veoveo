# Computers Domain

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo identity and Work Context | Canonical TaskOwner authority, named user/service principals, tenant and context isolation; current implementation admits private ownership |
| SurrealDB / SurrealQL 3.2.4 | Existing qualified platform client/server pin; schema-full records, atomic multi-record admission and outbox, conflict-only bounded transaction retry |
| Veoveo Computers JSON | Public DTOs live in `contract/`; provider identities and persisted authority remain internal |
| XChaCha20-Poly1305 and HMAC-SHA-256 | Private command, output-capability and maintenance-checkpoint envelope v1; installation-owned keys, random 192-bit nonces, distinct derived encryption and fingerprint keys and authenticated purposes; no public wire extension |
| Shared Tasks | Queued command references carry only Computer/execution IDs; migrations 0061–0067 add private command admission, one-shot dispatch, containment, protected output access, known-result settlement and bounded preparation. Current observation leases guard domain journal transactions; native dispatch and Task result projection belong to `servers/computers-mcp` |
| Veoveo internal `request_context` | Required verified source principal and token metadata for new operation admission; accepted execution evidence contains no bearer secret |
| Veoveo retained instance identity | Migration 0068 stores an optional replacement UUID on Computers and lifecycle operations; absence identifies the original instance, whose ID equals the Computer UUID |
| Veoveo private retained maintenance | Migrations 0069–0071; immutable source/target, shared `provider_wait` Task, exclusive Computer fence, bounded step journal, explicit resumption receipts and protected checkpoint; private provider worker and public service projection share this journal |
| Veoveo `computer_attach` and session-grant ledger | Resource-scoped interactive authority, one-use browser tickets and bounded renewal; private storage profile introduced by migration 0058 |
| Veoveo automation grant v1 | Named principal and OAuth-client binding, explicit read/execute/start/stop permissions, bounded lifetime and execution limits; private additive migration 0060 |
| Veoveo CLI grant v1; OpenShell `0.0.116` pairing profile | Private named-grant and connection ledger, eight-character confirmation code and fixed IPv4 loopback callback shape; additive migration 0059; public adapter qualified in the Bioma installation |

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

`replacement_instance_id` identifies a maintenance replacement independently of the
provider installation UUID. It is absent for the original instance and must otherwise
be non-nil and distinct from the Computer UUID. Lifecycle admission snapshots it;
dispatch, observation and settlement compare it against the current Computer fence.
The runtime resolves that exact instance for lifecycle, terminal and CLI access.
An old operation cannot settle a different instance even when its template is unchanged.
The identity field does not authorize maintenance or adopt storage by itself.

## Delivery Boundary

The current checkpoint implements private collection admission, operation admission
and shared Task linking, dispatch receipts, durable observation budgets and correlated
domain settlement. The native worker lives in `servers/computers-mcp`. The browser terminal and stock CLI project durable renewable grants through the
installed service. Named automation grants now have a durable ledger and current
authority checks. Their public command and grant projection shares this domain. Remaining work includes
deletion with storage acknowledgement, execution and Artifact movement in
`docs/COMPUTERS_PLAN.md`. Domain tests alone do not establish installed acceptance.

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

The inventory uses the indexed owner/Computer boundary and returns at most 128
outstanding grants. Expired, revoked and unredeemed expired-ticket rows are excluded.
Grant IDs are addresses; the domain verifies their parent and current owner before
revocation. Revocation requires current Computer read authority and only reduces the
owner's existing access. It does not require permission to issue new access or stop
the Computer. A repeat preserves the first revocation and emits no duplicate event.
This owner reduction is available through native HTTP. MCP invocation additionally
passes the gateway's normal tool-exposure and `tools/call` admission.

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

## CLI Pairing And Connection Grants

`cli_grants` implements the private ledger for the stock OpenShell `0.0.116` adapter.
The public pairing, restricted tunnel and grant panel consume these domain APIs;
installed qualification is recorded in `docs/COMPUTERS_PLAN.md`. The wire adapter
preserves the client's binary stream:
its SDK writes received text frames into gRPC bytes as well. Browser terminal controls
cannot enter that stream. This custom pairing is not an OAuth device authorization flow
or proof of possession.

An authenticated browser starts a challenge for one owned Ready Computer. The request
contains a name, the stock client's eight-character confirmation code and a port in
1024–65535. The only admitted callback destination is the native client's fixed
`http://127.0.0.1:{port}/callback`; an HTTP projection must never accept a callback host.
It must enforce session and CSRF, show the exact code for comparison with the terminal,
and require explicit confirmation before calling `confirm_cli_pairing`. The domain
cannot infer human confirmation from an HTTP request.

Challenges expire after 120 seconds and bind the owner, Computer, profile, OAuth client,
Work Context and sign-in family. An owner/context guard limits admission to five
challenges per minute across replicas, including consumed challenges. Reusing a code
for that Computer is rejected. Only its domain-separated hash is stored. Challenge
confirmation consumes the row atomically with a fresh named grant and its audit event.
Competing confirmations produce one bearer. A lost response requires new pairing;
the database cannot recover the secret.

The credential is a versioned locator with 256 random bits. It has no Debug, Display
or implicit serialization implementation. Storage retains only its domain-separated
SHA-256 hash. It authorizes attachment to the recorded Computer under the recorded
session family and current policy. It carries no lifecycle, arbitrary forwarding,
agent-execution or provider-administration authority. The shared installation policy
limits outstanding browser and CLI grants together. The grant's absolute expiry and
idle expiry cannot exceed that policy or the sign-in family's lifetime.

Each accepted connection has its own durable UUID and exact provider resource/process.
At most sixteen unexpired connections share one grant across all worker replicas.
Closing a connection leaves the parent grant valid for subsequent stock CLI commands.
Stopping or replacing a process invalidates old connections; a fresh connection may
bind the current Ready run of the same Computer. Only successful current renewal may
open provider I/O. Renewal checks the grant, connection, policy, directory, family and
exact Computer run. It records a lease of at most thirty seconds, with database latency
charged against the returned monotonic deadline. A closed or expired connection cannot
revive. Timer ticks, output and transport keepalives cannot extend idle access.

Owner inventory returns names and lifetime metadata without credentials or family IDs.
Current read authority permits an owner to revoke a grant, including after losing
contributor membership. Revocation ends every attached connection under its bounded
lease while leaving Computer execution and retained storage unchanged.

Migration 0059 adds private schema-full tables without rewriting browser grants or
retained Computers. Apply it before the updated admission query runs. Deploy all
Computers workers that count the shared browser/CLI quota before enabling public CLI
pairing. Rolling overlap is safe while CLI admission remains disabled. The older
runtime clients use database-scoped connections without schema migration, so additive
tables do not change their reads. An older migration binary rejects a database ahead
of its catalog; rollback must retain the qualified migration runner. Drain CLI admission
and let existing connection leases close before rolling back a CLI-capable worker.
Keep the grant ledger for expiry and revocation; rollback removes no retained files.

The isolated real-store tests exercise one-use confirmation, cross-replica grant and
connection limits, shared browser quota, code replay and rate limits, owner reduction,
passive renewal, expired access, source-token expiry, current policy, process changes
and logout. They do not qualify the public stock CLI or blocked network I/O.

## Named Automation Grants

`automation_grants/` owns a separate private ledger for named user or service
principals. The owner chooses one Computer, OAuth client, permission set and absolute
expiry. Execute additionally requires time and output limits. The domain requires
current owner authority for every granted action and denies transitive grant issuance.
The principal must already exist and be enabled in the same tenant. The admitted
OAuth client must serve the same profile and authorization server.

Issuance commits the request fingerprint, grant, quota guard and audit event together.
Competing exact retries return one grant; changed input under the same request UUID
fails. An exact retry after revocation or a policy reduction returns the original
record without recreating authority. Revocation advances the durable revision and
emits one event, including under concurrent retries. Inventory returns at most 64
unexpired, unrevoked records for the owner. Grant IDs identify records and are never
credentials. Automation grants use their own quota rather than consuming browser
or CLI connection capacity.

Every use binds the actual source principal, issuer, subject, kind, OAuth client,
tenant, Work Context, profile, Computer and provider instance. Current principal,
owner and action policy must still permit access. The source principal's actual
labels must cover the retained data and output labels. A policy reduction clamps
execution limits and the maximum lifetime; disabling issuance also denies new use.
The original owner's browser logout does not revoke this separately issued agent
authority. New admission through a browser-bound credential requires its own live family.
Accepted work continues under the independent named grant after that family closes;
current source and owner accounts, action policy and grant expiry still govern it.

`AutomationAuthority` is a permission-specific read valid for at most 30 seconds.
It is not a native dispatch ticket. A Task admission or dispatch must compare the
stored grant revision and current installation policy atomically with its operation
fence. Only an Execute read exposes execution limits. Public grant routes, scoped
lifecycle Tasks, the native command worker and active-job cancellation remain integration
work. Revoking the ledger alone does not stop a running Computer.

Command Task observation and cancellation use `authorize_command_task`. Execute
includes access to that principal/client's own command progress and cancellation;
it grants no broader Computer or Artifact read. Each request rechecks the current
named grant, source identity, Work Context, profile and inherited output labels.
A direct Computer owner can observe under current Read policy and cancel under Stop
policy, including after the agent grant is revoked. A delegated effective actor cannot
enter that direct-owner path. The original execution actor remains the Task owner.
Facades retain the authenticated caller for request audit and use the returned owner
only to look up the already-authorized Task. The bounded metadata read excludes command
ciphertext and output capabilities; the resulting authority expires even during a slow
Task response. This adds no stored format or independent authority cache.

Migration 0060 adds private tables without rewriting Computers or transport grants.
Apply it before enabling the automation surface. Existing runtime connections do not
run migrations; additive tables do not require unrelated service rebuilds. Retain the
qualified migration runner on rollback, disable automation admission and resolve its
active work before withdrawing workers that understand it. Keep retained files and
revocation records intact.

Native tests use separate clients of an isolated pinned store. They cover competing
issuance and revocation, request conflicts, quota and policy reductions, exact client
binding, owner logout, disabled principals and retained-label enforcement. Generated
Console validators enforce bounds and closed input objects. The selected Zod
converter does not enforce JSON Schema `uniqueItems`; the Rust wire deserializer
rejects duplicate and empty permissions before constructing its bounded set. Public
protocol integration must retain that strict deserialization. These checks do not establish public agent execution.

## Retained Maintenance Admission

`queue_maintenance` reserves one replacement UUID under current `update_template`
policy and contributor authority. Migration 0069 stores its original source binding,
installation-selected target, request identity and accepted actor without provider
payloads. The transaction acquires the existing Computer operation fence and writes
the audit event. Concurrent exact retries find the same replacement and repair the
same shared `provider_wait` Task if linking was interrupted. A changed target conflicts.

A Ready or Stopped source retains its exact resource and process identity. An exhausted
initial Create can enter maintenance while its original provider effect remains unknown.
That original operation stays unresolved and retained quota stays charged. Taking the
new fence does not prove provider absence or authorize a second writer. The allocator's
physical claim and abandonment rules still govern transfer. Active command executions
must finish before this admission; coordinated drain is worker integration work.

Admission closes new and renewing attachments through the existing exclusive fence.
It leaves the observed phase, current instance and template intact.

`progress` preserves at most six step receipts: Stop, Capture, Retire, Transfer, Create
and Restore. An already stopped source starts at Capture. An unknown initial Create
uses Transfer and Create; it has no applied source policy to capture. Every new step
requires current named authority, valid for five seconds including policy-read latency.
Its intent commits under the exact shared Task lease before a dispatch ticket can leave
the domain. Lost commit replies yield no ticket. Dispatch IDs remain in the history
after settlement and cannot be reused for another step.

Each step has a persisted 180-second recovery deadline and at most eight admitted reads.
Backoff and charged read identities survive lease takeover. Observation tickets admit
no new provider mutation. The owning worker may repeat only a separately qualified
idempotent allocator operation with the same complete identity. Exhaustion preserves
the Computer fence and marks recovery required. Policy loss and cancellation after a
dispatch also preserve the journal. Cancellation before the first dispatch releases
the new fence; an unresolved initial Create regains its original fence and unknown
outcome. Ordinary request retries never reset an uncertain dispatch or its budget.

`resume_maintenance` requires private Computer ownership and current `resume_update`
policy. The original `update_template` authority continues to govern every subsequent
dispatch. A recovery request names the existing Task and exact paused update timestamp.
Its immutable receipt archives the previous progress and current actor/decision. One
transaction reopens a finite observation window on the last step, retains its dispatch
identity and evidence, and resumes the shared Task. A pending cancellation requires
its exact timestamp as explicit acknowledgement. Concurrent cancellation rejects the
commit; a later cancellation again prevents dispatch. A request retry returns current
progress without reopening a window or withdrawing a newer cancellation.

Resumption preserves the Computer fence, original source and replacement, quota and
encrypted checkpoint. An unfinished step admits observation only. A completed final
step resumes target verification without repeating Restore. Missing checkpoint keys
still require repair before the worker can proceed. No operator permission authorizes
discarding an unresolved effect. The domain permits only the owning principal under
installation-selected recovery policy; installation administration does not create
cross-owner read or recovery access.

Migration 0071 adds each step's observation-window start and private resumption receipts.
Drain older Computers readers and workers before applying it: their closed step decoder
cannot read the new field. The backfill copies original dispatch time and preserves
deadlines, charged reads and dispatch evidence. Keep this schema and compatible workers
during template rollback. The isolated migration test reconstructs a pre-0071 step and
proves exact preservation after the migration. Public recovery actions and native
resumption qualification remain separate delivery work.

Capture settlement creates its encrypted checkpoint in the same transaction as the
step receipt. The worker must reopen and validate that checkpoint before retirement.
Only complete histories reach adoption. Final target verification uses the last step's
remaining deadline and read budget; a missed observation cannot create an unbounded
verification loop. Conclusive settlement ends recovery backoff, allowing verification
to proceed immediately. Adoption consumes that fresh observation ticket and atomically
replaces the recorded instance/template and resource/process, marks Ready and releases the fence under current
action authority. Quota and the retained owner do not change. The owning worker must
qualify the exact target run before this private adoption call; domain fixtures alone
do not prove physical retirement, storage transfer or provider policy loading.

Task projection and retention-pin acknowledgement are repairable through bounded
worker discovery. The provider worker composes these transitions in
`servers/computers-mcp`. Operator recovery, executable worker wiring, public update
projection and installed template qualification remain implementation work.

## Computer Confidentiality

Maintenance checkpoints are bounded opaque adapter bytes. Their private binding covers
the operation and request, provider, Computer, owner and actor, source and target
instances/templates, source resource/process, and retained labels. `ComputerKeyRing`
protects them with a distinct maintenance purpose. The adapter validates its private
format after authenticated opening; the domain has no provider dependency. Encryption
and decoding grant no maintenance admission or physical writer authority. A separate
private `computer_maintenance_policy` row retains the encrypted checkpoint. Missing
keys, binding changes and damaged envelopes fail closed without recapture after retirement.

`secrets/` protects argv, environment and finite stdin before durable
admission. The envelope authenticates the Computer, named grant, actual actor,
owner, request and execution identities together with the selected provider run
and template. The immutable binding also captures retained-home labels and both
Computer-owner and actual-actor output-policy labels, including classification.
Time and output limits are inside the encrypted payload. It reuses
the qualified guest frame rather than defining another command serializer.

Replacement commands include their instance UUID in the authenticated binding. Initial
commands omit that optional member and keep the same v1 associated-data encoding;
existing initial envelopes need no decrypt-and-reseal migration. Rebinding an envelope
between initial and replacement instances, or between two replacements, fails
authentication. Current run checks and journal transactions enforce the same identity
at admission, dispatch, continuation, output completion and interruption settlement.
JSON binding UUIDs compare through canonical text at the SurrealQL boundary; the
Computer and lifecycle journal retain typed UUID fields. Older workers must be drained
before replacement admission, since they can address only the original instance.
Selecting an older template retains workers that understand replacement identity.
An older worker binary is unsupported after adoption.

Installation-owned 256-bit keys derive separate encryption and fingerprint keys
through HMAC-SHA-256 with fixed domain labels. XChaCha20-Poly1305 uses a fresh random
192-bit nonce for every seal. Authenticated purpose labels separate command bytes,
output-capability receipts and maintenance checkpoints even under the same key.
The private request fingerprint is keyed; a database
copy does not expose an unkeyed digest for guessing command secrets. Opening checks
both envelope authentication and the exact bounded guest frame, including refusal
of trailing bytes. Errors contain no command or parser excerpts. Secret-bearing
payloads and envelopes have no Debug or Display surface. Plaintext buffers and raw
keys are zeroized on ordinary drop. The shared Artifact receipt type still owns its
secret as a String, so not every transient secret copy is zeroized; this is not a claim against privileged memory
inspection or forensic recovery from an arbitrary host.

A bounded key ring writes with one active key and retains up to four keys for reads.
Exact request comparisons use the original envelope's key after rotation. A missing
key or damaged envelope fails closed and cannot create a fresh dispatch. Key removal
requires draining or re-encrypting all work and backups that need it. Automatic
re-encryption and Secret mounting remain integration work;
the codec itself does not authorize a command or establish installed key rotation.

The implementation uses the existing RustCrypto dependencies. The authoritative
crates.io registry confirmed stable chacha20poly1305 0.11.0, hmac 0.13.0 and zeroize
1.9.0 on 2026-09-10; the workspace now pins these exact versions without changing
the selected releases. Tests cover run/owner/grant rebinding, authenticated limits,
randomized ciphertext, retained-key retry, key removal, corrupt envelopes and strict
framing. No provider or installation is needed for these pure checks.

## Durable Command Admission

`commands/` consumes a current Execute authority read and an encrypted payload. Its
transaction rechecks the control revision, enabled source and owner principals,
caller session family, grant revision and installation policy fingerprint. The
retained Computer owner context and exact provider run must still match. It commits
the command, request identity, exclusive execution slot and outbox event together.
An authority read prepared before revocation or a policy reduction cannot admit work.

Request identity binds the actual source principal and OAuth client as well as the
canonical actor, Computer and Work Context. Concurrent exact retries return the same
command. Changing argv, environment, stdin, limits or grant under that identity
fails. Comparisons use the original encrypted envelope, including its original key
and run binding. Exact retries resolve accepted input before applying tightened
execution limits; a subsequent dispatch still requires current limits and authority.
The original grant must remain usable to obtain a fresh admission read.

The execution slot is separate from the lifecycle fence. Queued work leaves browser
and CLI attachment authority available. Stop remains an explicit owner action. Start
refuses an unresolved command slot, because a new run cannot make an old command's
outcome known. The worker must settle or fence that command before releasing its slot.

A bounded provider-scoped scan recovers commands after a lost admission response.
`ensure_command_task` rereads the ledger in the same store and creates one shared
ProviderWait Task using its original identity and actual actor. The Task contains
only Computer/execution IDs and a retention pin. Neither its request nor the audit
event contains command bodies, ciphertext or input fingerprints. The private ledger
holds the authenticated envelope until its execution lifecycle permits disposal.

Migration 0061 introduces queued admission. Migration 0062 adds the guarded dispatch
transition and evidence. The native command worker composes dispatch, output
Artifacts and settlement; its production wiring and public projection remain delivery work. Apply the migration before updating lifecycle admission,
and upgrade all workers that can Start before enabling command admission. Rollback
must drain and settle execution slots before an older lifecycle worker returns;
removing a slot without termination evidence is forbidden.

Native cases cover competing retries, changed input, distinct-request contention,
private Task reconstruction, stale authority, policy reductions, principal disablement,
continued browser admission and blocked replacement Start. The fixture owns its
isolated database and creates no provider processes.

## Command Dispatch

A queued-to-dispatched transaction holds the current shared Task lease and refuses
cancellation. It compares the named grant revision, current policy and enabled
source/owner accounts, then fences the exact retained owner, template, resource and
process. A concurrent lifecycle Stop conflicts with its Computer write. Only the
winning dispatch UUID can yield a ticket. An unknown database reply, expired lease
or lost ticket never authorizes another command launch.

The decision records current owner and source policy without command contents.
Effective execution limits take the smaller admitted and current grant limits.
The durable deadline also ends at the grant's effective expiry. The local dispatch
authority expires within five seconds of the dispatch commit, bounded further by
its policy snapshot and Task lease. Receipt delivery cannot restart that window.
Worker renewal checks the named grant independently of the original admission token.

Execute grants require `onInterruption: "stop_computer"`. Qualified cancellation
and containment stop the admitted Computer run, which can end other processes on
that run. Retained files remain. This consent grants no independent agent Stop
action and never permits stopping a replacement run. The private envelope's v1
profile admits only this interruption scope; a future scope requires an explicit
codec change. The domain journals containment and interruption settlement; the
service worker publishes outputs through its protected Artifact capability.

Isolated-store tests qualify one dispatch under contention, a successor refusing
redispatch after loss of the original ticket, cancellation, grant revocation,
principal disablement, changed native run, lost lease, corrupt command ciphertext,
current limit reduction and accepted-work continuity after caller-family revocation.
They launch no provider process and are not installed execution evidence.

## Active Command Authority And Preparation

`command_continuation` checks the exact dispatch metadata and current shared Task
lease, then evaluates the named grant with current source/owner policy and directory
state. Its metadata-only query excludes encrypted command and capability bodies.
The worker holds the already authenticated request. An active read has a four-second
deadline, and its permission expires within five seconds of the read's beginning.
The worker keeps the previous expiry timer active while a refresh is pending.
Cancellation, grant loss and a changed native run end foreground I/O. Policy changes
can shorten runtime and output limits; later increases cannot restore a limit that
the active worker already narrowed.

Migration 0066 bounds queued preparation to five minutes. Both dispatch and expiry
refusal use the database clock. An expired queued command releases its exclusive
slot as undispatched; the same reason cannot settle a dispatched command. A worker
that lacks the qualified template or output receipt exposes preparation state until
that limit. Native-store cases qualify early-expiry refusal, aged dispatch refusal,
slot release and retained fencing after dispatch.

## Command Interruption And Containment

Migration 0063 adds interruption intent, a single Stop dispatch receipt, bounded
termination observations and terminal Task delivery. Cancellation while Queued
releases the command slot only when the shared Task records cancellation and no
dispatch escaped. A current authority refusal or changed run can also abort queued
work. These paths leave the Computer phase and retained files intact.

After dispatch, cancellation, deadline expiry, authority loss or an unknown command
outcome enters Containing under the Task lease. Cancellation and deadline reasons
are checked against durable state. The original `stop_computer` execution scope
authorizes containment even after the named grant is revoked. It permits one Stop
of the saved resource and process; a replacement run cannot receive that Stop.

An owner may already have admitted a lifecycle Stop. Containment observes that run
without acquiring a second Stop ticket. If the owner's Stop is definitively aborted
before dispatch, containment can acquire its first ticket after Ready is restored.
Its transaction sets Stopping while preserving the separate command slot. Once
that ticket is committed, a lost reply cannot authorize another submission.

Authenticated native Stop completion or a charged authoritative read can prove that
the original run ended. Settlement records the exact evidence identity and ingestion
time with the outcome and releases the command slot atomically. An independently
owned lifecycle fence remains intact until that lifecycle operation settles. A
Cancelled result means termination was observed after cancellation. Other interrupted
commands become Failed with an explicit interruption reason; observed termination
does not recover the foreground exit code or undo command writes.

Observation admits at most eight reads within the original 180-second deadline.
Backoff and read identity persist across workers. Each local read also fits the
remaining Task lease and a ten-second request bound. Exhaustion records Recovery
Required without releasing the command slot. Worker discovery stops scheduling a
waiting recovery Task until explicit recovery changes its state.

The shared Task is projected after domain settlement. A durable delivery marker
precedes retention-pin acknowledgement. Discovery can repair a lost acknowledgement;
it cannot recreate an already delivered command Task. Successful command results,
Artifact output and native worker integration remain required before public execution.

Isolated-store tests cover cancelled-before-dispatch refusal, revoked-grant containment,
competing Stop receipts, lost Stop replies, independent owner Stop completion or
pre-dispatch abort, replacement-run refusal, count/deadline exhaustion and interrupted
Task delivery. These fixtures supply internal observations and do not establish
provider termination or installed cancellation latency.

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


## Protected Command Output Access

Migration 0064 adds one optional encrypted output-capability envelope to the private
command record. The trusted service obtains a real Artifact capability with the
forwarded caller identity. Its request reserves two occurrences for stdout and stderr,
uses the admitted total-byte limit and requires the immutable inherited-label floor.
The Artifact service remains responsible for output ownership and clearance. A domain
attachment receipt is not proof that the Artifact service issued an arbitrary supplied
secret; only the trusted service path may call that interface.

Capability expiry includes the requested runtime, a two-minute publication allowance
and five minutes for preparation. A queued retry requests a new capability only when
the retained receipt cannot cover execution and publication. A compare-and-set
transaction preserves the first sufficient receipt across replicas. Refresh replaces
only an insufficient queued receipt; dispatch never changes output authority. An
unreadable envelope requires operator repair and cannot silently trigger replacement.

The dispatch transaction compares the exact retained envelope and refuses any capability
that expires before the execution deadline plus its publication allowance. The private
ticket carries the decrypted receipt. Public Tasks and audit events contain neither
the receipt nor its capability ID. These checks add no provider call. A failed
preparation leaves queued work without dispatch authority; service retries and the
worker's bounded preparation failure still need integration.

This is a coordinated cut of the unreleased private command profile: its binding now
requires output labels and its authenticated data includes the envelope purpose. No
command records have been admitted on the installed service. Upgrade the migration
runner and all command readers before admitting public commands. There is no decoder
for the earlier unreleased envelope shape.

Isolated-store tests cover missing preparation, replica races, delayed issuance replies,
insufficient lifetime, queued renewal and corruption. Pure tests cover purpose
substitution, inherited-label tampering and retained-key decryption. Native execution
with real Artifact redemption, successful-result settlement and public agent admission
remain required before this path is usable.


## Known Command Completion

The original dispatch ticket may become an ephemeral exit receipt only before its
execution deadline. The native worker supplies the qualified exit and exact stdout
and stderr counts; invalid counts, unknown exit 124 and an out-of-range exit fail
closed. The exit receipt grants a finite publication interval. It cannot be serialized
or recreated from a current Computer observation, because that observation says
nothing about the foreground command's past exit.

The worker must publish both streams through the prepared Artifact capability before
settlement. The domain checks distinct UUIDv7 occurrences and exact observed byte
counts against the admitted output limit. Under the current Task lease, one transaction
compares the dispatch, provider run and exclusive slot, then records Completed, the
result and its audit event while releasing the command slot. The trusted worker owns
verification of actual Artifact receipts; domain fixture IDs do not qualify publication.

Migration 0065 adds this terminal state and its metadata result. The Computer's phase
and any independent owner Stop remain unchanged. Nonzero exit is a known result and
maps to a completed shared Task containing a tool error. A late cancellation remains
in Task history. Once containment is admitted, a stale exit receipt cannot win result
settlement. Lease loss, replacement run, invalid output or unknown native exit retains
the command slot for containment or recovery.

Domain completion precedes Task projection. Both command and lifecycle acknowledgement
wrap their guard and delivery-marker update in one transaction. A thrown statement
outside a transaction does not stop later statements in a SurrealDB batch. The command
acknowledgement checks the Task's completed
status, exact structured result and tool-error flag before marking delivery. A missing
pin acknowledgement remains discoverable; a delivered command cannot recreate a Task.
All command readers must understand Completed before public command admission. Native
worker execution, real Artifact publication and the public end-to-end journey remain
integration gates.

Migration 0067 adds the canonical execution result address to completed journals and
their successful shared Task projection. The migration transaction checks that the
Task identifies Computers execution and that its structured result and error bit agree
with the journal. An inconsistency aborts the whole migration. Exact result reads use
current command Task authority, with distinct actual actor and direct owner oversight.
The migration requires drained old command readers/writers and no downgrade decoder.

Automation inventory includes current management hints and the checked installation
ceilings. One fresh management authority snapshot supplies the hints; issuance and
revocation continue to evaluate their own current authority. Exact grant reads authorize
the owner and do not scan the active inventory, preserving expired/revoked results.

Public maintenance eligibility reuses the admission source-state check and reads the
execution slot. A retained maintenance fence is busy even when its initial Create had
an unresolved outcome. Eligibility grants no ticket; admission still atomically checks
the source, slot, current policy and replacement identity. `ControlAuthority` projects
update permission with the same named tool policy and a five-second freshness bound.
