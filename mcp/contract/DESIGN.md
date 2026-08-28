# Veoveo MCP Server Contract

This document is the normative contract for every hosted MCP server and every
extension registered with a Veoveo installation. It consolidates the protocol,
schema, runtime, packaging, documentation, and self-description requirements
that were previously stated across `AGENTS.md`, `docs/TECH_DESIGN.md`, and
`docs/ENTERPRISE_DEPLOYMENT.md`; those documents now point here. The crate in
this directory, `veoveo_mcp_contract`, implements the shared mechanics that
make most of the contract hold by construction.

**Contract revision: 3.** The crate exports the same value as
`veoveo_mcp_contract::CONTRACT_REVISION`. A server declares the revision it
complies with in its crate documents and in its contract resource.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | protocol version `2026-07-28`; Discover is mandatory and Initialize is excluded from the hosted profile |
| MCP Streamable HTTP | stateless POST requests with JSON terminal responses capped at 8 MiB after serialization; SSE is used only by methods whose final flow requires a stream; protocol sessions, reconnect GET, DELETE, and replay are excluded |
| MCP Tasks, SEP-2663 | official `tasks/get`, `tasks/update`, and `tasks/cancel`, optional task notifications, opaque task IDs, and typed terminal payloads |
| MCP multi-round requests, SEP-2322 | `input_required`, protected opaque `requestState`, and retry `inputResponses`; server-initiated elicitation is excluded |
| MCP subscriptions | request-scoped `subscriptions/listen` with an authorized accepted filter; resource subscribe and unsubscribe are excluded |
| JSON Schema 2020-12 | ordinary SDK and Pydantic generation with bounded references and composition; controlled gateway, fragment, binding, and provenance schemas remain typed |
| W3C Trace Context and Baggage | `traceparent`, `tracestate`, and `baggage` in MCP request metadata with the authenticated HTTP boundary as the trust gate |
| OAuth 2.0, RFC 8414, RFC 9207, RFC 8707, RFC 9728, and OpenID Connect Discovery 1.0 | private-installation profile with pre-registered clients, exact issuer and resource binding, step-up scopes, and `private_key_jwt`; Dynamic Client Registration is excluded |
| `veoveo.io/gateway-server-fragment/v1` | extension-owned server capabilities and platform requirements |
| `veoveo.io/gateway-binding/v1` | installation-owned exposure, policy, artifact audience, and recording producer declarations |
| `veoveo.io/gateway-composition-provenance/v1` | exact input/output SHA-256 identities and contributed-object summaries |
| `veoveo.io/live-view/v4` | provider-neutral authoritative camera descriptions, typed camera regions in shared encoded products, actor-and-browser authorizations, hardware encode identity, WebSocket H.264 endpoints, and redacted connection tokens |
| `io.veoveo/app-resource-dependencies` | deterministic gateway projection of exact cross-server App resource-read requirements admitted under active profile and actor authority |

Each hosted server manifest declares separate typed upstream URLs for MCP and
health traffic. The health URL is an unauthenticated HTTP `GET` endpoint whose
successful response means the process can serve traffic. The gateway never
uses an MCP request, an authentication failure, or a method rejection as a
health signal.

## Live View Extension

The live-view extension describes cameras rendered by the authoritative domain
runtime. A simulation server owns its logical camera rigs and persistent stream
products whose typed regions map one or more cameras into encoded frames. A live-view
authorization identifies the gateway
actor and browser instance, but it never allocates rendering or encoding state. Any
number of authorized viewers may consume the same encoded product within the host's
ordinary network and process limits.

The shared types define camera poses, optics, smoothing, health, stream policy,
typed product regions, NVIDIA NVENC metadata, and WebSocket H.264 endpoints. Product state reports a bounded
authoritative-source-to-render sample count and p95 in integer microseconds; the
implementation defines the exact source and render events that bracket that measurement.
Domain-owned resource URIs use the
canonical shape `{scheme}://session/{session_id}/live-view/{live_view_id}`. The
contract does not prescribe Isaac, USD paths, a scene mirror, or a common renderer.

## Scope And Discovery

The contract governs the servers in `servers/*-mcp/` and any independently
deployed extension whose gateway entry joins an installation's catalog.

Checks are generic over a discovered catalog and never enumerate servers by
hand:

- In the repository, a server is any directory under `servers/` whose name
  ends in `-mcp`, regardless of implementation language.
- Against an installation, the server set is the gateway control-plane
  catalog.

Hosted servers may consume another server's canonical resources when the active
installation profile and policy expose them. This is the supported integration
path for reusable domain capabilities such as Map data. An App that performs a
cross-server read declares a typed App dependency; the gateway projects only
the caller-authorized declaration. This grants no direct storage, private HTTP,
renderer, or credential access, and it does not authorize mutations.
- Transport-invariant checks additionally name the known MCP endpoints that
  live outside `servers/`: the gateway itself, the bridges, and showcase
  extensions such as `showcase/sumo/sumo-mcp`.

Adding a server means the checks find it. No conformance manifest, Console
page, or documentation index requires editing when a server is added.

Hosted server manifests may declare exact cross-server App resource
dependencies. Each declaration binds one server-owned `ui://` App resource
to a registered target server, that server's canonical URI scheme, a
non-root prefix, a required scope, permitted operations, and optional data
labels. Validation rejects incomplete or mismatched declarations. The
gateway projects only dependencies admitted by the caller's active profile,
scopes, and labels; the eventual resource read remains subject to ordinary
Work Context and resource authority.

## Protocol Surface

Veoveo does not flatten MCP into a collection of convenience tools. Each
server uses the protocol surface that matches its domain:

| Need | Canonical MCP surface |
|---|---|
| action | tool with declared input and output JSON Schemas |
| durable action | task-augmented tool through the MCP tasks API |
| addressable state | resource or resource template |
| discovery | resource list/template plus completion |
| reusable interaction | prompt |
| live condition | `subscriptions/listen` and a resource notification filter |
| progress/result wake | task-ID filter on `subscriptions/listen`; `tasks/get` remains the correctness path |
| cross-server identity | canonical URI and resource link |

A successful terminal task that creates an addressable product returns one
top-level `result_uri` in `structuredContent`. The value is the canonical URI
owned by the producing domain. The adjacent human-readable content is a short,
identity-free status and contains one resource link for that result. Typed
provenance and artifact metadata remain in structured content. A task that does
not create an addressable product omits `result_uri` and does not invent a
resource identity.

Growing domain collections are read through bounded domain-owned pages with a
stable order and opaque cursors. Exact canonical-URI reads use the owning
domain identity and do not require a full collection scan. `resources/list`
advertises stable roots and templates rather than enumerating every dynamic
record. Completion queries are bounded at their authoritative store.

An agent reads resources through a governed current-profile adapter rather than an
unrestricted protocol peer. The adapter admits absolute domain resource URIs and
bounded text or JSON content. It rejects browser, local-file, network, credential,
fragment, HTML, event-stream, binary, and oversized inputs. Episode-local accounting
limits read count, resource families, bytes, wall time, and pagination depth.

A missing resource remains JSON-RPC `Invalid Params` (`-32602`) on the wire. The agent
may receive a fixed correction record containing a sanitized requested URI, a stable
code, static guidance, `automatic_retry: false`, and the remaining safe budget. The
adapter never copies upstream error text or data into model context. Authorization,
timeout, transport, and internal failures remain fixed generic failures, while a
schema-valid domain rejection remains an ordinary tool result with `isError: true`.

Compatibility helpers are allowed only when they are explicit product features
for clients that cannot use the richer MCP surfaces well. They must be
additive projections over the canonical protocol behavior and must reuse the
same typed models, policy checks, audit paths, task state, artifact
identities, and resource URIs. Hidden fallbacks, alternate completion paths,
unaudited content URLs, and second sources of truth are prohibited.

The gateway records authorization and execution as separate audit facts. A
policy event reports whether `tools/call` was admitted. After an admitted call
returns, a tool-call event reports `succeeded` or `failed`, the bounded result
kind, duration, and the JSON-RPC error code when the failure crossed that
boundary. It never records tool arguments, provider payloads, credentials, or
an upstream error message. Both records carry the same trace identity, which
keeps policy admission distinct from domain or protocol completion.

Every tool declares its exact MCP task support as `required`, `optional`, or
`forbidden`. A full-MCP client receives that declaration unchanged. A
`tools_compat` registration may explicitly enable the direct task-call
adapter. For that registration alone, the gateway projects `required` as
`optional`; it leaves `optional` and `forbidden` unchanged. A direct call to
the projected tool starts the same final-extension task, waits on its canonical
subscription, returns the terminal tool result, and attaches the canonical
task ID. The typed `veoveo://task/{task_id}` resource exposes its current
status and terminal result without introducing another task store.

## Stateless Transport And Explicit State

Every network endpoint uses MCP Streamable HTTP with stateless POST requests.
Clients call Discover and attach the selected protocol version, client identity,
and effective per-request capabilities to each ordinary request. Terminal results
use JSON. A request uses SSE only when its final protocol flow requires a stream,
including `subscriptions/listen` and in-flight progress. Initialize, protocol
session IDs, reconnect GET, DELETE, and Last-Event-ID replay are rejected.

Legacy HTTP+SSE is unsupported. Network stdio is not a transport or
registration value. Stdio may exist only between one local bridge process and
the child MCP server whose lifecycle that bridge owns. The gateway always sees
the bridge as a Streamable HTTP endpoint.

Every cross-call state value is an opaque typed handle whose authority, expiry,
integrity, and replay rules are validated on each request. Domain sessions such as
UAV simulation sessions remain application state; they never derive authority or
lifetime from MCP transport locality.

The gateway signs a fresh short-lived internal assertion for every upstream
request. Connection reuse depends only on validated transport security and
catalog generation; request authority stays in the assertion and request metadata.

Gateway traffic with the same validated transport-security configuration and
active catalog revision shares one process-wide HTTP connection pool and one
initialized TLS trust store. Construction is single-flight under concurrency.
A catalog revision or transport-security change selects a new pool identity.

Capability declarations name the exact signal a server can produce.
`tools.listChanged`, `prompts.listChanged`, and `resources.listChanged` are
independent claims. The gateway merges and forwards those upstream claims. An
isolation-mode profile additionally declares gateway-owned resource and tool
list changes because its authorized federated catalog grows as independent
discoveries complete.

Federated resource discovery and isolation-mode tool discovery never wait for
an uncached hosted server. The gateway returns the authorized per-server cache
entries already available and attaches a typed
`veoveo.io/gateway-discovery-degradation` result metadata document naming only
the missing server, surface, and bounded failure code. Each missing server starts
one background discovery for the exact catalog generation and invocation
authority. Repeated list calls share that single in-flight operation. A healthy
server commits and publishes its matching MCP `listChanged` notification as soon
as it responds, regardless of any other server. A failed operation is not cached
and becomes eligible on the next explicit list call.

A profile whose work requires a complete tool catalog sets
`discovery_failure_mode` to `fail_closed`; its tool list fails until every
exposed server is reachable, which prevents an autonomous client from retaining
a silently incomplete toolset. Upstream `listChanged` notifications invalidate
successful per-server entries and wake callers without polling. Direct resource
reads and tool calls remain fail closed.

## Schemas And Types

Tool inputs publish JSON Schema 2020-12 generated by the ordinary rmcp/Schemars
path in Rust and the official SDK/Pydantic path in Python. References and
composition are allowed when emitted by those generators. Recursive or excessive
schemas are rejected by explicit depth, node-count, reference-count, branch-count,
and serialized-size bounds.

Strong types govern every controlled shape: typed structs, enums, and explicit
domain types wherever the shape is known or owned by this contract. Raw JSON
is reserved for genuinely open-ended boundaries.

Content digests establish integrity and provenance. They do not become a
parallel public address. Artifact occurrences use fresh opaque UUIDv7
identities and may be presented under the producing domain's canonical scheme.

## Runtime Boundary

A hosted server owns its domain models and declared schemas and consumes the
shared mechanics of `veoveo_mcp_contract` rather than reimplementing them:
task records and the task runtime, webhook waiters, resource subscriptions,
URI conventions, Work Context propagation, and internal identity.

- Durable operations run on the shared task runtime and the final task
  extension.
- Artifact and recording operations present the forwarded short-lived
  internal identity signed by the gateway.
- Administrative HTTP, when a server has it, is served only under the
  server's canonical mount and reached through the gateway admin route.
- A server has no private control database. Durable state lives in the
  platform stores.
- A server has no private byte route. Bytes flow through the artifact plane.
- Every Rust Streamable HTTP endpoint applies the shared terminal-response
  middleware after final JSON serialization. A response through 8 MiB is
  delivered unchanged. A larger response is discarded in full and replaced
  by JSON-RPC error `-32010` with diagnostic code
  `response_budget_exceeded`, `maximum_bytes`, and `actual_bytes` when the
  completed byte count is available. A body collection failure uses
  `response_serialization_failed`. Neither diagnostic contains a partial
  result or an internal error detail.

## Deployment Identity

Ordinary hosted MCP endpoints and the gateway run behind load-balanced replicas.
Durable Tasks, explicit handles, and shared event sources preserve correctness
across replica changes. A workload remains singleton only when it owns real
exclusive state or hardware, such as DuckDB or a GPU simulation runtime; protocol
session locality is never a reason for singleton deployment.

## Packaging And Registration

A server ships as an OCI image with a versioned Helm chart. Its gateway entry
is registered in the typed control plane with its routes and capabilities, and
states the contract revision the server complies with. An external server
publishes a `gateway-server-fragment/v1`; an installation grants exposure and
policy only through a separate `gateway-binding/v1`. The offline composer
emits an ordinary validated control plane. Extensions follow this pattern
without adopting Veoveo's source build; the mechanics are in
[`docs/EXTERNAL_EXTENSIONS.md`](../../docs/EXTERNAL_EXTENSIONS.md).

## Well-Known Surface

Every server is self-describing. Under its canonical URI scheme it serves:

| Resource | Content |
|---|---|
| `{scheme}://docs` | index of the server's documents |
| `{scheme}://docs/{doc_id}` | a document body: at minimum `agents` (the crate `AGENTS.md`) and `design` (the crate `DESIGN.md`) |
| `{scheme}://contract` | machine-readable contract declaration: contract revision, per-item compliance status, and embedded-document evidence; Discover and list methods own the observed runtime surface |

On its administrative mount the server serves the same material as a read-only
HTTP projection at `{mount}/admin/docs/llms.txt` (an index in llms.txt form) and
`{mount}/admin/docs/{doc_id}`. Links in `llms.txt` are relative to that directory:
`agents` and `design`, not paths that repeat `docs/`. The projection requires the
same gateway-issued internal identity as the server's MCP endpoint. It is not a
public exception to refused-by-default authentication and does not establish an
alternate domain administration API.

Documents are embedded at build time from the crate, so a running server
serves the manual for exactly the version deployed, including in offline
installations. The `veoveo_mcp_contract::docs` module provides the embedding,
declaration, and rendering machinery; consuming it is the intended way to
comply.

The Console renders these resources generically; the gateway generates an
installation llms.txt from the catalog. Neither requires per-server work.

## Crate Documents

Documentation lives beside the code it governs, written for agents first and
readable by humans:

- `DESIGN.md` — the server's domain contract, including its standards and
  protocols profile.
- `AGENTS.md` — the agent work manual, delta-only over the repository root
  `AGENTS.md`, with required sections `Purpose`, `Invariants`,
  `Build And Test`, and `Contract Compliance`. The compliance section lists
  checklist items with status `met` or `pending`, so gaps are declared rather
  than silent.

Server crates are named `*-mcp`.

## Compliance Checklist

| ID | Level | Requirement |
|---|---|---|
| C01 | MUST | Each capability uses the canonical MCP surface for its need per the Protocol Surface table. |
| C02 | MUST | Every tool declares input and output JSON Schemas; an addressable terminal product has one top-level canonical `result_uri`, while a no-product task omits it. |
| C03 | MUST | Durable operations are task-augmented tools on the shared task runtime. |
| C04 | MUST | Addressable state is exposed as resources or resource templates under the server's canonical scheme; growing collections use bounded domain-owned pages and exact reads do not scan the full collection. |
| C05 | MUST | The server is not flattened to a tool-only convenience surface. |
| C06 | MUST | Compatibility helpers are additive projections reusing canonical models, policy, audit, tasks, and URIs. |
| C07 | MUST | Tool input schemas use JSON Schema 2020-12 and pass the shared depth, node, reference, branch, and size bounds. |
| C08 | MUST | Schemas are generated through ordinary rmcp/Schemars or official SDK/Pydantic machinery. |
| C09 | MUST | Controlled shapes use strong domain types; raw JSON only at open boundaries. |
| C10 | MUST | Shared mechanics, including the final 8 MiB serialized JSON response cap, come from `veoveo_mcp_contract`, not reimplementation. |
| C11 | MUST | Artifact and recording operations use the forwarded internal identity. |
| C12 | MUST | Administrative HTTP exists only under the canonical mount. |
| C13 | MUST | No private control database. |
| C14 | MUST | No private byte route. |
| C15 | MUST | The server ships as an OCI image with a versioned Helm chart. |
| C16 | MUST | The gateway entry is registered in the typed control plane with routes, capabilities, and policy. |
| C17 | MUST | The registration and crate documents state the contract revision. |
| C18 | MUST | Docs resources are served under `{scheme}://docs`. |
| C19 | MUST | The contract declaration resource is served at `{scheme}://contract`. |
| C20 | MUST | The admin mount serves `docs/llms.txt` and document bodies. |
| C21 | MUST | Served documents are embedded at build time from the crate. |
| C22 | MUST | `DESIGN.md` exists beside the crate and pins the domain profile. |
| C23 | MUST | `AGENTS.md` exists beside the crate with the required sections. |
| C24 | MUST | The crate is named `*-mcp`. |
| C25 | MUST | Every network endpoint uses Streamable HTTP; stdio exists only inside a local bridge that owns its child. |
| C26 | MUST | Streamable HTTP is stateless, requires final per-request protocol metadata, returns JSON terminal responses, and rejects session, reconnect, DELETE, and replay surfaces. |
| C27 | MUST | `subscriptions/listen` uses an authorized accepted filter, a request-scoped sink, bounded backpressure, and a shared restart-safe event source; legacy subscribe and unsubscribe are absent. |
| C28 | MUST | Tool, prompt, and resource list-change capabilities are declared independently and match emitted notifications. |
| C29 | MUST | Ordinary hosted servers and the gateway tolerate load-balanced replica changes; singleton workloads name the real exclusive state or GPU owner. |
| C30 | MUST | Requests with equivalent upstream transport security share one catalog-revision-scoped HTTP connection pool and TLS trust store without sharing request authority. |
| C31 | MUST | Readiness calls Discover and required list methods, compares the observed surface with installation allow/require policy, and fails closed on mismatch. |

## Enforcement

Verification is layered and discovers servers per the Scope And Discovery
rules:

- **Repository structure** — `mcp/conformance` asserts C22, C23, and
  C24 for every discovered server crate, including required `AGENTS.md`
  sections and a parseable `Contract Compliance` declaration.
- **Protocol conformance** — the conformance client validates advertised
  schemas (C07) and the client-facing protocol shape against a running
  server. Certification for `veoveo.io/hosted-mcp/v3` always checks C18–C21;
  a profile cannot forbid resources or omit the administrative docs URL.
  The client reads `{scheme}://contract`, requires the selected revision,
  matches its server identity to the discovered implementation. It follows the
  links actually published by `llms.txt` and authenticates those requests with
  credentials supplied out of band after proving the projection rejects an
  unauthenticated request at both its index and document bodies. Credentials
  may be sent to the docs projection only when it has the same HTTP origin as
  the MCP endpoint.
  Internal-assertion verification still proves only the selected boundary
  checks; forged, expired, and misaddressed assertion cases remain explicit
  profile or component tests.
- **Construction** — C03, C10, C18–C21 are inherited by consuming
  `veoveo_mcp_contract`; avoiding them requires bypassing the shared crate,
  which review treats as a contract change.
- **Transport conformance** — the shared Streamable HTTP constructor, terminal
  response-budget middleware, gateway upstream client pool, and deployment
  checks enforce C10 and C25–C31 for first-party Rust servers. Packaged servers
  must pass the same black-box checks.
- **Review** — C05, C06, C09, C13, and C14 are review-enforced boundaries;
  their violation is architectural, not stylistic.

The installation manifest owns allowed and required exposure policy. Discover
and list methods own runtime observations. Readiness intersects and compares
those two surfaces without copying observations into the contract resource.
