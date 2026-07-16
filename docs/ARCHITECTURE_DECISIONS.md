# Veoveo Architecture Decisions

This document records the product and architecture boundaries that all Veoveo
implementations must preserve. It is normative. Detailed component designs may
change, but a change to one of these decisions requires an explicit replacement
decision rather than an implicit compatibility path.

## Product boundary

Veoveo is installed and operated by its owner. Each installation is autonomous:

- there is no Veoveo-operated control plane, identity service, artifact index,
  telemetry sink, license service, or mandatory public hostname;
- the installation owner selects its own hostname, ingress, identity provider,
  object store, secret manager, and observability destinations;
- `veoveo.bioma.ai` is one Bioma installation and may appear only in a clearly
  labeled deployment example;
- connected and offline installations expose the same product capabilities.

Kubernetes is the supported installation form and Helm is its package contract.
k3d runs that same chart for local development.

## Tenancy

One installation represents one enterprise boundary and may contain multiple
internal tenants. A tenant is a hard data and authorization partition inside
that installation, not a customer account in a vendor service.

Tenant and principal identities resolve through one canonical platform identity
mapping. Tasks, artifacts, frames, recordings, agents, grants, audit events, and
outbox events must use those same canonical record identities. A subsystem must not
invent a parallel tenant or principal namespace.

## Durable platform store

SurrealDB `3.2.1` is the required durable platform store. The canonical topology
is one SurrealDB node using RocksDB storage. Application services may scale
horizontally; database high availability is not claimed by this release.

SurrealDB owns durable identity, control-plane revisions, policies, tasks,
provider jobs and webhook events, artifact metadata and grants, coordinate
registries, recordings and segments, agents and wakes, audit evidence, and the
transactional outbox.

The transactional outbox and replayable changefeed are the source of truth for
cross-process work. SurrealDB LIVE queries are a latency optimization because
their delivery order is best effort; a consumer resumes from its durable outbox
checkpoint after a disconnect or restart.

Schema migration uses installation-admin credentials. Runtime workloads use a
database-level runtime user and do not run migrations on connection.

## DuckDB execution

DuckDB remains an arbitrary-SQL analytical capability. Veoveo does not replace
SQL with a fixed query builder or a read-only subset.

Each request executes in a bounded sandbox with locked configuration and
extensions, memory/thread/spill limits, response row and byte limits, governed
external-source attachment, and container defense in depth. External data enters
through governed ingest, artifact, or explicitly authorized HTTPS attachment
paths. A mutating query interrupted after execution begins fails as
`interrupted_indeterminate` and is never replayed automatically.

DuckDB is not the durable multi-process platform database.

## Task execution and provider completion

Long-running work uses the shared durable task runtime. Task IDs are UUIDv7 and
state transitions, leases, cancellation, results, and outbox events are atomic.
Idempotency keys are scoped by tenant, principal, profile, server, and operation.

Recovery is explicit per task:

- `resume`: deterministic, side-effect-safe work may be reclaimed after its
  lease expires;
- `webhook_wait`: a durably submitted provider job waits for its signed webhook;
- `interrupted_indeterminate`: interrupted mutating work fails and is not run
  again automatically.

Provider job completion is webhook-only. Missing webhook delivery is an
operational failure. Veoveo does not poll provider status, add polling fallback,
or query a provider during timeout recovery.

## Artifacts and sharing

Every artifact occurrence has a fresh opaque UUIDv7 identity and canonical
`artifact://{id}` URI. Content hashes provide integrity and tenant-local blob
deduplication; they are not public addresses. Equal bytes in different tenants
never share a storage key or authorization record.

The artifact service is the byte-level policy enforcement point. Domain servers
forward the gateway-signed identity they received and cannot mint identities for
background completion. Asynchronous producers redeem bounded, expiring artifact
write capabilities that were issued while a live identity was present.

Artifacts support two distinct sharing modes:

- authorized grants to users or groups, still constrained by tenant and label
  policy;
- read-only anyone-with-link bearers for artifacts explicitly marked
  releasable.

Link tokens are random, stored only as hashes, default to seven days, may not
exceed thirty days, and are revocable. Public links never confer write or admin
access. Large authorized downloads are policy-checked and audited before a
sixty-second object-store URL is issued.

## MCP protocol surface

The gateway and hosted servers use the MCP protocol features that fit their
domains: tools, resources and templates, prompts, completions, tasks,
subscriptions, notifications, structured content with declared schemas, and URI
identities.

Tool helpers for clients with weak resource or task support may be added only as
explicit projections over the canonical behavior. They reuse the same models,
policy checks, audit events, task state, and artifact identities. They are not a
second implementation or a fallback completion path.

## Hosted server administration

Agent operations use MCP and the shared Task API. A hosted MCP server may pair
that protocol surface with contract-defined HTTP administration at its canonical
`{mount}/admin/*` path when the domain has installation-managed catalogs,
configuration, acquisition, or lifecycle workflows.

The gateway is the governed entry point for these APIs. Its canonical route is
`/admin/{profile}/servers/{server}/{*path}`. The gateway resolves the server from
the active profile, authorizes the read or write, records audit evidence, and
forwards a short-lived internal identity assertion. The owning server validates
that identity and its administrative scope before applying the operation through
the same domain models and state used by MCP.

Browser administration enters through explicit same-origin projections in the
Console BFF. Administrative API clients may use the protected gateway route
directly. Each server's design document owns its administrative resources,
authorization scopes, persistence records, and Console integration.

## Identity and internal trust

Operator authentication is provider-independent OIDC/OAuth with discovery and
JWKS verification. Keycloak is the integration-test identity provider; Entra is
a reference configuration, not a product dependency.

The gateway alone signs short-lived internal identity assertions with Ed25519.
Hosted services receive a public JWKS trust bundle, require a `kid`, and never
receive the private signing key. Rotation distributes overlapping old and new
public keys before the gateway changes its signing key.

Refresh-token rotation remains strict across gateway replicas, with one bounded
exception for concurrent stateless BFF delivery. For a few configured seconds,
the consumed token may redeliver the identical successor from an authenticated
encrypted envelope; afterward, reuse revokes the family as replay. The envelope
key is separate from signing and browser-session keys, plaintext is not
persisted, and delivery ciphertext is excluded from logs, audit, outbox, and
console projections. A successor consumption clears its envelope atomically;
otherwise expired envelopes are ineligible immediately and physically removed by a
dedicated one-minute GC pass.

Helm deployments separate migration-admin and runtime database
credentials, use existing Kubernetes Secrets, support service-mesh mTLS, and
apply default-deny network policy. The k3d profile binds local projections to
loopback and keeps TraCI inside the cluster.

## Operations console

The React console is an operational interface, not a marketing site. Its first
screen is the live installation: health, work, artifacts, agents, recordings,
MCP topology, policies, audit evidence, and installation state.

The in-install console BFF owns browser login, PKCE, encrypted HttpOnly sessions,
CSRF enforcement, and authorized API aggregation. It is not a source of truth;
mutations go through the gateway or owning service, and reads come from governed
platform projections.

## Recording and simulation

The recording hub is a durable push path. Producers push Rerun log streams;
the hub does not poll producers. Segment writes are fsynced, crash-decodable,
verified before optimized replacement, and cataloged as governed tenant records.
A recording MCP server exposes authorized discovery, queries, subscriptions,
and artifact publication instead of exposing the unauthenticated Rerun proxy or
catalog directly.

SUMO is a domain showcase over these same contracts: one process owns TraCI,
pushes world state to the recording hub, exposes MCP controls, and uses the
shared durable task runtime. It does not carry a private compatibility task
protocol or shell-based smoke framework.

## Offline operation

An offline bundle contains all pinned external images, Veoveo images, the Helm
chart, configuration schemas, checksums, and SBOMs.
Bundle creation occurs in a connected build environment; installation and
verification must not require a registry, package index, vendor API, or Veoveo
service. Provider-dependent features may be unavailable offline without changing
the platform, artifact, recording, SQL, policy, or agent contracts.
