# Veoveo Architecture Decisions

This document records the product and architecture decisions that every Veoveo
implementation must follow. It is normative. Component designs may change, but
changing one of these decisions requires an explicit replacement decision, not a
quiet compatibility path.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| MCP, JSON-RPC, JSON Schema, OAuth and OpenID Connect | Versions and supported subsets in the normative [MCP server contract](../mcp/contract/DESIGN.md) |
| HTTP, WebSocket, SSH and internal gRPC | Policy-checked control, artifact transfer, and Computers access; [contract evolution](CONTRACT_EVOLUTION.md) records the new profile requirements |
| UUIDv7, SHA-256 and Git identity | Opaque domain identities, integrity, and exact source provenance |
| SurrealQL and transactional outbox | Durable store and recovery under the pinned SurrealDB implementation; no database HA claim |
| OCI, Helm, Kubernetes and GitOps | Immutable release artifacts and installation-owned reconciliation |
| S3-compatible object APIs | Private Artifact storage under the [Artifact service design](../platform/artifacts/service/DESIGN.md) |
| Rerun RRD/Data Protocol, Arrow, H.264 and Media Capabilities | Recording and playback subsets under [Recordings](RECORDINGS.md) and the owning component designs |
| NVIDIA CUDA, Vulkan, RTX and NVENC; WebGPU and WebGL | Hardware execution and visual acceptance under the runtime-specific pinned profiles |
| WGS84, ENU/NED, MAVLink, TraCI and 3D Tiles | Domain integration boundaries defined by Frames, UAV, SUMO, and View designs |

## Contract evolution

[`CONTRACT_EVOLUTION.md`](CONTRACT_EVOLUTION.md) records the decisions accepted on
September 9 for provider recovery, core capacity, human and agent authority, renewable
access, verification, evidence reuse, dependency qualification, deployment boundaries,
and future Artifact transfer profiles. Internal models are the single source of each
type. A public adapter or a persistent or deployment transition needs an explicit
support window, owner, migration, and qualification. Hidden compatibility paths are
prohibited. The document's implementation table shows which accepted policies have
shipped.

## Product boundary

The owner installs and operates each Veoveo installation, and each installation runs
on its own:

- Veoveo operates no control plane, identity service, artifact index, telemetry
  sink, license service, or required public hostname;
- the installation owner chooses its hostname, ingress, identity provider, object
  store, secret manager, and observability destinations;
- `veoveo.bioma.ai` is one Bioma installation and may appear only in a clearly
  labeled deployment example;
- connected and offline installations offer the same product capabilities.

Kubernetes is the supported installation form and Helm is its package format. k3d
runs the same chart for local development.

## Computers

Computers is a core Veoveo capability. Standard releases include its native API,
Console pages, lifecycle, retained storage integration, and installation package.
Each installation configures capacity, the development images it allows, policy,
provider connections, and trust. Operators control admission and maintenance.

A core capability may run on local, remote, or on-demand compute once that
compute is qualified. The Console and API stay available when capacity is
unconfigured, exhausted, or under maintenance, and they show the user what to do
next. Setup can finish without allocating workers. Computers acceptance requires a
configured provider and real retained execution. Offline qualification requires
every selected local runtime and storage input.

Ownership and action authority use the platform's principals and Work Context
policy. Humans and services can receive explicit action grants. Computers are
private to their owner by default, and the number of Computers per owner is a quota
setting. An installation that wants only humans to use Computers expresses that in
policy rather than in the domain model.

The first supported profile is a personal development Computer. Its owner has
explicit authority, execution can be reconnected, files and caches survive
Stop/Start, and agents can be given scoped control. Browser, CLI, and agent access
all check the same domain authority. Renewable attachments expire and are revoked
independently of the retained work. The native Console page and the `computers-mcp`
server both present the same Computers domain. The gateway routes and checks policy
like it does for any server and contains no provider controller. Build publication
and production promotion keep their own separate authority.

The selected profile is deployed with the Bioma configuration.
[`COMPUTERS_PLAN.md`](COMPUTERS_PLAN.md) tracks the remaining gates: clean and offline
installation, broader provider profiles, performance qualification, and installed MCP
conformance.

## Release and installation ownership

Veoveo release engineering publishes OCI images and Helm charts. Production image
references are locked by digest, and each chart version identifies one committed
release. The publisher holds no cluster credentials and never applies customer
resources.

The installation owner keeps a private configuration repository with Helm values,
gateway control data, public trust material, image locks, and any Kubernetes
resources outside the charts. Secret values stay in the owner's secret manager and
reach workloads through existing Kubernetes Secret references. Each setting has one
owner. A second deployment document must not repeat chart selection, values, Secret
bindings, or apply order.

The installation owner also supplies the Kubernetes cluster and the reconciliation
controller. Flux is the reference GitOps controller; Veoveo does not depend on it at
runtime. The Veoveo root Kustomization starts only after the controller and its
repository credentials exist, and it cannot install, upgrade, or delete that
controller. Direct Helm and other GitOps controllers use the same charts and
configuration.

The platform chart and each independently deployable MCP extension chart reconcile
as separate applications. Once a customer-authored extension's image and chart are
published, it needs nothing from Veoveo's build system. Its gateway registration
still uses the standard control-plane, internal-trust, policy, audit, task, artifact,
and URI contracts.

Typed deployment profiles exist only for disposable development environments in this
repository. They are not an installation API for enterprises. The Rust smoke harness
checks a reconciled installation; it does not orchestrate installation.

## Tenancy

One installation represents one enterprise and may contain several internal
tenants. A tenant is a hard partition of data and authorization inside that
installation. It is not a customer account in a vendor service.

Tenant and principal identities resolve through one platform identity mapping.
Tasks, artifacts, frames, recordings, agents, grants, audit events, and outbox
events all use those record identities. No subsystem may define its own tenant or
principal namespace.

## Durable platform store

SurrealDB `3.2.4` is the required platform store. The supported topology is one
SurrealDB node on RocksDB storage. Application services may scale horizontally; this
release makes no claim of database high availability.

SurrealDB stores identity, control-plane revisions, policies, tasks, provider jobs
and webhook events, artifact metadata and grants, coordinate registries, recordings
and segments, agents and wakes, audit records, and the transactional outbox.

Cross-process work is driven by the transactional outbox and its replayable
changefeed. SurrealDB LIVE queries only reduce latency, because their delivery order
is best effort. After a disconnect or restart, a consumer resumes from its outbox
checkpoint.

Schema migrations run with installation-admin credentials. Runtime workloads connect
as a database-level runtime user and never run migrations.

## DuckDB execution

DuckDB accepts arbitrary SQL for analysis. Veoveo does not replace it with a fixed
query builder or a read-only subset.

Each request runs in a sandbox with locked configuration and extensions, memory,
thread, and spill limits, response row and byte limits, policy-checked attachment of
external sources, and container-level isolation as a second layer. External data
arrives through ingest, artifacts, or explicitly authorized HTTPS attachment. A
mutating query interrupted after it starts fails as `interrupted_indeterminate` and is
never replayed automatically.

DuckDB is not the platform's multi-process coordination database; SurrealDB is.

## Optimization execution

Optimization offers typed problem families for vehicle routing, route scenarios,
continuous convex problems, and linear mixed-integer problems. It has no generic
planning graph and no support for the retired planner contract. Map owns geography
and publishes immutable `veoveo.io/travel-model-artifact/v1` matrices. Optimization
uses those matrices as-is and never recomputes GIS costs.

The Rust Optimization server handles public identities, authorization, validation,
compilation, long-running tasks, solver admission, artifacts, and independent
verification. A pod-local Python sidecar only runs NVIDIA cuOpt, over
`veoveo.io/cuopt-executor/v1`. The sidecar uses the digest-pinned cuOpt 26.08 and
CUDA 13.3 image, requests one NVIDIA GPU, and refuses to start when the runtime,
driver, or device is missing. There is no CPU solver, GPU-optional profile, or public
executor endpoint.

A solver's own status never counts as acceptance. Before publishing an immutable
solution linked to its problem and run, the server recalculates route feasibility,
variable bounds, integrality, constraints, and objective values itself.

## Recording ingest

Producers upload recordings with one authenticated, resumable batch protocol, whether
they run in Kubernetes, on a local network, or across the internet. Network location
changes only the route to the gateway. Every producer presents an OAuth
client-credentials token for the installation's recording resource. The token binds
the producer to an installation-owned tenant, dataset, application allowlist,
classification, labels, retention policy, and quota set.

Native Rerun gRPC ends at a forwarder on the producer's loopback interface. Recording
Hub accepts only the gateway's short-lived internal assertion. It never exposes the
Rerun proxy as an installation service, NodePort, or public route. The forwarder and
Hub both keep each batch until a monotonic checkpoint is durable, which makes replay
idempotent.

A batch counts as durable once it is validated, fsynced to the batch journal, and
checkpointed in SurrealDB. Small internal RRD parts are written in journal order.
Finishing a recording, or rolling over to a new shard, runs one compaction pass that
publishes an immutable, footer-indexed archive shard to object storage, and the pass
aborts on any error. A shard normally covers one hour or 192 MiB of unoptimized input.
For video, the forwarder starts a new batch at every H.264 sample, and Hub starts a
new shard only at an access unit containing SPS, PPS, and IDR, so each shard can be
decoded on its own. Frozen and sealed archive shards are the long-term record.

Console live playback loads recent history, then follows the writing segment after
each Hub flush. Completed playback opens one Rerun Data Protocol dataset per
recording. Its immutable shards are layers of one stable segment, and Rerun fetches
footer-indexed chunks only as the view needs them. Live messages use the same dataset
and segment identity. No request opens every shard, rebuilds the recording, or
concatenates it.

Stream replay and Reason tasks can analyze a recording that is still being written.
Their read plan takes only the ingest parts Hub has fully acknowledged, copies them
to task-local storage, and records their identities in the output's provenance. A
task never reads a partial Hub write and never attaches to a producer proxy.

## Task execution and provider completion

Long-running work uses the shared task runtime. Task IDs are UUIDv7. State
transitions, leases, cancellation, results, and outbox events commit atomically.
Idempotency keys are scoped by tenant, principal, profile, server, and operation.

Each task declares how it recovers:

- `resume`: deterministic work without side effects may be reclaimed after its
  lease expires;
- `webhook_wait`: a provider job that was durably submitted waits for its signed
  webhook;
- `interrupted_indeterminate`: interrupted mutating work fails and is not run
  again automatically.

These are the recovery classes implemented today. The accepted extension is a
provider-specific completion and recovery profile under
[`CONTRACT_EVOLUTION.md`](CONTRACT_EVOLUTION.md#ce-01-provider-completion-follows-qualified-semantics).
Native authenticated events are preferred. A qualified adapter may also use
definitive synchronous results and limited status reconciliation against the
provider, and an API-only provider may declare a polling profile with fixed limits.
When an event is missing or a wait expires, the task keeps the effect marked as
uncertain and keeps its resource fence. Recovery never re-sends a mutation unless it
is proven safe. Media keeps its signed-webhook profile until a replacement passes its
own qualification.

## Artifacts and sharing

Every artifact occurrence gets a fresh opaque UUIDv7 identity and an
`artifact://{id}` URI. Content hashes are used for integrity checks and for
deduplication within a tenant. They are never public addresses. Identical bytes in
two tenants never share a storage key or authorization record.

The artifact service enforces policy on every byte read and write. Domain servers
forward the gateway-signed identity they received and cannot create identities to
finish work in the background. Asynchronous producers instead redeem an artifact
write capability that was issued while a live identity was present. The capability
is limited in size and scope and expires.

Artifacts can be shared in two ways:

- authorized grants to users or groups, still limited by tenant and label policy;
- read-only links that anyone holding them can open, for artifacts explicitly
  marked releasable.

Link tokens are random and stored only as hashes. They last seven days by default,
at most thirty, and can be revoked. A public link never gives write or admin access.
Clients reach artifacts through the installation origin set by
`global.publicBaseUrl`. Large authorized downloads, ranged downloads, and public-share
downloads are checked against policy and streamed through the Artifact service.
Object storage is private, has no hostname clients can reach, and never redirects a
client.

If a future transfer profile uses a separate installation-owned endpoint, the
Artifact service still decides access. Such a profile must prove its method and
object scope, quota, integrity, publication, and revocation behavior, and show a
measured benefit. Presigned object transfer is not supported today, partly because
URL expiry alone cannot revoke a stream that is already running. The implementation
gate is
[`CE-09`](CONTRACT_EVOLUTION.md#ce-09-artifact-authority-can-admit-qualified-transfer-routes).

## MCP protocol surface

The gateway and hosted servers use whichever MCP features fit their domain: tools,
resources and templates, prompts, completions, tasks, subscriptions, notifications,
structured content with declared schemas, and URI identities.

Helper tools for clients with weak resource or task support may be added only as
thin wrappers over the standard behavior. They reuse the same models, policy checks,
audit events, task state, and artifact identities. They never implement the
operation a second time or complete it by a different path.

Each profile chooses how it handles a failed server during discovery. By default,
the gateway leaves the unavailable server out and reports the gap in typed metadata.
A profile that declares `fail_closed` discovery refuses the whole tool list instead,
so an autonomous client never acts on a toolset that is incomplete without saying so.
No profile may drop a server silently.

## Hosted server administration

Domain administration is part of each hosted server's MCP contract. Servers use
scoped tools for changes, resources for reads, tasks for long-running work, and MCP
Apps when a browser view suits the domain. These use the same typed models, policy
checks, audit records, task state, artifact identities, and resource URIs as every
other operation.

A server may also declare an HTTP administration API at `{mount}/admin/*` for an
accepted client or installation workflow that cannot use MCP. The gateway exposes it
at `/admin/{profile}/servers/{server}/{*path}`. The gateway looks up the active
catalog, authorizes the operation, records it in the audit log, and forwards a
short-lived internal identity assertion. The server validates that assertion and
applies the request through the same domain models and state that its MCP tools use.

Every catalog entry declares an explicit `health_url` next to its MCP endpoint. The
gateway sends that URL an unauthenticated GET and treats only a success status as
healthy. It never infers health from an MCP request, an authentication failure, or a
rejected method. A fragment without a health endpoint fails control-plane validation.

An HTTP administration API adds to MCP; it never replaces it, invents other resource
identities, or keeps its own copy of state. Generic server documentation under
`{mount}/admin/docs/*` is read-only. The Console uses installation-wide BFF routes for
platform administration and hosts each server's MCP Apps for that server's workflows.
Each server's design document lists any accepted HTTP API, its scopes, and how it maps
to the server's MCP resources and tools.

## Identity and internal trust

Operators sign in through any OIDC/OAuth provider that supports discovery and JWKS
verification. Keycloak is the identity provider used in integration tests. Entra is
a reference configuration, not a product dependency.

Only the gateway signs short-lived internal identity assertions, using Ed25519.
Hosted services receive a public JWKS trust bundle, require a `kid`, and never see the
private signing key. During rotation, services receive both the old and new public
keys before the gateway switches signing keys.

Refresh-token rotation is strict across gateway replicas, with one limited exception
for concurrent requests from the stateless BFF. For a few configured seconds, a
just-consumed token can receive the same successor token again, delivered from an
authenticated, encrypted envelope. After that window, reusing the token counts as
replay and revokes the whole token family. The envelope key is separate from the
signing and browser-session keys. Plaintext tokens are never persisted, and the
delivery ciphertext never appears in logs, audit records, outbox events, or Console
data. Consuming the successor clears its envelope in the same transaction. Expired
envelopes can no longer be delivered, and a dedicated garbage-collection pass removes
them every minute.

Helm deployments use separate migration-admin and runtime database credentials, read
existing Kubernetes Secrets, support service-mesh mTLS, and apply default-deny network
policy. The k3d profile binds local endpoints to loopback and keeps TraCI inside the
cluster.

## Operations console

The React Console is a working operations tool. Its first screen shows the live
installation: health, work, artifacts, agents, recordings, MCP topology, policies,
audit records, and installation state.

The Console BFF inside the installation handles browser login, PKCE, encrypted
HttpOnly sessions, CSRF checks, and authorized API aggregation. It stores no state of
its own. Changes go through the gateway or the owning service, and reads come from
platform data the caller is authorized to see.

## Agent control

Policy decides who may control an agent, not the kind of principal making the
request. Reading an agent's state, messaging it, and answering its pending input
requests are gateway actions, and every caller must pass the selected profile's action
policy, whether a signed-in user or a service principal. This replaces the earlier
rule that only humans could control agents. An installation that wants human-only
control writes that as policy. One that allows automated responders grants that
authority to named principals. A message never carries implicit authority. Messages
are attributed to their sender, are idempotent, and land in the caller's own tenant
and Work Context.

## Recording and simulation

Recording Hub receives pushed data and stores it durably. Producers push Rerun log
streams; Hub never polls producers. Segment writes are fsynced, can be decoded after a
crash, are verified before an optimized copy replaces them, and are cataloged as
tenant records with access control. The recording MCP server offers authorized
discovery, queries, subscriptions, artifact publication, and one read-only Redap view
per recording. It exposes no unauthenticated Rerun proxy or general catalog.

SUMO is a domain showcase built on the same contracts. One process owns the TraCI
connection, pushes world state to Recording Hub, exposes MCP controls, and uses the
shared task runtime. It has no private task protocol of its own and no shell-based
smoke framework.

The UAV simulation showcase applies the same control model on GPUs. One MCP server
serializes changes to each simulation session. A cluster-private adapter runs Isaac
Sim, Cesium for Omniverse, Newton rigid bodies, the CUDA Warp UAV plant and sensors,
PX4 HIL, MAVLink, and sensor capture. Google Photorealistic 3D Tiles, rendered inside
Isaac through Cesium ion, are part of the delivered world. View MCP loads Google tiles
through its own separate source, which cannot stand in for the tiles loaded in the
simulator.

Frames MCP supplies the WGS84 origin and local frame identity. The UAV adapter loads
that definition before physics starts and does high-rate ENU/NED conversion locally.
Camera, transform, vehicle, mission, collision, and tile state go into Recording Hub
as typed Rerun streams. Stream reads newly encoded simulator camera frames directly for
live processing. Its reproducible replay profile reads recordings by identity, never
a media URL private to the simulator.

The simulator also produces the operators' live camera views. Each logical camera has
one final smoothed pose and one continuous RTX/NVENC H.264 stream, and every
authorized actor and browser shares that stream. Camera smoothing changes only the
logical camera's transform at the current simulator tick. Nothing else renders,
encodes, or stores a copy of the scene, the camera poses, or the browser
authorizations.

The reference deployment runs six GPU workloads at once: the UAV simulator, View,
Stream, Reason, the cuOpt executor, and the Rerun viewer. Each Helm workload requests
its GPU normally and schedules independently. No profile disables one GPU service to
make room for another, so the cluster must have capacity for all six.

Visual workflows require hardware acceleration and stop without it. Visual browser
acceptance, interactive demonstrations, screenshots used as visual evidence, and
publication rendering all need a headed browser with hardware-backed WebGPU or WebGL.
Both APIs are probed when available. SwiftShader, llvmpipe, and other software
rasterizers do not count as hardware.

Headless browser tests may check nonvisual behavior such as forms, authorization,
navigation, and keyboard handling. Their results cannot stand in for required visual
or GPU acceptance. Tests may use maintained tooling in their own language, following
the shared rules for prerequisites, cleanup, diagnostics, and evidence. A labeled
screenshot from a headless failure is a diagnostic, never visual acceptance.

Browser H.264 playback is the one software exception. The exact Media Capabilities
configuration must report `supported` and `smooth`, and the UI labels the path as
software decode unless it is also `powerEfficient`. This exception does not relax the
hardware requirement for browser graphics, server-side NVENC, GPU rendering,
simulation, Stream perception, Reason, or cuOpt.

## Offline operation

An offline bundle contains every pinned external image, the Veoveo images, the Helm
chart, configuration schemas, checksums, and SBOMs. The bundle is built in a connected
environment. Installing and verifying it must not need a registry, package index,
vendor API, or Veoveo service. Features that depend on an outside provider may be
unavailable offline, but the platform, artifact, recording, SQL, policy, and agent
contracts stay the same.
