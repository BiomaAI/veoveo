# Veoveo Workspace Plan

Status: approved for implementation on September 15, 2026. Delivery is active;
the acceptance gates below are not yet complete.

## Standards And Protocols

Workspace uses the platform's OAuth authorization-code flow with PKCE, encrypted
same-origin browser sessions and CSRF protection. Its browser API is authenticated
HTTP JSON with Server-Sent Events for change notification. Rust owns the DTOs and
generates their TypeScript representation. This API is a Veoveo application contract;
it does not claim to be an MCP transport.

Capability execution uses the supported MCP `2026-07-28` profile, including the
Tasks extension, through the existing Rust gateway. Embedded Apps follow MCP Apps
`2026-01-26` and the explicit extensions in
[`mcp/apps-extension/DESIGN.md`](../mcp/apps-extension/DESIGN.md). Computers and
Artifacts retain their current native contracts. SurrealDB uses the repository's
qualified `3.2.4` release. New frontend dependencies receive exact qualified pins.

## Product

Workspace is the daily productivity client for people working with other people
and agents. Console remains the installation administration client. A person opens
Workspace, creates a chat, invites collaborators, adds agents and completes work
through the capabilities their organization permits.

A chat belongs to a human owner and one Work Context. Its participants can include
any admitted combination of humans and agents. A private conversation with one
assistant is one configuration of that model. Organization policy and capacity
remain the upper bound on choices made by a chat owner.

The first release is a responsive web application at `/workspace/`. It shares the
existing browser edge and public origin. Desktop packaging is later work. The
implementation must be usable at `https://veoveo.bioma.ai`; Bioma supplies the
installation configuration, while the product and code remain Veoveo.

## Decisions

| Concern | Decision |
|---|---|
| Client composition | React and TypeScript with Vite; own navigation, state, identity and interaction design. Qualify assistant-ui's external-store runtime as a presentation component. |
| Chat authority | Rust domain operations backed by the platform store. Browser role labels and component state never establish authorization. |
| Ownership | One human owner, explicit ownership transfer, owner-controlled membership and participation settings. Owners remain subject to organization policy. |
| Invitations | Invite an identified eligible human; acceptance discloses that admitted members receive the chat's shared history. No public bearer invitation grants chat access. |
| Agent identity | Invite a configured agent definition. Execute it in a separate context identified by chat and agent; do not attach a chat to the autonomous agent's global inbox or memory. |
| Agent participation | Mention or explicit assignment by default. A configured default assistant handles unaddressed requests. Automatic participation requires an owner setting and bounded triggers. |
| Multiple responses | Independent run records and output messages. Different agents can stream together while humans continue sending messages. |
| Conversation history | Shared history for current members. Selective history and message-level audiences are later work. Removal stops future access; it cannot erase copies already received. |
| Capabilities | Existing governed Tasks, Artifacts, Apps and Computers. Admission to a chat never grants resource access or execution authority. |
| Deployment | Reuse the gateway and browser edge initially. Add a process only when measured isolation or scaling needs justify it. Static-client changes must not compile Rust. |

The [assistant-ui external-store adapter](https://www.assistant-ui.com/docs/runtimes/custom/external-store)
supports application-owned messages and persistence. Its default assistant-message
joining and thread-wide running state need deliberate adaptation: use
`joinStrategy: "none"`, preserve actual author identity, and make run controls
specific to each run. Qualification starts with two humans and two agents rather
than a single-assistant demo. A component that cannot express that interaction can
be replaced without changing the chat contract.

## Authority And Privacy

Every request resolves the current authenticated human, tenant and Work Context.
The domain checks active chat membership on reads and writes. Membership changes,
invitation acceptance, message admission and run admission are transactional. A
client cannot submit another person's author identity. Foreign-chat identifiers
must not reveal whether the referenced object exists.

Only an authorized owner or explicitly permitted member may invite participants.
Invitees must independently have access to the Work Context. Agent definitions
must be admitted for that context. Adding an agent discloses its provider and
capability scope before it receives shared history. Retained messages keep their
original author attribution when a participant leaves.

An agent run records both the acting agent and initiating human. Tool authority is
the intersection of current human delegation, chat policy, the agent's admitted
capabilities and the target resource policy. Agent service credentials cannot widen
that intersection. Membership alone never supplies a Computer automation grant.
Revocation is checked again before dispatch and before exposing new results.
Cancellation requests are distinct from revoking access and do not imply undoing
an already accepted external effect.

Artifact references carry identity, not implicit read grants. Sharing a file is an
explicit governed action; the client distinguishes a readable attachment from one
requiring access. A result derived from private inputs must not automatically enter
shared history: the execution boundary must admit those inputs for the chat's
audience or keep the result private to the initiating actor. The initial release
must fail closed for unsupported private-to-shared publication.

Model credentials stay on the server. App frames keep their opaque-origin sandbox
and exact allowlists. Chat content is untrusted input, including text written by an
agent. Markdown must not execute HTML. Logs and deployment evidence exclude chat
bodies, credentials and signed Artifact capabilities.

## Persistence And Recovery

| Entity | Identity and lifecycle |
|---|---|
| Chat | Stable ID, tenant, Work Context, owner, title, archive state, settings revision and committed event sequence. |
| Membership | Chat and principal identity, human/agent kind, role and active/removed state. Agent membership refers to an admitted definition. |
| Invitation | Chat, inviter, invitee, expiry and pending/accepted/declined/revoked state; acceptance rechecks current authority. |
| Message | Stable client request identity, server sequence, actual author, reply target and typed content/attachment references. Human messages are immutable initially. |
| Run | Chat, selected agent, initiating human, triggering message, context boundary, output message, state and execution fence. |
| Task reference | Owning service and durable Task identity, exact run association and authority; Task state remains owned by its service. |
| Event | Per-chat committed sequence and typed change reference. Replay is bounded and membership-authorized. |

Message admission and its event commit together. Concurrent writers contend on the
chat's sequence head, because a database sequence allocation alone does not prove
commit order. A repeated request returns the original result only when its actor
and payload agree. A changed payload with the same request identity is a conflict.

Each run freezes a context boundary at admission. Later chat messages do not alter
an in-flight prompt silently. Per-chat agent state excludes all other chats. Runs
have independent cancellation, usage limits and deadlines. Agent-authored output
does not recursively start other agents by default. Any enabled delegation carries
an explicit parent, finite depth and shared budget.

Durable state survives a browser close and service replacement. Streams notify the
client about committed changes; reconnect reads authoritative state from the last
cursor. Gaps or expired cursors request a bounded snapshot. A browser never repeats
execution merely because a response or stream was lost. Workers use leases and
fences so a replaced worker cannot publish late output. Uncertain external dispatch
is reconciled through the owning Task authority and its idempotency contract;
unsupported ambiguous effects are surfaced, not replayed automatically.

## Experience

The client opens to the person's chats and outstanding work. A chat header identifies
the Work Context and participants. The composer offers explicit agent selection and
mentions. Messages show the real author, reply context and attachment access state.
Several active agents occupy distinct messages, each with its own status and stop
control. Sending a human message remains available during agent execution.

An activity view keeps durable Tasks visible beyond the originating chat bubble.
Input requests and approvals show who can act, what is requested and the affected
capability. Decisions use the existing Task authority. Refreshing the page restores
pending work without starting it again.

Reuse the governed upload queue and App host after removing administration-specific
assumptions. Computers is a core Workspace capability: show permitted Computers,
open terminals through existing grants and make agent delegation explicit. Load
terminal, visualization and App viewers on demand. Preserve keyboard navigation,
readable focus states and accessible authorship/status announcements.

## First-Class MCP Tasks

User priority confirmed September 15: Tasks are a release requirement for the client
experience. A model run and an MCP Task retain separate identities and lifecycles.
The client must not represent every long operation as an assistant typing indicator.

| Surface | Required behavior |
|---|---|
| In-chat activity | An authorized Task card identifies the initiating person, acting agent, capability, current state and available actions. Several tasks update independently. |
| Persistent activity | A person's activity view restores outstanding work across chat navigation, reload and browser-edge replacement. It links back to its chat/run without resubmitting the tool. |
| Progress | Preserve reported progress and status messages. Unknown progress stays indeterminate; do not manufacture percentages or an ETA. |
| Input required | Present the exact outstanding request with accessible schema-driven controls, actor eligibility and clear submit/decline actions. Preserve opaque request state on the server. Stale responses cannot decide a newer request. |
| Completion | Show typed success or failure and governed result links. A completed tool can return a domain error; a transport failure does not establish task failure. |
| Cancellation | Distinguish stopping model output, requesting task cancellation and revoking capability access. Continue showing the authoritative task outcome after a cancellation request. |
| Reconnect | Read `tasks/get` for the recorded opaque identity, then restore request-scoped task notifications. Never replay `tools/call` to recover status. |
| Privacy | Chat membership grants no Task or result access. Private operation detail remains in the initiating person's authorized activity until an explicit governed sharing path admits the audience. |

Rust owns native MCP `2026-07-28` Tasks (`tasks/get`, `tasks/update`, `tasks/cancel`)
and multi-round request semantics. The browser receives typed presentation DTOs.
Request-scoped `subscriptions/listen` supplies task-ID wakes; `tasks/get` supplies
current correctness state. Provider completion remains owned by the domain service.
The client must not add provider polling or invent a second Task state machine.

The existing App-host registry is in memory and tied to a view. It cannot be the
Workspace recovery record. Persist the task reference, invocation association and
current-authority binding before presenting an accepted operation. Any unresolved
dispatch remains explicit and fenced; recovery may not issue a second side effect.

Acceptance must exercise a real task-augmented tool, progress, an input round,
independent cancellation, reconnect and revoked access. Reload and browser-edge
replacement must preserve the same task ID. Model text fixtures and task-shaped
mock cards cannot satisfy this installed-release gate.

## Delivery Sequence

| Step | Deliverable | Acceptance gate |
|---|---|---|
| 1 | Owning designs, typed chat model, ordered schema migration and transactional operations | Real database tests for membership, invitation acceptance, cross-context denial, sequence ordering and idempotency. |
| 2 | User-scoped gateway/browser APIs and replayable change stream | Ordinary users can collaborate without administrative grants; forgery, revocation and cross-chat references are denied. |
| 3 | Workspace web entry and assistant-ui adapter | Two humans and two agents retain separate authors/messages; simultaneous updates, in-flight sending and reconnect behave correctly. |
| 4 | Isolated agent execution through shared Rust agent/MCP components | Real model responses, independent cancellation, bounded participation, restart recovery and no context or authority leakage. |
| 5 | Activity, governed uploads, Apps and Computers | A durable tool operation and input decision survive reload; readable and denied attachments behave correctly; Computer access follows existing grants. |
| 6 | Scoped build evidence and Bioma-configured deployment | Public authentication and collaboration work on the installed release; headed hardware-browser acceptance and green GitHub evidence. |

Steps may overlap where dependencies allow. An API stub, scripted model fixture or
local screenshot does not satisfy the installed-release gate. Tests use explicit
fixtures; production must not contain demo identity or canned agent-response paths.

## Build And Deploy Discipline

Record check duration, image build duration, rollout duration and unexpected rebuilds
in the existing iteration audit. Keep static assets outside Rust compiler inputs.
Reuse qualified binary layers and publish only affected components. Choose focused
checks through the evidence recorder and commit their current receipts with each
build-input change. Do not spend delivery time on unrelated benchmark expansion or
a new receipt-format project.

The intentionally paused Isaac Sim workload remains paused. Acceptance must preserve
existing user Computers and retained data. Temporary test identities and chats must
be scoped and cleaned up through supported operations.

## Progress

- Release 162 adds sandboxed Apps and the durable App-origin Task bridge. Installed
  testing found that Workspace requested only three of its already registered
  nonadministrative scopes. Listed Time Tasks were denied by current policy. The
  browser configuration now requests the approved client scope set, with a rendered
  deployment test preventing drift. Existing sessions must sign in again to receive
  those scopes; domain and Work Context policy still authorize every operation.

- Release 161 serves uploads and the native Computers view. A real public upload
  downloads byte-for-byte and restores after reload. GitHub run `35038309200`
  passes. The Computers view correctly exposes the current profile's collection,
  but existing Console-owned Computers are hidden because resource ownership
  includes profile identity. Their shared owner quota also prevents a new Workspace
  Computer. This cross-client ownership decision remains active work; installed
  Workspace terminal acceptance has not passed.

- App-origin binding is being added to the existing private operation journal.
  The database test rejects other people, other profiles and other App URIs while
  recovering the original Task. Gateway enforcement and the embedded client remain
  implementation work.

- Release 160 adds governed Task result preview and download. The original media
  Task survived the browser-edge replacement; its authorized PNG renders in the
  RTX 4090 headed browser and the download returns the same governed file. The
  edge now also has local shared upload and Computers control composition with
  Workspace-specific session authority. Their client integration remains in progress.

- Release 159 is active at `/workspace/` on veoveo.bioma.ai. Ordinary browser SSO,
  two real agent responses and an agent-created native media Task pass installed
  acceptance. The Task completed through its domain and survived browser reload
  with the identical opaque ID and one operation receipt. GitHub report run
  `35034777083` passes. Governed result actions, capability views and the remaining
  installed multi-user/input/replacement gates are still being completed.

- Agent tool execution now uses the native operation journal. Exact configured
  allowlists intersect current human discovery, repeated identical requests keep
  one Task identity, and cancelled runs cannot dispatch later actions. A real local
  model/MCP/runtime fixture passes. Private results remain in the initiating
  person's Activity; automatic analysis or chaining of private outputs is not
  supported by this first shared-chat execution boundary. Public rollout remains pending.
- Native MCP operation routes and the first-class Task client are implemented
  locally. Real protocol/runtime acceptance covers durable recovery, Task input,
  subscriptions and cancellation semantics. A headed RTX 4090 browser fixture
  exercises private chat activity, personal activity, input forms and reload
  without dispatch. Agent capability invocation and installed acceptance remain
  delivery gates; these local checks do not establish public availability.

- Private MCP operation receipts now persist dispatch intent, native Task references
  and multi-round continuation fences. Database acceptance proves single dispatch
  under contention, private ownership, stale-form rejection and late settlement
  after revocation. The native MCP client, Task UI and installed acceptance remain
  active implementation work.

- Approved design committed as `034ac8aa`.
- The first store checkpoint implements human-owned chats, explicit invitation
  acceptance, member removal, ownership transfer, separate settings revisions,
  immutable text messages and ordered event replay. Its owning design is
  [`platform/store/src/workspace/DESIGN.md`](../platform/store/src/workspace/DESIGN.md).
  The scoped suite passes 52 tests, including real two-client concurrency and
  revocation races; strict Clippy and formatting pass. This checkpoint does not yet
  expose a browser API, agent participation or a deployed Workspace.
- Existing agent control exposes an agent-wide wake/episode projection. It lacks a
  chat boundary and cannot serve as Workspace's shared history.
- The next checkpoint adds the human-scoped gateway and browser APIs, generated
  client contracts and safe people search. HTTP tests cover two collaborating
  humans, an outsider, forged authors, removed membership and current policy
  revocation. Browser-edge tests cover cookie authority, CSRF, fixed destinations,
  bounded responses and OAuth return paths. The interactive client and replayable
  stream are still being implemented; this checkpoint is not a deployed Workspace.
- The human client and contentless SSE stream are implemented. Local acceptance
  covers latest-first paged history, owner controls, two human authors, stable
  interrupted-send retries, reload and mobile layout on headed RTX 4090 WebGL.
  The assistant-ui adapter separately preserves two concurrent agent outputs and
  two human authors. These fixtures do not establish real agent execution.
- Workspace has a dedicated OAuth client/profile with native MCP Tasks and
  `operator:use` admission. Its browser routes, session cookies and authenticated
  encryption domain are separate from Console. Installation tests prove that the
  client cannot request `admin:manage` or client-credentials grants. The edge still
  shares the public origin; this is authority separation, not XSS isolation.
- Existing kernel model construction is coupled to the full analytical runtime.
  Extract or reuse the narrow execution boundary as implementation requires; do not
  pull DuckDB and Rerun into the gateway solely to produce chat responses.
- The run-store checkpoint adds per-chat agent admission, immutable prompt boundaries,
  independent run records, transactional claims, publication fences, cancellation,
  bounded concurrency and persisted interruption after worker loss. The scoped
  database suite passes 55 tests. Model dispatch and the browser run controls still
  remain implementation work; the store fixtures do not execute a model.
- The model-response checkpoint adds installation-configured agents, real Rig HTTP
  streaming, named context, per-agent recipient selection and independent stop
  controls. Local Rust and headed browser fixtures cover concurrent responses,
  cancellation and reconnect without redispatch. The initial response runner has
  no external tools; first-class MCP Tasks, capability use, production model
  configuration and installed acceptance remain required delivery work.
