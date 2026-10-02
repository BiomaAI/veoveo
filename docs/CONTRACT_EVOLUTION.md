# Veoveo Contract Evolution

Customization happens in forks, as described in
[`FORK_DEVELOPMENT.md`](FORK_DEVELOPMENT.md). The MCP protocol surface, runtime policy,
and separate component deployments are the same for fork code as for upstream code.

Status: accepted policy direction on 2026-09-09 following the user's request to
renegotiate contracts for ecosystem usefulness, Computers, UX, performance, and
security. The decisions govern contribution rules and architecture boundaries.
Provider observation, retained Computers, renewable access and scoped source evidence
now have implementation checkpoints below. Complete release closure and additional
profiles retain their own gates. A documented permission does not advertise an
unimplemented or unqualified capability.

## Standards And Protocols

| Boundary | Supported or planned profile |
|---|---|
| MCP `2026-07-28` | Existing canonical hosted-server profile in [the MCP contract](../mcp/contract/DESIGN.md); this decision does not change its wire version or claim additional upstream features |
| HTTP semantics, RFC 9110 | Control requests, explicit admission, bounded transfer, and transport-independent interpretation of uncertain replies |
| OAuth security, RFC 9700; native applications, RFC 8252 | Design guidance for scoped authority, safe renewal, and native-client redirects; these decisions do not certify every OAuth feature |
| WebSocket, RFC 6455; SSH | Terminal and CLI attachment transports; Veoveo lease, replay, and revocation controls are repository-owned extensions |
| gRPC and Protocol Buffers | Internal OpenShell adapter protocol pinned with the selected provider artifacts; a watch alone is not a durable delivery guarantee |
| OpenShell `0.0.116` | Reviewed handoff baseline, including explicitly recorded provider patches; broader compatibility remains unqualified |
| JSON Schema `2020-12` | Generated controlled-domain contracts |
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

Implementation includes reviewing whether the contract can express the accepted
architecture. Owners may refine outdated contracts and implement the required changes
without waiting for the user to identify each gap. Explain material tradeoffs, update
the owning design and active plan, and record a decision here when ownership or a
repository-wide guarantee changes. Qualification governs when the new behavior can be
advertised.

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

## CE-06: Checks Run Locally Without Recorded Evidence

Developers run the checks a change touches with native commands before committing it.
The repository records no test results and has no CI. GPU CI workers are the intended
replacement; [Continuous Integration](CONTINUOUS_INTEGRATION.md) describes that
direction.

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

## CE-10: Repository Identifiers Use The Veoveo Domain

Veoveo owns `veoveo.ai`. MCP extension identifiers and `_meta` keys use the
`ai.veoveo/` prefix, which follows the MCP reverse-DNS rule. Schema, format, evidence,
and Kubernetes keys use `veoveo.ai/`, and OCI labels use `ai.veoveo.`. Installation
domains never appear in core identifiers.

Identifiers under `io.veoveo`, `veoveo.io`, and `ai.bioma.veoveo` move to these forms
in one coordinated hard cut. The cut ships no adapters, aliases, or readers for old
names. A component that receives an old identifier rejects it with a diagnostic that
names the replacement. The reference installation deletes its store, object storage,
recording journals, and retained Computers state and reinstalls from the new lock.
Every other store, including local development stores, is recreated. Migrations `0031`
and `0032` therefore change in place, and fork servers release with the new
identifiers before they rejoin an installation. The
[implementation plan](PLATFORM_FOUNDATIONS_PLAN.md#phase-1-identifier-hard-cut)
lists every identifier and the derived identities that change with it.

## CE-11: Knowledge Reaches Agents Through Resources

Servers that publish knowledge expose it as MCP resources and remain the system of
record for their domains. Adoption is optional and follows a concrete discovery or
reuse need. A hosted server can expose ordinary resources without knowledge
collections. The
[`ai.veoveo/knowledge-source`](../mcp/knowledge-extension/DESIGN.md) extension marks
collections, and every read of a member returns a typed observation with a revision,
content digest, modification time, and access descriptor. The gateway's audit records
for reads of declared collections carry the observed revisions, so Veoveo keeps one
audit log and no separate read ledger. The `knowledge-mcp` index caches content for
search and returns links to the owning resources. It never becomes a second source of
truth.

Observations explicitly describe the source's read policy. The recorded Work Context
does not imply sharing: tenant-wide, owner/grant, context-sharing, selected-context
sharing and context/profile-constrained subject policies are distinct typed variants.
Sources may require selected-context membership without an owner or grant shortcut.
Collection declarations carry the source's required typed scope names.
Typed read grants carry optional deadlines; a record deadline applies to every read
path. The index enforces these conditions in SQL before decoding and pagination. A
domain whose policy cannot be expressed must extend the shared contract before it
declares an indexable collection. The initial extension is undeployed and adopts this
wire model through a hard cut; experimental indexes are rebuilt without compatibility
adapters.

Connectors to external systems follow the same contract. A connector projects vendor
records as resources, declares its collections, and names the external record in each
observation. It reads with an installation-owned credential, and Veoveo enforces the
source's access rules through the observation's access descriptor. Per-user delegation
to external systems requires its own decision.

## CE-12: One Audit Record Per Logical Action

Veoveo keeps one audit log, specified in [the audit design](AUDIT.md). The gateway,
the Artifact service, simulator live views, Computers, and upload publication write the
same typed record through one writer library. The earlier separate paths end with
this decision.

A request writes one record for authentication, authorization, and its outcome. A
tool call adds one completion record. A discovery list writes one record with the
visible-set digest in place of one record per item, because the policy revision on the
record reproduces every item decision. A streamed session writes its opening, its
summary, and every denial, in place of one record per chunk or range request. Every
denial and every authorization issuance still requires a committed record.

Sealed blocks with Merkle roots and signed heads make tampering detectable. Retention
is an installation parameter with no default, and a configured export must receive a
block before its records can be deleted. Audit stays in the platform store beside the
records it references, and readers follow it through LIVE queries with change-feed
recovery rather than polling. The replacement is a hard cut: the new table replaces
`audit_event`, and existing audit rows are discarded.

## CE-13: Modular Types And Server-Owned Contracts

Scope names and resource identities are Veoveo concepts used by protocol adapters.
The accepted foundation is a small `veoveo-types` crate under `platform/types`, owning
validated names, platform identity and attribution, generic resource URI handling,
and protocol-independent traits. Identity includes principal, tenant, group, role,
Work Context and delegation IDs, policy labels and versions, access subjects, and
invocation provenance. Resolved invocation authority, Work Context membership levels,
output defaults and capability levels also belong here. Domain contracts can retain the
complete authority value without importing MCP. Configuration, authenticated membership
matching and access decisions stay with their existing owners; serializing a claimed
authority does not establish permission. Native Task UUID identity also belongs here, with Store owning
record conversion and TaskRuntime owning lifecycle and external lookup admission.
Opaque MCP handles keep their protocol-owned profile. Authentication and authorization stay with their existing
owners; domain-owned Artifact and coordinate contracts do not belong in this foundation.
It has no dependency on RMCP, a server runtime, a database client, a GPU backend, or
an individual server. MCP-specific traits and descriptor conversion stay in
`mcp/contract` and consume the foundational types.

Each server library owns its closed scope enum, domain IDs, resource variants, and
public request and response types. A Rust server exposes these through a `contract`
feature that builds with default features disabled. Runtime modules and their dependencies
are optional, and binaries require the runtime features. Tests, CLI tools, and
cross-server consumers use the owning library. A separate contract crate needs a
concrete dependency or independent release requirement.

Media owns its prediction summaries and generation result DTOs. Protocol utilities
consume Media's contract feature directly; extracting those DTOs preserves their
published schema and gives MCP core no dependency on Media.

Recording's [domain contract](../platform/recordings/contract/DESIGN.md) owns its
public models, UUIDv7 IDs, resource addresses and catalog positions. The MCP server’s
isolated contract feature exposes those types to cross-server consumers. Hub and Video
import the domain crate directly. A separate crate resolves the concrete dependency
cycle: the MCP runtime uses Hub implementations, while Hub publishes the same public
Recording identities. The gateway builds policy targets through the owner’s URI type.
MCP core owns no Recording DTOs or resource vocabulary and imports neither package.
SQL authorization and Store cursor conversion stay with the runtime owners.

UAV owns its simulator and live-view v4 model in the server library's isolated
contract feature. Flight clients consume those types directly. Gateway identity
conversion stays in UAV's authenticated adapter; the public model takes foundational
identities. Schema publication selects the live-view models without adding a UAV
dependency to MCP core or including vehicle schemas in those published artifacts.

Frames' [domain contract](../platform/frames/contract/DESIGN.md) owns its public spatial
values and resource builders. The server's isolated `contract` feature re-exports that
model. Recording projections import the domain crate because Frames runtime depends
on RRD, which already imports Recording's domain contract. The separation resolves
this concrete cycle without domain dependencies in MCP core. Revision-scoped frame
references describe coordinates; their type does not establish resolution or authority.

The gateway preserves domain payloads and unknown extension metadata. Its
[forwarding contract](../platform/gateway/DESIGN.md) projects only explicit protocol
addresses and the MCP Apps resource link. A new producer's URI vocabulary requires no
gateway change. Namespace conversion inside a domain value belongs to its adapter,
which must preserve identity and digest relationships.

The domain runtime owns SQL that implements its authorization and persistence rules.
It uses Store's connection, record primitives and transaction facilities; the shared
schema catalog stays in Store. Domain driver records and queries belong behind the
runtime feature, where they can use the owning contract without making Store depend
on a server. Shared platform policies, such as Task ownership, stay in their platform
runtime and expose typed operations that declare the authority they establish.

The Artifact plane's common model lives in `platform/artifacts/contract`. Its MCP
server depends on the plane's HTTP client, so placing that client's required model
in the server package would create a Cargo dependency cycle. The separate library
owns Artifact identities and metadata below both adapters; it adds no process.

Lexical resource selectors belong to `veoveo-types`. Gateway policy and Store consume
the same scheme, prefix and restricted-template vocabulary. The shared policy evaluator
can return an owning scheme intersected with profile selectors for database admission;
consumers apply the complete selection before ranking and pagination. Server-owned
resource builders and RFC 6570 expansion keep their separate responsibilities.

The foundational `AccessGrant` trait supplies subject, level and expiry to the shared
access evaluator. Domain grant records implement it without importing MCP. The generic
`AccessRequest` names resource tenant and labels and borrows the owner's grant type;
Knowledge can reuse the Artifact service's policy evaluator without manufacturing an
Artifact identity. This is an internal hard cut with unchanged authorization behavior.

Scope mapping has one declared wire spelling per variant. Generic policy accepts
validated names because an installation may introduce scopes unknown to core. Domain
authorization helpers accept the owning enum. A caller's grants remain a set of
validated names; unrelated scopes do not need conversion into a server's closed enum.
Resource variants carry their specific ID and query types. Domain builders select
route shapes, while a maintained URI library handles component encoding and parsing.
Builders validate field combinations before exposing usable values. Authorization
and persisted parent relationships still require current policy and database checks.

`ResourceUri` validates concrete references with the existing RFC 3986 parser;
`ResourceTemplateUri` owns RFC 6570 declarations and expansion. Malformed escaping
and unexpanded templates fail at concrete construction and deserialization. The
generic reference permits network resource URLs with ports; server-owned routes use
the stricter component profile before domain admission. No historical-text fallback
enters the concrete type.

Shared extension traits are public and open to external implementations. Core has
no exhaustive domain scope or resource registry. A new server adds its own library
and registration data without changes to foundational or MCP core source. An external
fixture must prove this, including a consumer that imports only the server contract.

Compiler checks establish typed construction and API requirements. Shared server
setup provides protocol machinery. Hosted conformance reports and owner-local tests
qualify behavior such as authorization, SQL filtering before limits, and recovery.
A marker trait cannot certify these behaviors. Python and other SDKs follow the same
ownership and wire contracts through their language-native types and validation.

This decision preserves current wire spellings and policy semantics during extraction.
Internal callers move to one implementation without compatibility aliases. The
Foundations rollout is an explicitly coordinated hard cut across servers, clients
and stored formats. Its reference data is disposable. Ship one current contract and
remove historical readers, migration shims and compatibility-only tests introduced
during that work. Current-format task restart, authorization and failure recovery
still require qualification. Implementation and remaining qualification are tracked in the
[foundations plan](PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

## Delivery And Decision Checkpoints

| Work | Owner and shortest implementation path | Acceptance and release relationship |
|---|---|---|
| Policy replacement | Root AGENTS, architecture decisions, this document, development/CI guidance | Delivered by this documentation change; scan active documents for conflicting universal rules |
| Provider recovery | Shared Task runtime/store and Computers adapter; see CODEMAP | Fault tests prove exact correlation, lost submission recovery, no duplicate effect, and migration; required before Computers mutation release |
| Core authority and capacity | Computers domain, gateway policy, Console, installation package | Human and service-principal journeys, private defaults, quotas, truthful setup/readiness; required for the initial supported profile |
| Renewable access | Gateway authority, BFF/Computers relay, native provider sessions | Real browser and stock CLI renew across replicas; revocation bound holds during blocked I/O; required for Computers release |
| Appropriate test tooling | Console test ownership, SDK, existing Rust fixtures, xtask dispatch | One maintained framework and shared fixture lifecycle; migrate cases that remove measured churn, preserve visual gates |
| Qualified upgrade cadence | Dependency/image owners and release compatibility inputs | Record review date and support state; independent targeted upgrade changes; no new package pin in this change |
| Deployment and storage efficiency | Image planner, Computers/provider package, installation owner | Asset-only and no-op runs reuse unchanged artifacts; retained homes survive maintenance; required affected-path acceptance |
| Artifact route experiment | Artifact service, upload client, installation ingress | Matched performance/security comparison first; a separate implementation decision follows measured evidence |
| Modular types and server contracts | `platform/types`, shared MCP integration, and each server library | Foundations Phase 3: foundation, Time, Map, and Frames contract isolation pass independent consumer checks; Time resources/cursors and Frames world addresses use shared URI components; Time and Frames own typed persistence interfaces and private driver records; Map owns geodetic names and RRD owns its recording metadata; gateway completion and discovery use typed templates; the [unified audit contract](../platform/audit/contract/DESIGN.md) preserves native Task and opaque route identities. Concrete URI admission passes on the reference installation. Map consumes checked setup and typed direct authoring addresses. Remaining Store domain keys, template adoption, DTO relationships and owner-specific installed qualification are pending |

The [Computers design](../platform/computers/DESIGN.md#qualification-limits) owns its
qualification gates. Its release
must pass the supported user journey on a clean installation and on Veoveo with
the Bioma configuration. Broader provider matrices, throughput experiments, and
future CI infrastructure have separate checkpoints. Functional or security failures
in the selected profile remain release blockers; unrelated experiments do not.

Implementation checkpoint: shared Tasks now admit a separate `provider_wait` class.
Recovery preserves nonterminal state and cancellation; observation claims use the
shared lease transaction and cannot enter ordinary execution claims. The additive
stored enum requires compatible readers before admission. Computers now journals dispatch, charges a persisted observation budget and commits
domain settlement before shared Task projection. The worker integrates those boundaries
with the native runtime and production retained allocator. Current dispatch authority
and public action/read projection use the same fresh policy and directory snapshot.
Durable browser, CLI and named automation grants now have installed public
observations. Agent execution publishes governed output Artifacts and rejects a
revoked grant or a different Computer. Governed Artifact import/export and named
Start/Stop authority also have installed acceptance. The
[Computers design](../platform/computers/DESIGN.md#qualification-limits) records the
qualification limits. Clean/offline release closure, broader provider
qualification and full installed-evidence composition remain work. Store-backed
fixtures exercise competing replicas and unchanged existing profiles.

The shared evaluator now lives in `platform/policy`. Gateway policy calls delegate to
that implementation, while background services can validate a revision without importing
the gateway's agent runtime or analytics dependencies. This extraction preserves policy
semantics. Authentication, current Work Context, grant/session revocation and authoritative
revision freshness remain required inputs to a Computer access or dispatch lease.

The MCP contract now follows the final Tasks extension handshake. The former
per-tool task-support requirement was stale documentation from the removed core
Tasks model. The existing pinned runtime already uses per-request extension
capabilities; the Computers facade must reject missing support before accepting
durable work. This correction adds no alternate protocol version or provider profile.
