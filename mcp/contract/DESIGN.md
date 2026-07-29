# Veoveo MCP Server Contract

This document is the normative contract for every hosted MCP server and every
extension registered with a Veoveo installation. It consolidates the protocol,
schema, runtime, packaging, documentation, and self-description requirements
that were previously stated across `AGENTS.md`, `docs/TECH_DESIGN.md`, and
`docs/ENTERPRISE_DEPLOYMENT.md`; those documents now point here. The crate in
this directory, `veoveo_mcp_contract`, implements the shared mechanics that
make most of the contract hold by construction.

**Contract revision: 2.** The crate exports the same value as
`veoveo_mcp_contract::CONTRACT_REVISION`. A server declares the revision it
complies with in its crate documents and in its contract resource.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | protocol version `2025-11-25` with the Veoveo hosted-server requirements in this document |
| MCP Streamable HTTP | sessionful event-stream responses, reconnectable GET, and explicit DELETE; legacy HTTP+SSE is excluded |
| JSON Schema 2020-12 | self-contained controlled tool, gateway, fragment, binding, and provenance schemas |
| OAuth 2.0 and protected-resource metadata | typed installation profiles and the extensions declared below; unsupported grants are rejected |
| `veoveo.io/gateway-server-fragment/v1` | extension-owned server capabilities and platform requirements |
| `veoveo.io/gateway-binding/v1` | installation-owned exposure, policy, artifact audience, and recording producer declarations |
| `veoveo.io/gateway-composition-provenance/v1` | exact input/output SHA-256 identities and contributed-object summaries |
| `veoveo.io/live-view/v1` | provider-neutral owner-scoped camera stream state, hardware encode identity, endpoint metadata, and redacted connection token |
| `io.veoveo/app-resource-dependencies` | deterministic gateway projection of exact cross-server App resource-read requirements admitted under active profile and actor authority |

## Scope And Discovery

The contract governs the servers in `servers/*-mcp/` and any independently
deployed extension whose gateway entry joins an installation's catalog.

Checks are generic over a discovered catalog and never enumerate servers by
hand:

- In the repository, a server is any cargo workspace member under `servers/`
  whose crate name ends in `-mcp`.
- Against an installation, the server set is the gateway control-plane
  catalog.

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
| live condition | resource subscription and notification |
| progress/result wake | task subscription |
| cross-server identity | canonical URI and resource link |

Compatibility helpers are allowed only when they are explicit product features
for clients that cannot use the richer MCP surfaces well. They must be
additive projections over the canonical protocol behavior and must reuse the
same typed models, policy checks, audit paths, task state, artifact
identities, and resource URIs. Hidden fallbacks, alternate completion paths,
unaudited content URLs, and second sources of truth are prohibited.

Every tool declares its exact MCP task support as `required`, `optional`, or
`forbidden`. A full-MCP client receives that declaration unchanged. A
`tools_compat` registration may explicitly enable the direct task-call
adapter. For that registration alone, the gateway projects `required` as
`optional`; it leaves `optional` and `forbidden` unchanged. A direct call to
the projected tool starts the same final-extension task, waits on its canonical
subscription, returns the terminal tool result, and attaches the canonical
task ID. The typed `veoveo://task/{task_id}` resource exposes its current
status and terminal result without introducing another task store.

## Transport And Sessions

Every network endpoint uses MCP Streamable HTTP. The server creates a session
for initialization and retains it while its event stream is connected.
Explicit DELETE ends the session immediately. A disconnected session keeps a
fixed 60-second reconnection grace, then the canonical session manager closes
its transport and drops the owning handler. This bound also cleans up clients
that terminate without sending DELETE. Responses use event-stream framing,
including ordinary request results. Direct JSON responses are not part of the
Veoveo profile.

Legacy HTTP+SSE is unsupported. Network stdio is not a transport or
registration value. Stdio may exist only between one local bridge process and
the child MCP server whose lifecycle that bridge owns. The gateway always sees
the bridge as a Streamable HTTP endpoint.

Sessions carry subscriptions, task and progress signals, and every other MCP
notification. Notification delivery preserves protocol order by awaiting the
session peer. Each delivery has a fixed bound on backpressure. A server never
detaches peer delivery into an unowned task.

The gateway owns one immutable internal invocation authority for each
gateway-to-server session. It signs a fresh, short-lived internal assertion for
every Streamable HTTP request in that session, including GET reconnection and
DELETE cleanup. A static bearer captured at initialization is prohibited
because its expiry would break a live notification stream and prevent session
cleanup. The session owner stops minting assertions when the session ends.
Within that owner, upstream connection reuse keys contain the actor's stable
authorization attributes and resolved invocation authority. The timestamp of
each HTTP bearer re-verification is audit metadata and never creates a new
protocol session.

Protocol sessions remain independent because their authority, subscriptions,
notifications, and cleanup are independent. Their HTTP transport does not.
Gateway traffic with the same validated transport-security configuration and
active catalog revision shares one process-wide connection pool and one
initialized TLS trust store. Construction is single-flight under concurrency.
A catalog revision or transport-security change selects a new pool identity.
This boundary prevents catalog fan-out and health probes from rebuilding the
system trust store per server or per session while preserving session-local
authorization and protocol state.

Capability declarations name the exact signal a server can produce.
`tools.listChanged`, `prompts.listChanged`, and `resources.listChanged` are
independent claims. The gateway merges and forwards only the declared claims.

## Schemas And Types

Tool inputs publish one canonical JSON Schema 2020-12 document generated from
the request type. The document has an object root, contains no references, and
declares the immediate JSON type of every property. Object-shaped unions
expose `type: object` alongside their variants. Recursive tool arguments are
outside this profile; domain contracts model bounded collections explicitly.

Rust servers import `tool` from `veoveo_mcp_contract`, which selects the
shared Schemars generator for every `Parameters<T>` handler and supplies the
closed empty-object schema for handlers without arguments. Python servers pass
each Pydantic request model through `veoveo_mcp.schema.mcp_input_schema`
before publishing it.

Strong types govern every controlled shape: typed structs, enums, and explicit
domain types wherever the shape is known or owned by this contract. Raw JSON
is reserved for genuinely open-ended boundaries.

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

## Deployment Identity

One active process owns each logical MCP endpoint. Kubernetes workloads for
MCP servers, the gateway MCP frontend, and stdio bridges run with one replica
and a non-overlapping replacement strategy. A PodDisruptionBudget does not
pretend that a singleton is highly available.

Capacity is expressed as separately named and registered MCP endpoints.
Load-balanced replicas under one endpoint identity are prohibited because
sessions, subscriptions, notifications, and task links belong to one process.
Stateless services outside the MCP boundary may retain independent replica
configuration.

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
| `{scheme}://contract` | machine-readable contract declaration: contract revision, per-item compliance status, and the server's capability inventory |

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
| C02 | MUST | Every tool declares input and output JSON Schemas. |
| C03 | MUST | Durable operations are task-augmented tools on the shared task runtime. |
| C04 | MUST | Addressable state is exposed as resources or resource templates under the server's canonical scheme. |
| C05 | MUST | The server is not flattened to a tool-only convenience surface. |
| C06 | MUST | Compatibility helpers are additive projections reusing canonical models, policy, audit, tasks, and URIs. |
| C07 | MUST | Tool input schemas follow the canonical 2020-12 profile: object root, no references, immediate types. |
| C08 | MUST | Schemas are generated through the shared machinery (`tool` macro; `mcp_input_schema` for Python). |
| C09 | MUST | Controlled shapes use strong domain types; raw JSON only at open boundaries. |
| C10 | MUST | Shared mechanics come from `veoveo_mcp_contract`, not reimplementation. |
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
| C26 | MUST | Streamable HTTP is sessionful and every response uses event-stream framing rather than direct JSON. |
| C27 | MUST | Notification delivery is ordered, awaited by the owning session, bounded for backpressure, and never detached. |
| C28 | MUST | Tool, prompt, and resource list-change capabilities are declared independently and match emitted notifications. |
| C29 | MUST | Each logical MCP endpoint has one active process and a non-overlapping replacement strategy. |
| C30 | MUST | Gateway sessions with equivalent upstream transport security share one catalog-revision-scoped HTTP connection pool and initialized TLS trust store while retaining independent MCP session state. |

## Enforcement

Verification is layered and discovers servers per the Scope And Discovery
rules:

- **Repository structure** — `mcp/conformance` asserts C22, C23, and
  C24 for every discovered server crate, including required `AGENTS.md`
  sections and a parseable `Contract Compliance` declaration.
- **Protocol conformance** — the conformance client validates advertised
  schemas (C07) and the client-facing protocol shape against a running
  server. Certification for `veoveo.io/hosted-mcp/v1` always checks C18–C21;
  a profile cannot forbid resources or omit the administrative docs URL.
  The client reads `{scheme}://contract`, requires the selected revision,
  matches its server identity to the initialized implementation, and compares
  its stable capability inventory with the observed MCP lists. It follows the
  links actually published by `llms.txt` and authenticates those requests with
  credentials supplied out of band after proving the projection rejects an
  unauthenticated request at both its index and document bodies. Credentials
  may be sent to the docs projection only when it has the same HTTP origin as
  the MCP endpoint.
  Internal-assertion verification still proves only the selected boundary
  checks; forged, expired, and misaddressed assertion cases remain explicit
  profile or component tests.
- **Construction** — C03, C08, C10, C18–C21 are inherited by consuming
  `veoveo_mcp_contract`; avoiding them requires bypassing the shared crate,
  which review treats as a contract change.
- **Transport conformance** — the shared Streamable HTTP constructor, gateway
  upstream client pool, and deployment checks enforce C25–C30 for first-party Rust servers. Packaged
  servers must pass the same black-box checks.
- **Review** — C05, C06, C09, C13, and C14 are review-enforced boundaries;
  their violation is architectural, not stylistic.

Capability inventories contain stable tool names, resource URIs, resource
template URIs, prompt names, and task-augmented tool names. Tools, templates,
and prompts match the observed lists exactly. Declared resources must appear in
the observed list, which may also contain dynamic instance resources. Every
declared task must name an observed tool, and a non-empty task inventory
requires the task extension capability. These comparisons make protocol
surface changes reviewable diffs rather than silent drift.
