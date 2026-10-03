# Veoveo MCP Server Contract

This document is the normative contract for every hosted MCP server and every
domain workload registered with a Veoveo installation. It consolidates the protocol,
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
| JSON Schema 2020-12 | ordinary SDK and Pydantic generation with bounded references and composition; gateway configuration, deployment and provenance schemas remain typed |
| RFC 6570 URI Templates | Checked MCP setup uses the foundational ASCII-template profile and iri-string 0.7.14 parser; domain tests qualify expansion against typed resource addresses. Gateway policy selectors retain their documented restricted matching language. |
| W3C Trace Context and Baggage | `traceparent`, `tracestate`, and `baggage` in MCP request metadata with the authenticated HTTP boundary as the trust gate |
| OAuth 2.0, RFC 8414, RFC 9207, RFC 8707, RFC 9728, and OpenID Connect Discovery 1.0 | private-installation profile with installation or governed managed-client registrations, exact issuer and resource binding, step-up scopes, and `private_key_jwt`; OAuth Dynamic Client Registration is excluded |
| Veoveo resumable artifact HTTP upload | repository-owned JSON admission/completion and raw part PUT contract, UUIDv7 idempotency, SHA-256 integrity, and bounded browser-safe 64-bit counters; independent of MCP methods |
| Veoveo upload assertion | EdDSA JWT with `artifact-upload` audience and signed control-plane/context digests; restricted to the HTTP upload service |
| Veoveo access-token `session_family` | Signed UUIDv7 refresh-family binding for browser tokens; current family revocation is enforced by the gateway, and absence supplies no renewable session authority |
| Veoveo internal `request_context` | Signed source principal, verified access-token metadata and required audit request correlation; includes OAuth client, optional session family and managed-agent execution metadata; contains no bearer value and grants no independent renewal permission |
| Veoveo `computer_attach` policy action | Interactive access to an exact `computer://computers/{id}` resource; a platform action evaluated alongside current resource-read permission, without an MCP method |
| `ai.veoveo/app-resource-dependencies` | deterministic gateway projection of exact cross-server App resource-read requirements admitted under active profile and actor authority |
| `ai.veoveo/knowledge-source` | Veoveo extension that declares resource collections as knowledge, with typed read observations and conditional reads; specified in [the knowledge source extension](../knowledge-extension/DESIGN.md) |

Each hosted server manifest declares separate typed upstream URLs for MCP and
health traffic. The health URL is an unauthenticated HTTP `GET` endpoint whose
successful response means the process can serve traffic. The gateway never
uses an MCP request, an authentication failure, or a method rejection as a
health signal.

## Domain Contract Ownership

Recording’s [domain contract](../../platform/recordings/contract/DESIGN.md) owns its
public models and addresses below Hub and the MCP server. The server library exposes
those types through its isolated `contract` feature, which the gateway’s Recording
adapter imports. MCP core owns neither those models nor a dependency on either package.

UAV's library owns the `veoveo.ai/live-view/v4` models for logical cameras, encoded
products and viewer authorizations. Consumers import its `contract` feature with
default features disabled. The [UAV design](../../servers/uav-sim-mcp/DESIGN.md#library-features)
defines that profile and its dependency boundary. Shared MCP mechanics do not depend
on the UAV library or enumerate its camera and resource vocabulary.

## Scope And Discovery

The shared crate also owns the closed Console bootstrap DTOs in
`src/gateway/console.rs`. This repository-owned HTTP projection carries authenticated
session presentation and branding. It is separate from the MCP protocol and from
administrator inventory. The [gateway projection design](../../platform/gateway/src/bin/gateway/console/DESIGN.md)
defines its authority boundary. Browser models are generated with
`cargo xtask release client-types`; Rust remains the wire source of truth.

The contract governs the servers in `servers/*-mcp/` and any independently
deployed domain server whose gateway entry joins an installation's catalog.

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
  workloads such as `showcase/sumo/sumo-mcp`.

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

## Typed Scopes And Resource Identities

Each owning contract declares a closed enum for the scopes its code understands.
The enum maps each variant to one OAuth wire spelling. Domain authorization helpers
accept that enum. A generic policy engine accepts validated `ScopeName` values because
installations and external providers can define additional scopes. `ScopeName` validates
syntax; it does not establish membership in a domain's supported scope vocabulary.

Server libraries own their domain scope enums, IDs, resource address variants and
Task operation enums. Operations implement `veoveo_types::TaskTypeDefinition`; the
`declare_task_types!` helper checks complete, distinct names at compilation. Shared
Task admission and snapshots carry `TaskTypeName`. MCP adapters can parse incoming
operation names into their own enums for exhaustive Task dispatch.
Shared infrastructure cannot require a vocabulary change when a server adds a scope
or resource family. The gateway consumes validated names and registration data without
a compiled registry of domain enums. Rust cross-server consumers import the owning
library's public contract with its `contract` feature and default features disabled.

[CE-13](../../docs/CONTRACT_EVOLUTION.md#ce-13-modular-types-and-server-owned-contracts)
defines the accepted separation of foundational types from MCP integration. The
foundational names, platform identity types, access subjects, resolved invocation
authority and output defaults, reference types, and provenance digests live in `veoveo-types`; consumers import
them directly. Authentication and authorization still use the existing Principal,
Work Context membership, and policy implementations. Domain-owned Artifact identity
and metadata live in [`veoveo-artifact-contract`](../../platform/artifacts/contract/DESIGN.md).
That owner also supplies `ArtifactUri`. Artifact service interfaces and server URI
conventions consume it and the foundational `ResourceScheme`; MCP does not enumerate
producing domains. Metadata JSON keeps its published identity fields with checked
ID/URI agreement. Other generic URI convention families still need builder adoption.
Coordinate contracts belong to the Map and Frames library contract features, with
recording-specific spatial metadata in RRD. MCP core has no coordinate domain registry
or dependency on those libraries.
Media prediction summaries and generation results belong to the Media library's
contract feature. Shared protocol infrastructure does not own those domain DTOs.
The foundation's `AccessGrant`, `ScopeDefinition` and `ResourceAddress` traits accept independent
domain implementations. The foundation validates concrete references through its URI
parser and gives template declarations a separate type. Its component parser and
builder apply the stricter profile for domain-owned addresses. Network resource
identities may include ports; constructing a generic reference does not admit a domain route.
Wider URI builder and checked setup adoption are implementation work in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).
`server_contract::McpServerContract` associates each server's scope and resource types
with its MCP configuration, documents, descriptors, and templates. A server implements
this open trait in its MCP feature; its public contract feature needs only the foundational
traits. `McpResource` builds descriptors from typed addresses, checks their parse round
trip, and rejects metadata that changes the address.

Task-backed resource addresses may implement the foundational `TaskResourceAddress`
trait. The [Task runtime](../../platform/task-runtime/DESIGN.md#task-backed-resource-observation)
can then deliver requested Task status and resource invalidations from one authorized
watch. Domain libraries own the backing identity and resource admission; MCP core
contains no resource-to-Task registry.

`McpServerSetup` checks implementation and document identity, required document coverage,
resource capability, and duplicate declarations before handlers serve discovery. It
provides sorted resources and templates and compares typed permissions against an
authenticated grant set. An empty scope vocabulary is valid. Undeclared permissions
fail the membership check, and unrelated grants do not invalidate the caller's set.
`McpResourceTemplate` binds metadata to the foundational `ResourceTemplateUri`, which
uses the RFC 6570 parser. Its builder rejects a descriptor that changes the admitted
template. Setup checks template uniqueness, and owners qualify expansion against
their resource parser. The foundational `ResourceSelector`, `ResourceUriPrefix` and
`ResourceUriTemplate` types own the gateway's restricted lexical selection language.
Their matching semantics are separate from RFC 6570 template expansion. This setup establishes API and declaration consistency;
hosted conformance and domain tests establish the relevant behavior. Authentication,
resource visibility, and operation policy stay with their existing owners.
The [Python SDK](../../sdk/python/DESIGN.md#checked-mcp-setup) implements these associations
as generic protocols and checked setup. Its foundational module has no MCP imports;
Python owners supply their own scope enum, resource variants and parser. Datasheet
consumes that setup before starting its dependencies.
Gateway template discovery and resource completion use `PolicyTarget::ResourceTemplate`;
concrete resource targets use their existing variants. The shared evaluator preserves
the selector language and checks action/target consistency. Native administration
uses `PolicyTarget::PlatformTask` with a foundational Task UUID; MCP routes use the
opaque `PolicyTarget::Task` identity. Both apply the same server exposure and policy.
The gateway converts these targets into the [unified audit contract](../../platform/audit/contract/DESIGN.md).
Server contracts may use the foundation's `scope_enum!` declaration helper to generate
their scope conversions and schemas from one set of wire spellings.
The helper also supports empty vocabularies. Generic control-plane validation checks
scope syntax and configured grant relationships; transport adapters enforce their
owner's required permissions before activating a Gateway catalog.

Handlers, task admission, and configuration defaults share the owning definitions.
The wire protocol still carries scope strings, and its parser validates them before
passing them into policy code. A scope name conveys no authority by itself; checks
compare it with the authenticated caller's current grants.

Domain IDs keep their specific types in internal requests, cursors, and query APIs.
Resource URI constructors require the corresponding ID types. Parsers validate the
wire URI and return a typed resource variant before dispatch. Driver bindings and
serialization may convert those types to text. This rule applies across Veoveo;
existing string-based interfaces require migration when their contracts change.

The shared resource URI abstraction delegates component parsing and encoding to a
maintained URI library. Domain builders own route shapes and accepted parameter
names. They must reject duplicate or unsupported parameters and validate each parent
ID before dispatch. A generic URI parser cannot establish domain identity or parent
relationships. Code does not use interpolation, concatenation, or delimiter chains
to assemble or parse dynamic resource URIs. Fixed discovery literals and URI
templates describe the same routes and require agreement tests with the builders.
Migration must qualify existing wire spellings and normalization behavior before
changing a published URI. The workspace already pins `url`; adopting another parser
requires a concrete unsupported URI profile and dependency qualification.

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

The final Tasks extension negotiates `io.modelcontextprotocol/tasks` in Discover
and each request's client capabilities. The server decides whether an admitted
`tools/call` returns a durable Task. An operation that requires Tasks rejects a
client without the extension using `-32021` and its required-capabilities payload,
before accepting work. The published
[Tasks extension](https://github.com/modelcontextprotocol/ext-tasks/blob/main/schema/2026-07-28/schema.ts)
replaces the older per-tool `execution.taskSupport` and request `task` handshake;
those fields are not part of this profile.

Durable Task ownership binds the actual actor, tenant, Work Context, profile and
invocation provenance. Current access checks apply on every read, update, cancel
and subscription. Signing in again or changing unrelated scopes does not create a
different owner. Retained data labels and current domain permissions still constrain
results and actions. The gateway's persisted transition is specified in
[`Task Routes`](../../platform/gateway/src/state/task_routes/DESIGN.md).

A `tools_compat` registration may explicitly enable the direct task-call adapter.
That adapter supplies the negotiated capability upstream, waits on the canonical
subscription and returns the terminal result with its canonical Task ID. It shares
the original Task and authority. The typed `veoveo://task/{task_id}` resource exposes
current status and terminal result without introducing another task store.

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

The gateway signs a fresh internal assertion for every upstream request. Ordinary
requests have a 60-second maximum assertion lifetime. A `subscriptions/listen` POST
has a 15-minute maximum, because the source enforces assertion expiry throughout the
stream. Both lifetimes end at the source access token's expiration when it is earlier.
This permits a longer bearer lifetime only for assertions minted when opening a
subscription; those credentials stay on the private gateway-to-server transport.
Clients renew their own authority and reopen listeners before that deadline.
Connection reuse depends only on validated transport security and
catalog generation; request authority stays in the assertion and request metadata.

Authenticated MCP and HTTP proxy requests include typed `request_context` in the
internal assertion. It preserves the JWT-verified principal before delegated actor
derivation and its access-token metadata. Rust and Python verify the context against
the actor, tenant, Work Context, scopes and invocation provenance. The assertion's
expiration cannot exceed the source access token's expiration. Every supplied context
also carries the platform audit contract's checked request, trace and span IDs and
optional source IP address. Reusing a transport
never supplies another browser family's request context.

Bootstrap fixtures, reconstructed Task owners and upload-only assertions may omit
this field. Omission grants no durable or renewable request authority. A consumer
that admits renewable Computer access or stores current-policy execution authority
must require this context and perform its own current policy/grant checks. It must
not infer client identity or family membership from an old Task owner. Accepted
background work can outlive admission tokens only under its explicit durable
execution policy, with fresh dispatch authority and separately bounded access.

This optional signed-claim extension keeps MCP revision 3 unchanged. Update gateway
issuers and selected consumers before admitting the Computers profile. Existing
short-lived assertions without the field remain usable only for consumers that do
not require it. The gateway/auth owner must retire those deployment revisions before
Computers acceptance; rollback cannot admit Computers through an issuer that omits
the field. Shared direct/delegated/automated fixtures qualify both SDK readers.

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

Federated resource discovery and isolation-mode tool discovery start missing
servers independently. A list call gives these shared operations one two-second
settlement window across all selected servers, then returns the authorized
per-server cache entries available at that point. Warm complete catalogs return
immediately. This bounded window prevents routine cache expiry from producing an
empty first snapshot while keeping slow optional servers isolated. The gateway
captures entries valid when the list request begins and includes them even if
another server consumes the settlement window before response assembly. The
response attaches a typed
`ai.veoveo/gateway-discovery-degradation` result metadata document naming only
the missing server, surface, and bounded failure code. Each missing server starts
one background discovery for the exact catalog generation and invocation
authority. Repeated list calls share that single in-flight operation. An unfinished fetch reports `discovery_pending`; a failed fetch reports
`upstream_unavailable`. A healthy server commits independently and publishes its
matching MCP `listChanged` notification when content changes or a previously
reported gap recovers. An identical refresh does not generate another catalog
change. Discovery caches per-item admission decisions with the caller authority,
policy revision and catalog generation. Native catalog subscriptions invalidate those
decisions on change. Each list request commits one audit record with visible and denied
counts and the visible-set digest before returning. A warm list evaluates no policy.
A failed fetch becomes eligible on the next explicit
list call.

A profile whose work requires a complete tool catalog sets
`discovery_failure_mode` to `fail_closed`; its tool list fails until every
exposed server is reachable, which prevents an autonomous client from retaining
a silently incomplete toolset. Upstream `listChanged` notifications invalidate
successful per-server entries and wake callers without polling. Direct resource
reads and tool calls remain fail closed.

Workspace model preparation checks the degradation metadata for the servers of
its configured tools. An incomplete required surface stops preparation before a
model request; an unrelated unavailable server does not. Policy-filtered absence
in a complete surface remains authoritative.

Request-scoped subscription delivery invalidates the matching discovery cache before
forwarding its list-change notification. RMCP routes these notifications separately
from ordinary client callbacks; both delivery paths enforce the same invalidation.
Discovery fetches carry an exact claim. Invalidation retires both cached data and
pending claims, preventing late results from reinstalling old contents or settling a
replacement fetch. A disconnected catalog subscription invalidates its cache, and
recovery establishes a new subscription before fetching the catalog again.

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

Shared telemetry exports OTLP/HTTP protobuf logs and traces only when an endpoint
is configured. Its batch processors use OS threads, so the HTTP exporter selects
the blocking client explicitly. Cargo feature unification must not switch that
client to one requiring an ambient Tokio reactor. Standard per-signal and global
OTLP timeouts remain in milliseconds, with the upstream ten-second default.

A hosted server owns its domain models and declared schemas and consumes the
shared mechanics of `veoveo_mcp_contract` rather than reimplementing them:
task records and the task runtime, webhook waiters, resource subscriptions,
URI conventions, Work Context propagation, and internal identity.

- Durable operations run on the shared task runtime and the final task
  extension.
- Artifact and recording operations present the forwarded short-lived
  internal identity signed by the gateway. Durable Artifact reads may instead
  present an explicitly issued, bounded task-read capability through the dedicated
  internal routes defined by the [Artifact service](../../platform/artifacts/service/DESIGN.md).
  Capability issuance requires that gateway identity; a task capability cannot
  authenticate an ordinary operation or mint another capability.
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

## Hosting A Server

A Rust server is hosted through `veoveo_mcp_contract::hosting` and writes no
`impl ServerHandler`, router, authentication middleware, or host check. It
supplies three things:

1. **A checked setup.** Implement `McpServerContract` with the server's scope enum,
   typed resource address, documents and resource declarations, and force its
   `McpServerSetup` before Store or engine initialization.
2. **A domain.** Implement `DomainServer`: `tool_router` for the tools,
   `describe_tool` to adjust descriptors such as App links, `read` for one admitted
   address, and optionally `complete`. `read` receives a parsed address; its
   documents and contract variants answer `served_by_host()`, because the host serves
   them first.
3. **Optional task support.** A server with durable tasks wraps its
   `DurableTaskService` in `veoveo_task_runtime::DurableTasks`; a server without
   tasks uses the default `NoTasks`.

```rust
let server = HostedServer::builder(&*setup::SERVER_SETUP)
    .deployment(&public_deployment, args.allow_loopback_hosts)?
    .allowed_hosts(args.allowed_hosts.iter().cloned())
    .internal_trust(GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?)?
    .handler(move || {
        Hosted::new(MyDomain::new(state.clone()))
            .with_tasks(DurableTasks::tasks_only(MyTasks::new(state.clone())))
    })
    .build();
server.serve(SocketAddr::from(([0, 0, 0, 0], args.port))).await
```

The builder offers `build` only after the deployment, internal trust and handler
are set. `authenticated_routes`, `public_routes` and `readiness` add
server-specific HTTP. Inside a domain method, `gateway_identity`,
`forwarded_bearer` and `plane_caller` return the verified caller.

The host gives every server the same behavior:

| Behavior | Host rule |
|---|---|
| Routes | `{mount}/healthz`, optional `{mount}/readyz`, `{mount}/admin/docs/*` and `{mount}/mcp` |
| Authentication | Gateway internal assertion on MCP, admin and authenticated routes; the token audience is the server slug |
| Host validation | 400 without a Host authority, 421 for an authority outside the deployment's allowed hosts |
| Discovery | Authenticated `resources/list`, `resources/templates/list` and `tools/list` from the setup, sorted, 100 per page, private, five-second cache |
| Well-known reads | `{scheme}://docs` routes and `{scheme}://contract` |
| Address admission | An unparseable URI is Invalid Params (-32602) before domain code runs |
| Read cache | The private resource cache policy on every domain read |
| Tools | Durable tasks start first; other calls dispatch through the tool router |
| Shutdown | SIGTERM or Ctrl-C, cancelling in-flight MCP work |

`servers/duckdb-mcp` is the reference implementation.

## Deployment Identity

Ordinary hosted MCP endpoints and the gateway run behind load-balanced replicas.
Durable Tasks, explicit handles, and shared event sources preserve correctness
across replica changes. A workload remains singleton only when it owns real
exclusive state or hardware, such as DuckDB or a GPU simulation runtime; protocol
session locality is never a reason for singleton deployment.

## Packaging And Registration

Server manifests may approve Knowledge collections through the generic domain types
in [the Knowledge contract](../../platform/knowledge/contract/DESIGN.md). Each approval
belongs to the manifest's server and names registered data labels. The manifest must
offer resources and resource templates. Approvals do not add source-owned vocabulary
to MCP core.

An OAuth client's optional `knowledge_indexing` registration requires a tenant-bound
automated `private_key_jwt` client using only `client_credentials` and one dedicated
profile. Every granted collection needs an explicit installation approval and source
resource exposure. A `catalog-only` approval permits source-contract discovery;
member reads and root subscriptions require `index` approval at runtime.
The profile disables tools, prompts, Tasks, completion and Artifact
upload. Its policy allows only resource discovery, reads and subscriptions.
The [gateway](../../platform/gateway/DESIGN.md#knowledge-indexing-clients) enforces
collection and observation admission at runtime.

A server ships as an OCI image with a versioned Helm chart. Its gateway entry
is registered in the typed control plane with its routes and capabilities, and
states the contract revision the server complies with. Developers add hosted servers
inside their Veoveo fork and update the complete control-plane configuration.
Installation policy controls exposure and authority. Remote MCP integrations continue
through the supported gateway and bridge contracts. See
[`docs/FORK_DEVELOPMENT.md`](../../docs/FORK_DEVELOPMENT.md).

## Well-Known Surface

Every server is self-describing. Under its canonical URI scheme it serves:

| Resource | Content |
|---|---|
| `{scheme}://docs` | stable page of document entries with `id`, `title`, and concrete `uri`, in `items`; optional `nextCursor` names the last entry and continues through `?cursor=` |
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

Servers may adopt `ai.veoveo/knowledge-source` when their resources provide useful
knowledge. An adopting server publishes its document bodies as the `{slug}.docs`
collection. Each body is immutable for the running image, and its revision is the
SHA-256 of the embedded bytes. C18–C21 still require ordinary documentation from every
hosted server, independently of knowledge adoption.

The Rust embedding macro computes document digests during compilation. Checked
server setup attaches the docs collection to its template and declares the extension.
The shared read adapter requires the gateway-authenticated profile before serving
either a full body or a matching conditional response. An independently hosted
adapter may call the authorized-read helper after enforcing its own profile policy.
Document indexes contain at most 32 entries per page. Unknown cursors, duplicate
IDs and unsupported document query parameters fail admission.

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
| C11 | MUST | Artifact and recording operations use the forwarded internal identity; durable Artifact reads may use the service's explicitly issued bounded task-read capability. |
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
| C32 | MUST when adopted | A server that declares `ai.veoveo/knowledge-source` publishes its `{slug}.docs` collection and satisfies K01–K10 for every declared collection under the [knowledge extension](../knowledge-extension/DESIGN.md). A server that does not declare the extension marks C32 not applicable. |

## Enforcement

Verification is layered and discovers servers per the Scope And Discovery
rules:

- **Repository structure** — `mcp/conformance` asserts C22, C23, and
  C24 for every discovered server crate, including required `AGENTS.md`
  sections and a parseable `Contract Compliance` declaration.
- **Protocol conformance** — the conformance client validates advertised
  schemas (C07) and the client-facing protocol shape against a running
  server. Certification for `veoveo.ai/hosted-mcp/v3` always checks C18–C21;
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
- **Knowledge sources** — the conformance client checks K01–K08 for every
  collection a server declares; review enforces K09 and K10. Together they
  establish C32.
- **Review** — C05, C06, C09, C13, and C14 are review-enforced boundaries;
  their violation is architectural, not stylistic.

The installation manifest owns allowed and required exposure policy. Discover
and list methods own runtime observations. Readiness intersects and compares
those two surfaces without copying observations into the contract resource.

### Resource source recovery

`SubscriptionHub` carries either an exact changed URI or a reconciliation signal.
A listener projects reconciliation only onto its accepted resource identities.
The shared resource listener sends initial invalidations after registering its
receivers. Its initial list invalidation allows a catalog consumer to wait for
observation setup before listing. Task-runtime resource fan-out also sends initial
resource and requested catalog invalidations. The SDK filter acknowledgement alone
precedes asynchronous source setup. Knowledge sources implement the readiness requirement in K07.
Content-only reconciliation does not emit a resource-list change. Servers emit list
changes when discovery membership or descriptor metadata changes, independently of
updates to an existing resource's contents.
Broadcast lag requests reconciliation instead of silently discarding the gap.
Resource-list lag likewise emits an invalidation. These notifications contain no
resource content and never grant read authority. Time and Recording share projected
Store LIVE sources across their local listeners. Source reconnection invalidates
accepted identities after a delivery gap. Idle sources do not emit timer-generated
resource changes: declared resource subscriptions can wake an agent. Client-side
catalog reconciliation reads authoritative state without invoking a model.

Task-backed resource subscriptions share the Task runtime's current-owner watch.
On a new database LIVE connection, the runtime rereads admitted identities under SQL
visibility predicates. That baseline restores resource invalidations after expired
event history without forwarding denied Task payloads or emitting idle timer changes.
