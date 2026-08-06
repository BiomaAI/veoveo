# rmcp 3 And MCP 2026-07-28 Migration Plan

Status: approved implementation direction.

Revalidated: 2026-08-06 against Veoveo `3dba3913`, the complete official MCP
`2026-07-28` changelog, and the current stable upstream releases named below.

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
| MCP extension negotiation | per-request typed capability intersection for Tasks, MCP Apps, authorization extensions, and admitted Veoveo extensions |
| MCP OAuth Client Credentials extension | `io.modelcontextprotocol/oauth-client-credentials` for admitted machine clients, narrowed to `private_key_jwt` by the Veoveo profile |
| JSON Schema 2020-12 | complete controlled tool schemas, including bounded same-document references and composition |
| OAuth 2.1 draft 13 and RFC 6750 | bearer-token authorization profile for HTTP-hosted MCP |
| RFC 8414, RFC 9728, and OpenID Connect Discovery 1.0 | protected-resource and authorization-server discovery with issuer-bound metadata |
| RFC 9207 | authorization-server issuer identification and validation |
| RFC 8707 | canonical MCP resource indicators carried through authorization and token requests |
| RFC 7523 | `private_key_jwt` authentication for installation-owned confidential machine clients |
| W3C Trace Context and W3C Baggage | standard MCP trace propagation; baggage is untrusted observability input and never authorization state |
| RFC 9110 | standard MCP request-header syntax, matching, size rejection, and transport behavior |
| MCP Apps `io.modelcontextprotocol/ui`, ext-apps `2026-01-26` | separate official extension retained across the core protocol migration |
| `rmcp` `3.1.1` | audited Rust SDK baseline; implementation begins by verifying the latest stable `rmcp` release and pinning that exact version |
| Rig `0.41.0` | audited upstream agent-runtime baseline; the migration targets a fresh branch from current upstream and an exact released or justified fork revision |
| MCP Python SDK `2.0.0` | audited final-profile Python baseline; the released Tasks gap is filled only through its typed extension API |
| MCP TypeScript client and server `2.0.0` | audited modular final-profile TypeScript baseline; the legacy `@modelcontextprotocol/sdk` 1.x package is excluded |

The version numbers above record the audited state on 2026-08-06. Repository
dependency policy still requires rechecking authoritative upstream releases when
implementation begins. A newer stable patch replaces the audited baseline. An exact
Git revision is allowed only when an identified upstream fix has not reached a stable
release, and the reason must remain beside the pin.

Authoritative upstream sources are:

- the [MCP `2026-07-28` release](https://github.com/modelcontextprotocol/modelcontextprotocol/releases/tag/2026-07-28);
- the [MCP `2026-07-28` changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog);
- the [official Tasks overview](https://modelcontextprotocol.io/extensions/tasks/overview);
- the [MCP OAuth Client Credentials extension](https://modelcontextprotocol.io/extensions/auth/oauth-client-credentials);
- the [MCP authorization profile](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization);
- [OAuth 2.1 draft 13](https://datatracker.ietf.org/doc/html/draft-ietf-oauth-v2-1-13),
  [RFC 6750](https://www.rfc-editor.org/rfc/rfc6750.html),
  [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414.html),
  [RFC 9728](https://www.rfc-editor.org/rfc/rfc9728.html), and
  [OpenID Connect Discovery 1.0](https://openid.net/specs/openid-connect-discovery-1_0.html);
- [RFC 9207](https://www.rfc-editor.org/rfc/rfc9207.html),
  [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707.html), and
  [RFC 7523](https://www.rfc-editor.org/rfc/rfc7523.html);
- [W3C Trace Context](https://www.w3.org/TR/trace-context/) and
  [W3C Baggage](https://www.w3.org/TR/baggage/);
- [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html);
- the [`rmcp` 3.1.1 release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.1.1);
- the [`rmcp` 3.1.1 conformance roadmap](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.1.1/ROADMAP.md);
- the [Rig 0.41.0 release](https://github.com/0xPlaygrounds/rig/releases/tag/v0.41.0);
- the [MCP Python SDK 2.0.0 release](https://github.com/modelcontextprotocol/python-sdk/releases/tag/v2.0.0);
- the [MCP TypeScript client 2.0.0 release](https://github.com/modelcontextprotocol/typescript-sdk/releases/tag/%40modelcontextprotocol%2Fclient%402.0.0)
  and [server 2.0.0 release](https://github.com/modelcontextprotocol/typescript-sdk/releases/tag/%40modelcontextprotocol%2Fserver%402.0.0).

## Objective

Veoveo will publish hosted-server contract revision 3 as its one MCP implementation
profile. `rmcp` will own the standard protocol models, lifecycle, transport, headers,
Tasks methods, multi-round request shapes, subscriptions, response caching, and
ordinary schema generation.

Veoveo will own platform policy and durable behavior. The platform retains task
execution, persistence, authorization, audit, resource identity, event persistence,
provider completion, and application behavior. Those concerns are not SDK
duplication.

Revision 3 also removes duplicated protocol authority. Installation configuration
owns what may be exposed. Discover and the list methods own what a running server
actually exposes. The gateway validates the two views and fails closed without
turning self-reported server metadata into an authorization source.

The migration is complete when every in-repository and supported external-facing
surface speaks MCP `2026-07-28`, the superseded protocol code is deleted, and
acceptance demonstrates stateless cross-replica behavior. A deployment that supports
both the old and new protocol is not an intermediate deliverable.

## Official Changelog Coverage

The official changelog is the release ledger for this migration. Every entry has an
implementation owner and acceptance evidence. An entry marked as excluded still has a
deletion or rejection check; exclusion never means that the repository leaves an old
path in place.

### Major changes

| Changelog entry | Veoveo coverage |
|---|---|
| 1. Remove protocol sessions and `Mcp-Session-Id` | Stateless transport and explicit-handle work in phases 1 and 2; transport, replica, handle, and hard-cut acceptance |
| 2. Remove initialization and carry version, capabilities, and identity per request | Per-request metadata and effective capability intersection in phases 1 and 2; discovery, effective-capability, and protocol-error acceptance |
| 3. Add mandatory `server/discover` | Observed server-surface authority and readiness in phases 1 and 2; discovery and surface-authority acceptance |
| 4. Replace GET and legacy resource subscriptions with `subscriptions/listen` | Shared listener adapter in phase 4; subscription, restart, and replica acceptance |
| 5. Remove `ping`, `logging/setLevel`, and roots-list change; move log level to request metadata | Old methods are deleted in phase 2. Veoveo does not advertise deprecated MCP Logging and emits no `notifications/message`; deprecated-feature and hard-cut acceptance prove both facts. |
| 6. Move redesigned Tasks to `io.modelcontextprotocol/tasks` | Durable official Tasks and direct-call adapter in phase 3; Tasks, task-notification, adapter, restart, and replica acceptance |
| 7. Replace server-initiated requests with multi-round requests | Durable MRTR input in phase 5; direct and in-task multi-round acceptance |
| 8. Require `resultType` on every result | SDK-owned result discrimination in phase 2; ordinary, task, extension, and input-required wire acceptance |
| 9. Remove SSE resume and redelivery | Replay deletion and fresh-request retry rules in phase 2; transport, cancellation, and hard-cut acceptance |

### Minor changes

| Changelog entry | Veoveo coverage |
|---|---|
| 1. Add extension capability maps | Typed per-request extension intersection in phases 1 and 2 |
| 2. Define OpenTelemetry trace propagation | W3C trace context and untrusted baggage handling in phase 6 |
| 3. Return tools deterministically | Deterministic ordering and cache evidence in phase 6 |
| 4. Require MCP routing headers and support `x-mcp-header` | SDK-owned standard headers and installation-admitted parameter headers in phase 6 |
| 5. Require cache TTL and scope | Explicit cache policy for every `CacheableResult` in phase 6 |
| 6. Change resource-not-found to `-32602` | Shared resource error mapping and deletion check in phase 6 |
| 7. Validate authorization-response issuer | RFC 9207 handling in phases 1 and 6 |
| 8. Set Dynamic Client Registration `application_type` | Dynamic Client Registration is removed from the Veoveo profile. The hard-cut gate proves no registration path remains that could omit the field. |
| 9. Bind credentials to authorization-server issuer | Issuer-keyed credentials and metadata in phase 6 |
| 10. Permit full JSON Schema 2020-12 and any structured-content value | Bounded schema support in phase 6, including references, composition, non-object result roots, and fractional numeric keywords |
| 11. Remove elicitation completion notification and ID | MRTR request state replaces both in phase 5 |
| 12. Allocate the MCP server-error range and renumber final errors | Typed `-32020`, `-32021`, and `-32022` handling in phases 1 and 2; Veoveo-defined errors remain in `-32000..-32019` and acceptance rejects collisions with the MCP-reserved range |

### Deprecations, schema, and process

| Changelog entry | Veoveo coverage |
|---|---|
| Roots, Sampling, and Logging deprecated | Excluded and deleted in phases 2, 5, and 8 |
| HTTP+SSE reclassified as deprecated | Excluded from the final transport and hard-cut search |
| Non-`none` Sampling `includeContext` values deprecated | Removed with Sampling and included in the hard-cut search |
| Dynamic Client Registration deprecated | Removed; pre-registration is canonical and Client ID Metadata Documents remain an explicit future installation choice |
| JSON Schema numeric minimum, maximum, and default accept numbers | Fractional values survive generation, serialization, and validation in phase 6 acceptance |
| Feature lifecycle and deprecated-feature registry adopted | Phase 0 snapshots the registry used for the cut. Phase 8 rejects every deprecated surface rather than relying on its removal deadline. |
| SEP workflow formalized | Upstream protocol changes use accepted SEP and upstream PR evidence. This governance change adds no Veoveo wire behavior. |

The implementation report closes this ledger row by row and links the corresponding
test or deletion evidence. The full upstream schema and specification diff remains an
input to conformance; the summary table is not a substitute for that diff.

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
| Server surface authority | installation policy owns allowed and required exposure; Discover and list methods own observed runtime capability |
| Contract resource | retain revision, compliance, and embedded documentation identity; remove the hand-maintained duplicate live capability inventory |
| Effective capabilities | compute a typed per-request intersection across the client, gateway, installation, upstream, and active policy |
| Cross-call state | use opaque explicit handles whose authority, lifetime, and replay behavior are checked on every request |
| Result discrimination | let the final `rmcp` codec own required `complete`, `input_required`, and extension result discrimination |
| Tool errors | actionable argument and business validation returns `CallToolResult` with `isError: true`; protocol errors remain JSON-RPC errors |
| Resource errors | use final JSON-RPC `Invalid Params` (`-32602`) for a missing resource; remove the old MCP-specific code |
| Audit | record one durable policy event per logical action and durable authentication lifecycle or denial events; ordinary successful bearer verification is trace metadata |
| Tracing | propagate W3C trace context; never derive authority from `clientInfo`, `serverInfo`, `tracestate`, or `baggage` |
| Tool names | accept the final MCP tool-name grammar for local tools; keep installation slugs separately constrained |
| Custom MCP headers | implement and validate the standard mechanism; hosted servers may use `x-mcp-header` only through installation-admitted routing policy |
| Deprecated features | do not adopt Roots, Sampling, or MCP Logging |
| OAuth client registration | pre-registration is the private-installation baseline; Client ID Metadata Documents are optional future work and Dynamic Client Registration is unsupported |
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

### Duplicated server surface authority

The current control plane records core capability booleans and stable tool and prompt
names in `ServerManifest`. Every server then builds another `CapabilityInventory` for
its `{scheme}://contract` resource. Conformance compares that second declaration with
the live MCP lists, while `ServerInfo` carries another capability view.

These copies attempt to prove consistency but leave several hand-maintained sources
for the same runtime fact. They also make extension installation depend on declarations
that the gateway still has to verify against a running server.

Revision 3 separates expected policy from observation:

- profiles and bindings declare allowed or required names, resource selectors, scopes,
  extension exposure, and cross-server dependencies;
- server registration declares transport identity, resource ownership, routes, and
  installation requirements;
- Discover and the list methods report the observed running surface;
- readiness and conformance compare the observed surface with installation policy;
- the contract resource records revision and compliance without repeating the lists.

`clientInfo`, `serverInfo`, and Discover identity remain useful evidence. They are
self-reported and never authorize a request.

### Error, naming, trace, and audit narrowing

Current local tool names allow only lowercase gateway-safe characters even though the
final MCP grammar permits case-sensitive ASCII letters and dots. The restriction comes
from `mcp/contract/src/gateway/wire.rs::validate_gateway_name`, which currently serves
several identifier roles. Current tool handlers also return `Invalid params` for many
domain validation failures that a model could correct after receiving a tool execution
error. `servers/artifact-mcp/src/bin/server/handler.rs`, for example, maps
`max_downloads == 0` to a protocol error.

The gateway generates arbitrary correlation tokens rather than accepting and
propagating the standard W3C trace context now defined by MCP. Successful bearer
verification also produces durable authentication audit records in addition to the
logical action's policy audit. Stateless per-request authentication would multiply
that duplicate evidence. The authenticated-gateway smoke currently requires at least
ten successful `bearer_jwt` rows, and the HTTP gateway smoke requires at least two.
Those assertions preserve the duplication and must be replaced with logical-action
cardinality and token-lifecycle evidence.

Revision 3 uses separate types for installation slugs, local tool names, gateway
projections, W3C trace identity, and extension identifiers. It records successful
authentication as part of the logical action while retaining durable events for token
lifecycle, denial, replay, and policy decisions.

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

### Veoveo changes since the initial audit

The initial plan landed on 2026-07-30. Veoveo `3dba3913` adds several owners that the
protocol cut must now address explicitly:

- `veoveo.io/live-view/v2` makes the domain simulator authoritative for camera rigs,
  stable encoded products, and actor-and-browser viewer leases. The revision-3 MCP
  contract preserves these strong types and canonical resource URIs. It does not move
  GPU media, pose traffic, or viewer leases into protocol-session state.
- Simulation View now has durable desired state, transactional outbox events, reactive
  reconciliation, explicit renewal and retry deadlines, and ephemeral viewer leases.
  These event sources can feed `subscriptions/listen`. Files such as
  `servers/simulation-view-mcp/src/state/session.rs` own domain sessions and survive
  the deletion of MCP sessions.
- Recording MCP now exposes scoped Redap archive reads and a native live RRD channel
  through gateway and Console routes beside its MCP endpoint. Those binary and HTTP
  adapters remain non-MCP data planes. The migration changes Recording MCP discovery,
  resources, task behavior, and catalog subscriptions without reclassifying the live
  RRD channel as legacy MCP SSE.
- Recording Hub and its forwarder now use durable queue and catalog events for upload,
  rollover, and playback wakeups. Phase 4 reuses those typed sources where an MCP
  notification projects the same fact; it does not add interval polling.
- Gateway and Console now own shared TLS initialization, auth-scoped outbound HTTP
  clients, recording playback routes, and viewer configuration. The MCP client cut
  separates protocol state from those reusable transport and product clients instead
  of deleting them together.
- Deployment and visual acceptance now have focused Rust entrypoints in
  `testing/deployment-smoke` and `testing/browser-smoke`, while `cargo xtask smoke`
  remains the repository coordinator. Final acceptance uses the focused harnesses and
  the existing hardware-GPU browser gate.

The repository still pins `rmcp` 2.2.0, uses the old Rig fork revision, and publishes
hosted contract revision 2. This document remains a future hard-cut plan; none of the
new product paths makes revision 3 partially deployed.

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

Every result uses the final wire discriminator. Ordinary results carry
`resultType: "complete"`, multi-round interim results carry
`resultType: "input_required"`, and extension results use the extension's typed
variant. `rmcp` owns this wire bookkeeping. Domain handlers do not hand-build or strip
the discriminator.

Missing required per-request metadata returns HTTP `400` with JSON-RPC `Invalid
Params` (`-32602`). A request for another protocol revision returns HTTP `400` with
`UnsupportedProtocolVersion` (`-32022`) and typed requested and supported versions.
The client does not downgrade after either response.

The gateway keeps process-wide HTTP connection pools and TLS configuration keyed by
validated transport identity. It does not keep an upstream protocol handler attached
to a downstream request peer. Internal assertions remain short-lived and are minted
for each upstream HTTP request.

A broken response stream has no protocol replay. A client that retries issues a new
JSON-RPC request ID and re-evaluates whether the operation is safe to repeat. A
non-idempotent domain action requires its own typed idempotency contract; transport
failure does not make it retryable.

Closing a streaming HTTP response cancels that request. The server stops producing
request-scoped progress and sends no later response on the closed stream. Cancelling a
stream that already returned a durable Task does not cancel the Task; cooperative task
cancellation still requires `tasks/cancel`.

The final `rmcp` codec retains the specification's mandated rule that an absent
`resultType` from an earlier-version peer decodes as `complete`. Veoveo never uses that
decode rule to admit, advertise, or downgrade to an earlier protocol version.

### Server surface authority

Server registration and installation policy no longer duplicate the live protocol
inventory.

Installation-owned configuration retains:

- server slug, canonical resource scheme, upstream endpoint, mount ownership, and
  transport-security identity;
- profile exposure for tools, prompts, resources, completions, and admitted
  extensions;
- required scopes, policy selectors, App resource dependencies, and other
  installation requirements;
- explicit compatibility-helper admission.

The running server owns:

- supported protocol versions and core or extension capabilities through Discover;
- current tools, prompts, resources, and templates through the standard list methods;
- cache policy and list-change signals for that observed surface.

The gateway performs one readiness observation for each registered server at a
catalog revision. A required name or extension that is absent fails readiness. An
observed name that policy does not expose remains unreachable. `Exposure::All`, where
retained, is an explicit installation decision to admit the observed surface rather
than an implication derived from the server's self-description.

The live `{scheme}://contract` resource continues to prove the deployed contract
revision, compliance declaration, and embedded document identity. It does not contain
a separately maintained list of live tools, prompts, resources, templates, or
task-capable tools. Conformance records the observed lists directly.

### Per-request capabilities and extensions

The gateway derives one `EffectiveClientCapabilities` value for each request:

```text
client-declared capabilities
∩ gateway-supported capabilities
∩ installation-admitted extensions
∩ upstream-discovered capabilities
∩ active profile policy
```

Only the effective value is forwarded upstream. A capability mentioned on a previous
request has no effect. Unknown extension fields are not forwarded through the gateway
unless the installation admits the exact extension identifier and the gateway has an
explicit safe forwarding rule.

The intersection is evaluated independently at each protocol hop. The direct-call
adapter terminates a non-Tasks downstream call. When installation policy admits the
adapter, the gateway becomes the upstream client and may advertise Tasks on that
separate hop because the gateway consumes the task itself. It never marks the
downstream client as Tasks-capable or returns a Task wire result to that client.

Core MCP surfaces remain typed core capabilities. Tasks, MCP Apps, authorization
extensions, and `io.veoveo/app-resource-dependencies` use the standard extension map.
Breaking revisions of a Veoveo extension use a new identifier. A server may return
extension-specific results only when the current request declares the required
extension.

When an operation cannot proceed without a missing client capability and no admitted
adapter terminates that need, the server returns HTTP `400` with
`MissingRequiredClientCapability` (`-32021`) and the typed
`requiredCapabilities` value. It does not guess support or wait for a callback that
cannot arrive.

Authenticated OAuth client identity, Work Context, and the gateway's internal
assertion remain authoritative. `clientInfo` and `serverInfo` are recorded for
display, diagnostics, and evidence only.

### Explicit state handles

Every value that carries state across requests is explicit. This includes Tasks,
application-owned live objects, playback access, pagination cursors, multi-round
request state, and any server-specific workflow handle.

The hosted-server profile requires:

- an opaque bounded wire representation;
- a documented lifetime and cleanup rule;
- authorization against the current principal, Work Context, profile, and target on
  every use;
- a clear distinction between identity and capability, because possession is not
  authorization;
- actionable expiry or unknown-handle errors;
- integrity protection when client modification could affect authority or behavior.

First-party internal identities may remain UUIDv7. External task and application
handles are opaque strings. Gateway task identities remain source-qualified so two
servers may mint the same upstream value safely.

Pagination cursors are sealed and bind the list method, normalized parameters,
catalog or data revision, and effective authorization context. A cursor cannot cross
principals or be replayed against another list.

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

Task projection obeys the final lifecycle:

- `CreateTaskResult` is returned only after `tasks/get` can read the durable task;
- `working` and `input_required` remain non-terminal;
- `completed`, `failed`, and `cancelled` are immutable terminal states;
- a completed tool call may contain `isError: true`, because a tool execution error is
  still the terminal result of that call;
- `failed` is reserved for a JSON-RPC execution failure and includes the protocol
  error;
- `tasks/update` and `tasks/cancel` acknowledge accepted intent without claiming that
  the worker has already changed state;
- repeated input-response keys are idempotent and do not repeat a side effect;
- cancellation intent and worker-confirmed cancellation remain separate internal
  events;
- the completed payload validates against the result schema of the original method.

Every task read, update, cancel, and notification subscription checks current
authority as well as the retained task owner. Authority at task creation never grants
permanent access by itself. The task TTL is the protocol retention promise. Internal
retention pins may keep storage longer but do not extend what an expired external
handle promises.

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
mechanism and a cluster-shared secret. The protected payload binds the original
method, salient-parameter digest, principal, Work Context, profile, issuing server,
expiry, and protocol revision. Servers reject a mismatched or expired state. A
one-time business action also records consumption durably because integrity and
expiry alone do not prevent replay.

The profile bounds request-state bytes, input-request count, total response bytes,
round trips, and lifetime. Input keys are unique and stable for one round. A client
cannot satisfy an input request that was not issued, and duplicate answers cannot
repeat a side effect.

Revision 3 supports form elicitation for current approval and interactive-input
workflows. URL-mode elicitation is excluded until a product workflow requires its
out-of-band browser and origin policy. Roots, Sampling, and MCP Logging are
deprecated and excluded. Server-side model work continues through Veoveo's governed
provider integration rather than MCP Sampling.

Consent-sensitive input is classified by platform policy. A host never silently
model-fills an approval, credential, destructive confirmation, or other
human-required response.

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

One typed `SubscriptionsListen` policy action replaces the broad legacy subscribe
actions. Authorization evaluates each requested filter:

- exact resource subscriptions check the projected server and resource URI;
- task subscriptions check every task ID and the Tasks extension capability;
- tool, prompt, and resource list-change signals check exposure to that catalog;
- an extension filter requires exact installation admission.

Unsupported filters may be omitted from the acknowledged subset. An unauthorized
target rejects the request instead of silently creating partial authority. The
acknowledgment is the first message on the stream, and every later notification
carries the subscription ID.

Task notifications contain complete task state and may replace polling for a
connected client. `tasks/get` remains the recovery and correctness path.
Request-scoped progress stays on its originating response stream and never moves to
the listener. Revision 3 emits no protocol log messages because MCP Logging is
excluded.

The per-request `io.modelcontextprotocol/logLevel` field is accepted as
non-authoritative metadata and has no effect when Logging is not advertised. A server
never emits `notifications/message` merely because an earlier request selected a log
level, and it never retains that value across requests.

The protocol no longer requires singleton deployment. An endpoint remains singleton
only when a real resource-owner constraint requires it, such as exclusive GPU
capacity or a live simulation process.

### Schemas and strong types

Rust handlers use the ordinary `rmcp` tool macro and current Schemars generation.
Python handlers use the official SDK and Pydantic schema generation. Strong domain
types remain mandatory.

Veoveo validation enforces:

- the object root required for controlled tool inputs;
- complete output schemas and structured-content validation for any JSON root,
  including arrays, scalars, and `null` where the declared schema permits them;
- bounded depth and total subschema count;
- bounded strings, arrays, maps, and numeric domains where the tool contract
  controls them;
- same-document references at untrusted extension boundaries;
- deterministic schema serialization for release evidence.

The validation does not reject a schema merely because it contains `$ref`,
`allOf`, `anyOf`, `oneOf`, or another JSON Schema 2020-12 construct.
External references are not fetched automatically. Any explicitly supported reference
resolver is installation-owned, allowlisted, size-bounded, cycle-bounded, and included
in release evidence.

JSON Schema `minimum`, `maximum`, and `default` retain fractional JSON numbers. No
generator, intermediate Rust value, conformance snapshot, or validator narrows those
keywords to integers.

### Tool names and error semantics

The final MCP specification recommends tool names between 1 and 128 characters,
case-sensitive, using ASCII letters, digits, underscore, hyphen, or dot. Revision 3
makes that recommendation mandatory for `LocalToolName` to give every Veoveo client
one deterministic grammar. `ServerSlug` remains a separate lowercase installation
identifier. Gateway composition rejects a projected tool name that exceeds the MCP
bound or collides with another projected name. It does not truncate or silently
rename a tool.

Tool failures have one shared classification:

| Failure | Wire result |
|---|---|
| malformed JSON-RPC or `CallToolRequest` envelope | JSON-RPC protocol error |
| unknown tool | JSON-RPC protocol error |
| tool input schema, domain, or business validation | completed `CallToolResult` with `isError: true` and actionable content |
| downstream API or domain execution failure the model can act on | completed `CallToolResult` with `isError: true` |
| server failure that prevents the request from producing a tool result | JSON-RPC server error |

The same classification survives Tasks. A tool execution error produces a
`completed` Task containing the exact `CallToolResult`. A JSON-RPC failure produces
a `failed` Task containing the protocol error.

The shared Rust and Python handler paths convert controlled validation errors into
typed tool execution errors. Individual servers do not reproduce this mapping.
Conformance submits schema-valid and domain-invalid arguments and proves the error is
visible to the model.

Resource methods separately adopt the final protocol mapping. An unknown resource URI
returns JSON-RPC `Invalid Params` (`-32602`), not the superseded MCP-specific
`-32002`.

Veoveo-defined JSON-RPC server errors use only `-32000..-32019`. The shared error type
reserves `-32020..-32099` for MCP and owns the final `HeaderMismatch` (`-32020`),
`MissingRequiredClientCapability` (`-32021`), and `UnsupportedProtocolVersion`
(`-32022`) models. Component-local errors cannot allocate from that reserved range.

### Cache metadata

Every final `CacheableResult` receives an explicit policy. This includes Discover,
`tools/list`, `prompts/list`, `resources/list`, `resources/templates/list`, and
`resources/read`.

| Result class | Default cache policy |
|---|---|
| authorization-filtered catalog or resource | `private` |
| deterministic public contract or immutable documentation | `public` when the content is independent of authority |
| live resource read | `private`, zero or short TTL |
| installation configuration | `private`, revision-bound TTL |

List ordering is deterministic. An `rmcp` client cache is scoped to one authenticated
client identity. Catalog and domain caches may remain when they cache a product fact
rather than a protocol response.

Task and multi-round results do not enter the response cache. A task's `ttlMs`
describes external task retention and is not a response-freshness hint.

TTL is a freshness hint, not a background polling schedule. Clients read again only
when they need stale data. A client that deliberately polls applies jitter and
backoff. List-change notifications invalidate the relevant cached pages immediately.
Multi-round retries carrying `requestState` or `inputResponses` are never cached.

Private cache keys include the request method, normalized parameters, protocol
version, effective authorization context, and catalog or policy revision. A bearer
token string is not persisted as the cache identity. Public cache scope is permitted
only when the response is identical for every authority.

### Audit and observability

Protocol statelessness does not create a second audit event for authentication on
every request. The durable boundary is:

- token issuance, refresh, revocation, replay, and credential denial;
- one policy decision for each logical MCP or administrative action;
- durable task, input, subscription, artifact, recording, and other governed state
  transitions required by their domain contracts.

Successful bearer verification becomes structured metadata on the logical action's
trace and policy record. It remains measurable without creating a separate durable
`bearer_jwt` allow event for every list, read, or task poll. Cache hits and SSE
keep-alive comments do not create policy or authentication audit records.

Every MCP request accepts W3C `traceparent`, `tracestate`, and `baggage` in `_meta`.
The gateway validates and forwards trace context, creates a new trace when none is
usable, and stores the canonical trace ID on audit evidence. `tracestate` and
`baggage` are bounded and sanitized. They never supply tenant, principal, Work
Context, scopes, data labels, assurances, routing authority, or policy input.

`clientInfo` and `serverInfo` remain self-reported diagnostics. Gateway-generated
downstream results identify the gateway as the MCP server. Upstream implementation
identity is retained in deployment evidence and traces rather than projected as
authenticated identity.

### HTTP routing headers

`rmcp` owns `MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name`, encoding, and the
standard header-mismatch response: HTTP `400` with JSON-RPC `-32020`. The gateway
still enforces the security boundary. Any component that reads the body verifies that
the decoded header values match it before routing or policy use.

Hosted tools may declare `x-mcp-header` only when installation configuration admits
the exact tool, property path, and header name for a routing need. The annotated
property follows the final static-reachability constraints and has type string,
integer in the IEEE 754 safe range, or boolean. JSON Schema `number`, compound values,
and case-insensitively duplicated header names are rejected.
Configuration rejects headers that carry or resemble:

- credentials, cookies, internal assertions, or secrets;
- personally identifiable information or sensitive domain data;
- principal, scope, role, assurance, or authorization decisions;
- an unverified tenant or Work Context;
- standard forwarding, host, trace, or MCP protocol headers.

An `Mcp-Param-*` value remains model-controlled input. A router may use it to select
capacity or locality only after the server validates the matching body and
independently enforces authority. Header and aggregate size limits are installation
settings, and oversized requests fail before decoding or body parsing.

### Authorization

The gateway remains the protected resource and policy enforcement point. The final
MCP authorization profile adds these migration requirements:

- publish RFC 9728 protected-resource metadata and support both RFC 8414 and OpenID
  Connect authorization-server discovery;
- use installation-pre-registered OAuth clients as the canonical private-deployment
  registration model;
- validate every present authorization-response issuer through RFC 9207 and reject an
  absent issuer when authorization-server metadata advertises it;
- emit `iss` on Veoveo authorization responses and advertise that behavior;
- bind client credentials and cached authorization metadata to that issuer;
- carry RFC 8707 resource identity through initial authorization and refresh;
- accumulate and reauthorize step-up scopes without dropping prior grants;
- include the exact missing scopes for the current operation in an insufficient-scope
  challenge;
- advertise `io.modelcontextprotocol/oauth-client-credentials` only when the
  installation admits machine clients;
- keep `client_credentials` restricted to installation-owned confidential clients
  using `private_key_jwt`.

Dynamic Client Registration is not part of the Veoveo profile. Client ID Metadata
Documents remain optional future work for an installation that deliberately accepts
previously unknown clients. They are not a revision-3 acceptance requirement and do
not add an outbound metadata-fetching SSRF surface to the private baseline.

The `rmcp` auth feature is enabled only in clients that need its OAuth flow. Hosted
servers do not acquire a full client auth dependency merely to validate the
gateway-issued internal assertion.

### Language clients

Rust uses one workspace `rmcp` version. A Rust package may not introduce a second
major version through an integration dependency.

The audited Python baseline is official `mcp` 2.0.0. That release implements the
`2026-07-28` core profile and explicitly does not ship Tasks. Veoveo therefore
generates strong Pydantic models from the exact official extension schema and
registers the methods through the SDK extension API. It does not reproduce HTTP,
JSON-RPC, header, discovery, or streaming middleware.

The audited TypeScript baseline is the modular
`@modelcontextprotocol/client` 2.0.0 and `@modelcontextprotocol/server` 2.0.0 line.
The Console uses `tasks/get`, `tasks/update`, `tasks/cancel`, and
`subscriptions/listen`. MCP Apps remains a separate extension. The legacy
`@modelcontextprotocol/sdk` 1.x package is not a fallback.

## Ownership And Deletion Boundary

| Surface | Migration action |
|---|---|
| `mcp/task-extension` | delete after all callers use `rmcp` Tasks |
| `platform/gateway/src/mcp/final_tasks.rs` | delete the raw request and SSE client |
| legacy task branches in gateway tool projection | replace with one `CallToolResponse` path |
| `ServerManifest` live tool, prompt, and capability inventories | retain installation identity and exposure policy; remove duplicated observations supplied by Discover and list methods |
| per-server `CapabilityInventory` and contract-resource live lists | delete; conformance records the observed surface directly |
| `McpSurfaceCapabilities` and per-tool `TaskExposure` | replace with typed core capability and extension policy plus the per-request effective intersection |
| `mcp/contract/src/session.rs` | delete protocol session ownership and cleanup |
| stateful transport configuration | replace with explicit final-profile stateless configuration |
| `mcp/contract/src/subscriptions.rs` | replace stored peers with the shared event-to-listener adapter |
| server-local subscribe/unsubscribe handlers | delete |
| `GatewayAction::{ResourcesSubscribe, ResourcesUnsubscribe, TasksSubscribe}` | replace with one typed listen action that authorizes each requested filter |
| `mcp/schema-macros` | delete after handlers use the ordinary `rmcp` macro |
| forced schema inlining and no-reference conformance rules | delete |
| `mcp/task-contract::ProtocolTaskId` | replace with an opaque external ID and canonical gateway-owned task identity |
| `TaskRetentionPin` | move to `platform/task-runtime` if it remains a runtime policy |
| `mcp/task-contract` | retire if no protocol-independent types remain after the move |
| Console `McpSessionPool` | replace with an auth-scoped client pool |
| manual protocol catalog caches | replace with final cache metadata where they do not own a product fact |
| unsealed pagination cursors and process-local cross-call handles | replace with typed opaque handles carrying bounded lifetime and authorization checks |
| `resources/subscribe` and `resources/unsubscribe` policy actions | replace with accepted `subscriptions/listen` filters and resource authorization |
| `tasks/result`, `tasks/list`, and `taskSupport` | delete from models, policy, apps, tests, examples, and docs |
| custom standard MCP routing headers and version metadata | delete; `rmcp` owns them |
| arbitrary correlation-token parsing | replace with validated W3C trace context |
| per-request successful bearer-verification audit events | remove; attach verification metadata to the logical action's trace and policy evidence |
| server-local domain failures returned as `Invalid params` | replace with the shared typed tool-execution error mapping |
| resource-not-found code `-32002` | replace with final JSON-RPC `Invalid Params` (`-32602`) |
| gateway-safe local tool-name validation | split into final `LocalToolName`, lowercase `ServerSlug`, and collision-checked projected-name types |
| protocol `ping`, `logging/setLevel`, and `notifications/roots/list_changed` | delete |
| `notifications/elicitation/complete` and `elicitationId` | delete; multi-round retries carry application correlation in protected `requestState` |
| legacy elicitation, Roots, Sampling, MCP Logging, HTTP+SSE, and non-`none` `includeContext` paths | delete rather than negotiate deprecated features |
| Platform Store task records and `platform/task-runtime` | retain |
| gateway auth, policy, internal assertions, and audit | retain |
| provider webhook completion | retain |
| domain outbox, replay, and LIVE acceleration | retain |
| MCP Apps typed host policy and UI resources | retain |
| direct-call task adapter | retain with final protocol behavior |
| Simulation View and UAV domain sessions, desired state, deadlines, and ephemeral viewer leases | retain; these are explicit application state rather than MCP protocol sessions |
| `veoveo.io/live-view/v2` authoritative camera and stream-product types | retain as a typed negotiated domain extension |
| Recording Redap, native live RRD channel, gateway playback route, and Console viewer adapter | retain beside MCP; these data planes are not HTTP+SSE or `subscriptions/listen` |
| focused `testing/deployment-smoke` and `testing/browser-smoke` harnesses | retain and add final-profile deployment and headed hardware-GPU cases |

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

The `rmcp` 3.1.1 audit found broad final-profile support, including Discover,
stateless Streamable HTTP, standard headers, cache metadata, multi-round requests,
official Tasks models, task operations, and subscriptions. The release also fixes
handler-macro cache hints, exposes MRTR state to tool handlers, and disambiguates
input-required results.

The tagged roadmap reports 100 percent client and server conformance for the dated
`2026-07-28` core suite. Tasks remains extension coverage: nine scenarios are expected
failures and one is skipped upstream. The release still does not route task-status
notifications through `SubscriptionSink`; its core `SubscriptionFilter` has no task-ID
selector and the sink explicitly drops `TaskStatusNotification`. The roadmap also
names sessionless explicit handles, request association, and client TTL caching as
specification features without conformance scenarios.

The migration therefore requires:

1. check the latest stable `rmcp` patch when implementation begins;
2. contribute the narrow task subscription-filter and sink change upstream if it is
   still absent;
3. run the dated core suites without Veoveo allowlists and close every Tasks extension
   scenario needed by the Veoveo profile;
4. supplement upstream conformance for task notifications, explicit handles,
   request-stream association, and client TTL behavior;
5. verify the 3.1.1 cache-hint and MRTR fixes through Veoveo integration tests;
6. pin an exact accepted upstream revision only when a required fix is not released,
   then remove that Git pin at the next stable release.

This gap does not justify preserving `mcp/task-extension`.

## Implementation Plan

The phases below describe dependency order. They are not supported deployment modes.
Each phase ends in a coherent commit, while the branch remains unshipped until the
complete gate passes.

### Phase 0: Upstream prerequisites

- Create the fresh Rig branch from current upstream.
- Revalidate the current stable Rig and `rmcp` releases.
- Revalidate MCP Python SDK 2 and the modular MCP TypeScript client and server 2
  releases; do not select either SDK's legacy major.
- Snapshot the official deprecated-feature registry and complete specification/schema
  diff used by the cut.
- Resolve `rmcp` task subscription routing, Tasks extension scenarios, and uncovered
  final-profile behaviors from the upstream gate.
- Land or pin the Rig `rmcp` 3 client foundation.
- Land the protocol-neutral deferred-tool and final Tasks stack.
- Record exact upstream and fork revisions used by Veoveo.

### Phase 1: Contract revision 3

- Rewrite `mcp/contract/DESIGN.md` for MCP `2026-07-28`.
- Increment `CONTRACT_REVISION`.
- Define the installation-owned policy surface and the Discover-owned observed
  surface, including fail-closed readiness comparison.
- Define `EffectiveClientCapabilities`, opaque cross-call handles, final task
  semantics, subscription-filter authorization, tool-error classification, W3C trace
  handling, and the final local tool-name grammar.
- Remove the duplicated capability inventory from the contract resource and replace
  it with revision, compliance, and embedded-document evidence.
- State the private-installation OAuth profile: pre-registered clients, RFC 9207
  issuer validation, RFC 8707 resource binding, step-up scopes, and no Dynamic Client
  Registration.
- Update `docs/CODEMAP.md`, architecture documents, server compliance sections, and
  MCP Apps design.
- Replace the compliance checklist entries for sessions, subscriptions, tasks, and
  schemas.
- Update gateway fragments and compatibility manifests with the final protocol and
  contract revision.
- Preserve `veoveo.io/live-view/v2`, Recording playback v8, and other current domain
  extensions as typed extension contracts rather than folding their data planes into
  MCP transport state.
- Add an enforcement rule that rejects the superseded protocol vocabulary.

The implementation change and normative revision land together. Revision 3 is never
published while revision 2 behavior remains deployed.

### Phase 2: Shared stateless transport

- Pin one exact `rmcp` 3 version in the workspace.
- Replace the canonical server and client configuration.
- Migrate Discover and final request metadata.
- Move `serverInfo` to the standard result `_meta` field and keep `clientInfo`
  optional, self-reported, and non-authoritative.
- Use final `resultType` wire discrimination through the SDK rather than
  application-built result envelopes.
- Return the final typed `-32602`, `-32021`, and `-32022` HTTP `400` responses for
  missing metadata, missing capabilities, and unsupported versions.
- Reserve `-32020..-32099` for MCP and reject component-defined collisions.
- Implement the per-request effective capability intersection and reject unknown or
  unadmitted extensions.
- Observe server lists at readiness, fail when a required surface is absent, and keep
  unexposed observations unreachable.
- Remove initialization, session IDs, GET reconnect, DELETE, replay, and session
  cleanup.
- Remove protocol `ping`, `logging/setLevel`, and
  `notifications/roots/list_changed`.
- Keep request log level request-scoped and emit no protocol log notification because
  the final Veoveo profile does not advertise Logging.
- Treat closure of a streaming HTTP response as request cancellation without turning
  it into transport replay or implicit durable-task cancellation.
- Preserve Streamable HTTP origin validation and DNS-rebinding protection across the
  transport rewrite.
- Separate HTTP/TLS pooling from protocol request state.
- Migrate all hosted Rust servers, the stdio bridge, gateway, Console BFF,
  conformance client, and smoke support.

### Phase 3: Official durable Tasks

- Implement official task handlers over `platform/task-runtime`.
- Add the durable gateway task mapping and opaque upstream IDs.
- Make creation durable before returning a task and enforce immutable terminal state.
- Separate a completed tool result carrying `isError` from a failed JSON-RPC task.
- Make input updates idempotent, treat cancel as accepted intent, validate terminal
  output schemas, and reauthorize every later task operation.
- Migrate every task-producing server.
- Rewrite the direct-call compatibility adapter.
- Migrate Console and agent task consumers.
- Delete both old task protocols and the obsolete shared task wire types.

### Phase 4: Subscriptions and replicas

- Implement the event/outbox-to-`SubscriptionSink` adapter.
- Migrate resource, catalog, task, and domain change-notification paths. Keep
  request-scoped progress on its originating response stream.
- Reuse the current Simulation View reconciliation outbox and Recording catalog/queue
  events where they own the same durable fact. Do not add interval polling.
- Authorize exact task IDs, resource URIs, catalog surfaces, and extension identifiers
  in each listen filter.
- Reject unauthorized filters and acknowledge only the supported subset of authorized
  filters.
- Send the typed acknowledgment first, tag every later notification with the
  subscription ID, and send the final empty result before a graceful server closure.
- Keep native Recording RRD streams, Redap, WebRTC, and private runtime streams outside
  `subscriptions/listen`.
- Delete stored peer registries and legacy resource subscription handlers.
- Remove the universal singleton deployment rule.
- Enable at least two gateway replicas and one ordinary hosted-server replica in
  acceptance.
- Retain explicit singleton settings only for workloads with a documented resource
  owner.

### Phase 5: Multi-round agent input

- Implement durable `inputRequests`, request state, response, authority, and audit
  records.
- Bind protected request state to the original method, salient parameters, principal,
  Work Context, profile, issuing server, expiry, and protocol revision.
- Enforce byte, input-count, round-trip, and lifetime limits. Record durable
  consumption for one-time business actions.
- Migrate agent approvals and interactive inputs.
- Implement in-task input through `tasks/update`.
- Remove parked elicitation waiters and old Rig elicitation callbacks.
- Remove `notifications/elicitation/complete` and `elicitationId`.
- Support form elicitation only. Do not add URL mode, Roots, Sampling, or MCP Logging.
- Prove request and task input survive process restart.

### Phase 6: Schemas, names, errors, caching, headers, and authorization

- Move every Rust handler to the ordinary `rmcp` schema path.
- Delete `mcp/schema-macros` and the forced-inlining generator.
- Add bounded JSON Schema 2020-12 validation.
- Permit schemas and structured content with any valid JSON root, prohibit automatic
  external-reference fetching, and bound any installed resolver.
- Preserve fractional `minimum`, `maximum`, and `default` values through generation,
  serialization, snapshots, and validation.
- Replace gateway-safe local tool names with the final MCP grammar while preserving
  separate lowercase server slugs and collision-checked projections.
- Centralize Rust and Python tool-error mapping. Domain and actionable execution
  failures return `CallToolResult` with `isError: true`.
- Replace the legacy missing-resource error with JSON-RPC `-32602`.
- Centralize the MCP-reserved error range and reject Veoveo-defined codes in
  `-32020..-32099`.
- Set explicit TTL and cache scope on discover, list, and read results.
- Key private caches by effective authority and policy revision. Treat TTL as
  on-demand freshness rather than a polling schedule.
- Delete custom standard-header construction and validation.
- Validate standard header/body agreement and admit `x-mcp-header` only through typed
  installation routing policy with strict size and authority exclusions.
- Replace arbitrary correlation tokens with bounded W3C trace context and untrusted
  baggage.
- Stop emitting a separate durable successful bearer event per stateless request.
  Preserve one logical policy event plus durable token lifecycle, replay, and denial
  events.
- Implement the final issuer, resource, registration, and step-up requirements.
- Align dependent cryptography and HTTP crates with the selected `rmcp` feature set.

### Phase 7: Python, TypeScript, templates, and external artifacts

- Pin the latest stable MCP Python SDK 2 and modular MCP TypeScript client/server 2
  releases. Remove the legacy TypeScript `@modelcontextprotocol/sdk` package if it is
  present in a generated or external fixture.
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
- Run the acceptance matrix through `cargo xtask smoke`, the focused deployment smoke,
  and the focused headed hardware-GPU browser smoke.
- Search the source, schemas, rendered charts, generated documentation, and web
  bundles for obsolete protocol surfaces.
- Distinguish legitimate domain sessions, viewer leases, native RRD channels, Redap,
  and WebRTC from deleted MCP session and replay vocabulary in that search.
- Remove the old Rig branch after the published replacement and Veoveo cutover are
  independently recoverable.

## Acceptance Matrix

| Area | Required evidence |
|---|---|
| Changelog ledger | every official major, minor, deprecated, schema, governance, and process entry is closed by linked implementation, rejection, deletion, or no-wire-impact evidence |
| Dependency graph | one exact `rmcp` 3 version; no second major through Rig or another package; Python and modular TypeScript SDKs use their current final-profile major and no legacy TypeScript SDK remains |
| Discovery | every client and hosted server completes the `2026-07-28` Discover lifecycle without Initialize |
| Surface authority | required installation surfaces fail readiness when absent; unexpected observed surfaces remain unreachable; the contract resource contains no duplicate live inventory |
| Effective capabilities | each hop receives the typed client, gateway, installation, upstream, and policy intersection; a previous request and an unknown extension cannot expand it; an unmet requirement returns HTTP `400` with `-32021` |
| Adapter negotiation | a non-Tasks downstream client receives no Task wire result; the admitted gateway adapter may negotiate and consume Tasks only on its separate upstream hop |
| Transport | no session header, reconnect GET, session DELETE, or Last-Event-ID; terminal results are JSON; stream closure cancels only the owning request; origin and DNS-rebinding checks remain enforced |
| Protocol errors | missing required metadata returns HTTP `400` with `-32602`; a non-final version returns HTTP `400` with `-32022` and cannot trigger downgrade; component-defined errors cannot use the MCP-reserved `-32020..-32099` range |
| Result discrimination | every ordinary, task, extension, and multi-round result has the final SDK-owned discriminator; handlers do not carry a second result envelope; absent earlier-version discriminators decode as complete without enabling protocol downgrade |
| Official conformance | dated server and client suites pass with zero Veoveo-allowlisted failures; every Tasks extension scenario required by the profile passes or has direct equivalent evidence |
| Tasks | creation is durable before return; immediate, working, input-required, completed, failed, cancelled, TTL, poll interval, idempotent update, accepted cancel intent, immutable terminal state, and terminal-schema paths pass |
| Task errors | a completed tool result may carry `isError: true`; only a JSON-RPC failure yields a failed Task |
| Task notifications | a task listener observes authorized status changes; loss of an optional notification does not prevent correctness through `tasks/get` |
| Compatibility adapter | a client without Tasks receives the canonical terminal result and task resource without invoking an alternate task store |
| Opaque IDs | two servers may return the same upstream task string without a gateway collision |
| Handles and cursors | expired, tampered, cross-principal, cross-profile, cross-method, and replayed values fail without revealing another authority's state |
| Replica routing | a task created through one gateway replica is read, updated, cancelled, or completed through another |
| Restart | a gateway or server restart between task creation and observation does not lose canonical state |
| Multi-round request | direct and in-task form input survives serialization, authorization recheck, restart, and retry; tamper, expiry, replay, response-size, round-count, and unissued-input checks fail closed |
| Subscriptions | acknowledgment is first, every event carries the subscription ID, persistent events replay from the outbox, live events arrive through one accepted listener filter, graceful server closure returns the final empty result, unsupported authorized filters may be omitted, and any unauthorized target rejects the request |
| Stateless scaling | two gateway replicas and one replicated ordinary MCP server pass without sticky sessions |
| Schemas | bounded same-document references, composition, fractional numeric minimum/maximum/default values, and object, array, scalar, or null structured-content roots validate; automatic external fetches and malformed, cyclic-over-limit, or resource-exhausting schemas fail closed |
| Tool errors | malformed envelopes and unknown tools produce JSON-RPC errors; schema-valid domain failures and actionable execution failures produce model-visible `isError` tool results in direct and task flows |
| Resource errors | a missing resource returns JSON-RPC `-32602`; the superseded MCP-specific code is absent |
| Tool names | case-sensitive final-profile names, including dots and uppercase characters, pass; empty, overlength, invalid-character, and projected-collision cases fail |
| Caching | private entries never cross principals or policy revisions; immutable public content honors TTL and notification invalidation; TTL expiry causes no unsolicited background polling |
| Audit volume | one successful stateless action produces one durable policy event rather than a second `bearer_jwt` allow event; token lifecycle, replay, credential denial, and policy denial remain durable |
| Trace context | valid W3C context propagates; malformed or oversized context is rejected or replaced as specified; baggage and self-reported client or server identity cannot change authority |
| MCP headers | standard method and name headers agree with the decoded body; mismatch returns HTTP `400` with JSON-RPC `-32020`; unadmitted, authority-bearing, and oversized parameter headers fail before routing |
| Authorization | pre-registered clients pass issuer, resource, refresh, exact step-up, issuer-bound cache, and confidential `private_key_jwt` behavior; Dynamic Client Registration is absent |
| Logging metadata | request log level never persists across requests; no request receives `notifications/message` because the Veoveo profile does not advertise Logging |
| Deprecated features | Roots, Sampling, MCP Logging, URL-mode elicitation, HTTP+SSE, non-`none` `includeContext`, Dynamic Client Registration, legacy sessions, and legacy subscriptions are not advertised or accepted |
| Current product protocols | authoritative live-view v2, Recording playback v8, native RRD channels, Redap, WebRTC, pose ingress, and private runtime streams retain their documented boundaries and are not routed through MCP subscriptions or mistaken for legacy MCP transport |
| Domain state | Simulation View and UAV domain sessions, reconciliation state, durable deadlines, and viewer leases survive the removal of protocol sessions; no domain handle acquires authority from process locality |
| Rig | blocking and streaming agent runs have identical task semantics; serialized pending runs resume against reconnected task handles |
| Python and TypeScript | Python SDK 2, modular TypeScript client/server 2, templates, Console, and MCP Apps use only final methods and response shapes; the Python Tasks binding is a typed extension because 2.0.0 does not ship Tasks |
| Extensions | anonymous external conformance and integration fixtures consume the published revision-3 artifacts without Veoveo source |
| Deployment | rendered charts carry the intended replica and GPU-owner constraints; images and charts use immutable release identities; focused deployment acceptance passes |
| Repository | workspace tests, Clippy, docs, Python tests, frontend tests, Rust smokes, profile acceptance, showcases, focused deployment smoke, and focused headed hardware-GPU browser acceptance pass |
| Hard cut | searches find no `2025-11-25`, `2026-06-30`, `Mcp-Session-Id`, `tasks/result`, `tasks/list`, `taskSupport`, `resources/subscribe`, `resources/unsubscribe`, protocol `ping`, `logging/setLevel`, `notifications/roots/list_changed`, `notifications/elicitation/complete`, `elicitationId`, resource error `-32002`, or legacy TypeScript `@modelcontextprotocol/sdk` outside explicit historical migration evidence |

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

### Surface-policy drift

Discover and list methods may change after a server upgrade. Readiness compares the
observed surface with the installation revision before admitting traffic. Missing
required names fail closed, while newly observed names remain hidden until installation
policy admits them.

### Opaque-state confusion and replay

Stateless requests make every surviving handle security-sensitive. Strong types keep
task IDs, request state, cursors, and application handles from crossing method
boundaries. Integrity, expiry, current authorization, and durable one-time consumption
cover the cases that possession alone cannot.

### Audit loss during volume reduction

Removing duplicate successful-authentication rows must not remove policy evidence.
Acceptance compares one logical action with its trace, authentication metadata, and
policy record. Token lifecycle, replay, denial, and governed state transitions retain
their existing durable evidence.

### Untrusted routing and trace metadata

Model-controlled parameter headers, `clientInfo`, `serverInfo`, trace state, and
baggage can describe a request but cannot authorize it. Installation allowlists, body
agreement, strict size limits, sanitization, and independent policy evaluation prevent
these inputs from becoming an authority channel.

### Private OAuth onboarding

Pre-registration favors deterministic private installations but requires an explicit
client-onboarding step. Installation documentation and typed configuration own that
step. Veoveo does not add Dynamic Client Registration or remote client-metadata
fetching merely to remove the administrative action.

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

- row-by-row closure of the official `2026-07-28` changelog ledger;
- exact Veoveo, Rig, and `rmcp` revisions;
- official conformance versions and complete results;
- workspace and language dependency locks;
- surface-readiness and effective-capability evidence;
- task, opaque-handle, multi-round, subscription, and replica evidence;
- schema, tool-name, tool-error, caching, header, trace, authorization, and audit
  evidence;
- preservation evidence for live-view v2, Recording playback v8, native RRD, Redap,
  WebRTC, domain sessions, and durable reconciliation boundaries;
- deployed image and chart digests;
- full showcase and visual acceptance evidence;
- the list of deleted protocol modules.
