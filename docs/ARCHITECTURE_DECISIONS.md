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

## Release and installation ownership

Veoveo release engineering publishes OCI images and Helm charts. Production image
references are digest-locked, and chart versions identify one committed release.
The publisher does not hold cluster credentials or apply customer resources.

The installation owner maintains a private configuration repository containing Helm
values, gateway control data, public trust material, image locks, and Kubernetes
resources outside the charts. Secret bytes remain in the owner's secret-management
system and enter workloads through existing Kubernetes Secret references. One setting
has one owner; an additional deployment document must not repeat chart selection,
values, Secret bindings, and apply order.

The installation owner also supplies the Kubernetes cluster and reconciliation
controller. Flux is the reference GitOps implementation, not a Veoveo runtime
dependency. The Veoveo root Kustomization begins after the controller and repository
credentials exist, and it cannot install, upgrade, or delete that controller. Direct
Helm and other GitOps controllers consume the same package and configuration contracts.

The platform chart and independently deployable MCP extension charts reconcile as
separate applications. A customer-authored extension does not require Veoveo's build
system once its image and chart are published, but its gateway registration continues
to use the canonical control-plane, internal-trust, policy, audit, task, artifact, and
URI contracts.

Typed deployment profiles are confined to disposable repository-development
environments. They are not an enterprise installation API. The Rust smoke harness
verifies a reconciled installation and does not own installation orchestration.

## Tenancy

One installation represents one enterprise boundary and may contain multiple
internal tenants. A tenant is a hard data and authorization partition inside
that installation, not a customer account in a vendor service.

Tenant and principal identities resolve through one canonical platform identity
mapping. Tasks, artifacts, frames, recordings, agents, grants, audit events, and
outbox events must use those same canonical record identities. A subsystem must not
invent a parallel tenant or principal namespace.

## Durable platform store

SurrealDB `3.2.3` is the required durable platform store. The canonical topology
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

## Optimization execution

Optimization exposes typed vehicle-routing, route-scenario, continuous convex,
and linear mixed-integer problem families. It does not expose a generic planning
graph or preserve the retired planner contract. Map owns geography and publishes
immutable `veoveo.io/travel-model-artifact/v1` matrices; Optimization consumes
those matrices without recomputing GIS costs.

The Rust Optimization server owns public identities, authorization, validation,
compilation, durable tasks, solver admission, artifacts, and independent
verification. A pod-local Python sidecar owns only NVIDIA cuOpt execution through
`veoveo.io/cuopt-executor/v1`. The sidecar runs the digest-pinned cuOpt 26.06 and
CUDA 13.2 image, requests one NVIDIA GPU, and fails closed when the runtime,
driver, or device is unavailable. There is no CPU solver, GPU-optional profile,
or public executor endpoint.

Solver status is not acceptance evidence. The control plane recalculates route
feasibility, variable bounds, integrality, constraints, and objective values
before publishing an immutable solution linked to its exact problem and run.

## Recording ingest

Recording ingest uses one authenticated, resumable batch protocol from Kubernetes, a
local network, or the public edge. Network placement changes only the route to the
gateway. Every producer presents an OAuth client-credentials token for the installation's
recording resource and is bound to an installation-owned tenant, dataset, application
allowlist, classification, labels, retention policy, and quota set.

Native Rerun gRPC terminates at a loopback producer forwarder. Recording Hub accepts only
the gateway's short-lived internal assertion and never exposes the Rerun proxy as an
installation service, NodePort, or public route. The forwarder and hub retain batches
until a monotonic durable checkpoint makes replay idempotent.

Durability begins with a validated, fsynced batch journal and its SurrealDB checkpoint.
Small RRD parts are the ordered write-path materialization of that journal. They remain
internal. Finishing and rollover run one fail-closed object-store compaction pass that
publishes an immutable, footer-indexed archive shard. The normal shard boundary is one
hour or 192 MiB of unoptimized input, and video rollover starts the next shard at a
decoder-reentrant batch. The producer-local forwarder begins a durable batch at every
H.264 sample. Hub recognizes only an access unit containing SPS, PPS, and IDR as an
eligible shard boundary. Frozen and sealed archive shards remain the governed long-term
recording authority.

Console live playback receives bounded recent history and follows the authorized writing
segment after each Hub flush. Completed playback opens one recording-scoped Rerun Data
Protocol dataset. Immutable shards are layers of its stable segment, and Rerun fetches
footer-indexed chunks as the active view requires them. Live messages use the same dataset
and segment identity. No request path opens every shard, rebuilds, or concatenates the
whole recording.

Bounded Stream replay and Reason tasks may analyze the writing recording before rollover.
Their governed read plan captures only complete acknowledged ingest parts, copies those
parts into task-local storage, and records their immutable identities in output
provenance. A task never reads an incomplete Hub write or attaches to a producer proxy.

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
access. Every client-facing capability uses the installation origin selected by
`global.publicBaseUrl`. Large authorized, ranged, and public-share downloads are
policy-checked and streamed through Artifact service. Object storage remains private,
has no client-addressable hostname, and never issues a redirect to a client.

## MCP protocol surface

The gateway and hosted servers use the MCP protocol features that fit their
domains: tools, resources and templates, prompts, completions, tasks,
subscriptions, notifications, structured content with declared schemas, and URI
identities.

Tool helpers for clients with weak resource or task support may be added only as
explicit projections over the canonical behavior. They reuse the same models,
policy checks, audit events, task state, and artifact identities. They are not a
second implementation or a fallback completion path.

Federated discovery failure is a profile-selected decision, not an implicit
behavior. The default isolates an unavailable server and reports its degradation
through typed metadata; a profile that declares `fail_closed` discovery refuses
the whole tool list instead, so an autonomous client can never act on a silently
incomplete toolset. No profile may degrade silently.

## Hosted server administration

Domain administration is part of the hosted server's MCP contract. Servers use
scoped tools for mutations, resources for reads, durable tasks for long-running
work, and MCP Apps when a browser view fits the domain. These surfaces retain the
same typed models, policy checks, audit evidence, task state, artifact identities,
and canonical resource URIs as every other operation.

A server may declare an additive HTTP administration projection at
`{mount}/admin/*` when an accepted client or installation workflow cannot use the
canonical MCP surface. The gateway exposes that projection at
`/admin/{profile}/servers/{server}/{*path}`. It resolves the active catalog,
authorizes the operation, records audit evidence, and forwards a short-lived
internal identity assertion. The owning server validates that assertion and
applies the request through its canonical domain models and state.

Health is a declared contract, never an inference. Every catalog entry names an
explicit `health_url` beside its MCP endpoint; the gateway probes it with an
unauthenticated GET and treats only a success status as healthy. An MCP request,
an authentication failure, or a method rejection is never a health signal, and a
fragment without a health endpoint fails control-plane validation.

An HTTP projection never replaces MCP, invents alternate resource identities, or
becomes a second source of truth. Generic server documentation under
`{mount}/admin/docs/*` remains a read-only self-description projection. The
Console uses installation-wide BFF routes for platform administration and hosts
domain MCP Apps for server-owned workflows. Each server design document declares
any accepted HTTP projection, its scopes, and its relationship to the canonical
MCP resources and tools.

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

## Agent control

Agent control is policy authority, not principal-kind authority. Reading agent
state, sending an agent a message, and answering a pending input request are
gateway actions that every caller, signed-in user and service principal alike,
must pass through the selected profile's action policy. This replaces the earlier
human-only restriction: an installation that wants human-only control expresses
it as policy, and one that admits automated responders grants that authority to
named principals explicitly. Messages never carry implicit authority, remain
actor-attributed and idempotent, and land inside the caller's exact tenant and
Work Context.

## Recording and simulation

The recording hub is a durable push path. Producers push Rerun log streams;
the hub does not poll producers. Segment writes are fsynced, crash-decodable,
verified before optimized replacement, and cataloged as governed tenant records.
A recording MCP server exposes authorized discovery, queries, subscriptions,
artifact publication, and one recording-scoped read-only Redap projection. It
does not expose an unauthenticated Rerun proxy or general catalog.

SUMO is a domain showcase over these same contracts: one process owns TraCI,
pushes world state to the recording hub, exposes MCP controls, and uses the
shared durable task runtime. It does not carry a private compatibility task
protocol or shell-based smoke framework.

The UAV simulation showcase follows the same control boundary at GPU scale.
One MCP server serializes mutations for each simulation session, while a
cluster-private adapter owns Isaac Sim, Cesium for Omniverse, Newton rigid bodies,
the CUDA Warp UAV plant and sensors, PX4 HIL, MAVLink, and sensor capture. Google
Photorealistic 3D Tiles rendered inside Isaac through Cesium ion are part of the
delivered world contract. View MCP's
direct Google source is independent and is not a substitute for simulator tile
residency.

Frames MCP supplies the durable WGS84 origin and local frame identity. The UAV
adapter materializes that definition before starting physics and performs
high-rate ENU/NED conversion locally. Camera, transform, vehicle, mission,
collision, and tile state enter Recording Hub as typed Rerun streams, and
Stream consumes newly encoded simulator camera frames directly for live
processing. Its reproducible replay profile consumes governed recording
identities rather than a simulator-private media URL.

The authoritative simulator also owns operator live cameras. Logical cameras share one
final smoothed pose, while every actor-and-browser viewer lease reserves a preallocated
camera clone and an isolated RTX, NVENC, and native WebRTC product. Camera smoothing
changes only the logical operator-camera transform at the current simulator tick. The
platform does not mirror scenes, transport visualization poses, reconcile a second
renderer, relay media, or persist browser leases.

The reference deployment runs the authoritative UAV simulator, View, Stream, Reason,
the cuOpt executor, and the Rerun viewer concurrently. Their Helm workloads
declare ordinary GPU requests and remain independently schedulable. No
application profile disables one GPU service to admit another; cluster capacity
must satisfy the complete six-workload declaration.

Visual workflows fail closed without hardware acceleration. Browser automation,
interactive demonstrations, screenshots, and publication rendering require a
headed browser with hardware-backed WebGPU or WebGL. Both APIs are probed when
available; SwiftShader, llvmpipe, and software rasterizers do not count as
hardware evidence.

Browser H.264 playback is the only software exception. The exact Media
Capabilities configuration must report `supported` and `smooth`, and the UI
labels the path as software decode unless it is also `powerEfficient`. This
exception does not relax hardware-backed browser graphics, server-side NVENC,
GPU rendering, simulation, Stream perception, Reason execution, or cuOpt
optimization.

## Offline operation

An offline bundle contains all pinned external images, Veoveo images, the Helm
chart, configuration schemas, checksums, and SBOMs.
Bundle creation occurs in a connected build environment; installation and
verification must not require a registry, package index, vendor API, or Veoveo
service. Provider-dependent features may be unavailable offline without changing
the platform, artifact, recording, SQL, policy, or agent contracts.
