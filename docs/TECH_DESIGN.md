# Veoveo Technical Design

This document explains how the self-hosted Veoveo components implement the decisions
in `ARCHITECTURE_DECISIONS.md`. The design starts from protocols. Requests, responses,
identities, and persisted records use explicit Rust types or declared schemas, and all
durable state and service ownership stay inside the installation.

## Standards And Protocols

This table is the cross-component protocol contract. Domain design documents narrow
the data standards they implement, and the root README has a shorter product-level
version.

| Standard or protocol | Technical boundary |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | Version `2026-07-28`; JSON-RPC 2.0 over stateless Streamable HTTP between client and gateway and between gateway and server. Per-request metadata and `server/discover` replace protocol sessions. Ordinary responses use JSON; `subscriptions/listen` carries request-scoped event streams. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Complete MCP tool input schemas with size limits, generated from Rust or Python types. Local references and composition are supported; remote references are rejected. Persisted and structured-result models use the same typed vocabulary. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; task creation, discovery, lifecycle updates, cancellation, terminal payloads, and subscriptions use official MCP messages rather than a job REST API. |
| [MCP Apps SEP-1865](../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; server-owned `ui://` resources use the sandboxed MCP Apps host bridge. |
| OpenID Connect and OAuth 2.0 | OIDC Core login; S256 PKCE; Client Credentials and JWT Bearer grants; RFC 8414 authorization-server metadata; RFC 9728 protected-resource metadata; RFC 8707 resource indicators; signed JWT/JWS/JWK tokens and key discovery. |
| MCP Enterprise-Managed Authorization / ID-JAG | Explicit enterprise grant profile with durable replay protection, client binding, tenant mapping, and scope reduction. |
| HTTPS and HTTP range semantics | External acquisition, MCP transport, provider webhooks, and artifact delivery. Cleartext HTTP is used only inside declared cluster trust boundaries. |
| OpenTelemetry OTLP/HTTP | Optional traces and logs from shared server instrumentation. Export stays off unless the installation supplies an endpoint. |
| Veoveo recording ingest | Version `2026-09-23`; authenticated protobuf batches and separate Blueprint publications preserve native Rerun 0.38.1 stores, ordering, idempotency, decoder-safe rollover markers, and policy-scoped replacement of a single recording. |
| Rerun 0.38.1 gRPC, RRD, Rerun Data Protocol, and `VideoStream` | Producer-local log ingestion, immutable records over time and space, lazy per-recording viewer playback, and H.264 Annex B video with exact timeline indices. |
| S3-compatible object API | Private Artifact service storage only. The bundled store is digest-pinned RustFS `1.0.0-rc.3`, the latest non-preview release candidate, because RustFS has no stable release. SurrealDB is the system of record for occurrences, identity, grants, release state, shares, policy, and audit. Clients download over HTTP streaming and byte ranges through the installation origin. |
| NVIDIA cuOpt 26.08 and CUDA 13.3 | Digest-pinned hardware-GPU execution for heterogeneous routing, BatchSolve scenarios, continuous LP/QP/QCQP/SOCP, and linear MILP. `veoveo.io/travel-model-artifact/v1` is the repository-owned Map handoff; `veoveo.io/cuopt-executor/v1` is a private pod-local adapter protocol, not a public contract. |
| Kubernetes, Helm, and OCI images | Workload graph, declarative installation configuration, registry-first delivery, GitOps reconciliation, and offline bundle material. |
| Domain standards | Map, Optimization, Time, Frames, View, UAV, Recording, Perception, and Reason designs each pin their own geospatial, solver, temporal, 3D, vehicle, and media profiles. |

## Capability Model

The normative server contract, including the mapping from need to MCP feature that
every hosted server follows, is [`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md).
This section describes how the gateway exposes servers under that contract.

The gateway discovers each upstream server's tools, resources, and prompts and
combines them into a profile. It prefixes tool names only when combining them: a
server's local `run` tool becomes `media__run`. Resource URIs keep their owning
scheme.

Each profile chooses what happens when discovery fails. By default, the gateway drops
the failing server from the combined list and reports the gap in typed degradation
metadata. A profile may instead declare `fail_closed` discovery, in which case any
unavailable server fails the whole tool list, so an autonomous client never works
from a toolset that is silently incomplete.

Each catalog entry declares two typed upstream URLs: the MCP endpoint and a required
health endpoint. The gateway sends `health_url` an unauthenticated GET and treats only
a success status as healthy. It never reads an MCP response, an authentication
failure, or a rejected method as a health signal. Health state feeds the Console. For
profiles that isolate failures, it does not affect discovery.

### Tool input schemas

The schema profile is defined in
[`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md#schemas-and-types): one JSON
Schema 2020-12 document per tool input, with an object at the root. References and
composition within the same document are supported. External references are rejected,
and limits on depth, node count, references, branches, and serialized size cap the
cost of validation.

Rust servers generate schemas through RMCP and Schemars. Python servers use Pydantic
through the SDK's `veoveo_mcp.schema.mcp_input_schema`. Each domain type owns its
schema. The gateway neither flattens references nor rewrites the schema vocabulary.

Unbounded recursive arguments are not supported. Domain contracts model collections
with explicit size limits. Servers deserialize the structured value the schema
describes and never accept arguments encoded as JSON strings.

The MCP conformance client's `info` command validates every advertised tool schema
against its declared dialect and checks that it has this shape.

Clients that support MCP Tasks call the official Tasks methods directly through a
gateway profile. The gateway routes task-augmented tool calls, get, update, cancel,
and subscriptions to the owning server and keeps the task ID unchanged. At that point
it applies profile exposure, ownership, policy, audit, and resource-URI mapping. The
standard MCP task methods are offered to clients that negotiate them, on top of the
same upstream extension.

Some registered clients are marked `tools_compat`. They get a reduced tool set built
on the same upstream operation, task ID, policy decision, audit path, subscription,
artifact identity, and result. Task support for these clients also requires the
explicit direct-adapter flag. The compatibility tools add no second protocol and store
no state of their own. Clients with full MCP support never see them.

The gateway declares tool, prompt, and resource list-change support separately, and
each declaration matches what the upstream server supports. There is no single switch
for all notifications. Request-scoped subscription streams acknowledge exactly the
filters they accepted. A catalog notification clears the matching discovery cache
before the gateway forwards it. The gateway tracks fetches in flight, so an older
response cannot bring back cached contents that were just cleared. Cache expiry and
client reconciliation with fixed limits recover from missed notifications without
calling a model. A read from a cold cache across several servers waits for one
settlement window with a fixed length, then reports any servers that are still missing.

The gateway and ordinary hosted endpoints can run as load-balanced replicas under the
[deployment identity contract](../mcp/contract/DESIGN.md#deployment-identity).
Helm sets the replica count per component, and the gateway, browser edge, and
Computers worker each have their own setting. A component that exclusively owns
storage, a provider, or a GPU may need to run as a single instance, such as the
private Computer host. Protocol sessions never force a single instance. The isolated
stdio bridge owns its child process; network stdio is not a registration option.

MCP client hosts may keep their own per-user tool permissions after OAuth grants
change, and the gateway has no control over them. Signing in again refreshes identity
and scopes but does not always reset the host's tool selection. After an installation
adds tools to a profile, operators refresh the connector's tool permissions, or
reinstall the connector if the host does not pick up the new catalog. A new
conversation then loads the updated selection. The gateway still checks every tool
call regardless of what the host has selected.

## Component Boundaries

```text
edge
  +-- console-bff -> Console and Workspace browser APIs, assets and event feeds
  +-- mcp-gateway -> hosted MCP servers
  +-- artifact-service -> public share redemption only
  +-- media-mcp -> signed provider webhooks and curated provider input files

mcp-gateway
  +-- external OAuth/OIDC and gateway authorization server
  +-- profile catalog, policy, protocol projection, audit
  +-- Workspace chat, agent-run and private-operation authority
  +-- Computers projection -> Computers worker -> private Computer host
  +-- short-lived internal identity issuer
  +-- SurrealDB control/runtime state

hosted MCP server
  +-- server-local Rust models and declared schemas
  +-- shared task runtime when operations are durable
  +-- canonical domain administration through MCP tools, resources, tasks, and Apps
  +-- optional declared HTTP projection under its canonical mount
  +-- forwarded internal identity for artifact/recording operations
  +-- no private control database or byte route

artifact-service
  +-- byte policy-enforcement point
  +-- SurrealDB occurrence/grant/share/capability records
  +-- S3-compatible blob storage

recording-hub
  +-- gateway-authenticated protobuf ingest
  +-- fsynced batch journal and monotonic checkpoints
  +-- crash-decodable RRD materialization
  +-- SurrealDB stream, recording, and segment catalog

recording-forwarder
  +-- producer-loopback Rerun gRPC receiver
  +-- persistent bounded queue and replay
  +-- OAuth private-key client and gateway upload
```

Binary entrypoints parse configuration, initialize dependencies, assemble routers, and
hand behavior to focused modules. Shared crates define the platform vocabulary. Domain
tool schemas stay in the server that owns them.

### Human Clients And Computers

Console administers the installation at `/console/`. Workspace, at `/workspace/`, is
the everyday chat client, and each chat's owner decides which people and agents take
part. Both are served by the Rust browser edge, with separate OAuth clients and
encrypted cookies. Rust code handles persistence, authorization, agent execution, and
MCP transport. The [Workspace client design](../apps/workspace/DESIGN.md) covers
presentation.

Everyone in a chat sees each agent run's execution phase and how many operations it
has started. Tool arguments, input requests, and results appear only in the private
Activity panel of the person who started the run. A personal event feed keeps each
user's operation list, invitations, and Task alerts current across page navigation.
After a reconnect, the client reloads state from the server and never resubmits work.
Progress bars show measured progress only when a tool reports it. Otherwise progress
is shown as indeterminate.

Computers is a core capability. Each Computer keeps its identity and files, accepts
browser and stock CLI connections, and can run scoped agent work. The
[domain](../platform/computers/DESIGN.md) holds authority and the durable record of
requested operations. The [worker](../servers/computers-mcp/DESIGN.md) makes provider
calls and exposes operations as Tasks. The gateway does not depend on any provider.
Each installation chooses its capacity and its qualified provider and storage profile.

## Hosted Server Administration

A hosted server owns one domain contract under its catalog identity. MCP is the
primary interface for domain reads, changes, long-running work, and interactive App
views:

```text
MCP client or MCP App host
  -> gateway MCP profile
  -> {server mount}/mcp
  -> typed tools, resources, tasks, and notifications

accepted administrative API client
  -> gateway /admin/{profile}/servers/{server}/{*path}
  -> {server mount}/admin/{path}
  -> additive projection over the same domain models and state
```

Every administrative tool or resource goes through the normal gateway MCP profile.
The gateway applies the server's method and target policy, records the operation, and
forwards the caller's short-lived internal assertion. Long-running administrative work
uses the shared Task API.

An HTTP administration API is optional and must be declared in the owning server's
design. The gateway reads the active catalog revision, checks that the selected
profile includes the server, classifies the method as `AdminRead` or `AdminWrite`, and
applies the same policy and audit path as MCP calls. The proxy passes through the
request body, up to a size limit, along with the headers needed for content type,
idempotency, conditional writes, caching, and retry guidance.

The server validates the internal assertion and the domain's administrative scope.
HTTP handlers reuse the server's MCP request and response models and the same
application state. They add no other identities or storage. Durable domain records
live behind `veoveo-platform-store`, and their ordered schema migrations run during
installation bootstrap.

The Console has explicit BFF routes for installation-wide workflows. Workflows that
belong to one server are MCP Apps, discovered from that server's resources. The
browser never receives the gateway bearer token. Each server's design document lists
its MCP administration, persistence records, authorization scopes, App resources, and
any accepted HTTP API.

An App can message an always-on agent only when its listed resource names that agent
in `io.veoveo/agent-message-targets`. The sandboxed frame sends a UUIDv7 and a short
text message through the host bridge. Console then delivers it through its existing
authenticated human-message route. That route applies CSRF checks, sender attribution,
Work Context policy, audit, idempotency, and wake ordering, and the frame never sees
cookies or gains agent authority.

## Durable Platform Store

SurrealDB `3.2.4` is the only platform coordination store, and the Rust client pins
the matching `3.2.4` release. The supported release runs one RocksDB-backed node.
Installation bootstrap connects with root credentials, applies ordered migrations,
creates or rotates the database runtime user, and publishes the first gateway control
revision. Long-running services connect with database-scoped credentials and never run
migrations.

`veoveo-platform-store` defines the Rust record types and persistence APIs for:

- tenants, principals, groups, server/profile identities, and policies;
- Work Contexts, invocation authority, ownership defaults, and access requests;
- immutable gateway control revisions and the active revision pointer;
- access tokens, refresh families/tokens, authorization state, ID-JAG replay state, and
  JWT revocations;
- tasks, owners, leases, results, retention pins, provider jobs/events, and usage;
- artifact blobs, occurrences, grants, share links, and write capabilities;
- coordinate frames/operations, recording datasets/layers, agents/episodes/wakes;
- audit events and the transactional outbox.

A state change that other processes must see writes its domain record and outbox event
in one transaction. Consumers checkpoint their position in the outbox. SurrealDB LIVE
queries can cut latency, but after a reconnect a consumer always catches up from its
checkpoint, because LIVE ordering and delivery are not guaranteed.

DuckDB is not used for platform coordination. It serves arbitrary analytical SQL and
local agent analysis.

## Durable Task Runtime

`veoveo-task-runtime` knows nothing about MCP. It handles UUIDv7 task creation,
idempotency, leases, claims, progress, input requests, cancellation, terminal results,
retention, recovery, and outbox transitions. Idempotency is scoped by tenant,
principal, profile, server, and operation.

`rmcp` handlers expose official MCP Tasks `2026-07-28` directly: discovery,
task-required tool calls, get, update, cancel, and event-stream subscriptions. Each
handler reads the shared runtime's task snapshots and keeps no task model of its own.

Each durable operation declares one recovery class:

- `Resume`: deterministic work with no side effects. After a lease expires, a new
  worker may pick it up and continue from the persisted request and capability state.
- `WebhookWait`: an external provider job was durably submitted and now waits for its
  signed callback.
- `ProviderWait`: the operation keeps its fence while a qualified provider adapter
  checks the provider for the real outcome. These observation leases cannot become
  ordinary execution claims and never permit replay of a mutation whose effect is
  uncertain.
- `InterruptedIndeterminate`: execution may have changed something. Recovery marks the
  task failed and never repeats the operation.

Every long-running server, including Media, Time, and Computers, uses this runtime.
No server keeps its own in-memory task registry or task URI scheme.

## Provider Completion

This section describes the Media profile as implemented. The broader accepted provider
policy is in [`CONTRACT_EVOLUTION.md`](CONTRACT_EVOLUTION.md#ce-01-provider-completion-follows-qualified-semantics).
Computers has its own provider-observation profile and the `ProviderWait` recovery
class, described in the [runtime design](../platform/runtimes/computers/DESIGN.md).
That profile adds no status API to Media.

The Media server handles its client side and its provider side asynchronously and
separately:

1. A live gateway identity creates a durable task and an artifact write capability
   with a size limit.
2. The provider submission and the provider-job binding commit before the server
   reports that the task has detached.
3. The task enters `WebhookWait`.
4. The provider sends a signed terminal webhook.
5. The server records the event once, redeems the artifact capability issued in step
   1, stores usage, commits the task result, and emits outbox events.

The Media callback handler accepts signed events. Duplicate events have no extra
effect, and after a restart the server replays any recorded events it had not yet
processed. Provider CDN URLs and opaque payloads never reach clients. A webhook that
never arrives is an operational failure. No timeout falls back to querying the
provider's status.

Cancellation works differently in each direction. `tasks/cancel` first commits the
local cancellation, then records a best-effort request to delete the provider job and
whether it was accepted, not deleted, failed, or timed out. A provider's deletion
acknowledgement is not treated as proof that compute stopped or that a refund is due.
If a signed terminal webhook arrives later, it settles the provider job's final state
and triggers billing reconciliation. The cancelled task stays cancelled. Webhook
processing does not fetch provider outputs, redeem the artifact write capability,
create artifacts, or replace the task result. Completion never polls the provider.

## Artifact Plane

Each artifact occurrence gets a fresh opaque UUIDv7 and an `artifact://{id}` URI. The
content hash verifies integrity and allows deduplication within a tenant. Storage keys
include the tenant, so identical content in two tenants is stored separately.

Each occurrence also records its Work Context, producer, the request that created it,
policy revision, output owner, and initial grants. The gateway derives these from the
authenticated identity and the active control-plane revision. Domain services receive
them in the signed internal assertion and cannot substitute ownership or provenance
supplied by the caller.

The artifact service combines:

- hard tenant isolation;
- mandatory data-label clearance;
- user and group grants at the ordered levels `read < write < admin`;
- retention and release state;
- gateway policy on the external route;
- its own authorization check against the forwarded gateway identity.

Domain servers cannot create identities to finish background work. Asynchronous
output uses a capability issued while the live principal was present. The capability
is tied to one task and one purpose, has a size limit and an expiry, and is redeemed
with an idempotency key.

The two sharing modes stay separate:

- Authorized sharing creates a user or group grant. A group's role caps the grant
  level, and a grant can never widen label clearance.
- Public sharing requires the artifact to be `releasable` or `released`, then creates
  a random read-only bearer token. Only its hash is stored. Links expire after at most
  thirty days, optional download limits are counted atomically, and revocation takes
  effect immediately.

A caller who passes tenancy and clearance checks but lacks need-to-know can request
access. Context custodians and owners can see the review queue. Deciding a request
requires artifact `admin` authority, and an approval creates the grant in the same
SurrealDB transaction that closes the request. The Console shows the service's exact
decision, what contributed to it, and the artifact's recorded provenance.

The complete model and guidance for mapping it to an enterprise's roles are in
[`WORK_CONTEXT_GOVERNANCE.md`](WORK_CONTEXT_GOVERNANCE.md).

Authorized browser downloads go through
`/artifacts/{profile}/{artifact_id}/download`. The gateway checks policy, writes an
audit record, issues a short-lived internal assertion, and streams the Artifact
service's response with backpressure. Console downloads use
`/console/api/artifacts/{artifact_id}/download` through the BFF. Full downloads, HEAD,
and single-range requests keep their content headers and never reveal a storage
address. `/s/{token}` is the only route for redeeming a public link. Domain servers
have no artifact byte routes of their own.

Every client-facing path uses the single origin set by `global.publicBaseUrl`. Object
storage has no ingress, public endpoint, DNS name, or presigned client URL. RustFS
accepts cluster traffic only from the Artifact service and bucket initialization.

Because the public path contains a bearer token, edge access, APM, and WAF logs must
suppress `/s/*`. Helm renders `/s` as its own Ingress, with the default ingress-nginx
annotation that turns off access logging. Installations using another controller must
replace it with that controller's equivalent. Application audit events never record
the raw token.

## DuckDB Runtime

DuckDB accepts arbitrary SQL for `query` and `execute`, because a restricted query
builder would remove most of its analytical value. Isolation is applied around the
engine instead:

- database files are chosen by the authenticated owner's identity;
- the server serializes access per owner workspace and uses a persistent singleton
  PVC in Helm;
- configuration and extension loading are locked before any user SQL runs;
- the official DuckDB Spatial extension for the embedded DuckDB version is pinned
  into the image, verified at startup, and loaded before that lock;
- memory, threads, spill, execution time, result rows, and result bytes all have
  limits;
- external sources must come through ingest, artifact resolution, or explicitly
  allowed HTTPS attachment;
- exported bytes are stored as artifacts through a task capability;
- container capabilities, writable paths, process count, and network access are
  restricted.

Spatial geometry, CRS, R-tree, and MVT functions are available as analytical SQL.
DuckDB is not the tile, style, or map-rendering service, even though it can compute
geometries and vector-tile blobs; Map owns those.

Read-only query and export tasks use `Resume`. Mutating execute and ingest tasks use
`InterruptedIndeterminate` once execution may have started, because a mutation cannot
be safely replayed.

## Map And Optimization Decision Plane

Map builds immutable travel models from one map release, one mobility-profile
version, and one restriction snapshot. A travel model fixes the order of locations and
vehicle types, cost and transit-time units, unavailable arcs, and provenance.
Optimization loads that exact Map-owned occurrence through the artifact service.

The Optimization control container accepts typed routing, route-scenario, continuous
LP/QP/QCQP/SOCP, and linear MILP inputs. It validates identifiers and every
cross-reference, combines sparse terms, applies solver profiles with fixed limits, and
writes a prepared problem checked by digest. A single admission queue sends each
prepared problem to the Python executor over a private length-prefixed Unix-socket
protocol.

The executor owns the CUDA context and a fixed-size RMM pool inside the pinned cuOpt
image. It has no cluster Service and no tenant, policy, artifact, or task authority.
It reports ready only when it finds the expected cuOpt version and a visible NVIDIA
GPU.

Solver output returns to Rust, which independently checks route feasibility, bounds,
integrality, constraints, and objective values. The server then publishes immutable
`optimization://problem`, `optimization://run`, and `optimization://solution`
resources along with the verification results. A caller can run `verify_solution`
again with a chosen tolerance without rerunning cuOpt.

## Gateway Identity And Policy

External identity works with any provider. The control-plane configuration is
validated against its schema and describes the OIDC issuer, JWKS, claim mapping,
tenant mapping, authorization endpoints, clients, profiles, scopes, server exposure,
and policy rules. Keycloak is used for real integration tests, and the Bioma example
uses Entra.

At interactive login, the gateway builds a short display label from the verified OIDC
`name`, `preferred_username`, or `email` claim, in that order, and falls back to a
shortened form of the subject. The label travels next to the immutable principal
through the authorization code, signed access token, and rotating refresh grant. It
never replaces the issuer, subject, principal ID, tenant, Work Context membership, or
any policy input.

The gateway supports:

- protected-resource and authorization-server metadata;
- authorization code with PKCE;
- client credentials and client assertions;
- MCP Enterprise-Managed Authorization / ID-JAG;
- signed access tokens bound to a profile and resource;
- durable rotating refresh tokens, limited duplicate delivery, family replay
  detection, revocation, audit, and garbage collection;
- per-method policy checks that take the target into account;
- Ed25519 internal assertions with `kid`, issuer, audience, principal, tenant, labels,
  scopes, and short expiry.

Unknown profiles, servers, methods, resources, task IDs, artifact IDs, issuers, keys,
and policy targets are all rejected. Audit records carry explicit principal attributes
and decision context. They never contain prompts, artifact bytes, provider payloads,
tokens, link bearers, webhook bodies, or signed URLs.

Audit retention deletes old records in batches. For each audit kind, it selects at
most 1,024 record IDs through the `resource_type, occurred_at` index, and each delete
has a two-second database deadline. A full batch schedules another pass one second
later. A partial or empty batch returns to the hourly schedule, and failures retry
after one minute. Retention respects the configured age cutoff and never deletes other
domains' records. Migration 0072 adds the index. Bootstrap builds it online with
SurrealDB's `CONCURRENTLY` index builder before the migration commits
(`platform/store/src/migration_preparation.rs`). Apply the migration before rolling
out the new cleanup worker. Older workers keep working during the upgrade, and a
rollback leaves the index in place.

The gateway gives each server's Apps and opaque upstream resource schemes their own
namespace. When a server's typed outputs refer to resources owned by another
registered server, its manifest declares those schemes in
`referenced_resource_schemes`. The gateway passes those URIs through unchanged and
rejects declarations for schemes that no server in the same control plane owns.

Refresh-token rotation is a durable compare-and-swap. The winning request stores an
XChaCha20-Poly1305 envelope holding the successor token, only for the configured short
delivery window. The envelope's AAD binds the authorization server, profile, OAuth
client, token family, and generation. A concurrent request that presents the token
just consumed receives that same successor, and the gateway records an audit event
with reason code `refresh_token_duplicate_delivery`. After the window, reusing the
token counts as replay and revokes the whole family. The delivery key is a separate
base64-encoded 32-byte installation secret. Plaintext tokens and delivery envelopes
never appear in logs, audit payloads, outbox events, or Console snapshots. Envelopes
stop being deliverable at their deadline. Consuming the successor clears its envelope
in the same transaction, and a dedicated one-minute garbage-collection pass removes
any expired ciphertext.

The gateway's runtime and admin modules share one policy and audit path. Cancelling a
task from the Console calls the owning server's official Tasks endpoint and never
edits task rows. Artifact release, grant, and link changes call the artifact service.
The Console snapshot is a filtered copy and never includes token hashes or reusable
link URLs.

## Console Browser Boundary

`console-bff` is the only place that holds browser sessions. It performs the gateway
OAuth login with PKCE and keeps access and rotating refresh tokens in an
XChaCha20-Poly1305 encrypted, HttpOnly, SameSite cookie. A separate encrypted cookie
carries authorization state during login. Unsafe requests must present a CSRF token
that matches in constant time.

Upload part requests use the access token from the session cookie without rotating the
refresh token, so an upload request delayed by the ingress cannot replay an old
refresh token or overwrite a newer cookie. Short upload status requests do refresh the
session, and the Console calls that path before retrying a failed part. Saved uploads
survive a fresh sign-in by the same user in the same Work Context.

The Console event stream sends `artifact_upload` notifications only to the uploading
user in the matching tenant and Work Context. Each notification carries an upload ID
and state, and the browser fetches receipts from the status endpoint, which checks
authorization again. The endpoint `GET /admin/{profile}/console/artifacts/{artifact_id}`
lets a completed upload in the queue open its artifact even when that artifact is
outside the snapshot's current catalog page.

The OAuth protected-resource URL stays public even when the BFF reaches the gateway
over a cluster-private URL. The Apps MCP client matches both URLs by their exact
`/mcp/<profile>` path and sends the public host as the `Host` header on the internal
connection. Installation CA roots are added to the platform trust store for every
outbound BFF client, and the BFF refuses to start if that trust material is unreadable
or invalid.

The React app receives installation data and one-time share URLs, never a gateway
bearer token. The BFF sets CSP, frame denial, MIME-sniffing prevention, a same-origin
referrer policy, and `no-store` on API responses. The installation snapshot is how the
browser learns it is signed in, and catalog and live-stream requests start only after
the snapshot loads. Every unauthorized response goes through one shared login
transition that does not retry, so parallel API failures cannot start competing OAuth
flows.

The snapshot carries a trusted display name for every principal it includes, and
metadata from the authenticated identity takes precedence over stored data. The
Console shows these names in the top bar and in the access, agents, and artifact
views, keeps the principal ID in tooltips for troubleshooting, and shows a shortened
identifier for any principal without a name rather than making one up.

Recording playback also goes through the BFF. The BFF offers authorized same-origin
routes for the playback manifest and recent live data, and the gateway checks
`recording://` resource policy and audits every access. The manifest grants one
read-only Redap session limited to that recording and host. The browser then talks
directly to the same-origin read-only Rerun service and fetches only the
footer-indexed chunks the current view needs. It loads the Rerun viewer version that
matches the RRD producer on demand. Artifact previews use a separate inline route with
a size limit on text reads.

## Recordings And Agents

Recording Hub stores data that producers push to it. Producers send native Rerun log
messages to a forwarder on the loopback interface. The forwarder obtains an OAuth
client-credentials token and uploads sequenced protobuf batches, each with a size
limit, through the gateway. It starts a new batch at each H.264 video sample, and Hub
allows a rollover only where the first access unit contains SPS, PPS, and IDR. Public,
local-network, and Kubernetes producers all use this same resource and protocol.

Hub validates each complete Rerun payload and fsyncs it into a deterministic journal.
It advances its SurrealDB checkpoint only after the journal rename is durable. One
ordered materializer compacts input windows of one hour or 192 MiB with Rerun's
object-store profile, starts video shards at a batch a decoder can begin from, writes
the footer manifest, and publishes immutable RRD archive shards under the stream's
authenticated tenant, owner, dataset, classification, and labels. Raw Rerun ingest,
internal parts, and filesystem paths are never exposed for ingress or reads.

`recording-mcp` applies tenant and label authorization to discovery, queries,
subscriptions, artifact publication, playback sessions, and following recent live
data. It presents immutable shards as layers of one Redap dataset segment per
recording and maps the live tail to that same identity. The Console shows both in one
Rerun timeline without downloading every shard or building another RRD. SUMO uses the
same path: one process owns the TraCI connection, publishes Rerun world frames, and
exposes traffic controls, resources, and tasks.

The agent kernel runs episodes with fixed limits and persists scheduling through
`veoveo-agent-runtime`. Tool tasks detach when an episode ends. Task descriptors,
watcher leases, retry schedules, retention pins, results, and wakes all survive a
process restart. The gateway route keeps the upstream Task ID opaque, as the protocol
requires, and for Tasks from the shared first-party runtime it also keeps a direct
record reference. The episode that consumes a wake verifies it, releases the Task's
retention pin, marks the delivery consumed, acknowledges the wakes, and writes the
outbox receipt, all in one SurrealDB transaction. Outbox and changefeed events wake
the next episode. DuckDB and RRD recordings serve as the agent's analytical memory.
Chat history is not the source of truth.

A periodic scheduler heartbeat shows that the wake path is working. A batch that
contains only a heartbeat is acknowledged under the agent's lease without starting an
LLM episode. Operator messages, task results, resource changes, answered input
requests, and explicit timers still trigger wakes. If one arrives together with a
heartbeat, the batch runs a normal episode.

Agent manifests separate the gateway's public origin from the address the agent uses
to reach it. OAuth audience and protected-resource identity use the public origin,
while an agent inside the cluster may connect through a private service address. Both
must be bare HTTP(S) origins, and the kernel keeps the public host in requests sent
over the private route. Every manifest string supports `${VAR}` substitution at
deployment, which fails if a variable is missing and runs before typed decoding and
validation. One reviewed manifest can therefore start several agents with separate
identities, without generating a copy per installation. A manifest may also list a
limited set of absolute MCP resource URIs that wake the agent when they change.

When its token rotates, the kernel connects a new session and restores every
subscription before switching to it. If a subscription fails, the previous
authenticated session stays active. Every MCP request runs the same serialized
freshness check before it is sent, so concurrent callers cannot switch sessions
against each other. The current session also provides one resource-read tool. It
returns text and JSON within per-episode limits on reads, resource families, bytes,
wall time, and pagination, and returns only fixed correction fields when the input is
invalid. Protocol, authorization, transport, and storage details never enter the
model's context.

Signed-in users and services control agents through the gateway, and the Console BFF
carries the browser path. The agent pod is never reachable from outside. Every caller
must pass the selected profile's action policy. An operator message is committed as an
idempotent wake keyed by UUIDv7, scoped to the caller's tenant, Work Context, and the
agent's public key within that tenant, so it can arrive while an episode or detached
task is still running. The agent's own profile governs its MCP tool session, and the
caller's administrative profile never replaces it. Console snapshots and change
events identify an agent by its tenant-scoped `agent_key`, the same identifier every
control route accepts. Internal SurrealDB record keys never serve as public
identifiers. The snapshot also carries the current runner-lease deadline, and the
Console shows an agent as offline once that deadline passes or if there is no lease,
without polling or changing episode state.

Answers to input requests use the same policy path and durable record. The kernel
opens a SurrealDB LIVE query before re-reading that record, which removes the race
between subscribing and reading without polling. An answer that arrives after the
episode stops waiting becomes a new wake instead of being lost. Messages and answers
are untrusted input attributed to their sender. Domain policy still decides whether a
proposed change can take effect.

## Deployment

Helm defines the Kubernetes service graph, and k3d runs that chart locally with
loopback ingress and profile-owned values.

Helm uses separate Secrets for bootstrap and runtime database access, emits
default-deny network policy, supports an existing object store and SIEM credentials,
can require strict Istio mTLS, gives the singleton DuckDB server persistent RWO
storage, and keeps recording ingest inside the cluster. SurrealDB HA is not claimed.

The offline builder resolves digest-pinned external images, builds Veoveo images with
exact tags, exports configuration schemas, records image identities, emits SPDX SBOMs
and checksums, and packages the Helm configuration. The loader verifies every file
before import and every image reference after import, keeps the verification results,
and makes no network calls.

## Hardware-Backed GPU And Visual Execution

GPU workloads request the Kubernetes resources they need and fail readiness when the
accelerator or a matching runtime is missing. Optimization also checks the pinned
cuOpt version and a hardware GPU when its sidecar starts, and it has no CPU solver.
Browser visual acceptance runs in a headed browser and probes both WebGPU and WebGL
when available. At least one high-performance path must be hardware-backed.
SwiftShader, llvmpipe, software adapters, and software-rasterizer warnings fail the
visual workflow.

The Stream App queries Media Capabilities for the exact H.264 codec, dimensions,
bitrate, and frame rate. A configuration that is supported and smooth may use the
browser's software decoder when `powerEfficient` is false, and the App labels it as
software H.264 decode. The App claims hardware decode only when `powerEfficient` is
true. Browser graphics and all server-side GPU work are hardware-backed either way.

## Verification

Smoke orchestration is written in Rust. The harness manages child processes and
containers, readiness, timeouts, cleanup, MCP and HTTP calls, assertions, and
evidence. `cargo xtask smoke` only builds the harness and the local binaries a
scenario needs, then runs the typed scenario.

New behavior checks may use maintained Console, browser, or SDK-language harnesses
that follow the shared lifecycle, diagnostics, and evidence rules in
[`CONTRACT_EVOLUTION.md`](CONTRACT_EVOLUTION.md#ce-05-verification-uses-the-owning-ecosystem).
Headless results cannot replace headed hardware visual acceptance.

Coverage includes:

- real SurrealDB 3.2 migration and runtime credentials, and durability across
  processes;
- gateway OAuth, Keycloak login, refresh rotation and replay, internal assertions,
  policy, admin operations, audit, and task and artifact routing;
- webhook-only media completion across a process restart, with event replay;
- task recovery classes, deterministic resume output, capability redemption, and
  quotas;
- arbitrary DuckDB SQL and classification of interrupted queries;
- recording crash recovery, rollover, catalog rebuild, and SUMO push readback;
- k3d with GPUs, headed hardware browser runs, Helm and schema checks, the offline
  manifest and loader, and the Console build.

The full behavior matrix lives in `testing/smoke` and the focused crate tests. These
checks, not this document, are the evidence that the behavior works.
