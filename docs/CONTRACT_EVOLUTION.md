# Veoveo Contract Evolution

Status: accepted policy direction on 2026-09-09 following the user's request to
renegotiate contracts for ecosystem usefulness, Computers, UX, performance, and
security. This change updates contribution rules and architecture decisions.
Runtime work in the delivery table remains planned. A documented permission does
not advertise an unimplemented or unqualified capability.

## Standards And Protocols

| Boundary | Supported or planned profile |
|---|---|
| MCP `2026-07-28` | Existing canonical hosted-server profile in [the MCP contract](../mcp/contract/DESIGN.md); this decision does not change its wire version or claim additional upstream features |
| HTTP semantics, RFC 9110 | Control requests, explicit admission, bounded transfer, and transport-independent interpretation of uncertain replies |
| OAuth security, RFC 9700; native applications, RFC 8252 | Design guidance for scoped authority, safe renewal, and native-client redirects; these decisions do not certify every OAuth feature |
| WebSocket, RFC 6455; SSH | Terminal and CLI attachment transports; Veoveo lease, replay, and revocation controls are repository-owned extensions |
| gRPC and Protocol Buffers | Internal OpenShell adapter protocol pinned with the selected provider artifacts; a watch alone is not a durable delivery guarantee |
| OpenShell `0.0.116` | Reviewed handoff baseline, including explicitly recorded provider patches; broader compatibility remains unqualified |
| JSON Schema `2020-12` | Generated controlled-domain contracts and planned language-neutral test receipts |
| `veoveo.io/local-test-report/v2` | Current repository-wide local evidence format; scoped receipts are future work and have no published schema yet |
| OCI, Helm, Git content identities | Existing exact artifact publication and installation-owned deployment boundaries |
| NVIDIA GPU APIs, WebGPU, WebGL | Hardware workload and headed visual evidence remain required; headless behavior checks have a separate evidence class |
| S3-compatible object APIs | Current private Artifact storage adapter; alternative transfer endpoints require a separately qualified profile |

## Decision Method

A contract must identify the user-visible guarantee, its security boundary, the
failure semantics, and evidence that proves it. Implementation choices may vary
inside that boundary. A second mechanism needs a concrete provider or consumer
requirement and focused qualification; flexibility does not justify a generic
framework for hypothetical integrations.

Current guarantees remain in force until their replacements pass acceptance.
Existing clients receive only implemented capabilities in discovery and actionable
diagnostics for unsupported profiles. Historical delivery records describe their
tested revisions and do not override these decisions for new work.

## CE-01: Provider Completion Follows Qualified Semantics

Completion is a durable, authenticated observation of one operation. Prefer the
provider's native event mechanism. Permit definitive synchronous responses and
bounded authoritative status reads when the provider supports them. An API-only
provider may use a declared polling profile with a finite recovery budget. An
event-driven provider does not acquire an unconditional background poller.

Every adapter declares the following contract before admission:

| Property | Required definition |
|---|---|
| Identity | Tenant, provider instance, immutable resource identity, operation identity, request fingerprint, and relevant run/generation epoch |
| Dispatch | Durable intent before the call, provider idempotency scope and retention, and recovery from a lost submission response |
| Observation | Authenticated source, authoritative outcome fields, ordering, duplicate behavior, and consistency limits |
| Recovery | Resume cursor or baseline acquisition, retained history, gap detection, and exact conditions permitting a status read |
| Budgets | Per-request deadline, concurrency and request-rate limits, backoff with jitter, total recovery deadline, and operator resumption of exhausted budgets |
| Cancellation | Whether cancellation is accepted, whether execution actually stopped, and when resources or billing can be settled |
| Evidence | Persisted source identity, observed generation/cursor, outcome, timestamp, and settlement transaction; exclude credentials and provider payload secrets |

Persist observations before acknowledging redeliverable input. A stream without
acknowledgments needs a recoverable checkpoint or authoritative baseline. A current
resource snapshot can settle only the facts it proves. Reaching a desired phase
does not prove that a particular command executed, and an eventually consistent
not-found response does not prove that a submitted Create had no effect.

Missing delivery enters visible recovery. Exhausting the budget records an
unresolved outcome and preserves the resource fence. Observation retries cannot
repeat an uncertain mutation. A mutation retry requires demonstrated provider
idempotency for the same identity within its retention window, or definitive proof
that dispatch had no effect. An operator recovery action uses the same rules and
cannot clear a lock by merely labeling the Task failed.

The supplied OpenShell protocol marks event/log tails best effort and exposes lag
warnings. First qualify its watch with exact retained-instance/run correlation and
authoritative recovery reads. If those cannot resolve uncertainty, add the smallest
provider change that supplies the missing identity or evidence. A webhook bridge
does not repair a source that can already lose the necessary facts.

Kubernetes explicitly requires clients to handle expired watch history. This is a
useful precedent for declaring gap recovery, not proof that OpenShell supplies the
same guarantees. See [Kubernetes API concepts](https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes).

The existing Media provider remains on its signed-webhook profile. Extending shared
Task recovery must preserve its submission, artifact capability, cancellation, and
billing behavior. Persisted recovery-state changes require a migration and a tested
mixed-version or drained upgrade before a new adapter uses them.

## CE-02: Core Capability And Capacity Are Separate Decisions

Computers ships with supported control contracts, Console UX, diagnostics, and
installation packaging. Local, remote, and on-demand providers may fulfill the
same capability after qualification. Installation-selected capacity and policy
never become a product feature-enable switch.

The UI exposes setup-required, exhausted, maintenance, and unavailable states with
permitted next actions. Installation validation checks the declared topology and
trust references. A configured provider with missing dependencies is invalid.
An explicitly unconfigured installation can finish platform setup but cannot claim
Computers operational acceptance. The Bioma release checkpoint requires real
working capacity. Allocating a Computer must succeed before it is reported ready.

An idle installation need not run unused compute workers. On-demand control must
publish cold-start progress and preserve retained volumes while capacity is zero.
Disconnected operation packages all inputs for its selected local topology. A
remote-only provider cannot establish offline Computers acceptance.

## CE-03: Humans And Agents Use Explicit Authority

Computers uses canonical tenants, principals, Work Context, and action policy.
Principal kind does not by itself grant or deny control. Separate ownership from
the currently authorized actor. Default personal access remains private; policy
can grant narrowly scoped agent access without transferring ownership or sharing a
human session token. Record both actor and delegated authority in audit events.

Computer cardinality, resource limits, idle policy, and admitted templates belong
to installation policy. A personal default of one Computer is a quota setting;
the model and APIs support a collection. Admission enforces per-owner, tenant, and
provider capacity transactionally. Changing quota does not silently delete or stop
existing work. Group collaboration is a later explicit profile with terminal-write
arbitration and distinct observe, attach, execute, manage, and share authority.

Agent-facing control uses ordinary governed resources and Tasks, with a bounded
execution interface for automation. It returns structured status and output
references; automation does not need to parse a browser terminal. A disconnected
client cannot cause an arbitrary shell command to be rerun.

## CE-04: Long-Lived Work Uses Renewable, Revocable Access

Separate the lifetime of a Computer, its processes, and an attachment grant.
Disconnecting or expiring an attachment ends access while retained work continues
under the Computer's execution policy. Stop, cancellation, suspension, and deletion
are explicit authorized lifecycle operations with separately visible effects.

Browser attachments remain tied to their session family. CLI pairing creates a
named, scoped grant with an installation-defined idle and absolute lifetime; the
first profile is session-bound and logout revokes its access. A future persistent
device grant requires explicit consent and separate revocation UX. Background
agent authority uses its service identity and delegation, with no dependence on
a human access token remaining in memory.

Short leases renew against current grant and policy state. Revocation events
accelerate closure, and an authoritative baseline plus bounded lease limits the
effect of a missed event. A disconnected observer cannot renew from stale cache.
Check revocation and deadlines independently of socket backpressure. Loss of
authority closes input and output transport by the declared bound; it does not
silently grant extra time or destroy the user's home.

Store grant secrets as hashes where possible and protect material that must be
recovered with installation-owned encryption. Restrict every grant to its tenant,
Computer, operations, audience, and client/session binding. CLI pairing uses a
one-use expiring challenge with rate limits and explicit confirmation. Validate
loopback destinations and browser origins. No token belongs in copied shell
commands, URLs, terminal output, analytics, or routine request logs.

Use existing identity infrastructure for renewal. Apply the scoped-token and replay
protections in [OAuth security guidance](https://www.rfc-editor.org/rfc/rfc9700)
and the redirect rules in [OAuth for native apps](https://www.rfc-editor.org/rfc/rfc8252).
The stock CLI adapter documents what its client can actually enforce; do not claim
proof-of-possession or standardized device authorization for a custom pairing flow.

## CE-05: Verification Uses The Owning Ecosystem

Use Rust for service invariants, TypeScript with maintained browser tooling for
Console behavior, and the SDK language for consumer tests. `cargo xtask` remains
the entrypoint for repository-specific coordination. It dispatches an owning
harness instead of reimplementing assertions or browser actions.

Every harness declares prerequisites, bounds, cleanup ownership, and an evidence
class. Isolate tests by tenant/resource identity. Preserve failure diagnostics,
redact secrets, and avoid independent implementations of the same domain fixture.
Rust service fixtures may expose a narrow launcher contract to a browser harness.
Browser tools already implement [actionability checks and retrying assertions](https://playwright.dev/docs/actionability).

| Evidence class | Establishes | Required environment |
|---|---|---|
| Unit/contract | Pure transitions, authorization decisions, schemas, protocol bounds | Focused native test environment |
| Integration/behavioral | Real persistence, HTTP/CLI behavior, form and terminal interaction | Declared services; headless browser permitted for nonvisual behavior |
| Visual/GPU | Rendered UX, video and simulation behavior, hardware execution | Headed hardware browser where applicable and actual required GPU workloads |
| Installed/release | Candidate artifacts compose under the declared topology and policy | Exact deployed artifacts/configuration and relevant integration, visual, recovery, and isolation evidence |

Headless results never establish visual or GPU acceptance. Required GPU workflows
continue to fail closed without hardware. Keep the browser H.264 software-decode
exception exactly scoped to supported, smooth playback with an honest UI label.
Headless failure screenshots may serve as labeled diagnostics. Apply secret redaction
and retention limits to traces and captures; they cannot become product publication
assets or visual qualification evidence.

## CE-06: Evidence Reuse Follows Actual Inputs

Replace whole-repository evidence invalidation with immutable per-check receipts.
Include the check identity/version, source input manifest, transitive dependencies,
toolchain, command, environment requirements, result, and diagnostics references.
Installed receipts additionally identify runnable images, configuration, provider
profile, cluster/runtime and GPU identity when relevant, and observation time.
Content identity supports reuse across worktrees; exact source provenance remains
recorded. Secret bytes do not enter receipts or hashes exposed as public evidence.

The planner declares dependencies; a test author cannot waive them after a failure.
Shared contracts, generators, migrations, lockfiles, build scripts, feature sets,
and security policy changes broaden the affected set. Unknown inputs conservatively
invalidate checks. Runtime-dependent evidence has explicit freshness and topology
requirements; identical source alone cannot prove the current installation works.

Use per-run files with atomic publication and an aggregate index. Parallel workers
must not overwrite one report. Retain failed attempts as history and identify the
current qualifying result explicitly. A release joins coverage for the complete
selected artifact/configuration closure, including retained components and their
compatibility. Reusing a receipt cannot hide a known failure or vulnerability.

The v2 report remains the current local status mechanism until this work lands.
It is not a security attestation. Do not change the report JSON by hand to imitate
scoped validation or manufacture a green result. Implementation routing is in
[Continuous Integration](CONTINUOUS_INTEGRATION.md).

## CE-07: Qualified Versions And Explicit Transitions

New dependencies and planned upgrades prefer the verified latest stable release.
An older supported pin requires a specific compatibility or qualification reason.
Ordinary consumer edits retain the qualified dependency set. Review security and
support status on a recurring cadence; applicable fixes and unsupported versions
receive an owner, mitigation, and replacement deadline.

A provider profile records exact deployed artifacts and the behaviors they qualify.
Adding another supported version requires targeted compatibility evidence rather
than a string alias. Handshake capabilities are necessary where relevant but cannot
replace tests of retention, security, replay, and recovery. Keep provider patches
small with regression cases and an upstream/removal plan.

Internal names and models use hard cuts. Published client edges and data formats
use declared versioned transitions when consumers cannot change together. Document
the supported window, owner, migration, downgrade limits, telemetry, and retirement
condition. An adapter projects the canonical domain and policy. It cannot disguise
unsupported Tasks, weaken authentication, or select a legacy profile silently.

## CE-08: Deployment Boundaries Need A Concrete Purpose

Focused modules compose inside a process unless privileges, fault containment,
independent scale, or release cadence justify another service. Computers execution
must survive ordinary gateway/BFF rollouts. Provider administration and host-volume
access remain outside the public gateway and untrusted Computer. Those are real
boundaries; each additional controller, image, or journal needs its own reason.

UI assets must reuse unaffected Rust binaries. Provider changes must not rebuild
Console. Template defaults must not replace retained running instances. Publish
once, qualify that artifact, and install its digest. Checkpoints resume from exact
inputs after failure. No-op operations produce no workload restart.

Disk accounting distinguishes disposable build caches from retained homes, journals,
installed rollback artifacts, and pinned template inputs. Cleanup follows declared
ownership and retention. A free-space target never authorizes deleting user data.

## CE-09: Artifact Authority Can Admit Qualified Transfer Routes

Artifact service remains the authority for metadata, tenant isolation, policy,
quotas, integrity, and publication. The current same-origin streaming route remains
the supported default. Permit a future installation-owned transfer endpoint when
its enforcement and measured benefit are qualified. Keep control and discovery at
the canonical installation origin; return typed transfer descriptors to clients.

First measure the existing route using matched endpoints, payloads, concurrency,
hardware, and network paths. Prefer removing unnecessary copies and proxy buffering
before adding topology. If needed, qualify a dedicated streaming endpoint with
bounded memory and equivalent policy/lease enforcement. Presigned direct object
transfer is a separate profile decision, not the default optimization.

A direct profile must define method/object/part scope, quotas, integrity before
publication, overwrite protection, completion authority, expiry, revocation of
new requests and active streams, CORS, credential exposure, and orphan cleanup.
Keep storage credentials private. Staging bytes are unavailable as Artifacts until
the authorized commit succeeds. Reject a profile that cannot meet the selected
data policy's revocation bound.

S3 documents presigned URLs as bearer authority and checks expiration when a request
starts; a download can continue beyond that time. Short URL expiry therefore cannot
establish immediate active-transfer revocation. See [S3 presigned URL semantics](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html).
This decision introduces no public storage hostname or redirect in the current
implementation and does not make a transfer experiment a Computers release gate.

## Delivery And Decision Checkpoints

| Work | Owner and shortest implementation path | Acceptance and release relationship |
|---|---|---|
| Policy replacement | Root AGENTS, architecture decisions, this document, development/CI guidance | Delivered by this documentation change; scan active documents for conflicting universal rules |
| Provider recovery | Shared Task runtime/store and Computers adapter; see CODEMAP | Fault tests prove exact correlation, lost submission recovery, no duplicate effect, and migration; required before Computers mutation release |
| Core authority and capacity | Computers domain, gateway policy, Console, installation package | Human and service-principal journeys, private defaults, quotas, truthful setup/readiness; required for the initial supported profile |
| Renewable access | Gateway authority, BFF/Computers relay, native provider sessions | Real browser and stock CLI renew across replicas; revocation bound holds during blocked I/O; required for Computers release |
| Appropriate test tooling | Console test ownership, SDK, existing Rust fixtures, xtask dispatch | One maintained framework and shared fixture lifecycle; migrate cases that remove measured churn, preserve visual gates |
| Scoped evidence | `tools/xtask/src/commands/test_report.rs`, owning tests and CI presentation | Unrelated edits retain valid receipts; shared inputs invalidate dependents; parallel runs lose no results; full release composition rejects gaps |
| Qualified upgrade cadence | Dependency/image owners and release compatibility inputs | Record review date and support state; independent targeted upgrade changes; no new package pin in this change |
| Deployment and storage efficiency | Image planner, Computers/provider package, installation owner | Asset-only and no-op runs reuse unchanged artifacts; retained homes survive maintenance; required affected-path acceptance |
| Artifact route experiment | Artifact service, upload client, installation ingress | Matched performance/security comparison first; a separate implementation decision follows measured evidence |

The Computers sequence is in [COMPUTERS_PLAN.md](COMPUTERS_PLAN.md). Its release
must pass the supported user journey on a clean installation and on Veoveo with
the Bioma configuration. Broader provider matrices, throughput experiments, and
future CI infrastructure have separate checkpoints. Functional or security failures
in the selected profile remain release blockers; unrelated experiments do not.

Implementation checkpoint: shared Tasks now admit a separate `provider_wait` class.
Recovery preserves nonterminal state and cancellation; observation claims use the
shared lease transaction and cannot enter ordinary execution claims. The additive
stored enum requires compatible readers before admission. Computers still needs its
provider dispatch journal, budget accounting and settlement integration. The current
store-backed fixtures exercise competing replicas and unchanged existing profiles.
