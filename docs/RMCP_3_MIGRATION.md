# rmcp 3 And MCP 2026-07-28 Migration Plan

Status: approved implementation direction.

This document records the investigation and hard-cut plan for moving Veoveo from
`rmcp` 2 and the MCP `2025-11-25` profile to `rmcp` 3 and MCP `2026-07-28`. It
does not change the current hosted-server contract by itself.
[`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md) remains authoritative until
the implementing change, contract revision, deployment update, and acceptance
evidence land together.

## Standards And Protocols

| Standard or protocol | Migration boundary |
|---|---|
| Model Context Protocol `2026-07-28` | sole target protocol version for hosted servers, the gateway, first-party clients, SDKs, templates, bridges, and conformance |
| JSON-RPC 2.0 | request, response, notification, error, and identifier envelope carried by MCP |
| MCP Streamable HTTP `2026-07-28` profile | stateless POST requests, ordinary JSON terminal responses, and streaming only where the selected method requires it; legacy initialization, protocol sessions, reconnect GET, explicit session DELETE, and event replay are excluded |
| MCP Discover | mandatory server capability and version discovery replacing initialization for the final profile |
| MCP Tasks extension, SEP-2663 | server-directed deferred request execution using `tasks/get`, `tasks/update`, `tasks/cancel`, optional task notifications, opaque task identifiers, and typed terminal payloads |
| MCP multi-round tool requests, SEP-2322 | `input_required`, `inputRequests`, opaque `requestState`, and retry `inputResponses`; this replaces the legacy server-initiated elicitation path |
| MCP subscriptions | request-scoped `subscriptions/listen` with accepted filters; legacy resource subscribe and unsubscribe methods are excluded |
| JSON Schema 2020-12 | complete controlled tool schemas, including bounded same-document references and composition |
| OAuth 2.0 protected-resource and authorization-server metadata | issuer-bound authorization, resource indicators, step-up scopes, and the final MCP authorization profile |
| RFC 9207 | authorization-server issuer identification and validation |
| MCP Apps `io.modelcontextprotocol/ui`, ext-apps `2026-01-26` | separate official extension retained across the core protocol migration |
| `rmcp` `3.0.1` | audited Rust SDK baseline; implementation begins by verifying the latest stable `rmcp` release and pinning that exact version |
| Rig `0.41.0` | audited upstream agent-runtime baseline; the migration targets a fresh branch from current upstream and an exact released or justified fork revision |

The version numbers above record the audited state on 2026-07-30. Repository
dependency policy still requires rechecking authoritative upstream releases when
implementation begins. A newer stable patch replaces the audited baseline. An exact
Git revision is allowed only when an identified upstream fix has not reached a stable
release, and the reason must remain beside the pin.

Authoritative upstream sources are:

- the [MCP `2026-07-28` release](https://github.com/modelcontextprotocol/modelcontextprotocol/releases/tag/2026-07-28);
- the [MCP `2026-07-28` changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog);
- the [official Tasks overview](https://modelcontextprotocol.io/extensions/tasks/overview);
- the [`rmcp` 3.0.1 release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.0.1);
- the [`rmcp` 3.0.1 conformance roadmap](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.0.1/ROADMAP.md);
- the [Rig 0.41.0 release](https://github.com/0xPlaygrounds/rig/releases/tag/v0.41.0).

## Objective

Veoveo will have one MCP implementation profile. `rmcp` will own the standard
protocol models, lifecycle, transport, headers, Tasks methods, multi-round request
shapes, subscriptions, response caching, and ordinary schema generation.

Veoveo will own platform policy and durable behavior. The platform retains task
execution, persistence, authorization, audit, resource identity, event persistence,
provider completion, and application behavior. Those concerns are not SDK
duplication.

The migration is complete when every in-repository and supported external-facing
surface speaks MCP `2026-07-28`, the superseded protocol code is deleted, and
acceptance demonstrates stateless cross-replica behavior. A deployment that supports
both the old and new protocol is not an intermediate deliverable.

## Decisions

| Concern | Decision |
|---|---|
| Protocol compatibility | hard cut to MCP `2026-07-28`; no initialization or session compatibility mode |
| Rust SDK | one exact workspace `rmcp` 3 version |
| Terminal transport | JSON response |
| Streaming transport | used only by `subscriptions/listen`, request progress, and other methods whose final protocol flow requires a stream |
| Tasks | official Tasks wire types and methods over the durable Veoveo runtime |
| Clients without Tasks | retain the explicit direct-call adapter as a product compatibility feature; it consumes the same canonical task |
| Task IDs | accept opaque upstream identifiers; mint source-qualified Veoveo gateway task identities for durable routing |
| Task notifications | optional acceleration; `tasks/get` remains the correctness path and honors the server poll interval |
| Multi-round requests | durable final-profile request state and input responses; no legacy server-initiated elicitation |
| Subscriptions | `subscriptions/listen` over shared event sources; no stored protocol peers |
| Replicas | ordinary hosted MCP services may scale without sticky sessions after cross-call state becomes durable or explicit |
| Schemas | ordinary `rmcp` and Schemars generation with Veoveo validation bounds; no forced inlining or blanket reference ban |
| Rig | fresh implementation from current upstream, not a rebase or cherry-pick of the old draft-Tasks commit |
| Python and TypeScript | final-profile official SDK lifecycle with thin typed extension bindings only where an official released binding is absent |
| Rollout | one coordinated source and deployment cut after the complete acceptance gate |

## Audited Baseline

### Workspace dependency

The workspace currently declares:

```toml
rmcp = "2.2.0"
```

The caret requirement is not an exact reproducibility pin. Twenty-four workspace
packages consume it directly or through a feature. The consumers include the shared
contract, gateway, Console BFF, agent kernel, conformance, smoke harness, stdio
bridge, first-party servers, and showcase servers.

The published Rig dependency is not currently usable as a replacement. Veoveo pins
`rig-core` to commit `215a3cfb9ec696c5d1d62b5c5d218c377e515236` in a fork. That
commit is based on the pre-0.41 runtime layout, pins `rmcp` 2.2, and adds draft
task orchestration across the agent and MCP adapter. Upstream Rig 0.41 split the
portable contracts into `rig-core` and the classic runtime into `rig-agent`; the
root `rig` crate is the supported facade.

### Current hosted-server contract

Contract revision 2 requires:

- MCP `2025-11-25`;
- `initialize` followed by an initialized session;
- `Mcp-Session-Id`;
- reconnectable GET and explicit DELETE;
- a 60-second disconnected-session grace;
- event-stream framing for ordinary responses;
- `Last-Event-ID` replay through the transport;
- `resources/subscribe` and `resources/unsubscribe`;
- one process per logical endpoint because peers and subscriptions are process state;
- per-tool `taskSupport` values of `required`, `optional`, or `forbidden`.

These are deliberate properties of the current contract. They become obsolete under
the final protocol and must be removed rather than adapted into aliases.

### Duplicate Tasks implementations

The repository currently carries two task protocols:

1. `rmcp` 2 experimental task models and request variants.
2. `mcp/task-extension`, a custom implementation of the `2026-06-30` draft.

The custom extension implements discovery, standard-looking headers, task models,
request metadata, listen requests, JSON/SSE middleware, error codes, and client
helpers. The gateway then has a second raw `reqwest` client in
`platform/gateway/src/mcp/final_tasks.rs`, including request construction and SSE
parsing.

The final protocol differs in material ways:

- task creation is server-directed after the client advertises the extension;
- `tasks/result` and `tasks/list` are absent;
- task identifiers are opaque strings;
- `tasks/get` returns status-specific payloads;
- `tasks/update` supplies outstanding input responses;
- `tasks/cancel` requests cooperative cancellation;
- task notifications use the common subscription channel;
- the missing-capability error is part of the final protocol model;
- per-tool `taskSupport` declarations no longer govern dispatch.

Keeping either current implementation would preserve a second protocol authority.

### Session and subscription infrastructure

`mcp/contract/src/session.rs` owns the bounded local session manager, cleanup
reaper, reconnect state, and local session identity. `mcp/contract/src/transport.rs`
enables stateful mode and disables JSON responses.

`mcp/contract/src/subscriptions.rs` stores session `Peer` values and publishes
legacy resource notifications through them. Server-local modules repeat that
pattern. The gateway caches upstream `RunningService` values keyed by identity and
connects them to downstream peers.

This architecture cannot be retained under request-scoped stateless responses. A
cached upstream handler must not hold the peer of an earlier downstream request.
HTTP connection pools remain reusable, but protocol response ownership becomes
request-scoped.

### Schema infrastructure

`mcp/contract/src/schema.rs` forces tool schemas to be self-contained and rejects
references. `mcp/schema-macros` wraps the `rmcp` tool macro to select that generator.
Conformance repeats the same restriction.

MCP `2026-07-28` permits the complete JSON Schema 2020-12 vocabulary. Veoveo still
needs validation bounds for untrusted schemas, but forced inlining and synthetic
type rewriting duplicate the SDK and narrow the standard without a current product
reason.

### Client caches and lifecycle

The Console BFF maintains an `McpSessionPool` and a manual application-catalog
cache. The gateway owns upstream protocol-session caches. Both contain useful HTTP
or authorization scoping, but their session vocabulary and protocol cache behavior
belong to the old lifecycle.

The final profile uses reusable authenticated clients and connection pools without
claiming that they are protocol sessions. Protocol response metadata supplies
`ttlMs` and `cacheScope`. Private entries never cross a principal or effective
authority.

### Rig fork

The pinned Rig commit added approximately 8,600 lines. It introduced generic task
handles, task resumers, pending agent-run state, task hooks, stream events, polling,
notification routing, and server-initiated elicitation.

The useful behavior is durable deferred-tool execution. The obsolete portion is its
draft MCP mapping. Reapplying the whole commit would restore `taskSupport`,
session-bound resumers, legacy elicitation, and superseded task methods. It would
also place runtime code in `rig-core` after upstream moved that ownership to
`rig-agent`.

The existing fork remains the implementation location. A fresh branch starts from
the current `0xPlaygrounds/rig` upstream main branch. The old task branch remains
read-only reference material until the new implementation proves behavioral
parity.

## Target Architecture

### Discovery and stateless transport

Every hosted server and client targets `2026-07-28` without a fallback:

- clients call Discover instead of Initialize;
- every request carries the final standard metadata and routing headers;
- servers do not mint or accept `Mcp-Session-Id`;
- the MCP endpoint has no reconnect GET or session DELETE behavior;
- terminal calls return JSON;
- no transport replay uses `Last-Event-ID`;
- a request that requires streaming owns its stream for that request.

The shared Rust configuration explicitly disables legacy session mode. The migration
must not rely on an SDK default because the default exists to support older protocol
versions.

The gateway keeps process-wide HTTP connection pools and TLS configuration keyed by
validated transport identity. It does not keep an upstream protocol handler attached
to a downstream request peer. Internal assertions remain short-lived and are minted
for each upstream HTTP request.

### Durable official Tasks

`rmcp` owns these protocol types:

- `CallToolResponse`;
- `CreateTaskResult`;
- `Task` and `DetailedTask`;
- `TaskStatus` and status-specific payloads;
- `GetTaskParams` and `GetTaskResult`;
- `UpdateTaskParams`;
- `CancelTaskParams`;
- task status notifications.

Veoveo implements the server handlers over `platform/task-runtime` and Platform
Store. The SDK's process-local task manager is not a durable store and is not used
by deployed services.

An external task ID is a strong opaque-string type. Veoveo does not require an
external server to use UUIDv7. A first-party task may continue using a UUIDv7
Platform Store identity internally.

The gateway mints a canonical task ID and durably records:

```text
gateway task ID
source and server identity
opaque upstream task ID
principal and effective authority
installation and profile identity
created and retention metadata
```

The mapping prevents collisions between independently owned servers. Any authorized
gateway replica can route a later task operation.

### Direct-call compatibility adapter

The adapter for clients without Tasks remains supported because many deployed MCP
clients do not yet advertise the extension. It is a projection over canonical
behavior rather than a second task implementation.

The adapter:

1. advertises Tasks to the upstream server;
2. calls the selected tool once;
3. returns an immediate completed result unchanged;
4. observes a returned Task through `tasks/get`, using notifications only to reduce
   latency;
5. supplies authorized task input through the same durable input workflow;
6. returns the terminal tool result and the canonical Veoveo task resource.

It does not inspect or rewrite per-tool `taskSupport`. It does not call
`tasks/result`. It does not create another task record.

### Multi-round requests

The agent kernel and interactive clients use `InputRequiredResult` for work that
needs client-side input before completion. They persist:

- the original request;
- `inputRequests`;
- opaque `requestState`;
- the effective authority and Work Context;
- accepted input responses and their audit identity.

The retry carries `inputResponses` and the exact opaque request state. A Task in
`input_required` receives responses through `tasks/update`.

Request state is integrity-protected through the audited `rmcp` request-state
mechanism and a cluster-shared secret. Servers never trust client-edited request
state.

The existing durable agent approval and UI workflow remains. The
`McpElicitationHandler`, parked session waiters, and server-initiated elicitation
wire path are deleted.

### Subscriptions and events

`subscriptions/listen` is the only protocol subscription method.

Persistent domain signals come from Platform Store outbox records. The listener
adapter performs replay from durable cursors and uses the existing LIVE path only to
reduce delivery latency. A process-local broadcast channel is acceptable for an
explicitly single-owner live or GPU resource whose state cannot move between pods.

The common adapter maps accepted MCP filters to event ownership and publishes
through `rmcp::SubscriptionSink`. It does not store a session peer in domain state.

The protocol no longer requires singleton deployment. An endpoint remains singleton
only when a real resource-owner constraint requires it, such as exclusive GPU
capacity or a live simulation process.

### Schemas and strong types

Rust handlers use the ordinary `rmcp` tool macro and current Schemars generation.
Python handlers use the official SDK and Pydantic schema generation. Strong domain
types remain mandatory.

Veoveo validation enforces:

- the object root required for controlled tool inputs;
- complete output schemas and structured-content validation;
- bounded depth and total subschema count;
- bounded strings, arrays, maps, and numeric domains where the tool contract
  controls them;
- same-document references at untrusted extension boundaries;
- deterministic schema serialization for release evidence.

The validation does not reject a schema merely because it contains `$ref`,
`allOf`, `anyOf`, `oneOf`, or another JSON Schema 2020-12 construct.

### Cache metadata

Every discover, list, and read result receives an explicit policy:

| Result class | Default cache policy |
|---|---|
| authorization-filtered catalog or resource | `private` |
| deterministic public contract or immutable documentation | `public` when the content is independent of authority |
| task or live state | `private`, zero or short TTL |
| installation configuration | `private`, revision-bound TTL |

List ordering is deterministic. An `rmcp` client cache is scoped to one authenticated
client identity. Catalog and domain caches may remain when they cache a product fact
rather than a protocol response.

### Authorization

The gateway remains the protected resource and policy enforcement point. The final
MCP authorization profile adds these migration requirements:

- validate the authorization-server issuer through RFC 9207;
- bind client credentials and cached authorization metadata to that issuer;
- include the required dynamic-registration application type;
- prefer Client ID Metadata Documents where the selected authorization server
  supports them;
- carry RFC 8707 resource identity through initial authorization and refresh;
- accumulate and reauthorize step-up scopes without dropping prior grants.

The `rmcp` auth feature is enabled only in clients that need its OAuth flow. Hosted
servers do not acquire a full client auth dependency merely to validate the
gateway-issued internal assertion.

### Language clients

Rust uses one workspace `rmcp` version. A Rust package may not introduce a second
major version through an integration dependency.

The Python SDK uses the final-profile release of the official `mcp` package. If the
official package lacks a released typed Tasks binding, Veoveo generates strong
Pydantic models from the exact official extension schema and registers the methods
through the SDK extension API. It does not reproduce HTTP, JSON-RPC, header,
discovery, or streaming middleware.

The Console moves to the current stable final-profile TypeScript SDK. It uses
`tasks/get`, `tasks/update`, `tasks/cancel`, and `subscriptions/listen`. MCP Apps
remains a separate extension. There is no legacy SDK fallback.

## Ownership And Deletion Boundary

| Surface | Migration action |
|---|---|
| `mcp/task-extension` | delete after all callers use `rmcp` Tasks |
| `platform/gateway/src/mcp/final_tasks.rs` | delete the raw request and SSE client |
| legacy task branches in gateway tool projection | replace with one `CallToolResponse` path |
| `mcp/contract/src/session.rs` | delete protocol session ownership and cleanup |
| stateful transport configuration | replace with explicit final-profile stateless configuration |
| `mcp/contract/src/subscriptions.rs` | replace stored peers with the shared event-to-listener adapter |
| server-local subscribe/unsubscribe handlers | delete |
| `mcp/schema-macros` | delete after handlers use the ordinary `rmcp` macro |
| forced schema inlining and no-reference conformance rules | delete |
| `mcp/task-contract::ProtocolTaskId` | replace with an opaque external ID and canonical gateway-owned task identity |
| `TaskRetentionPin` | move to `platform/task-runtime` if it remains a runtime policy |
| `mcp/task-contract` | retire if no protocol-independent types remain after the move |
| Console `McpSessionPool` | replace with an auth-scoped client pool |
| manual protocol catalog caches | replace with final cache metadata where they do not own a product fact |
| `resources/subscribe` and `resources/unsubscribe` policy actions | replace with accepted `subscriptions/listen` filters and resource authorization |
| `tasks/result`, `tasks/list`, and `taskSupport` | delete from models, policy, apps, tests, examples, and docs |
| custom MCP routing headers and version metadata | delete; `rmcp` owns them |
| Platform Store task records and `platform/task-runtime` | retain |
| gateway auth, policy, internal assertions, and audit | retain |
| provider webhook completion | retain |
| domain outbox, replay, and LIVE acceleration | retain |
| MCP Apps typed host policy and UI resources | retain |
| direct-call task adapter | retain with final protocol behavior |

Deletion is part of each migration concern. The repository does not land a new
implementation and leave an inactive old module behind for possible compatibility.

## Rig Migration

### Fresh upstream base

The existing `rozgo/rig` fork remains sufficient. GitHub does not need another fork
or repository.

The migration creates a new branch, such as `feat/rmcp-3-tasks`, from current
`0xPlaygrounds/rig` main. It does not rebase or cherry-pick
`215a3cfb9ec696c5d1d62b5c5d218c377e515236`.

The old branch remains available only to compare behavioral requirements:

- durable pending-tool state;
- cancellation and terminal-state handling;
- parallel task drain behavior;
- blocking and streaming parity;
- hook and evidence events;
- agent-run serialization and resume.

Draft MCP models and methods are not copied.

### Upstream contribution stack

The work is split into reviewable upstream concerns:

1. Upgrade Rig's MCP client to `rmcp` 3, Discover, stateless requests,
   `subscriptions/listen`, final response types, and explicit connection ownership.
2. Add a protocol-neutral deferred-tool contract to `rig-agent`: terminal results,
   opaque serializable descriptors, live handles, input-required state,
   cancellation, and an application-provided resolver.
3. Teach `AgentRun` and `AgentRunner` to persist and resume deferred tools with
   blocking and streaming parity.
4. Map official MCP Tasks and multi-round requests onto the generic contract using
   `tasks/get`, `tasks/update`, and `tasks/cancel`.

Rig does not receive Veoveo persistence, authorization, audit, resource URIs, Work
Context, or provider-webhook behavior.

### Connection ownership

Published Rig currently registers an MCP tool with a cloned `ServerSink`; the caller
must separately keep `RunningService` alive. The new API returns or requires a
connection guard that owns the running client, listener task, and registrations.
Tools hold a lightweight handle. The application holds the guard for the lifetime of
the remote catalog.

The handle can issue stateless requests and resolve Tasks without claiming to be a
protocol session.

### Veoveo cutover

VeoVeo pins each development checkpoint to an exact fork revision. After the upstream
stack is released, Veoveo replaces the Git pin with the exact crates.io `rig`
release and the `rmcp` feature.

The 0.41 split moves agent imports from direct `rig-core` ownership to the `rig`
facade or `rig-agent`. Veoveo uses the facade unless a measured compile or feature
boundary requires direct packages.

If Rig maintainers decline the generic deferred-tool contract, Veoveo uses published
Rig's serializable `AgentRun` state machine and drives its `CallTools` steps through a
small Veoveo adapter. That fallback does not restore the old draft MCP fork. It does
mean Veoveo, rather than Rig's high-level runner, owns tool scheduling.

## rmcp Upstream Gate

The `rmcp` 3.0.1 audit found broad final-profile support, including Discover,
stateless Streamable HTTP, standard headers, cache metadata, multi-round requests,
official Tasks models, task operations, and subscriptions.

The audited release does not route task-status notifications through
`SubscriptionSink` because the core accepted filter lacks task IDs. The tagged
roadmap also records incomplete client and server conformance against the then-current
alpha conformance suite.

The migration therefore requires:

1. check the latest stable `rmcp` patch for the task-filter and conformance fixes;
2. contribute the narrow task subscription change upstream if it is still absent;
3. pin an exact upstream revision only when the accepted fix is not released;
4. supplement upstream conformance with Veoveo task-notification acceptance;
5. remove the Git pin when the fix reaches a stable release.

This gap does not justify preserving `mcp/task-extension`.

## Implementation Plan

The phases below describe dependency order. They are not supported deployment modes.
Each phase ends in a coherent commit, while the branch remains unshipped until the
complete gate passes.

### Phase 0: Upstream prerequisites

- Create the fresh Rig branch from current upstream.
- Revalidate the current stable Rig and `rmcp` releases.
- Resolve `rmcp` task subscription routing and final conformance gaps.
- Land or pin the Rig `rmcp` 3 client foundation.
- Land the protocol-neutral deferred-tool and final Tasks stack.
- Record exact upstream and fork revisions used by Veoveo.

### Phase 1: Contract revision 3

- Rewrite `mcp/contract/DESIGN.md` for MCP `2026-07-28`.
- Increment `CONTRACT_REVISION`.
- Update `docs/CODEMAP.md`, architecture documents, server compliance sections, and
  MCP Apps design.
- Replace the compliance checklist entries for sessions, subscriptions, tasks, and
  schemas.
- Update gateway fragments and compatibility manifests with the final protocol and
  contract revision.
- Add an enforcement rule that rejects the superseded protocol vocabulary.

The implementation change and normative revision land together. Revision 3 is never
published while revision 2 behavior remains deployed.

### Phase 2: Shared stateless transport

- Pin one exact `rmcp` 3 version in the workspace.
- Replace the canonical server and client configuration.
- Migrate Discover and final request metadata.
- Remove initialization, session IDs, GET reconnect, DELETE, replay, and session
  cleanup.
- Separate HTTP/TLS pooling from protocol request state.
- Migrate all hosted Rust servers, the stdio bridge, gateway, Console BFF,
  conformance client, and smoke support.

### Phase 3: Official durable Tasks

- Implement official task handlers over `platform/task-runtime`.
- Add the durable gateway task mapping and opaque upstream IDs.
- Migrate every task-producing server.
- Rewrite the direct-call compatibility adapter.
- Migrate Console and agent task consumers.
- Delete both old task protocols and the obsolete shared task wire types.

### Phase 4: Subscriptions and replicas

- Implement the event/outbox-to-`SubscriptionSink` adapter.
- Migrate resource, catalog, task, progress, and domain notification paths.
- Delete stored peer registries and legacy resource subscription handlers.
- Remove the universal singleton deployment rule.
- Enable at least two gateway replicas and one ordinary hosted-server replica in
  acceptance.
- Retain explicit singleton settings only for workloads with a documented resource
  owner.

### Phase 5: Multi-round agent input

- Implement durable `inputRequests`, request state, response, authority, and audit
  records.
- Migrate agent approvals and interactive inputs.
- Implement in-task input through `tasks/update`.
- Remove parked elicitation waiters and old Rig elicitation callbacks.
- Prove request and task input survive process restart.

### Phase 6: Schemas, caching, headers, and authorization

- Move every Rust handler to the ordinary `rmcp` schema path.
- Delete `mcp/schema-macros` and the forced-inlining generator.
- Add bounded JSON Schema 2020-12 validation.
- Set explicit TTL and cache scope on discover, list, and read results.
- Delete custom standard-header construction and validation.
- Implement the final issuer, resource, registration, and step-up requirements.
- Align dependent cryptography and HTTP crates with the selected `rmcp` feature set.

### Phase 7: Python, TypeScript, templates, and external artifacts

- Pin the latest stable final-profile Python and TypeScript SDK releases.
- Generate a thin Python Tasks binding only if the official release lacks it.
- Migrate the Console bridge and MCP Apps task methods.
- Update the Python server template and source-free fixture.
- Rebuild standalone conformance and composer artifacts.
- Publish a compatibility manifest that names contract revision 3 and MCP
  `2026-07-28`.

### Phase 8: Deployment and final deletion

- Update Helm values, probes, replica settings, and gateway configuration.
- Build and publish every selected image from one committed revision.
- Deploy the complete final-profile platform.
- Run the acceptance matrix.
- Search the source, schemas, rendered charts, generated documentation, and web
  bundles for obsolete protocol surfaces.
- Remove the old Rig branch after the published replacement and Veoveo cutover are
  independently recoverable.

## Acceptance Matrix

| Area | Required evidence |
|---|---|
| Dependency graph | one exact `rmcp` 3 version; no second major through Rig or another package |
| Discovery | every client and hosted server completes the `2026-07-28` Discover lifecycle without Initialize |
| Transport | no session header, reconnect GET, session DELETE, or Last-Event-ID; terminal results are JSON |
| Official conformance | server and client suites pass with zero Veoveo-allowlisted failures |
| Tasks | immediate, working, input-required, completed, failed, cancelled, TTL, poll interval, update, and cancel paths pass |
| Task notifications | a task listener observes authorized status changes; loss of an optional notification does not prevent correctness through `tasks/get` |
| Compatibility adapter | a client without Tasks receives the canonical terminal result and task resource without invoking an alternate task store |
| Opaque IDs | two servers may return the same upstream task string without a gateway collision |
| Replica routing | a task created through one gateway replica is read, updated, cancelled, or completed through another |
| Restart | a gateway or server restart between task creation and observation does not lose canonical state |
| Multi-round request | direct and in-task input survive serialization, authorization recheck, restart, and retry |
| Subscriptions | persistent events replay from the outbox and live events arrive through one accepted listener filter |
| Stateless scaling | two gateway replicas and one replicated ordinary MCP server pass without sticky sessions |
| Schemas | bounded same-document references and composition validate; malformed, cyclic-over-limit, or resource-exhausting schemas fail closed |
| Caching | private entries never cross principals; immutable public content honors TTL and invalidation |
| Authorization | issuer, resource, refresh, and step-up behavior pass the final profile |
| Rig | blocking and streaming agent runs have identical task semantics; serialized pending runs resume against reconnected task handles |
| Python and TypeScript | SDK, template, Console, and MCP Apps use only final methods and response shapes |
| Extensions | anonymous external conformance and integration fixtures consume the published revision-3 artifacts without Veoveo source |
| Deployment | rendered charts carry the intended replica and GPU-owner constraints; images and charts use immutable release identities |
| Repository | workspace tests, Clippy, docs, Python tests, frontend tests, Rust smokes, profile acceptance, showcases, and GPU visual acceptance pass |
| Hard cut | searches find no `2025-11-25`, `2026-06-30`, `Mcp-Session-Id`, `tasks/result`, `tasks/list`, `taskSupport`, `resources/subscribe`, or `resources/unsubscribe` outside explicit historical migration evidence |

The official conformance runner may lag an SDK release. Veoveo supplements missing
scenarios but does not mark an upstream failure expected merely to make the gate
green.

## Risks And Controls

### SDK release timing

A required `rmcp` or Rig fix may not have reached crates.io. The temporary control is
an exact Git revision containing an upstreamable narrow change. Veoveo does not
maintain a private protocol clone.

### Task notification incompleteness

Task notifications improve latency and do not establish completion. The client always
supports bounded `tasks/get` observation using the server's poll interval.

### Cross-principal cache leakage

Final cache metadata introduces useful reuse and a new isolation obligation. Client
instances and private cache entries are keyed by effective authorization. Acceptance
uses distinct principals with overlapping catalogs and different allowed results.

### Agent runtime regression

The previous Rig fork contains valuable task lifecycle behavior even though its
protocol mapping is obsolete. The fresh implementation extracts its behavioral tests
before the old branch is retired. Blocking and streaming paths share one deferred-tool
state machine.

### Hidden session dependence

Tests that reuse one connection may conceal process-local state. Replica acceptance
changes the serving pod between requests and terminates a replica during active task
and multi-round workflows.

### Coordinated client cut

The platform does not retain an old MCP compatibility endpoint. First-party clients,
templates, SDK releases, gateway configuration, and servers ship as one compatibility
release. The direct-call task adapter remains because it is final-protocol client
adaptation, not an old protocol endpoint.

## Completion

The migration is done when contract revision 3 is the only published hosted-server
contract, every selected package and deployed image uses the final protocol, the
acceptance matrix passes from committed source, and the deletion search is clean.

The final implementation report records:

- exact Veoveo, Rig, and `rmcp` revisions;
- official conformance versions and complete results;
- workspace and language dependency locks;
- task, multi-round, subscription, and replica evidence;
- deployed image and chart digests;
- full showcase and visual acceptance evidence;
- the list of deleted protocol modules.
