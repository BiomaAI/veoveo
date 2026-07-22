# Veoveo Technical Design

This document explains how the self-hosted Veoveo components implement the
boundaries in `ARCHITECTURE_DECISIONS.md`. The architecture is protocol-first.
Known requests, responses, identities, and persisted records use explicit Rust types
or declared schemas. Durable state and service ownership remain inside the installation.

## Standards And Protocols

This table is the cross-component protocol contract. Domain design documents narrow
the data standards they implement; the root README provides the shorter product-level
catalog.

| Standard or protocol | Technical boundary |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP at client-to-gateway and gateway-to-server boundaries. Catalog projection preserves canonical resources, prompts, completions, subscriptions, notifications, and structured content. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Closed, dereferenced MCP tool input schemas generated from Rust or Python types. Controlled persisted and structured-result models use the same typed vocabulary. |
| [Veoveo final task extension](../mcp/task-extension) | Version `2026-06-30`; durable task augmentation, discovery, lifecycle methods, results, cancellation, and subscriptions use MCP messages rather than a job REST API. |
| [MCP Apps SEP-1865](../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; server-owned `ui://` resources use the sandboxed MCP Apps host bridge. |
| OpenID Connect and OAuth 2.0 | OIDC Core login; S256 PKCE; Client Credentials and JWT Bearer grants; RFC 8414 authorization-server metadata; RFC 9728 protected-resource metadata; RFC 8707 resource indicators; signed JWT/JWS/JWK tokens and key discovery. |
| MCP Enterprise-Managed Authorization / ID-JAG | Explicit enterprise grant profile with durable replay protection, client binding, tenant mapping, and scope reduction. |
| HTTPS and HTTP range semantics | External acquisition, MCP transport, provider webhooks, artifact delivery, and immutable RRD byte ranges. Internal cleartext HTTP exists only inside declared cluster trust boundaries. |
| OpenTelemetry OTLP/HTTP | Optional traces and logs from shared server instrumentation. Export remains disabled unless the installation supplies an endpoint. |
| Veoveo recording ingest | Version `2026-07-21`; authenticated protobuf batches preserve native Rerun messages, ordering, idempotency, and decoder-safe rollover markers. |
| Rerun gRPC, RRD, and `VideoStream` | Producer-local log ingestion, immutable time-and-space records, viewer playback, and H.264 Annex B video with exact timeline indices. |
| S3-compatible object API | Artifact bytes and presigned delivery. SurrealDB remains authoritative for occurrences, identity, grants, release state, shares, policy, and audit. |
| Kubernetes, Helm, and OCI images | Canonical workload graph, declarative installation configuration, registry-first delivery, GitOps reconciliation, and offline bundle material. |
| Domain standards | Map, Time, Frames, View, UAV, Recording, Perception, and Reason designs pin their geospatial, temporal, 3D, vehicle, and media profiles independently. |

## Capability Model

Veoveo does not flatten MCP into a collection of convenience tools. Each hosted
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

The gateway discovers these surfaces from upstream servers and projects them into a
profile. It prefixes tool names only at the aggregation boundary, for example local
`run` becomes `media__run`. Resource URIs keep their owning scheme.

### Tool input schemas

Tool inputs publish one canonical JSON Schema 2020-12 document generated from the
request type. The document has an object root, contains no references, and declares
the immediate JSON type of every property. Object-shaped unions expose `type: object`
alongside their variants. This profile preserves the full typed contract while making
the argument shape visible to clients that inspect a property without resolving schema
references.

Rust servers import `tool` from `veoveo_mcp_contract`. The macro selects the shared
Schemars generator for every `Parameters<T>` handler and supplies the closed empty-object
schema for handlers without arguments. Tagged Rust enums declare their object or string
type on the domain type itself. Python servers pass each Pydantic request model through
`veoveo_mcp.schema.mcp_input_schema` before publishing it.

Recursive tool arguments are outside this profile because a finite self-contained
schema cannot express unbounded recursion without references. Domain contracts model
bounded collections explicitly. Servers deserialize the structured value described by
the schema; the gateway does not rewrite schemas or convert JSON-encoded strings.

The MCP conformance client's `info` command validates every advertised tool schema
against its declared dialect and enforces this client-facing shape.

Full-MCP clients can use the final task extension directly through a gateway profile.
The gateway routes task-augmented tool calls, get, update, cancel, and subscriptions to
the owning server without changing the canonical task identity. It applies profile
exposure, ownership, policy, audit, and resource-URI projection at that boundary. The
standard MCP task surface is an additive projection over the same upstream extension
for clients that negotiate it.

Some registered clients are explicitly `tools_compat`. Their narrow projections are
implemented over the same upstream operation contract, task ID, policy decision, audit
path, subscription, artifact identity, and result. Task projection for these clients
requires the explicit direct-adapter flag. Compatibility behavior remains additive and
does not create a second protocol or source of truth. Full-MCP clients never receive
compatibility helper clutter.

The gateway advertises list-change support whenever an exposed upstream can emit
notifications, then forwards the upstream notification to the connected client. A new
authenticated session always receives the current policy-filtered catalog.

Client hosts may retain their own per-user tool permissions after OAuth grants change.
Those permissions are outside gateway authority: reconnecting authentication refreshes
identity and scopes, but it does not necessarily reset the host's selected tools. After
an installation expands a profile, operators refresh the connector's tool permissions
or reinstall the connector when its host does not ingest the changed catalog. A new
conversation then loads the updated selection. The gateway continues to enforce every
tool call independently of the host's selection.

## Component Boundaries

```text
edge
  +-- console-bff -> gateway admin and artifact download routes
  +-- mcp-gateway -> hosted MCP servers
  +-- artifact-service -> public share redemption only
  +-- media-mcp -> signed provider webhooks and curated provider input files

mcp-gateway
  +-- external OAuth/OIDC and gateway authorization server
  +-- profile catalog, policy, protocol projection, audit
  +-- short-lived internal identity issuer
  +-- SurrealDB control/runtime state

hosted MCP server
  +-- server-local Rust models and declared schemas
  +-- shared task runtime when operations are durable
  +-- optional contract-defined HTTP administration under its canonical mount
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
delegate behavior to focused modules. Shared crates own platform vocabulary; domain
tool schemas stay in the server that owns them.

## Hosted Server Administration

A hosted server can expose an agent protocol surface and an administrative HTTP
surface under the same catalog identity:

```text
MCP client
  -> gateway MCP profile
  -> {server mount}/mcp

administrative API client
  -> gateway /admin/{profile}/servers/{server}/{*path}
  -> {server mount}/admin/{path}

browser
  -> console-bff /console/api/{domain}/{path}
  -> gateway admin route
  -> {server mount}/admin/{path}
```

The gateway reads the active catalog revision and requires the selected profile to
contain the requested server. It classifies safe methods as `AdminRead` and mutation
methods as `AdminWrite`, evaluates profile policy, records the operation, and issues a
short-lived internal assertion for the owning server. The proxy preserves the bounded
request body and the HTTP headers needed for structured content, idempotency, conditional
writes, caching, and retry guidance.

The owning server validates the internal assertion and the domain's administrative
scope. Its handlers use server-local request and response models and the same application
state as its MCP implementation. Durable domain records live behind `veoveo-platform-store`;
their ordered schema migrations remain part of installation bootstrap.

The Console publishes explicit BFF routes for the administrative workflows represented
in its React application. The browser receives same-origin responses and never receives
the gateway bearer. Server design documents specify their endpoint catalogs, request and
response models, persistence records, authorization scopes, and Console projections.

## Durable Platform Store

SurrealDB `3.2.1` is the only platform coordination store. The canonical release uses
one RocksDB-backed node. Installation bootstrap connects at root scope, applies ordered
migrations, creates or rotates the database runtime user, and publishes the initial
gateway control revision. Long-running services connect at database scope and never run
migrations themselves.

`veoveo-platform-store` owns Rust record types and persistence APIs for:

- tenants, principals, groups, server/profile identities, and policies;
- Work Contexts, invocation authority, ownership defaults, and access requests;
- immutable gateway control revisions and the active revision pointer;
- access tokens, refresh families/tokens, authorization state, ID-JAG replay state, and
  JWT revocations;
- tasks, owners, leases, results, retention pins, provider jobs/events, and usage;
- artifact blobs, occurrences, grants, share links, and write capabilities;
- coordinate frames/operations, recordings/segments, agents/episodes/wakes;
- audit events and the transactional outbox.

Cross-process state changes write their domain record and outbox event in one
transaction. Consumers checkpoint an outbox cursor. SurrealDB LIVE delivery may reduce
latency, but reconnect always reconciles from the durable cursor because LIVE ordering
and delivery are not the authority.

DuckDB is not used for platform coordination. It remains the domain runtime for
arbitrary analytical SQL and local agent analysis.

## Durable Task Runtime

`veoveo-task-runtime` is protocol-neutral. It owns UUIDv7 task creation, idempotency,
leases, claims, progress, input requests, cancellation, terminal results, retention,
recovery, and outbox transitions. Tenant, principal, profile, server, and operation are
part of idempotency scope.

`veoveo-mcp-task-extension` implements the final `2026-06-30` extension wire contract:
discovery, task-required tool invocation, get, update, cancel, list, and SSE task
subscriptions. It projects the shared runtime's task snapshots; it does not persist a
parallel task model. Traits use native Rust return-position `impl Future`; the workspace
does not require `async-trait` for controlled async contracts.

Each durable operation declares one recovery class:

- `Resume`: deterministic and side-effect-safe. A new worker may reclaim an expired
  lease and continue from persisted request/capability state.
- `WebhookWait`: an external provider job was durably submitted and now waits for its
  signed callback.
- `InterruptedIndeterminate`: execution may have caused a mutation. Recovery marks the
  task failed and never repeats the operation.

Long-running servers use this same runtime: media, timeseries, optimization, frames, map,
Time, DuckDB, and SUMO. There is no server-local in-memory task registry and no alternate
task URI.

## Provider Completion

The media server keeps client/server async and provider/server async separate:

1. A live gateway identity creates a durable task and bounded artifact write capability.
2. Provider submission and the provider-job binding commit before the server reports
   successful detachment.
3. The task enters `WebhookWait`.
4. The provider sends a signed terminal webhook.
5. The server durably records the unique event, redeems the preissued artifact
   capability, stores usage, commits the terminal task result, and emits outbox events.

Any replica can receive a callback. Duplicate signed events are idempotent. Provider CDN
URLs and opaque payloads are not returned to clients. Missing webhook delivery is an
operational failure; no timeout path queries provider status.

Cancellation is intentionally asymmetric. `tasks/cancel` commits the local cancellation
first, then durably records a best-effort provider deletion request and its accepted,
not-deleted, failed, or timed-out outcome. Provider deletion acknowledgement is not treated
as proof of compute stoppage or a refund. If a signed terminal webhook arrives later, it is
authoritative for the provider-job state and triggers actual billing reconciliation. The
cancelled task remains terminal: webhook processing does not fetch provider outputs, redeem
the artifact write capability, create artifacts, or replace the task result. Completion
still has no provider-status polling path.

## Artifact Plane

An artifact occurrence has a fresh opaque UUIDv7 and canonical `artifact://{id}` URI.
The content hash verifies integrity and enables tenant-local deduplication. Storage keys
include tenant identity, so equal content across tenants never aliases.

Every occurrence also retains its Work Context, producer, invocation provenance,
policy revision, output owner, and initial grants. The gateway derives that authority
from authenticated identity and the active control-plane revision. Domain services
receive it in the signed internal assertion and cannot replace it with caller-supplied
ownership or provenance.

The artifact service composes:

- hard tenant isolation;
- mandatory data-label clearance;
- user/group discretionary grants with ordered `read < write < admin` levels;
- retention and release state;
- gateway policy at the external route;
- service-side authorization using the forwarded gateway identity.

Domain servers cannot mint background identities. Async output uses a capability issued
while the live principal was present. The capability is task-bound, size-bounded,
expiring, single-purpose, and redeemed with an idempotency key.

Sharing modes are intentionally separate:

- Authorized sharing creates a user or group grant. Group role caps grant level, and
  label clearance can never be widened by a grant.
- Public sharing first requires `releasable` or `released`, then creates a read-only
  random bearer. Only its hash is stored. Expiry is at most thirty days, optional
  download limits are atomic, and revocation is immediate.

A caller who satisfies tenancy and clearance can request discretionary access when
need-to-know is the remaining denial. Context custodians and owners can inspect the
review queue. Artifact `admin` authority is required for a decision, and approval
creates the direct grant in the same SurrealDB transaction that closes the request.
The Console projects the exact service decision, its contributing sources, and the
artifact's retained provenance.

The complete model and enterprise mapping guidance are in
[`WORK_CONTEXT_GOVERNANCE.md`](WORK_CONTEXT_GOVERNANCE.md).

Authorized browser downloads enter through
`/artifacts/{profile}/{artifact_id}/download`. The gateway evaluates policy, records
audit evidence, issues a short-lived internal assertion, and proxies the service's
sixty-second object-store redirect. Public bearer redemption is the only `/s/{token}`
route. Domain-specific artifact byte paths do not exist.

Because the public path contains a bearer, edge access/APM/WAF logs must suppress
`/s/*`. Helm renders `/s` as a dedicated Ingress whose default ingress-nginx
annotation disables access logging; installations using another controller must
replace it with that controller's equivalent. Application audit events never
record the raw token.

## DuckDB Runtime

DuckDB accepts arbitrary SQL for `query` and `execute`; a restricted query builder would
remove the intended analytical value. Isolation is applied around the engine:

- database files are derived from authenticated owner identity;
- the server has one serialized owner/workspace boundary and a persistent singleton PVC
  in Helm;
- configuration and extension loading are locked before user SQL;
- the official DuckDB Spatial extension matching the embedded DuckDB version is
  pinned into the image, verified at startup, and loaded before that lock;
- memory, threads, spill, execution time, result rows, and result bytes are bounded;
- external sources require governed ingest, artifact resolution, or explicitly allowed
  HTTPS attachment;
- export bytes enter the shared artifact plane through a task capability;
- container capabilities, writable paths, process count, and network reach are limited.

Spatial geometry, CRS, R-tree, and MVT functions remain analytical SQL. DuckDB does
not become the tile, style, or map-rendering service merely because it can compute
geometries and vector-tile blobs.

Read-only query and export tasks use `Resume`. Mutating execute and ingest tasks use
`InterruptedIndeterminate` once execution may have started. This preserves flexibility
without pretending mutations can be replayed safely.

## Gateway Identity And Policy

External identity is provider-independent. The control-plane configuration is validated
against its schema. It describes the OIDC issuer, JWKS, claim mapping, tenant mapping,
authorization endpoints, clients, profiles, scopes, server exposure, and policy rules.
Keycloak is used for real integration tests; Entra is shown in the Bioma example.

The gateway supports:

- protected-resource and authorization-server metadata;
- authorization code with PKCE;
- client credentials and client assertions;
- MCP Enterprise-Managed Authorization / ID-JAG;
- profile/resource-bound signed access tokens;
- durable rotating refresh tokens, bounded duplicate delivery, family replay detection,
  revocation, audit, and GC;
- per-method and target-aware policy checks;
- Ed25519 internal assertions with `kid`, issuer, audience, principal, tenant, labels,
  scopes, and short expiry.

Unknown profiles, servers, methods, resources, task IDs, artifact IDs, issuers, keys, or
policy targets fail closed. Audit records carry explicit principal attributes and decision
context but exclude prompts, artifact bytes, provider payloads, tokens, link bearers,
webhook bodies, and signed URLs.

Refresh rotation is a durable compare-and-swap. The winner stores only an
XChaCha20-Poly1305 successor envelope for the configured short delivery window. Its AAD
binds the authorization server, profile, OAuth client, token family, and generation. A
concurrent request using the just-consumed token receives that exact successor and an audit
event with reason code `refresh_token_duplicate_delivery`. Once the window expires, reuse is
delayed replay and revokes the whole family. The delivery key is a separate base64-encoded 32-byte
installation secret; plaintext tokens and delivery envelopes never enter logs, audit
payloads, outbox events, or console snapshots. The envelope becomes ineligible for
delivery at the configured deadline. Consuming that successor clears its envelope in
the same transaction; otherwise a dedicated one-minute GC removes expired ciphertext.

Gateway runtime and admin modules use the same policy/audit path. Console task
cancellation calls the owning server's final task extension; it never edits task rows.
Artifact release/grant/link mutations call the artifact service; the console snapshot is
only a safe projection and never includes token hashes or reusable link URLs.

## Console Browser Boundary

`console-bff` is the only browser session boundary. It performs gateway OAuth login with
PKCE, stores access and rotating refresh tokens in an XChaCha20-Poly1305 encrypted,
HttpOnly, SameSite cookie, and uses a separate encrypted authorization cookie during
login. Unsafe requests require a constant-time CSRF token match.

The React application receives installation projections and one-time share URLs, never a
gateway bearer. CSP, frame denial, MIME sniff prevention, same-origin referrer policy,
and no-store API responses are applied by the BFF.

Recording playback remains inside this boundary. The BFF exposes authorized same-origin
manifest and segment routes, including byte ranges, while the gateway evaluates the
canonical `recording://` resource policy and audits every access. The browser lazily loads
the Rerun viewer version that matches the RRD producer. Artifact previews use a separate
inline route and keep text reads bounded.

## Recordings And Agents

The Recording Hub is a push-based durability service. Producers send native Rerun log
messages to a loopback forwarder. The forwarder obtains an OAuth client-credentials token
and uploads bounded, sequenced protobuf batches through the gateway. It begins a batch at
each H.264 IDR, which gives storage a decoder-reentrant rollover boundary without changing
the producer's logical recording. Public, local-network, and Kubernetes traffic use this
same resource and protocol.

The hub validates each complete Rerun payload, fsyncs it into a deterministic journal,
and advances its SurrealDB checkpoint only after the journal rename is durable. One
ordered materializer compacts hour-or-192-MiB input windows with Rerun's object-store
profile, aligns video rollover to a keyframe-bearing batch, writes the footer manifest,
and publishes immutable RRD archive shards under the stream's authenticated tenant,
owner, dataset, classification, and labels. Raw Rerun ingest, durable parts, and
filesystem paths are not installation ingress or read surfaces.

`recording-mcp` applies tenant/label authorization to discovery, query, subscription,
artifact publication, range-capable archive reads, and bounded-history live following.
Console presents ordered archive sources and the current live tail in one persistent
Rerun timeline without constructing another RRD. SUMO uses the same path: one serialized TraCI owner publishes Rerun world
frames and exposes traffic controls, resources, and tasks.

The agent kernel runs bounded episodes and persists scheduling through
`veoveo-agent-runtime`. Tool tasks detach at episode end; durable descriptors, watcher
leases, retry schedules, retention pins, results, and wakes survive process restart.
Outbox/changefeed events wake the next episode. DuckDB and RRD are analytical memory
planes; chat history is not the source of truth.

## Deployment

Helm defines the canonical Kubernetes service graph. k3d runs that chart locally
with loopback ingress and profile-owned values.

Helm separates bootstrap and runtime database Secrets, emits default-deny network policy,
supports an existing object store and SIEM credentials, can require strict Istio mTLS,
uses persistent RWO storage for the singleton DuckDB server, and keeps recording ingest
internal. SurrealDB HA is not claimed.

The offline builder resolves digest-pinned external images, builds exact-tag Veoveo
images, exports configuration schemas, records image identities, emits SPDX SBOMs and
checksums, and packages Helm configuration. The loader verifies all files before
import, verifies image references after import, retains evidence, and performs no network
operation.

## Verification

All smoke orchestration is Rust. The harness owns child/container lifecycle, readiness,
timeouts, cleanup, MCP and HTTP calls, and assertions. The Justfile only dispatches it.

Coverage includes:

- real SurrealDB 3.2 migration/runtime credentials and multi-process durability;
- gateway OAuth, Keycloak login, refresh rotation/replay, internal assertions, policy,
  admin operations, audit, task and artifact projection;
- webhook-only media completion across process restart and replica boundaries;
- task recovery classes, deterministic resume output, capability redemption, quotas;
- arbitrary DuckDB SQL and interruption classification;
- recording crash recovery, rollover, catalog rebuild, and SUMO push readback;
- k3d/GPU, Helm/schema, offline manifest/loader, and console build contracts.

The complete behavior matrix is executable from `testing/smoke` and the focused crate
tests; documentation is not used as evidence in place of those checks.
