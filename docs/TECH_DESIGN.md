# Veoveo Technical Design

This document explains how the self-hosted Veoveo components implement the
boundaries in `ARCHITECTURE_DECISIONS.md`. The architecture is protocol-first.
Known requests, responses, identities, and persisted records use explicit Rust types
or declared schemas. Durable state and service ownership remain inside the installation.

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

Full-MCP clients receive the standard task surface whenever a profile exposes a
task-capable server. The gateway currently projects the hosted server's final task
extension onto that surface without changing task identity. The shared runtime still
owns policy checks and retention. Audit follows the existing task path, and completion
remains webhook-only. This projection is the migration boundary until hosted servers
implement the standard task handlers directly.

Some registered clients are explicitly `tools_compat`. Their narrow projections are
implemented over the same upstream operation contract, task ID, policy decision, audit
path, subscription, artifact identity, and result. Task projection for these clients
requires the explicit direct-adapter flag. Compatibility behavior remains additive and
does not create a second protocol or source of truth. Full-MCP clients never receive
compatibility helper clutter.

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
  +-- private Rerun gRPC ingest
  +-- crash-decodable RRD segments
  +-- SurrealDB recording/segment catalog
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

SurrealDB `3.2.0` is the only platform coordination store. The canonical release uses
one RocksDB-backed node. Installation bootstrap connects at root scope, applies ordered
migrations, creates or rotates the database runtime user, and publishes the initial
gateway control revision. Long-running services connect at database scope and never run
migrations themselves.

`veoveo-platform-store` owns Rust record types and persistence APIs for:

- tenants, principals, groups, server/profile identities, and policies;
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

## Recordings And Agents

The Recording Hub is a push-based durability service. Producers stream Rerun log
messages into a private gRPC proxy. The spooler routes by application prefix, fsyncs open
segments, keeps crash-decodable siblings, verifies frozen segments before replacement,
and writes governed recording/segment catalog records. Raw ingest and files are not
public ingress surfaces.

`recording-mcp` applies tenant/label authorization to discovery, query, subscription,
and artifact publication. SUMO uses the same path: one serialized TraCI owner publishes
Rerun world frames and exposes traffic controls, resources, and tasks.

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
