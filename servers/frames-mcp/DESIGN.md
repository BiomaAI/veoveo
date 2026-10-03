# Frames MCP Design

This document is the canonical design and operational contract for
`veoveo-frames-mcp`.

## Identity

```text
crate       veoveo-frames-mcp
folder      servers/frames-mcp
slug        frames
URI scheme  frames
endpoint    /frames/mcp
health      /frames/healthz
port        8793
```

## Standards And Protocols

Frames implements Model Context Protocol `2026-07-28` under Veoveo hosted MCP
contract revision 3. Resource and tool payloads use JSON. Geodetic positions
use WGS84, while ECEF positions use the EPSG:4978 coordinate reference system.
MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` defines the
server-owned `ui://frames/workspace.html` Frame Editor application.
Published revision integrity uses SHA-256 with the repository-owned canonical
`sha256:` plus 64 lowercase hexadecimal representation. Frame-world and
operation resources are Veoveo extensions rather than external protocols.
World, operation, usage, document, Artifact and dynamic stream addresses use the [foundational concrete URI profile](../../platform/types/DESIGN.md#concrete-resource-components):
URL 2.5.8 implements WHATWG parsing for these hierarchical custom-scheme routes,
with the domain restrictions described below.
Resource templates use RFC 6570 path variables and the optional `cursor` query
expansion. The concrete parser admits only each route's declared parameters.
Persistence uses the shared Store's pinned SurrealDB 3.3.0 client and SurrealQL
queries over its internal WebSocket connection.
Frames operation authority is an internal storage contract with a required profile
and the caller's original optional tenant identity.

Frames owns complete spatial-frame worlds and bounded coordinate conversion.
Map MCP owns Earth geography, projected coordinate reference systems,
geodesics, geofences, and routing. High-rate transforms remain in live streams
or governed recordings.

## Public Contract And Dependencies

The library's `contract` feature re-exports the shared
[Frames domain contract](../../platform/frames/contract/DESIGN.md): world IDs, frame
trees, resource addresses, conversion requests and operation provenance. Consumers
disable default features. Recording imports the domain crate directly because Frames
runtime consumes RRD, which already imports the Recording contract. This dependency
requirement places the public model below both runtimes.
The dependency graph contains foundational and Artifact value types, Map's geodetic
contract, serialization/schema support, and value validation libraries. It excludes
MCP integration, database clients, async runtimes, provider engines, and GPU libraries.
`runtime` adds local math, persistence, and recording/artifact adapters. The default
`mcp` feature adds hosted protocol wiring and enables the server binary.

Map owns CRS, datum, and ellipsoid names. Frames' provenance imports `CrsId` from that
library without calling Map. The operation-kind wire enum preserves its published
variants; this engine emits `frame_conversion`. RRD owns recording-specific frame
kinds and geofence metadata. MCP core imports none of these domain contracts.

World, revision, and frame address constructors require their specific IDs. A world
address builds a revision address; a revision address builds a frame address. The
foundation's URL 2.5.8 component builder constructs their existing `frames://world`
routes. Parsers validate the complete route and retain the decoded typed identities.
They reject credentials, ports, queries, fragments, escaped aliases, extra segments,
and URI spellings that require normalization. Each address implements `ResourceAddress`.
A typed address grants no authority and does not establish persisted parent membership.

### Hosted Resources And Setup

The server library adds `FramesResource` and `FramesDocument` to its isolated contract
feature. The resource enum composes the shared world's typed addresses with operation,
usage, Artifact and embedded-document routes. Request dispatch parses this enum once;
subscription admission accepts only its mutable world and usage variants. Dynamic
routes use the foundational URI builder. Parsers reject unknown documents, aliases,
unsupported query fields and cross-family paths before accessing services.

`FramesContract` implements the open `McpServerContract` trait. Startup checks the
configuration, fixed descriptors and templates before connecting to Store. Discovery
and initialization read that setup. `FramesMcp` implements the shared host's
`DomainServer`, which serves documents, the contract and `doc_id` completion and
calls Frames only for its own addresses. `FramesSubscriptions` receives requested
URIs as `FramesResource` values and checks world existence and task visibility for
the caller. The empty `FramesScope` vocabulary adds no domain
permissions; gateway operation policy and SQL owner/label selection govern access.
The fixed catalog declares resource subscriptions and omits resource-list changes.

### Frame Identity Admission

The [domain contract](../../platform/frames/contract/DESIGN.md#construction-and-admission)
defines identifier limits, parent-specific builders and public decoding. Frames rejects
inconsistent identity, tree, head or digest facts after SQL selects an authorized record.
Readers do not normalize identities or repair stored metadata.

## Frame worlds

A frame world is an authored identity with a mutable head. Each publication
creates an immutable, complete, rooted tree. A revision contains every frame
needed to interpret the world rather than one disconnected origin.

Each tree has exactly one `ecef_wgs84` root. Every other node names one parent
and one transform:

| Transform | Contract |
|---|---|
| `geodetic_tangent` | Anchors an ENU or NED child to an ECEF parent with WGS84 latitude, longitude, and ellipsoidal height. |
| `static_rigid` | Carries a finite translation in metres and a normalized XYZW quaternion. |
| `dynamic_stream` | Names a canonical stream URI and an entity path whose timestamped data resolves the transform. |

`FrameStreamUri` validates the concrete URI components and preserves the supplied
spelling. A producer builds its address with its own typed resource builder, then
converts that resource reference into `FrameStreamUri`. Frames accepts independent
producer schemes without a server registry or a dependency on their runtime. The
producer owns route meaning, query vocabulary and resource authorization. Shared URI
admission rejects credentials, ports, fragments, templates, duplicate query names,
malformed encoding and spellings that require normalization.

`FrameEntityPath` carries the producer's entity selector unchanged. It requires a
nonblank value of at most 2048 UTF-8 bytes without control characters. A selector is
not interpreted as a filesystem path. Both types validate JSON input before a dynamic
transform can be constructed. Existing JSON field names and string schemas are
preserved; the schemas describe the wire shape, while admission enforces these limits.
Retained values outside this profile stop the coordinated upgrade described above.
Reads do not normalize stored references because doing so would change revision hashes.

Frame bases include ECEF WGS84, ENU, NED, forward-right-down, optical
right-down-forward, and an explicit Cartesian axis mapping. Validation rejects
duplicate identities, missing parents, multiple roots, cycles, disconnected
nodes, invalid axes, non-finite transforms, and unnormalized quaternions.

Publication sorts nodes by frame identity before hashing. The SHA-256 digest is
stored beside the immutable revision. `ValidatedWorldTree` owns complete-tree admission,
canonical ordering and hashing in the contract feature, using the existing SHA-256 pin.
Its tree, root and digest are private. Parent traversal memoizes completed paths and
visits each edge once; the 10,000-frame limit also applies to contract decoding.

`FrameWorldRevision` requires a typed revision URI, a positive publication number,
and an admitted tree. It derives world and revision IDs, the world URI, root address
and digest. Its public accessors borrow immutable content. Decoding checks the wire's
repeated identities, root and digest against those derived values. The Store adapter
performs the same admission after SQL selects an authorized revision. A mismatch fails
the read without changing stored data. Per-frame resources select their requested node
in SQL and do not claim to have checked the entire revision's digest.

`FrameWorldSummary` derives its world ID from its address. The builder binds a head
revision to a positive publication number; an empty world has no head and revision zero.
`FrameSourceReference` derives its revision ID from its address. A source reference
carries a digest claim; resolving and authorizing the referenced revision establishes
its content. JSON field names and generated schemas preserve the published forms.
Current reads reject inconsistent metadata without changing stored values.

## Lifecycle

Frames starts empty. Helm does not create frames, origins, worlds, or
revisions. The server has no Frames bootstrap document, bootstrap CLI flag, or
bootstrap validation command.

Clients create and publish through MCP:

1. `create_world` creates empty authoring metadata.
2. `publish_world` validates and atomically publishes one complete tree.
3. Sessions pin the returned revision URI and a frame URI within that revision.

`create_world` is idempotent for identical metadata. `publish_world` returns the
current revision when the canonical tree digest already matches. Publishing a
different tree requires the expected head revision, which prevents lost
updates.

## MCP surface

| Tool | Execution | Result |
|---|---|---|
| `create_world` | direct | Empty world identity and mutable head metadata. |
| `publish_world` | direct | Immutable validated revision, digest, root, and updated world head. |
| `convert_frame` | direct | Bounded typed conversion with durable provenance and exact source revisions. |
| `batch_transform` | task required | Durable conversion with optional artifact output. |

Coordinate points are strongly typed:

```text
CoordinatePoint
  Wgs84(latitude_degrees, longitude_degrees, ellipsoid_height_m)
  EcefWgs84(x_m, y_m, z_m)
  WorldFrame(frame_uri, x_m, y_m, z_m)

CoordinateSpace
  Wgs84
  EcefWgs84
  WorldFrame(frame_uri)
```

A world-frame point always names a revision-scoped URI. Static transform chains
resolve locally and deterministically. A conversion through a dynamic node
fails with a request for timestamped stream or recording data; the server does
not invent a current pose.

`batch_transform` uses official Tasks. Direct calls are
rejected. Large results go through the artifact plane. The shared Task service selects
current owner, profile, tenant and label visibility in SQL before decoding public
reads or subscription updates. Cancellation repeats that selection in its write
transaction. Workers carry foundational Task IDs through claims and transitions.

## Resources

```text
frames://worlds
frames://worlds{?cursor}
frames://world/{world_id}
frames://world/{world_id}/revision/{revision_id}
frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}
frames://operation/{operation_id}
frames://artifact/{artifact_id}
frames://usage
frames://usage{?cursor}
frames://usage/task/{task_id}
```

The world resource carries mutable head metadata. Revision and frame resources
are immutable. Operations, artifacts, tasks, and usage require owner, tenant,
profile, and data-label isolation.

Discovery advertises fixed roots, documents, and templates without accessing Store
or the Artifact service. Clients browse authored worlds through `frames://worlds`,
which returns `{items, limit, next_cursor}` with at most 100 summaries ordered by
world ID. SQL applies tenant, label, and keyset predicates before the limit.
A v1 cursor carries its collection and typed last world ID as hexadecimal
JSON. It grants no access and provides no snapshot: each page uses current authority
and data, and clients restart at the root to observe insertions before their cursor.
The Frame Editor's shared workbench follows the cursor with Next and Previous actions.

`frames://usage` returns `FrameUsagePage`: at most 100 distinct Task references in
`items`, the fixed `limit`, and an optional `next_cursor`. TaskRuntime checks the usage
row and linked Task's server and tenant, then the principal, profile, optional tenant
spelling and complete required labels in SQL before grouping, ordering and limiting.
Exact Task usage reads use the same predicates. Deleted Tasks and inconsistent linked
metadata provide no access. Frames preserves its owner-based Task policy; this read
does not confer Work Context authority.

The contract library owns usage cursors and addresses. A v1 hexadecimal JSON cursor
carries the `frames://usage` collection and last native Task UUIDv7. Typed constructors
use the shared URI builder; parsing requires a canonical lowercase hyphenated UUIDv7
Task address and rejects aliases or extra components. Each entry's Task ID must agree
with its address. Page construction and decoding reject oversized, duplicate or
unordered entries, and a continuation must name the last entry of a full page.

World, revision, and frame identifier completion matches case-insensitively in SQL
before selecting at most 101 values. The MCP adapter returns at most 100 and omits
the total when more matches exist. Revision completion requires `world_id`; frame
completion requires both `world_id` and `revision_id`. Missing parents yield an empty
completion, malformed parent IDs fail admission, and SQL applies parent visibility.

The server emits resource-update notifications when mutable state changes.
Each replica shares Store LIVE observations of world heads, revisions, Tasks and domain
usage through its resource hub. Subscriptions admit world and usage indexes, visible
worlds and caller-owned task usage. Immutable revisions and frame definitions reject
subscriptions. Source reconnection invalidates accepted resource contents;
ordinary reads recheck current authority. World and usage cursor pages admit subscriptions and
are invalidated with the other accepted resource identities. The fixed discovery
surface declares no list-change notifications. The observer stops with the server.
Task-usage subscriptions may start before the first usage row; admission checks Task
ownership in SQL. Unauthorized and missing Tasks produce the same resource-not-found
response. Task changes invalidate accepted usage references even when no usage row changed.

## Operation Authority And Storage

Frames owns operation persistence in `state/operations`; Store provides its client,
schema migrations, record primitives and native table changefeeds. The runtime uses
SurrealDB SDK 3.3.0 to match Store's driver types.

`FrameOperationScope` requires typed principal, optional tenant, gateway profile,
and data-label clearance. The gateway assertion supplies those values for direct
calls; Task workers use the admitted Task owner. World sharing keeps its separate
`FrameScope` policy. Frames operation policy compares principal, tenant, profile and
labels; it does not grant Work Context membership or compare context identities.

Each operation stores its required profile record and original optional
tenant spelling beside the existing owner and tenant records. SQL applies all of those
values and required labels before returning provenance. A Task-linked operation also
requires its current Task to belong to Frames and agree on tenant, principal, profile,
owner-envelope identities and label clearance. Deleted or inconsistent parents deny
access. Unauthorized and absent operations share the resource-not-found response.

Recording checks Task authority inside the operation transaction, then creates the
immutable operation. Native changefeeds record its commit. Replaying identical
authority and provenance performs no mutation. Stored labels keep their 256-byte limit. Any changed authority, Task link, labels or provenance
conflicts. A failed transaction is accepted as a concurrent or acknowledged-late replay
only after reading the identical committed record under current access policy.

`FrameOperationUri` builds and parses operation addresses through the foundational
components. `CoordinateOperationRef` derives its ID from that typed address; its
constructor and frame builder prevent mismatched ID/URI pairs. The existing JSON fields
and schemas are preserved, while decoding rejects disagreement and malformed routes.
The lexical operation-ID profile is unchanged. Native persistence requires `op-` followed
by UUIDv7, as generated by the engine and admitted Task request. A reference alone
provides no proof that the operation exists.

### Operation Authority Integrity

The Rust operation record requires `OperationAuthority`. Store requires its authority
object and profile record, and preserves the distinction between an absent tenant key
and the explicit installation tenant. SQL checks those fields and current Task authority
before selecting provenance. The database rejects attempts to remove required authority.

Native qualification covers direct and Task caller isolation, current parent changes,
concurrent identical replay, conflicting replay, required-authority schema enforcement,
and atomic event publication. Separate Store connections read the same current operation
and an identical replay creates no second event. Installed qualification is a separate
gate in the foundations plan.

## Prompts

| Prompt | Purpose |
|---|---|
| `frames-frame-audit` | Reviews frames, units, axes, datum, origin, and approximation assumptions. |
| `frames-world-design` | Drafts one complete rooted tree for a robot, sensor, or simulation world. |
| `frames-transform-explain` | Explains recorded operation provenance without inventing missing transforms. |

## Calculation and provenance

WGS84 and EPSG:4978 conversion uses the WGS84 ellipsoid. ENU and NED anchors
apply their declared tangent rotation. Static rigid transforms compose through
the tree to ECEF and invert for the target frame.

Approximation permission is explicit. The current engine rejects approximate
conversion because no approximate implementation is exposed. Every successful
conversion stores a `CoordinateOperationProvenance` record before returning.
The output lists only frame-world revisions traversed while converting the
source points or target. Each sorted, deduplicated reference contains the
immutable revision URI, revision identity, and canonical SHA-256 digest.
Prefetched revisions that were not used are absent. A WGS84/ECEF-only
conversion returns an empty source list.

## Persistence

Frames owns world read queries in `state/reads.rs`, mutations in `state/worlds.rs`,
and private driver records in `state/records.rs`. Store supplies the connection and
schema catalog. The query API accepts Frames IDs and resource addresses;
conversion to database values happens at bindings. This dependency direction lets
cross-server consumers use the existing contract feature without introducing a Store
dependency on the Frames runtime. The domain crate supplies the same identity types.

World reads select the caller's tenant and require every world label in the caller's
clearance inside SQL. World visibility is shared within a tenant; publication requires
the world owner. Revision reads check the linked world's tenant, key, owner, and current
labels in the same query. Missing or inconsistent parents cannot authorize a revision.
Head reads additionally require the linked record, revision key, and revision number
to agree. Direct frame resources select only their requested node in SQL; coordinate
conversion loads the visible complete revision to resolve the transform chain.

World creation validates bounded metadata and the caller's typed data labels. A visible
world with identical metadata satisfies a repeated create within the tenant's sharing
policy. Other existing worlds prevent creation through the unique tenant/world key.
Creation writes the world and its event in one transaction.

Publication applies tenant, owner and current label clearance in SQL before selecting
the world. It checks the head's linked world, owner, tenant, revision key and number
inside the same transaction. An identical validated tree returns the current head;
a changed tree requires the expected head. The transaction publishes the revision,
updates the head and writes both events together. The result carries the world and
revision from that transaction's snapshot. Concurrent attempts cannot produce a result
assembled from different head versions. After a transaction error, Frames only returns
a currently authorized, matching committed head; it does not dispatch another write.
Driver errors preserve diagnostic causes internally while public messages omit hidden
record identities.

World scope carries typed `DataLabelId` values and resolved Store identities. The existing
installation tenant mapping and tenant-wide read sharing apply; world policy does not
add a profile or Work Context partition. The internal Store draft API is absent, so
callers cannot publish an opaque tree or supply its root and hash separately. Stored
record keys, schemas, event version 1 and wire payloads keep their existing forms.
Every writer applies the current publication owner and label checks before changing a world head.

SurrealDB stores:

- `frame_world` authoring identities and mutable heads;
- `frame_world_revision` immutable trees and digests;
- coordinate operations and provenance;
- task state, inputs, usage and ownership.

Migration `0027_frame_world_graphs.surql` removes the old flat `frame` table and
defines the world and revision tables. This is a hard cut. No alias or legacy
`frames://frame/{frame_id}` resource remains.

## Authentication and isolation

The hosted endpoint requires a gateway-signed internal identity and the
forwarded bearer authority. The assertion fixes the server slug, profile,
principal, tenant, labels, scopes, and expiry. Gateway policy admits actions and resource
access. Frames declares no additional domain scope vocabulary; its runtime enforces
the world, operation and Task ownership policies after authenticated admission.

Unknown and unauthorized worlds, revisions, frames, operations, tasks, usage,
and artifacts are indistinguishable at their resource boundary.

Operation reads use the owner policy and storage profiles specified below. Installed
qualification is tracked in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

## Module layout

```text
servers/frames-mcp/src/
  contract.rs             re-exports platform/frames/contract public models
  contract/resources.rs   complete hosted route vocabulary
  contract/scopes.rs      domain scope declaration
  engine.rs               coordinate conversion
  world.rs                transform resolution
  state.rs                typed world scope and revision adapters
  state/records.rs        private world/revision driver records
  state/worlds.rs         typed world creation and publication
  state/worlds/           transactional mutation SQL and native qualification
  state/operations.rs     typed operation authority and driver records
  state/operations/       transactional recording SQL and native qualification
  state/reads.rs          typed world queries with SQL visibility and parent checks
  state/completion.rs     scoped matching with typed parents before SQL limits
  state/read_tests.rs     isolated database authorization and parent-integrity cases
  state/catalog_tests.rs  keyset paging and completion beyond initial result windows
  artifacts.rs            artifact-plane integration
  uris.rs                 fixed route declarations and templates
  bin/server.rs           transport and MCP composition
  bin/server/
    app_state.rs
    config.rs
    completion.rs
    setup.rs
    outputs.rs
    ownership.rs
    prompts.rs
    resources.rs
    subscriptions.rs
    task_extension.rs
```

## Verification

Rust tests cover world validation, canonical hashing, persistence, WGS84/ECEF
math, arbitrary static tree conversion, schemas, ownership, and tasks. The Rust
smoke creates an empty world, publishes a multi-frame tree, reads its immutable
resources, converts through the tree, runs a durable batch, and verifies
artifact and usage isolation.
