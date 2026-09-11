# Computers Core Capability Plan

Status: revised on 2026-09-09 under the user's authorization to renegotiate the
contracts for Veoveo and Computers. Core product status and the policy decisions in
[Contract Evolution](CONTRACT_EVOLUTION.md) are accepted. This document defines the
implementation sequence and release gates; it does not claim runtime delivery or
production deployment. Implementation was authorized and set as the active goal on
2026-09-09. Computers is a Veoveo capability. Bioma supplies one
installation configuration and the public acceptance destination.

## Standards And Protocols

| Boundary | Planned supported profile |
|---|---|
| Veoveo identity and Work Context | Canonical tenants, principals, direct/delegated/automated invocation, action policy, session families, and bounded grants |
| MCP `2026-07-28`, contract revision 3 | Governed Computer resources, task-augmented lifecycle/execution tools, and subscriptions under the [server contract](../mcp/contract/DESIGN.md) |
| HTTP, RFC 9110; JSON Schema `2020-12` | Native Console projection over the same domain, generated Rust/TypeScript contracts, bounded payloads and typed errors |
| Shared Tasks and transactional outbox | Durable admission, operation identity, leases, cancellation, result publication, and recovery; provider observation extension remains to be implemented |
| OpenShell gRPC/Protocol Buffers over mTLS | Internal provider adapter with a qualified watch and authoritative reconciliation profile; no required webhook translation |
| OpenShell `0.0.116` | Reviewed stock CLI baseline and handoff provider patch graph; pin exact selected artifacts after qualification |
| WebSocket, RFC 6455; SSH | Browser binary terminal and stock CLI transport; terminal-v2 controls and ReplayComplete metadata are repository-owned extensions |
| OAuth security, RFC 9700; native apps, RFC 8252 | Scoped renewal and pairing design requirements; custom stock-CLI pairing does not imply standardized device-flow support |
| Retained storage | Reviewed `veoveo.io/computer-storage/v1` adapter candidate; actual volume/quota/fencing mechanism is selected and qualified before adoption |
| Artifact HTTP and `artifact://` resources | Existing governed file transfer and immutable publication; no terminal bytes or unbounded file bodies in MCP |
| OCI, Helm and installation-owned GitOps | Exact runtime artifacts and topology inputs; development deployment v7 remains confined to disposable environments |
| Behavioral and visual acceptance | Headless nonvisual checks permitted; final visual proof uses headed hardware WebGPU or WebGL; GPU workloads keep their mandatory acceleration |

The handoff contains additional OpenShell gateway and supervisor fixes. Check current
upstream support when selecting the initial profile, prefer qualified upstream fixes,
and record exact patched artifacts when needed. An unrelated consumer edit retains
its qualified pins. No dependency or image pin is introduced by this planning change.

## Product Decisions

| Decision | Product behavior and boundary |
|---|---|
| Core delivery | Native Console experience, canonical control contracts, diagnostics, installation package, and supported retained execution ship together |
| Capacity | Qualified local, remote, or on-demand providers can supply capacity; a product enable flag does not hide Computers |
| Authority | Humans and services use canonical principals and explicit policy; ownership is distinct from the actor currently working |
| Cardinality | The personal default is one Computer per owner; collection APIs and transactional quotas support other installation limits |
| Continuity | Navigation, transport loss, and ordinary token renewal preserve retained work; Stop ends execution and preserves the home |
| Automation | Agents use structured execution and lifecycle Tasks under scoped grants; no dependency on terminal scraping or a human browser session |
| Security | Provider/host administration stays outside the Computer; publication and promotion use existing scoped broker authority |
| Installation identity | Veoveo is the product; clean installation and the Bioma configuration consume the same release contracts |

The first release supports private personal Computers, stock CLI and browser access,
scoped agent execution, and Artifact file handoff. Group collaboration, persistent
device grants, remote-editor certification, app previews, alternative compute drivers,
and GPU-enabled Computer profiles follow explicit qualification milestones. They do
not require a new identity model. The software factory remains a separate orchestration
profile under [Factory Isolation](FACTORY_ISOLATION.md).

## User Journey And Experience Contract

1. Open Computers and see accessible Computers, permitted actions, retained storage,
   and capacity. An unconfigured installation shows a setup action to authorized
   operators. Other users see a useful availability explanation without secret or
   provider-internal configuration details.
2. Choose an admitted template and resource preset, then Create. Show admission,
   capacity/image preparation, starting, and ready progress. Reload and navigation
   recover the same operation. A timeout presents its known state and a recovery
   action using the original identity.
3. Connect and begin work. Focus and keyboard behavior are predictable, resize works,
   and terminal selection/copy are accessible. Bound retained replay and mark any
   truncation. Input becomes available only after replay completion and callback drain.
4. Reconnect to the same running process after navigation or network loss. Explicit
   Disconnect persists as an intentional choice. Transport reconnect never replays
   user input or repeats an arbitrary command whose effect is unknown.
5. Connect with the stock CLI through existing SSO pairing. Copied commands contain no
   secret. Show active grants, last use, expiry, and Revoke. Ordinary renewal preserves
   authorized work without repeated pairing.
6. Grant an admitted agent the actions needed on this Computer, with visible scope and
   duration. The agent can execute a bounded command Task, inspect structured progress,
   and publish selected results as Artifacts. Human and agent actions are attributable.
7. Import an authorized Artifact or export selected files through a governed Task.
   Show size, destination and overwrite behavior before a mutating import. Enforce
   path confinement, symlink/archive limits and quotas. Reuse the Artifact upload UI
   and SDK; a general file editor is a later UX enhancement.
8. Stop and later Start over the same repositories, files, coding-tool state and
   eligible caches. Explain that process memory ends on Stop. Default-template changes
   do not replace an existing Computer without an explicit maintenance operation.
9. Recover from provider, disk or authorization trouble through typed states. Uncertain
   effects remain protected. Deletion presents the actual retention consequence and
   requires explicit confirmation; routine cleanup cannot delete a home.

Use an accessible collection and detail layout with a clear primary action, responsive
terminal controls, screen-reader status announcements, and useful empty/error states.
Keep the focused terminal stable during background status updates. Progress reflects
observed phases; a spinner cannot conceal an exhausted recovery budget. Operator
reports expose correlation IDs and diagnostics without command text or credentials.

## State, Authority And Retention

Persist desired state, observed state and observation freshness independently of the
current operation. The UI must be able to distinguish starting from reconnecting and
an unavailable observer from a stopped process. A single Error state cannot safely
encode an unknown mutation outcome.

| Event | Access or execution effect | Retained-data effect |
|---|---|---|
| Disconnect or network loss | Attachment closes; execution follows its existing policy | Home and admitted background work remain |
| Attachment expiry or revocation | Input/output transport closes within the bound; no new grant from stale authority | Home remains; separately authorized policy may suspend execution |
| Stop | Confirm the selected run stopped and release compute admission | Keep the same allocation and recorded template |
| Agent command cancellation | Track request acceptance and actual process termination separately | Keep home; do not claim rollback of arbitrary command writes |
| Template maintenance | Fence source execution before replacement; checkpoint adoption | Reuse a qualified home or explicit snapshot/restore path |
| Delete | Stop/fence execution, revoke grants, then apply the declared retention policy | Purge only through an explicit authorized operation; report incomplete purge |

Use canonical direct, delegated and automated invocation metadata. Effective access
intersects actor policy, Computer grant and Work Context authority. A service cannot
name another owner or widen its own grant in a request. Model observe, attach, execute,
manage, share and delete separately. First-release delegated access is explicit and
private to named principals; group collaboration needs its own admission and terminal
writer policy. Exported files retain Work Context and Artifact policy.

Idle policy is installation-configured and visible. Active command Tasks and declared
background work hold execution leases; lack of keyboard input alone does not prove
idleness. Stopped Computers reserve storage. Quota reductions affect new admission and
explicit maintenance, with no silent eviction of existing homes.

## Proposed Component Ownership

These are planned paths. Add component DESIGN.md and AGENTS.md files with implementation
and update CODEMAP. Generate public types before exposing routes.

| Owner | Planned responsibility |
|---|---|
| `platform/computers/contract/` | Shared typed Computer, authority, operation, execution, terminal and grant vocabulary plus schema export; justified by multiple Rust/Console consumers |
| `platform/computers/` | Domain commands, policy application, admission, retained state and shared Task/store transactions |
| `servers/computers-mcp/` | Thin MCP projection and one deployable Computers worker/relay process; domain behavior delegates to the platform modules |
| `platform/runtimes/computers/` | OpenShell clients, supported provider profile, observation/recovery, terminal and volume adapters |
| Provider-host storage boundary | Privileged allocation and physical writer exclusion; reuse a qualified volume/provider mechanism before introducing a custom service |
| Existing gateway | Canonical catalog/action policy and existing governed MCP/admin routing; no provider SDK or host-volume credentials |
| Console BFF | Session/CSRF handling and bounded browser/CLI edge relay through narrow internal grants |
| `apps/console/web/src/computers/` | Native Computers page, generated client, controller, terminal state and grant/recovery UX |
| Existing deployment and image owners | Selected topology, independent artifact inputs, trust references and installation qualification |
| Owning test harnesses | Rust domain/provider/storage faults, TypeScript browser behavior and visual journeys, native stock CLI and SDK consumer checks |

The native Console page is an accepted platform projection over the Computers domain.
The MCP facade supplies the same state and actions to agents; neither facade owns a
second policy or lifecycle implementation. The BFF uses the existing governed server
projection for control; generic gateway modules need no Computers-specific controller.
Follow the complete domain-relevant MCP
contract, including discovery, resources, Tasks, subscriptions and well-known docs.
Terminal traffic uses its dedicated bounded transport.

Use one Computers deployable for worker execution and the MCP projection initially.
This keeps provider compilation and lifecycle restart independent of gateway/BFF
rollouts. A separate storage helper is justified only by required host privileges or
volume ownership. Do not add another controller or event journal merely to mirror a
module boundary. Replica loss may interrupt an attachment, but cannot stop or duplicate
provider execution through lost in-memory ownership.

## Delivery Sequence

### 0. Establish Contracts And Fast Feedback

Land the policy changes in AGENTS, architecture decisions and Contract Evolution.
Inventory handoff files, omitted manifests/migrations/store helpers, provider patches,
and existing upstream equivalents. Use the selected source as port units against
main, preserving unrelated history and local changes.

Create focused domain and provider tests and one maintained Console browser harness.
Use existing Rust fixture ownership through a narrow launcher when needed. Native
framework commands may run through the current test-report wrapper before new xtask
routing is implemented. Do not make rewriting existing smokes a prerequisite.

Scoped source evidence now uses immutable v3 receipts with owner-declared Cargo and
Console closures. The recorder and coverage verifier qualify concurrent publication,
retained failures and input/configuration invalidation. Installed adapters and complete
release-lock coverage composition remain work under Continuous Integration. Record
build inputs and cache churn throughout delivery.

Exit: approved target contracts are recorded, the dependency/gap inventory has owners,
and a focused check can run without building the full Veoveo smoke graph.

### 1. Qualify The Provider And Storage Boundary

Start with the reviewed Linux/amd64 Docker-backed retained profile. Veoveo's control
plane remains Kubernetes/Helm-installed. Package the compute-host prerequisites and
trust boundary; the provider's Docker socket never enters the gateway or Computer.

Declare the selected containment threat model. Qualify non-root identity, filesystem
and syscall restrictions, egress enforcement, and cross-owner attack cases on the
actual driver. Separate untrusted compute from control-plane credentials and storage.
A Docker-backed sandbox does not establish VM-grade isolation; stronger tenant threat
models require an independently qualified VM-backed boundary before admission.
Operators select admitted templates and network destinations, including the package,
Git and model endpoints needed for useful development. Policy changes stay attributable
and cannot be requested by a Computer to widen its own authority.

Reproduce the six provider fixes only where required behavior is absent upstream.
The gateway and supervisor branch after the common third patch; retain their distinct
artifact identities and tests. Record CLI, gateway, supervisor, driver, protocol and
storage compatibility in one supported profile. Capability negotiation cannot replace
native tests. A newly qualified upstream fix can retire its corresponding patch.

Prototype exact operation/run correlation across lost submission replies, watch loss,
lag and provider restart. The supplied watch tail is best effort. Use native watches
for timely observation and a bounded authoritative read for recovery when its result
proves the relevant outcome. No unconditional polling companion is added. If identity
or outcome evidence is insufficient, add the smallest provider repair before lifecycle
release. Do not manufacture reliability by wrapping the same lossy input in a webhook.

Choose a physically enforced volume mechanism for retained homes. Prefer a maintained
provider/volume facility if it proves quota, restore identity and single-writer safety.
Implement the missing narrow allocator only when necessary for the selected driver.
Document the actual filesystem/volume prerequisites, backup guarantees and host-failure
limits. A DTO byte limit and a database lease cannot enforce those properties alone.

Also prototype stock CLI renewal across provider-session expiry before completing the
browser UI. If the stock transport cannot renew in place, distinguish attachment
reconnect from process continuation and resolve the supported UX in this checkpoint.

Exit: exact candidate artifacts pass native retention, writer-fencing, replay, recovery,
and renewal probes. Unsupported provider assumptions become concrete repairs or explicit
profile limits; they do not become claims of completed functionality.

### 2. Implement Durable Lifecycle And Agent Control

Reuse canonical identity, Work Context and shared Tasks. Persist request fingerprint,
provider instance/resource identity, selected template/home, operation identity and run
epoch before dispatch. Apply quota admission and ownership transactionally. Same-key
same-input retries return the original operation; changed inputs are rejected.

Extend provider-wait recovery under CE-01. Qualify any stored enum/schema migration and
mixed-version compatibility or required drain. Preserve Media's existing webhook
behavior and cancellation/billing semantics. The Computers adapter records its own
observation profile rather than pretending a watch is a webhook.

Fix the handoff's conversion of LifecycleUnknown/WatchFailed into ordinary terminal
failure. Losing an observer does not release the active operation or home fence. A
budget-exhausted operation exposes Recovery Required while retaining identity. A read
that only proves present resource state cannot certify a past command result. Operator
recovery must supply authoritative evidence and use the same domain transitions.

Implement the MCP projection and native Console projection over these commands. Agent
execution uses explicit argv or an explicitly admitted shell, a confined working
directory, bounded environment/output, a deadline and durable request identity. Treat
arbitrary execution as mutating regardless of argv form. Stream bounded progress and
store authorized output Artifacts without recording command bodies in audit logs.
Cancellation waits for the admitted process-termination semantics; no automatic replay
of an interrupted execution with uncertain effects.

Exit: real-store concurrency and fault cases prove quotas, tenant/actor policy, delegated
and automated provenance, exact retry identity, observation recovery, stale-event
rejection, lease loss, cancellation, and no duplicate provider effect. Both facades see
the same state. Agent execution succeeds with its scoped grant and fails outside it.

### 3. Complete Retention And Governed File Movement

Prepare creates one allocation. Restore opens that exact allocation and refuses silent
seeding, formatting, substitution or resizing. Bind the volume to Computer identity,
owner UID/GID and template/storage compatibility. Fence the old physical writer before
attaching a replacement. Account for active, stopped and incomplete allocations, with
separate bounds for logs, temporary data and caches.

Protect a free-space reserve and provide actionable admission errors. Keep user homes
and journals outside routine image/Cargo cleanup. Validate retained templates and
rollback inputs before pruning images. Provide backup/export and tested restore for the
selected profile; local persistence across reboot is not a claim of host-loss durability.
Declare encryption-at-rest and backup-key ownership in that profile. Scope credential
injection to the Computer and operation, and preserve the owner's ability to revoke
coding-tool credentials. Logs and support bundles exclude terminal contents and tokens.

Implement Artifact import/export using existing authorization and transfer paths. Check
actual byte limits, extraction bounds, traversal/symlink confinement, overwrite policy,
interruption and integrity. Imported data cannot escape the admitted working tree or
silently acquire broader labels or grants.

Exit: files and useful caches survive Stop/Start and host restart. Disposable candidate
fixtures prove ENOSPC, incomplete allocation, identity mismatch, contested writers,
backup/restore and interrupted file transfer. Destructive fault tests do not run against
production user homes.

### 4. Deliver Browser And Stock CLI Continuity

Implement the user journey with generated types, shared invalidation events, canonical
snapshots and server-issued action flags. Keep terminal transport and lifecycle UI in
focused modules. Bound history and flow control, drain ReplayComplete before enabling
input/answerbacks, and reject stale reconnect callbacks after Disconnect or owner change.

Reuse existing SSO for one-use, expiring, rate-limited CLI pairing with explicit code
confirmation and validated loopback callback. Support the qualified stock client's
Computer-prefixed and root tunnel routes through an explicit adapter. Restrict methods,
SSH targets, origins and grants; no arbitrary HTTP/gRPC proxy or forged session header.
Pairing and session redemption must work across replicas without sticky routing.

Issue short attachment leases from current grant/policy state and renew before expiry.
Use revocation events plus authoritative resynchronization, with no stale-cache renewal.
Initial maximum authority staleness is 30 seconds, including cache and clock allowances;
renew at most every 10 seconds while attached. Target revocation closure within five
seconds in healthy operation. Enforce the hard bound during blocked reads/writes and
observer loss. Installation policy may tighten the limits. Grant absolute/idle limits
remain distinct from this short lease and do not slide indefinitely.

The first CLI grant is session-bound and revokes on logout. Closing the browser tab does
not itself log out. Persistent device grants are a later explicit-consent profile.
Service-principal execution uses its own authority. Revocation closes access; separately
specified suspension policy determines whether background execution must also stop.

Store grant state outside replica memory and rotate installation sealing keys with an
explicit overlap window. No secret enters copied commands, query strings, browser
storage, traces or ordinary logs. Enforce WebSocket Origin for browser attachment;
nonbrowser CLI connections authenticate through their narrow grant and cannot inherit
browser-cookie authority. Render terminal output as untrusted data, with automatic
clipboard, hyperlink and terminal-response behavior explicitly bounded.

Exit: headless behavior tests cover keyboard, replay, navigation and races. Headed
hardware acceptance proves usable layout and the real editing journey. Stock CLI work
crosses actual token/provider-session renewal through public ingress and alternating
replicas. Logout, policy removal, replayed pairing, blocked I/O, replica loss and grant
expiry satisfy the access bound. A broken SSH stream is reported as interrupted and
never implies an arbitrary exec was automatically resumed.

### 5. Qualify Retained Maintenance And Core Packaging

New template defaults apply to new Computers. Existing operations retain exact image,
configuration and home identities. Maintenance drains execution, checkpoints source and
target, fences the source writer, restores the selected home, qualifies the target, and
commits adoption. Rollback proves the target writer stopped before restoring the source.
If an upgrade changes on-disk data incompatibly, require a qualified snapshot/restore
path or declare the forward-only boundary before admission. Uncertainty retains locks.

Package Computers, provider and storage inputs for the chosen topology using existing
component/image ownership. Validate declared trust/configuration before writes. Always
register core control/Console surfaces; show Setup Required if capacity is explicitly
unconfigured. Missing inputs for a configured topology fail validation. Provider outages
make Computers unavailable while unrelated service readiness remains meaningful.

Include the complete disconnected artifact closure for the selected local topology.
No mandatory Veoveo-hosted service or unrecorded Internet pull may be needed to start a
retained Computer. On-demand or remote profiles are published only after their own
capacity, trust, failure and retention qualification.

Exit: a clean installation works with its own hostname, identity, policy and capacity.
Installed-template changes and rollback preserve files and prevent concurrent writers.
An explicitly unconfigured setup gives truthful diagnostics without claiming operational
Computers acceptance. The configured offline profile works without external downloads.

### 6. Deploy Veoveo With The Bioma Configuration

Publish the exact qualified immutable artifacts. Update installation-owned GitOps
configuration for Veoveo at `veoveo.bioma.ai`, reconcile it, and verify running digests,
configuration and the complete selected profile. Preserve retained instances unless an
explicit tested maintenance operation is part of the deployment.

Run the installed matrix below with real users and an admitted service principal. Use
at least two gateway/BFF replicas for shared grant, routing and revocation cases. Run
host destruction and disk-fault cases on isolated fixtures with the same candidate
artifacts; record which evidence came from fixtures and which from the public endpoint.

Exit: real browser, stock CLI, agent execution, file movement and retained recovery work
on Veoveo with Bioma configuration. A browser-only demo or API health check does not
complete this release.

## Build, Deployment And Performance Plan

| Representative change | Required behavior |
|---|---|
| Console source edit | Vite refresh and focused browser behavior; no Rust build, image publication or workload restart |
| Console production assets | Reuse the BFF binary and all Computers/provider artifacts |
| Domain or adapter edit | Focused input closure; rebuild and qualify only affected service/provider artifacts |
| Provider repair | Qualify the changed supported tuple; retained instances stay pinned until maintenance |
| Template default | Publish image/configuration once; preserve existing Computers and their useful caches |
| No-op deployment | Reuse exact artifacts and preserve workload identities |

Keep stable builder/worktree input paths and feature selection. Generate provider clients
from hash-verified local inputs. Budget cache storage explicitly and retain reusable
compiler dependencies within the free-space reserve. Every phase reports elapsed time,
cache/input invalidation causes, retry reason and affected workload identities.

Use three checkpoints: focused feedback during edits, affected-component qualification,
and full supported release acceptance. Dependency upgrades and broader performance
experiments run separately unless they resolve a selected-profile failure. Shortened
lease lifetimes accelerate routine fault cases; final qualification crosses real token
renewals. The v2 test-report workflow remains in force until scoped evidence ships.

| Measurement | Initial target and measurement boundary |
|---|---|
| Focused warm source check | Below 30 seconds on the declared developer host |
| Warm component build, qualification and rollout | Below two minutes, excluding provider rebuilds and sustained acceptance |
| Warm Start to usable terminal | p95 below 10 seconds with image present and capacity available; report provider startup separately |
| Reattach to input-ready | p95 below two seconds on the declared network with bounded history; include replay drain |
| Terminal responsiveness | p95 server/relay overhead below 50 ms; report measured network RTT and end-to-end echo separately |
| Access revocation | At most 30 seconds from committed platform revocation, including failure conditions; healthy-operation target below five seconds |
| Capacity and file transfer | Bounded memory/disk/concurrency under declared limits; record throughput with matched routes and payloads |

Latency targets require a recorded baseline, sample count, p50/p95, payload and network
conditions before a performance claim. Security bounds and bounded-resource behavior
are correctness gates. A latency miss creates a measured optimization issue and an
explicit release disposition. Additional host/provider/throughput comparisons are not
indefinite blockers for a usable, secure selected profile. Extend the source-controlled
[iteration audit](BUILD_DEPLOY_ITERATION_AUDIT.md) with costs and unresolved experiments.

## Release Acceptance Matrix

The native stock CLI probe now passes across provider SSH admission-token expiry.
The exact `0.0.116` client retains its shell and exchanges data after the configured
three-second credential expires. This supports renewing Veoveo connection authority
without reconnecting solely to replace the provider admission credential. Platform
grant persistence, pairing and public ingress are now deployed and qualified through
the stock CLI, as recorded in the public acceptance observations below.
Runtime lease enforcement now passes native terminal renewal and local real-mTLS
backpressure tests. Each attachment owns a physically closable provider connection;
revocation closes blocked transport while the Computer process remains available for
fresh authorized reattachment. This is a runtime checkpoint, not installed acceptance.

The domain now commits one dispatch receipt under the current shared Task lease,
charges a persisted observation budget and settles a matching resource/process before
Task publication. Deadline or read-budget exhaustion preserves the Computer fence in
Recovery Required. Real-store contention tests also enforce capacity and operation
admission on conditional writes. The worker now integrates native lifecycle dispatch and shared Task projection. The
production allocator and current attachment/delegation grants remain integration work.
Lifecycle dispatch now reads current policy and canonical account state and persists
the actual decision with its dispatch. Storage fixtures cannot override that check.

The production storage helper now passes isolated native mTLS, shared-mount restart
and physical writer-handoff cases. It records the exact source container, refuses a
transfer while a separate mount still holds the filesystem, resolves a lost handoff
response from durable state and preserves bytes across template changes. Old instance
identities cannot be reused. Native provider/worker integration, recovery for a target
that never acquired a physical writer, installed maintenance and public journeys remain
delivery work; this checkpoint does not establish release acceptance.

The helper and its private Docker daemon also pass process cold restart together.
The helper reopens its locked journal before Docker API readiness, resolving the
daemon's volume-plugin activation dependency. Physical operations retain their exact
engine checks. This case preserves the existing propagated mounts; compute-host
mount-namespace replacement and installed reboot retention remain separate gates.

The composite `computer-host` OCI image now passes the mount-namespace replacement
case. Its Rust launcher coordinates a private Docker daemon, provider and storage
helper with separate worker/guest trust and bounded startup/shutdown. An isolated
Computer retains its files across replacement of the entire host container, keeps its
Docker/provider identity and starts a new process. Packaging reuses provider artifacts
and the existing compiler families. Kubernetes manifests, installation-owned image and
configuration publication, retained maintenance and public journeys continue after
this checkpoint.

| Area | Required evidence |
|---|---|
| Core | Standard release surfaces, valid configured/unconfigured states, clean install and selected offline topology |
| Identity | Two real humans with similar display names plus a service principal; private defaults, explicit delegation, tenant/context denial and actor attribution |
| Quotas | Collection APIs, a quota above one, concurrent admission, exhaustion and reduction without silent deletion |
| Lifecycle | Lost replies, duplicate inputs, watch lag/gaps, authoritative reconciliation, stale epochs, budget exhaustion and cancellation preserve operation identity/fencing |
| Automation | Governed MCP resources/Tasks, structured execution, confined file/output handling, grant revocation and no uncertain command replay |
| Browser | Responsive accessible page, keyboard, safe replay, editing/testing, refresh/navigation, intentional Disconnect and headed hardware visual proof |
| CLI | Unmodified qualified client through real ingress, explicit pairing, both tunnel routes, renewal, narrow forwarding and honest interruption behavior |
| Authority | Cross-replica grants, fresh policy checks, key rotation, logout/revocation, expired/replayed challenges and blocked-I/O deadline enforcement |
| Retention | New process after Start over the same home; ENOSPC, partial allocation, physical writer fencing, backup/restore and host restart |
| Files | Authorized Artifact import/export, path/archive bounds, quotas, integrity and interrupted transfer |
| Maintenance | Retained template rollover, drain, replacement checkpoints, downgrade limits and tested rollback |
| Isolation | Effective containment/egress, no host socket or provider/admin credentials, denied cross-Computer access and broker-only publication/promotion |
| Installation | Actual source/configuration/image closure, Computers readiness, continued unrelated availability during provider faults |
| Iteration | Asset-only and no-op runs preserve unaffected artifacts/workloads; scoped test selection and measured source-to-running timing |

Each implementation checkpoint is a coherent commit with affected checks and current
recorded evidence. Report mocked, native, isolated installed, public installed, and
hardware evidence separately. The release is complete when the supported browser,
stock CLI and agent journey, retained storage/recovery, core packaging, and Bioma
installation pass. Future group, editor, preview, GPU and additional-provider profiles
retain their own acceptance and do not silently enter the supported matrix.

The CLI domain checkpoint adds durable one-use pairing, named session-bound grants,
shared browser/CLI quota and separately fenced connection leases. Closing a connection
preserves the pairing; a new connection may bind a later Ready run of the same Computer.
The isolated real-store cases cover independent replicas, rate/connection limits,
revocation and expiry. Public SSO confirmation, stock-client tunnel transport and
installed CLI acceptance remain required before this checkpoint becomes usable.

The worker checkpoint now composes a five-method restricted stock CLI facade with
the retained connection ledger and shared browser/CLI authority observer. A native
fixture runs unmodified CLI `0.0.116` through two production relay hops and two
service replicas. Both the Computer-prefixed and root tunnel paths carry real SSH.
Shell state survives source-token and initial lease expiry. Owner revocation closes
every relay within five seconds, prevents a subsequent command and leaves the
Computer Ready. The idle stock ProxyCommand can await local stdin before exiting;
wire closure and process shutdown are measured separately. Browser continuity and
retained lifecycle fault cases pass in the same native fixture. This fixture installs
a domain-issued credential in private stock configuration. Public SSO pairing and
installed ingress are still required.

The next application checkpoint adds typed HTTP pairing, generated Console models,
the explicit SSO code-confirmation page, shared browser/CLI access inventory and
fixed-profile root/prefixed CLI edges. A native check now obtains the credential
through actual HTTP pairing and confirmation on different service replicas, rejects
replayed confirmation and connects using the public Computer UUID. The BFF's local
wire tests qualify stock header adaptation, both route shapes and removal of private
lease text. Frontend tests qualify closed loopback delivery and revocation after
delivery failure. Public installation acceptance remains outstanding until these
application images, ingress route and migration are deployed together.

The public CLI checkpoint is installed at `veoveo.bioma.ai` in Helm revision 147.
Computers MCP and gateway use source `ba2161c8`; Console uses `d3832c35`, and the
coordinated installation selection is `c852b454`. The unmodified qualified CLI pairs
through the signed-in browser, connects by the public Computer UUID, retains shell
state across connection leases and reads the existing home as UID 10001. Named Revoke
and Console Sign out each close access and reject a new connection with the old
credential. A fresh browser connection confirms the denied commands did not run and
the original retained file remains. The native fixture supplies the precise bounded
closure and source-token-expiry evidence; the public journey does not measure those
bounds independently.

The installed consent-first page proves both allowed and denied local-device browser
permission. Denial creates no pairing request or grant. With permission allowed, the
credential-free local OPTIONS check completes before the two public issuance calls
and the one credential delivery. Headed Chrome has NVIDIA RTX 4090 WebGL; its fallback
WebGPU adapter does not establish hardware evidence. CLI auto-opening on the acceptance
host selected a different desktop browser under the isolated client configuration;
the final checks used the stock client's printed-URL path in the existing headed
browser. Browser modal interaction itself is not automated acceptance evidence.

The private execution launcher now passes native provider qualification. Its bounded
stdin frame keeps argv, environment and directory values out of command-preview logs.
The packaged template starts a real terminal in its retained home. Exact binary input,
internal symlinks and escape refusal pass. Timeout remains uncertain; the fixture then
uses Stop to fence a detached descendant and verifies retained bytes after restart.
That provider result does not establish public agent grants or Task cancellation.
The template and runtime changes require deployment with the subsequent agent surface.

The automation domain now persists grants for a named principal and OAuth client.
Concurrent issuance, exact retries, quota reductions and revocation pass against the
isolated store. Current access checks enforce both principals' policy and retained
labels; an agent grant survives the owner's browser logout. Migration 0060 and the
generated DTOs are local implementation work until the public grant surface and
fenced command Tasks are deployed. A current authority read is not a dispatch permit,
and revoking a grant alone does not establish process termination.

The private queued-command codec now encrypts the qualified guest frame under
installation-owned keys and authenticates the exact owner, actor, grant and native
run. Pure tests cover secret-free envelopes, rebinding refusal, tampering and request
comparison after key rotation. Command admission, key mounting and Task dispatch are
still required before this becomes usable agent execution.

Command admission now commits its encrypted request, exclusive execution slot and
audit event atomically. Native races resolve one command and one recoverable shared
Task; stale grant or policy reads cannot admit work. Browser attachment remains
available, while an unresolved command blocks a replacement Start. Migration 0061
currently covers queued admission. Dispatch, termination settlement and public agent
execution remain required; no command is launched by this checkpoint.

Structured execution now requires the admitted native run at the runtime boundary.
A changed resource or process rejects the request before its Start frame. The native
fixture retries an old run after Stop/Start and checks that its marker was never
written. This qualifies run selection alongside the durable execution slot; command
worker dispatch and public agent acceptance remain delivery work.

The private command journal now returns one dispatch ticket under a current Task
lease, exact native run and current named grant. Native-store cases reject replay
after ticket loss and recheck cancellation, current limits and principal state.
Execution grants explicitly consent to whole-run Stop for interruption containment.
The domain also journals original-run containment, bounded observations and
interruption settlement. Store tests cover independent owner Stop races, lost Stop
receipts, exhausted budgets and preserved replacement runs. Native worker integration,
successful-command output and the public agent journey remain required.

Task output capabilities now admit a mandatory inherited-label floor without a
new persistence format. Native Artifact tests qualify retention of that scope across
service instances. Computers now captures its mandatory home/output labels in the
immutable encrypted-command binding and stores the Artifact receipt in a separate
purpose-bound envelope. Migration 0064 and native-store tests make sufficient output
authority a dispatch prerequisite and preserve one usable receipt across replicas.
Known exit receipts and exact output references now settle to Completed under the
current Task lease in migration 0065. Nonzero exits retain their output as tool errors;
late cancellation and independent owner Stop retain their own history and fences.
The service worker now obtains real output publication through the internal Artifact
HTTP plane in its native fixture. Current authority expiry remains independently
polled during slow reads; policy reductions cannot extend the admitted command.
Migration 0066 bounds queued preparation and cannot expire dispatched work. Native
revocation begins containment within the five-second I/O bound, while confirmed
Stop includes the provider grace interval. Cancellation and lost-ticket containment
use the same journal and preserve retained files. Production key/configuration wiring
and the public execution journey remain required.

Remaining scoped evidence/build reuse, delegated agent execution, Artifact handoff, retained
operator recovery and the remaining clean/offline installation gates stay active.

## Follow-On Profiles

| Profile | Entry requirement |
|---|---|
| Team collaboration | Explicit group grants, per-session actor attribution and terminal writer arbitration; no shared human token |
| Remote editor | Qualify actual SSH process/subsystem and forwarding requirements, renewal and interruption with the selected editor |
| Private app preview | Per-Computer/port grants, isolated browser origin, WebSocket support and current policy; block arbitrary proxy destinations and metadata endpoints |
| GPU Computer | Qualified device isolation/placement and fastest compatible NVIDIA path; retain actual hardware execution proof |
| Additional providers/on-demand hosts | Same lifecycle, recovery, retention, authority and capacity matrix; truthful cold-start and scale-to-zero behavior |
| Persistent CLI device grant | Explicit consent, device binding where supported, independent lifetime/revoke UX and account-security event handling |
| Unattended factory | Existing factory broker and independent verification/promotion authority; personal Compute access grants no deployment rights |

## Reviewed Handoff

Current delivery checkpoint: the phase 0/1 private runtime port passes 60 local
transport/policy/stream/recovery tests and all-target Clippy. Persistable lifecycle
checkpoints bind provider and operation identity; one deadline-bounded read can
reconcile a lost observation without redispatch. Start requires a new process epoch on both the healthy watch and recovery paths.
The durable domain budget and native provider qualification remain in progress. Upstream
main is unchanged at `11f59d55`; the existing host has sufficient disk reserve and the
Bioma cluster is Ready. The installed stock CLI is `0.0.14` and cannot establish the
qualified `0.0.116` client profile. Runtime dependencies remain outside the gateway.
No provider or Computers deployment has been accepted at this checkpoint.

The next checkpoint adds `platform/computers` and its public contract crate.
Three isolated real-store tests use two clients to prove collection admission,
human/service ownership, unchanged-request replay, tenant/context isolation and
owner/tenant/provider quotas. Limits are durable policy; an old replica cannot
admit against a cached pre-reduction value. The checkpoint also passes public
schema tests, shared-store unit checks and all-target Clippy. The shared store's
environment-gated integration tests were not enabled; the Computers tests start
their own exact 3.2.4 database and apply the complete migration catalog.

Both provider patch branches reproduce their expected upstream trees. The stock
0.0.116 Linux AMD64 CLI archive verifies as SHA-256
`4fb4476d80a1875a0b83547ec3aba999cf0a2e2d75f95f2f709b622e2103520e`.
Both patched provider builds are complete. An isolated native test now proves create,
terminal replay, shell continuity on reattach, numeric UID 10001 and a new process
after Stop/Start. A separate native ext4 fixture also proves a 512 MiB physical capacity bound, ENOSPC,
confined writes and offline block backup/restore into a new provider resource with
preserved files and UID. That fixture explicitly removes the source containers;
production allocation, automatic writer fencing, durable renewable grants and
installation remain required. Native durable dispatch is qualified below.

Source package: `veoveo-openshell-handoff-2026-09-09.zip`, SHA-256
`b330a4016a25d182e206421c4eb019a1cd2fc0ae94e40b7d2056dcd654cba061`.
The package selects downstream source revision
`396a71200470043a5e7b2f6c3a855166c63e52e1` against Veoveo baseline
`11f59d55b487a4fa856f73cb31a498c7fb7e6d30`. Listed checksums passed review.
Its selected source is not a buildable checkout. Reported private runtime results
are context and do not substitute for the candidate's qualification.

The provider protocol's best-effort tails and lag warnings, the unknown-outcome
settlement bug, static CLI relay lifetime, patch graph and omitted allocator are
specific adoption risks. Phase 1 resolves them before broad integration. Upstream
references are [OpenShell 0.0.116](https://github.com/NVIDIA/OpenShell/releases/tag/v0.0.116)
and [OpenShell's supported isolation model](https://docs.nvidia.com/openshell/about/overview).

The shared Task runtime now has an additive `provider_wait` profile and a distinct
observation claim. Recovery preserves queued/running/waiting/cancel-requested Tasks
without reset or ordinary terminal failure. Migration 0052 requires compatible readers
before admitting this class; retained new-class records prevent an unqualified rollback
to old readers. Computers dispatch/budget integration now lives in its domain and worker.

The domain operation journal now admits one fenced action transactionally, preserves
the previous provider run and reconstructs its shared Task after interrupted linking.
Current context membership and output clearance are checked before mutation admission.
The Computers worker now connects this journal to native lifecycle dispatch and
bounded reconciliation. It repairs Task links and domain-to-Task projection. Dispatch
reads current action policy and enabled account state. Current attachment/delegation
grants, retained allocation and public/installed surfaces remain delivery work.

The native worker checkpoint passes against the pinned provider and isolated SurrealDB:
two replicas contend for Create, files survive Stop/Start, a successor observes a Stop
whose completion reply was lost to the domain, and cancellation remains in the Task
history when that known effect succeeds. Pre-dispatch cancellation/denial sends no
mutation. A lost Start ticket is never replayed; eight charged reads end in Recovery
Required with the original fence. Task-link/projection/pin repair has real-store
coverage. The fixture supplies its own prepared ext4 volume and storage adapter.
It now uses the domain's current policy checker and a real isolated control revision;
production allocation and attachment/delegation grant authority are still required.

The browser service checkpoint now uses production retained allocation and durable
access grants. Its native fixture creates one Computer through competing workers,
issues a ticket on one HTTP replica and redeems it on another. Real shell I/O and
resize cross the replay fence. Ticket replay is rejected, and the attachment continues
after source JWT expiry through current grant renewal. Policy removal and family
logout close access within the five-second fixture bound while the Computer stays
Ready. Shared service watchers accelerate revocation and invalidate their former
attachment epoch on observer loss. The recorded local suite passes 114 cases plus
this native composition. Public gateway/BFF transport, Console rendering, stock CLI
pairing, delegated execution, file movement and installed acceptance remain required.

The relay checkpoint adds sequenced current lease controls to unreleased terminal v2.
A shared gateway/BFF transport library preserves the service deadline and enforces it
through blocked input, blocked output and queued delivery. Its clock allowance requires
the platform processes to remain within one second of each other. Local native evidence
now crosses two relay hops for more than the original thirty-second lease without
reconnecting the shell. This qualifies the shared relay composition; actual gateway
authorization, BFF cookies and the Console still need their installed journey.

The gateway and BFF now expose the canonical Computer control and terminal routes.
Gateway admission applies exact domain actions, preserves signed source authority and
records durable policy decisions. The BFF uses its existing cookie, OAuth renewal and
CSRF boundary. Both edges enforce exact Origin for terminal access and preserve the
service-issued relay deadline. The BFF returns rotated cookies on HTTP upgrades and
on subsequent upstream failure. Local coverage passes 190 gateway/BFF cases, including
real WebSocket byte transport and the pinned database regression. Native Console
presentation, live invalidations and the complete installed authentication chain remain
delivery work; this checkpoint makes no public deployment claim.

The native Computer invalidation feed now shares the Console MCP resource pool with
Apps. It establishes the exact collection subscription before its baseline, keeps one
outgoing event queued, and releases its reference on disconnect, source loss or token
expiry. Unexpected source termination is explicit and triggers fresh client admission;
normal unsubscribe preserves other observers. The local gateway/BFF suite passes
194 cases. Native page integration and installed end-to-end qualification remain open.

The first native Console workspace now uses a small authenticated session projection,
with installation inventory loaded only for authorized administrators. Rust schemas
generate the Console and Computer TypeScript models and runtime validators. The page
shows current capacity, phases and action flags; lifecycle request IDs survive reload
and ambiguous responses. The lazy xterm WebGL terminal enforces replay callback drain,
bounded input/output, current lease deadlines and explicit disconnected state. Current
behavior coverage includes stale read/subscription epochs, same-ID recovery and blocked
rendering. These are local protocol/controller results, not headed browser acceptance.

Before release, qualify the real browser and full gateway/BFF/service subscription chain,
add the reactive operation read projection and health invalidations, expose exact replay
truncation, and finish the CLI, delegated execution, file and retention journeys. The
provider/allocator/service image and installation closure remain delivery work. The
Console checkpoint does not change the installed workloads or claim public availability.

Core packaging now has independent `computers-mcp`, `computer-storage`,
`computer-provider` and `computer-template` Bake targets. The provider image verifies
and builds the exact public-base/patch trees; its recorded OCI runtime closure remains
distinct from earlier host-native artifacts. Both standard Helm presets include
two-replica Computers control with truthful unconfigured state and explicit configured
trust/configuration references. The typed deployment selection includes that image.
Real Helm checks, all deployment-contract tests, affected lint and image builds pass.
An unchanged four-image build takes 5.134 seconds on this host. Private compute-host
startup, full installed provider/storage qualification, gateway registration, immutable
publication and Bioma activation remain the next delivery gates. These images and
render checks do not establish a working public Computer.

Before compute-host installation, provider adoption uncovered the native fixture's
shared worker/guest certificate. Gateway `0.0.117-veoveo.2` now admits only explicitly
named mTLS user certificates. Native fixtures give the supervisor its own transport
certificate, require its scoped sandbox JWT, and prove that the guest cannot call
ListSandboxes as a user. The OCI-built provider binaries pass the retained two-worker
lifecycle and renewed terminal fixture with this separation. Provider and supervisor
compilation now have separate source inputs. The installed compute-host topology and
public browser/CLI/agent journeys remain delivery work.

Storage startup now enrolls the engine reached through its explicit installation
socket. Operators configure `providerId` and `namespace`; the complete engine binding
is durable journal state. An empty journal accepts its first identity, while an
existing journal rejects a replacement engine. This removes the manual engine-ID
copy step. Eight storage tests and the native retained worker lifecycle pass with
the new configuration. Whole-host cold restart remains an installed qualification gate.

The configured `openshell-docker` capacity selection now includes the private host
and template in the typed deployment image closure. Helm provisions a retained PVC,
a single private host with explicit maintenance replacement, and separate two-replica
Computers control. Restricted-network installations admit only worker ingress and
declared registry egress. The exact published host and template pass whole-container
replacement with retained bytes in 39.4 seconds. Contract, chart, renderer and lint
checks pass. Installation-owned configuration, trust and public acceptance follow;
this checkpoint does not claim that capacity is already installed on Bioma.

Bioma now has installation-owned host/control JSON, the canonical retained template
fingerprint and separate provider/storage trust. Fresh enrollment uses a reusable
private-output command; CA keys stay outside the cluster. The selected capacity admits
two Computers per owner and four total, with an 8 GiB retained home each. The initial
Python/shell template has no outbound network permission. The gateway catalog exposes
Computers through explicit user and service policy. Two-replica gateway, Console and
Computers control are selected with the exact published images; Flux activation and
public user journeys are the next installed checks.
