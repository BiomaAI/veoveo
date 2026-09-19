# Agent Creation And Management Plan

Status: accepted and implementation started September 19, 2026. This is the active
delivery goal. Registry persistence is the first implementation checkpoint; API, UI,
managed provisioning and installed acceptance remain outstanding. Current component
contracts remain authoritative until their replacement lands.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Veoveo management API | Authenticated HTTP JSON, typed Rust DTOs and generated TypeScript; revision preconditions, idempotent mutations and cursor-based SSE observation are repository-owned contracts |
| Browser identity | Existing OAuth authorization-code flow with PKCE, separate Console/Workspace cookie authority and same-origin CSRF protection |
| Managed agent identity | Existing OAuth client-credentials flow with signed client assertions, explicit scopes, automated invocation provenance and current Work Context membership; managed registration is a proposed extension |
| MCP `2026-07-28` | Current [hosted-server profile](../mcp/contract/DESIGN.md), including discovery, notifications, subscriptions, Tasks and input requests; management HTTP routes are not an MCP server |
| Models | Existing repository-qualified Rig adapter and approved provider connections; the internal Chat Completions adapter does not imply support for every provider API |
| Kubernetes | Existing installation API and namespace policy for kernel workloads, retained storage and private credentials; lifecycle observation uses native watches and explicit recovery |
| Persistence | Existing platform-store transactions, outbox, current policy checks and runtime leases; new typed records require migrations |

This work introduces no dependency upgrade by itself. Any added client library or
touched dependency pin must satisfy the repository's upstream verification and
qualification rules. Provider job completion retains its existing contract; agent
management does not introduce provider status polling.

## Outcome

A permitted person can create an agent, choose an approved model, write its
instructions, select capabilities and publish it through Console or the API.
Workspace owners can then add that agent to chats in its admitted Work Context.
An operator can also create and operate a durable agent such as a UAV pilot through
the same management surface.

Routine agent changes are database operations. They do not require a source commit,
image build, Helm edit or gateway restart. Installing a new runtime template or a
new capability remains an installation operation.

Veoveo owns this feature. Bioma supplies model connections, grants, templates and
acceptance configuration; no Bioma identity, vehicle or model name belongs in the
generic implementation.

## Starting Point

| Surface | Implemented today | Change needed |
|---|---|---|
| Workspace definitions | [`VEOVEO_WORKSPACE_AGENTS`](../platform/gateway/src/bin/gateway/workspace/runs/config.rs) loaded at gateway startup | Durable editable catalog with publication and current authorization |
| Chat participants | Owner adds a definition through [Workspace routes](../platform/gateway/src/bin/gateway/workspace/runs/mod.rs); admission retains its digest | Bind an explicit published revision and disclose updates |
| Durable agents | [Kernel startup](../agents/kernel/src/bin/agent/run.rs) loads a manifest and registers the runtime record | Admit managed instances before startup and provision their resources |
| UAV packaging | [Helm](../showcase/uav-sim/deploy/helm/templates/agent-deployments.yaml) supplies isolated processes, identities and storage | Install a reviewed template; create individual pilots through the API |
| Console | [Agents view](../apps/console/web/src/views/Agents.tsx) observes state, sends messages and answers input requests | Author definitions, inspect authority and manage instances |

The chat worker and durable kernel have different context and lifetime requirements.
Keep both execution paths. Unify their management records and policy vocabulary.

## Product Decisions

| Concern | Decision |
|---|---|
| Creation modes | Offer `Chat assistant` and `Managed agent`. Each definition has one typed execution mode in the first release. |
| Default journey | Create a chat assistant from a blank form or duplicate an accessible definition. Managed execution is an explicit selection. |
| Ownership | Every definition belongs to one tenant and has an owner and home Work Context. Publication may admit additional contexts in the same tenant with the necessary management authority. |
| Delegation | Operators can grant creation and publication rights within specific contexts and model/capability ceilings. Ordinary membership alone does not grant publication or automated authority. |
| Runtime choices | Chat uses current gateway workers. Managed agents use installation-approved kernel templates. |
| Credentials | Authors select approved model connections and capability references. Secret values and arbitrary credential destinations are absent from definition input. |
| Publication | Draft edits remain private to their editors. Publish creates an immutable executable revision. |
| Updates | Existing chat participants and managed instances stay on their admitted revision until their owner/operator applies an update. |
| Revocation | Current policy, resource grants and emergency suspension override every pinned revision. |
| Computers | Agents acquire access through existing named grants; selecting the Computers capability grants no Computer access by itself. |
| Reactivity | Changes publish through the existing durable outbox and browser event streams. Observation never starts a model call. |

An agent admitted to a chat receives that chat's permitted history. It does not gain
another chat's memory or a durable pilot's global inbox. Adding an existing automated
principal directly to shared chat is outside this release. A future integration must
define both message authority and result audience before connecting those contexts.

## Domain Model

Rust owns the following types. Keep execution-specific fields in an enum rather than
optional fields whose combinations permit invalid states.

| Record | Required meaning |
|---|---|
| `AgentDefinition` | Stable ID, tenant, owner, home context, management revision, presentation, publication audience and enabled/archived state |
| `AgentRevision` | Immutable published instructions, approved model connection, exact capability selection, budgets and `Chat` or `Managed` execution settings; digest and author attribution |
| `AgentDraft` | Editable candidate with a concurrency revision, base published revision and validation findings; editing cannot change a running agent |
| `AgentInstance` | A managed runtime identity bound to one definition revision, one Work Context, template parameters, service principal, desired state and observed generation |
| `AgentLifecycleOperation` | Durable create/update/pause/resume/archive intent, request identity, phase, accepted generation, result or actionable failure; reuse existing journal/outbox patterns |

Existing Workspace participant/run IDs and durable episode/wake/Task IDs remain their
own records. A template is an installation-reviewed runtime package, not a user's
mutable definition. Model connections retain existing provider Secret references and
gain a governed authoring projection if the current catalog cannot supply one.

One definition may have many chat participants or managed instances. Each managed
instance has its own identity and retained memory. Duplicating a definition copies
only editable configuration that the caller may read; it never copies credentials,
resource grants, runtime memory or another owner's conversations.

Published revisions remain readable while referenced by work or audit evidence.
Archiving a definition removes it from new participant/instance admission and closes
publication; existing pinned users may continue under current policy. Disable is the
separate action that closes existing execution too. Physical purge is outside the
first release. Ownership transfer requires explicit authority and preserves identity.

## Authority And Capability Admission

Extend the existing gateway action-policy evaluator with separate permissions for
definition read, authoring-content read, create, edit, publish, use, instance deploy,
instance control, ownership transfer and archive. Existing message and input-request
actions continue to govern their existing operations. A route under `/admin/` does
not imply that every authorized caller must be an installation administrator.

Separate catalog visibility from editing and execution. Workspace participants see
the agent's disclosed purpose, model and capabilities. Only authorized editors see
the instruction body. Chat owners retain participant and participation controls.
An author cannot publish into a context they do not govern, transfer ownership to an
ineligible principal, or acquire service authority by changing a prompt.

Chat tool execution remains the intersection of the revision's allowlist and the
initiating human's current discovery and resource authority. The definition owner's
credentials never execute another person's chat request. A published endpoint or
tool name is not an authorization grant.

Managed instances receive distinct service principals with explicit scopes and Work
Context membership. The deployer must have authority to issue the selected automated
grant, not merely permission to create a chat definition. A context-authorized grantor
approves any authority beyond the deployer's delegable ceiling. Resource owners still
control UAV vehicle grants and named Computer grants. Audit retains the deployer and
grantor; autonomous actions carry their actual automated principal.

Model connection administration stays with installation operators. It validates
provider destinations and credential references. Ordinary authors cannot supply raw
URLs, Secret keys, environment variables, container images, shell commands or memory
migration SQL. Templates expose closed typed parameters. These restrictions keep
agent authoring separate from code deployment.

Human-friendly capability groups expand to exact admitted tool identities at
publication. A later MCP list-change event refreshes availability and validation; it
cannot silently add newly discovered tools to a published revision. Resource
subscriptions likewise require admitted URI patterns and bounded subscription counts.

Validate at publication and again at execution. Model removal, grant revocation,
membership changes, disabled definitions and exhausted budgets prevent new admission
and new dispatch. A cache invalidation is a performance optimization; correctness
must survive missed events. Use existing live authority checks and execution fences.

Apply per-run limits and installation/context quotas for model calls, output tokens,
concurrent work, active instances and retained storage. Reserve shared capacity
atomically across replicas. Report exhausted capacity before calling the model or
creating a workload. The first release reports token usage and configured limits;
it does not promise exact monetary billing across providers.

## Publication And Lifecycle

Publication validates the candidate without invoking a model. Return field-level
findings for unavailable capabilities, invalid parameters, model access and budgets.
An optional `Test` action is an explicit, bounded real run with a visible target
context and capability disclosure. It receives ordinary permissions and may have real
side effects; it must never masquerade as a harmless simulation.

Chat owners see an update diff before adopting a new revision. The diff includes
model, instructions where readable, capabilities and execution limits. An owner who
cannot read instructions sees that they changed and who published them. Publishing a
new revision does not silently widen any existing participant's behavior. An in-flight
run retains its revision, subject to current security checks. New chat admissions use
the latest enabled published revision.

Presentation-only name and description changes update without a new executable
revision. Executable changes remain visible in the adoption diff. An operator can
select an earlier retained revision for recovery when it still passes current policy;
that does not roll back memory migrations or restore revoked grants.

For managed instances, desired state is `Running`, `Paused` or `Archived`. Observed
state distinguishes provisioning, ready, busy, waiting for input/Task, draining,
paused, degraded and failed. Desired state and observed state are returned together;
accepting a mutation is not proof that it completed.

`Pause` closes new episode admission and lets the bounded current episode drain.
Retain pending wakes and Task identities. Completion evidence can still settle, but
it cannot start another episode while paused. `Resume` revalidates authority and
processes retained actionable work. When the workload is stopped, reuse or extend the
existing Task observation owner so completion is retained independently of model
execution; do not hold the kernel alive merely to generate periodic model calls.

`Stop current run` fences further model/tool dispatch. Already accepted MCP Tasks and
domain operations keep their own cancellation contracts. UI shows each cancellation
outcome and cannot claim that stopping a model stopped a vehicle or reversed an
external action. An emergency disable also closes new use and revokes managed
credentials, then attempts authorized domain cancellation separately.

An instance update drains its current episode before switching to the chosen
revision. Retain memory, identity and pending Task ownership. Template storage changes
require the template's migration qualification. Old and new workers cannot both own
the agent lease. Archiving an instance implies stopped admission and credential
revocation while preserving retained data and audit evidence.

## API And Client Experience

Use the existing gateway and browser edge. The following routes are proposed additions
under `/admin/{profile}`; final DTOs and handlers belong in their owning components.

| Route | Operation |
|---|---|
| `GET/POST /agent-definitions` | List visible definitions or create a draft |
| `GET/PATCH /agent-definitions/{id}` | Read or change authorized metadata |
| `GET/PUT /agent-definitions/{id}/draft` | Read or replace editable configuration |
| `POST /agent-definitions/{id}/validate` | Return typed publication findings |
| `POST /agent-definitions/{id}/publish` | Publish the exact validated draft revision |
| `GET /agent-definitions/{id}/revisions` | Paginated immutable revision history |
| `POST /agent-definitions/{id}/test` | Admit an explicit bounded test run |
| `POST /agent-definitions/{id}/disable` | Close admission and fence affected execution |
| `POST /agent-definitions/{id}/enable` | Restore admission after current validation |
| `POST /agent-definitions/{id}/archive` | Retire from new participant/instance admission and retain existing bindings |
| `GET /agent-models`, `GET /agent-templates` | Discover choices admitted to the author |
| `GET/POST /agents` | List governed managed instances or provision an instance |
| `GET/PATCH /agents/{id}` | Inspect state or request a revision/desired-state change |
| `GET /agent-operations/{id}` | Observe a durable lifecycle operation |

Existing `/agents/{id}/messages`, conversation and input-request routes remain the
canonical control surface for durable agents. Workspace keeps its current participant
routes and gains an explicit owner-controlled revision-update operation. Catalog
projection reads published registry entries admitted to that caller and context.

Every mutation has a client-generated request identity. Retrying the same request
returns the same result; reusing it with different content returns a conflict.
Updates supply the expected revision. Publish binds that exact draft and its current
authorization in one transaction. Creation and deployment reserve quotas in the same
admission transaction. Provisioning returns `202` with an operation ID rather than
blocking the browser on cluster readiness. Lists are bounded and cursor-paginated.
Definition creation may name a readable source revision to duplicate. Ownership
transfer is an explicit typed metadata mutation with its own policy action.

Console gains an `Agents` page with definitions and running instances. Creation asks
for name, purpose/instructions, model and capabilities, then presents the publication
audience and budgets. Managed mode adds an approved template, typed parameters and a
separate authority review. The detail page shows current revision, permissions,
activity, model usage and actionable provisioning failures. Routine setup requires no
JSON editor. Advanced capability selectors expose exact identities when needed.

Workspace gains `Create agent` for permitted authors, backed by the same domain API
through its own cookie/CSRF profile. A lightweight editor uses the same fields and
validation as Console. Users without authoring permission continue to choose from
their catalog. Creation does not automatically publish into another context or add
an agent to a chat the creator does not own.

SSE updates definition lists, availability, instance operations and chat update badges.
Streams retain bounded replay and current authorization; gaps cause an authoritative
refresh. Publish/subscribe changes propagate across gateway replicas without restart.
Display active model/tool phases using current feedback, and keep native Task progress,
input requests, cancellation and results in their existing authorized Activity view.
This feature does not change Workspace's private-result publication boundary.

## Managed Runtime Provisioning

Keep one isolated generic kernel workload per managed instance for the first release.
Reuse the existing image, runtime lease and retained-volume model. No image is built
for a new prompt or agent. A reviewed UAV template supplies its memory migrations and
closed session/vehicle parameters. Domain grants remain authoritative at execution.

Add a small Rust lifecycle manager with a separate deployment because it needs workload
and credential provisioning privileges. The gateway receives no Kubernetes write
credential. The manager consumes admitted durable intents, applies fixed templates
and records observations. It is a typed controller for this lifecycle, not a general
workflow engine, arbitrary manifest runner or new CRD framework.

Give the manager a dedicated namespace and narrowly scoped RBAC. Kubernetes RBAC alone
cannot constrain arbitrary fields of created Pods; installation admission policy must
enforce approved image digests, allowed service accounts, volume/Secret references,
resource limits and network placement. Kernel pods receive no Kubernetes API token.
Private service endpoints remain unreachable except through their admitted boundaries.

Managed OAuth registration must become durable rather than rewriting the entire
installation control-plane document for each agent. Extend the existing client and
membership resolver with a typed managed-client record bound to the instance and its
approved template. Use one effective resolver with explicit source ownership and
collision rejection. Both token issuance and subsequent request authorization check
current registration and revocation; deleting a registration cannot leave an issued
token usable until expiry. Keep private key material in the existing installation
Secret mechanism and expose only references/public keys in management state.

Provision identity, private credentials, retained storage and the workload under one
durable operation. External steps cannot share a database transaction: record each
intent and exact resource identity before dispatch, then persist the correlated
observation. Fences and idempotent resource identities prevent duplicate instances
after restart. Retry observation without repeating an uncertain side effect. Watch
recovery may re-establish the Kubernetes inventory; it never polls an MCP provider.

Cleanup removes only resources whose ownership and generation match the operation.
Retain volumes on failed provisioning and archive. A partial failure reports the
remaining resources and offers a scoped retry. There is no automatic recreation of
retained memory, silent authority escalation or automatic force-deletion of storage.

A shared multi-agent worker pool is deferred until measurements justify changing the
current isolation boundary. The registry/API does not require one to be useful.

## Migration And Configuration Ownership

Use a coordinated hard cut with one canonical registry. Import the existing Workspace
definitions and preserve participant IDs, displayed history and admitted content.
Resolve their model credentials into approved connection references. Record the
source digest and verify counts/bindings before removing `VEOVEO_WORKSPACE_AGENTS` and
its Helm path. Startup cannot overwrite definitions edited through the API.
Drain model runs and pause catalog/participant mutations for the cutover while
retaining chat history and private Task observation. Complete the import and activate
registry-aware replicas before reopening those mutations. Qualify the drain and
recovery path; old environment readers and new registry writers cannot overlap.

Bioma's Assistant and Reviewer become initial catalog records. Installation automation
may seed or update records through the same API with explicit revisions and request
identities. An operator chooses when to apply such an update; background GitOps does
not reset Console edits. Model connections, runtime templates and outer policy ceilings
remain installation configuration.

Migrate UAV pilots only after managed provisioning is qualified. Adopt existing runtime
IDs, principals, grants and retained volumes where their exact identity is provable.
Drain the old Helm-managed kernel and transfer workload ownership in a controlled
step before enabling the manager. Never leave two controllers managing one workload.
If an identity cannot be adopted, stop and require an explicit grant migration rather
than rebinding it by display name. Remove per-pilot Helm configuration after transfer;
keep the showcase's template installation and simulation configuration.

Take a targeted export of the records and configuration being migrated and prove its
restore procedure. This is migration recovery, not a new backup subsystem. Before any
new-format writes, a coordinated rollback can restore the prior configuration. After
managed edits or grants exist, recovery must preserve those records through a qualified
conversion or forward repair; rolling an old gateway over new state is unsupported.

## Implementation Sequence And Ownership

Each phase closes its API, authorization, UI and evidence loop. Phase 2 is a useful
chat release, but the full agent-management goal includes managed provisioning and UAV
migration. None of these phases is implemented by writing this plan.

| Phase | Deliverable and acceptance | Owners |
|---|---|---|
| 1. Registry and policy | Typed definitions/revisions/drafts, model-choice projection, mutation authorization, quotas, audit, events and API. Prove publication, concurrency conflicts, replica invalidation and denial cases. | `mcp/contract`, `platform/store`, `platform/policy`, focused gateway agent-management modules |
| 2. Chat authoring | Console editor, delegated Workspace editor, explicit revision adoption and registry-backed worker resolution. Import Assistant/Reviewer and remove environment-defined catalog. Create, publish and use an agent without restart. | `apps/console`, `apps/workspace`, gateway Workspace runs, Helm and Bioma seed configuration |
| 3. Managed execution | Instance API, durable managed identity resolution, lifecycle manager, safe template admission and kernel instance binding. Prove creation/recovery, pause/resume, revocation and retained memory. | `agents/runtime`, `agents/kernel`, proposed `agents/manager`, gateway identity and store, deployment chart |
| 4. UAV template and cutover | Package the existing pilot configuration as an approved template, preserve current identities/grants, transfer workloads and delete per-pilot deployment ownership. Provision a pilot through Console/API and run an authorized mission. | `showcase/uav-sim`, UAV domain grants, installation configuration and managed runtime |
| 5. Installed qualification | Real model and MCP Task flows, Computer grants, replica/process recovery, headed hardware browser acceptance, scoped release evidence and measured iteration costs at `veoveo.bioma.ai`. | Existing Rust smoke owners, client test owners and component release workflow |

Place domain persistence under a focused `platform/store/src/agent_management/`
module, with shared management DTOs separate from `mcp/contract/src/agents.rs` control
projections. Agent management handlers must not expand the existing control route file
into a mixed responsibility module. The proposed manager keeps provider-independent
lifecycle decisions separate from Kubernetes resource application.

Each new contract-bearing component gets an adjacent `DESIGN.md` before implementation.
Update Workspace run design, identity/policy documentation, Helm design, UAV packaging,
architecture decisions and CODEMAP in the phase that changes their contracts. This
plan is the cross-component sequence, not a substitute for those component documents.

## Acceptance And Performance

| Area | Required evidence |
|---|---|
| Authoring | Permitted user creates, edits, validates, publishes, duplicates and archives a definition through API and UI; stale edits conflict; forbidden authors cannot read private instructions |
| Isolation | Cross-tenant/context requests fail; publication cannot widen tool, model or service authority; duplicate copies no private runtime state |
| Chat revisions | Existing rooms retain their revision, owners see an update diff, concurrent runs retain their admitted revision, and revocation stops further dispatch |
| Tasks and feedback | Real MCP Task remains observable through navigation and worker replacement, input/cancellation stays authorized, private results stay private, no duplicate invocation on retry |
| Managed lifecycle | Restart between each provisioning step resumes one operation; quota reservations cannot be exceeded; stale workers cannot dispatch; pause/archive retains memory and Task ownership |
| Credentials | Disabled instance and revoked grant block new token issuance and already-issued token use; no private credential appears in browser responses, audit content or logs |
| Domains | A managed pilot cannot control a different vehicle; a Computer is usable only under an explicit named grant and its current limits |
| Idle behavior | An idle test spanning at least two configured heartbeat periods and a browser reconnect adds zero model calls; real operator messages, actionable resource changes and authorized Task outcomes remain valid triggers |
| Recovery | Missed catalog events, gateway replacement, manager replacement, worker crash and Kubernetes watch loss preserve admission state and expose unresolved work honestly |
| Installation | Create and use a new chat agent and a new managed pilot at `veoveo.bioma.ai`; verify Console and Workspace with headed hardware-backed graphics and record exact deployed revisions |

Target publish-to-catalog visibility at p95 under two seconds across both installed
gateway replicas. Target metadata mutations at p95 under 500 ms under a declared
local test load, excluding explicit model tests and external provisioning. These are
acceptance targets, not claims about current performance.

Record managed creation time by phase: admission, credentials, storage, scheduling,
image availability and kernel readiness. Compare it with the current manual pilot
deployment. Record image pulls and retained-storage cost per instance. A warm-path
agent creation must trigger zero image builds and zero gateway rollouts.

Run affected checks through the repository evidence recorder for implementation
changes. Keep smoke assertions in the typed Rust harness. UI behavioral tests cover
forms and authorization; visual acceptance uses the required headed GPU browser.
Use isolated test identities where a second human is unavailable and label that
evidence accurately. Pending human usability studies or broad load experiments remain
listed follow-ups; they do not replace core security, lifecycle and installed checks.

Build and deploy only affected components. Record elapsed build, publication and
rollout time plus repeated work in the existing development-iteration record. The
first chat release must not rebuild the agent kernel or simulation images. Definition
edits after installation must not invoke the release pipeline at all.

## Scope Limits

The first release includes definition authoring, governed publication, chat admission
and managed kernel lifecycle. It does not add an agent marketplace, a visual workflow
language, arbitrary uploaded runtime code, a new scheduler for timed model wakeups,
automatic inter-agent conversations, another secrets system or a new backup service.
Native MCP Tasks remain first-class work with their own authority and lifetime.
Changing their private-to-shared result contract requires a separate design.
