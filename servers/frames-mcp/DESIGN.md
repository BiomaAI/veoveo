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
World and usage addresses use the [foundational concrete URI profile](../../platform/types/DESIGN.md#concrete-resource-components):
URL 2.5.8 implements WHATWG parsing for these hierarchical custom-scheme routes,
with the domain restrictions described below.
Resource templates use RFC 6570 path variables and the optional `cursor` query
expansion. The concrete parser admits only each route's declared parameters.
Persistence uses the shared Store's pinned SurrealDB 3.2.4 client and SurrealQL
queries over its internal WebSocket connection.

Frames owns complete spatial-frame worlds and bounded coordinate conversion.
Map MCP owns Earth geography, projected coordinate reference systems,
geodesics, geofences, and routing. High-rate transforms remain in live streams
or governed recordings.

## Public Contract And Dependencies

The library's `contract` feature exposes world IDs, frame trees, resource addresses,
conversion requests, and operation provenance. Consumers disable default features.
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

### Frame Identity Admission And Upgrade

Frame, world, revision, and operation IDs admit 1–128 ASCII letters, digits, underscores,
hyphens, dots, or colons, excluding the complete values `.` and `..`. Those two values
are relative URL components and cannot identify resources. JSON still carries strings;
the schemas and all admitted route spellings keep the published 0.1.x representation.

The Frames owner requires a coordinated drain for the stricter identity admission.
Before upgrading an installation with retained data, decode its world records, complete
revision trees, operation provenance, and consumer references using the new contract.
An invalid retained identity stops the upgrade. Preserve the original data and resolve
its references under the owning domain's recovery procedure before retrying; do not
normalize or rewrite IDs during reads. The reference installation is rebuilt empty.
Mixed old/current producers are unsupported during this transition.

The change performs no persistent format conversion. Rollback restores the previous
binaries and their unchanged data; values emitted by the current constructors fit the
previous profile. Contract tests qualify the shared wire shapes, malformed admission,
parent-specific construction, and independent library consumption. Retained-data and
installed acceptance remain release gates in the foundations plan.

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

Frame bases include ECEF WGS84, ENU, NED, forward-right-down, optical
right-down-forward, and an explicit Cartesian axis mapping. Validation rejects
duplicate identities, missing parents, multiple roots, cycles, disconnected
nodes, invalid axes, non-finite transforms, and unnormalized quaternions.

Publication sorts nodes by frame identity before hashing. The SHA-256 digest is
stored beside the immutable revision. Consumers can therefore verify the exact
tree they received.

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
rejected. Large results go through the artifact plane.

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

### Catalog Upgrade

The Frames owner requires a coordinated drain for the world and usage collections'
array-to-page changes. Update installed clients to decode `FrameWorldPage` and
`FrameUsagePage` and follow their typed
cursor, then replace the drained server and refresh discovery and embedded App caches.
Mixed array/page servers behind one endpoint are unsupported. This transition changes
no persisted format. Rollback drains the server and restores the previous server/client
set with the same data. Native page and cursor cases qualify the new representation;
installed client acceptance is a release gate in the foundations plan.
The same drain admits the stricter usage address profile. Generated native Task
references already use that profile. Retained caller references must parse with
`FrameTaskUsageUri` before upgrade; rejection requires the caller to resolve the
correct native Task identity, without rewriting persisted Tasks or usage rows.

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

Frames owns world read queries in `state/reads.rs` and reuses `PlatformStore`'s
connection and driver records. The query API accepts Frames IDs and resource addresses;
conversion to database values happens at bindings. This dependency direction lets
cross-server consumers use the existing contract feature without introducing a Store
dependency on the Frames library or a second identity crate.

World reads select the caller's tenant and require every world label in the caller's
clearance inside SQL. World visibility is shared within a tenant; publication requires
the world owner. Revision reads check the linked world's tenant, key, owner, and current
labels in the same query. Missing or inconsistent parents cannot authorize a revision.
Head reads additionally require the linked record, revision key, and revision number
to agree. Direct frame resources select only their requested node in SQL; coordinate
conversion loads the visible complete revision to resolve the transform chain.

SurrealDB stores:

- `frame_world` authoring identities and mutable heads;
- `frame_world_revision` immutable trees and digests;
- coordinate operations and provenance;
- task state, inputs, usage, ownership, and outbox events.

Migration `0027_frame_world_graphs.surql` removes the old flat `frame` table and
defines the world and revision tables. This is a hard cut. No alias or legacy
`frames://frame/{frame_id}` resource remains.

## Authentication and isolation

The hosted endpoint requires a gateway-signed internal identity and the
forwarded bearer authority. The assertion fixes the server slug, profile,
principal, tenant, labels, scopes, and expiry.

Unknown and unauthorized worlds, revisions, frames, operations, tasks, usage,
and artifacts are indistinguishable at their resource boundary.

Operation reads currently check tenant and labels but lack owner enforcement and
persisted profile identity.
The [foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts)
tracks operation authority as required work before installed
acceptance. World read qualification does not establish those guarantees.

## Module layout

```text
servers/frames-mcp/src/
  contract/               public IDs, worlds, addresses, tool and result types
  contract/usage.rs       checked Task usage addresses, cursors and pages
  engine.rs               coordinate conversion
  world.rs                tree validation, hashing, and transform resolution
  state.rs                durable world, revision, and operation access
  state/reads.rs          typed world queries with SQL visibility and parent checks
  state/completion.rs     scoped matching with typed parents before SQL limits
  state/read_tests.rs     isolated database authorization and parent-integrity cases
  state/catalog_tests.rs  keyset paging and completion beyond initial result windows
  artifacts.rs            artifact-plane integration
  uris.rs                 canonical identities
  bin/server.rs           transport and MCP composition
  bin/server/
    app_state.rs
    config.rs
    completion.rs
    discovery.rs
    host.rs
    internal_auth.rs
    outputs.rs
    ownership.rs
    prompts.rs
    resources.rs
    task_extension.rs
```

## Verification

Rust tests cover world validation, canonical hashing, persistence, WGS84/ECEF
math, arbitrary static tree conversion, schemas, ownership, and tasks. The Rust
smoke creates an empty world, publishes a multi-frame tree, reads its immutable
resources, converts through the tree, runs a durable batch, and verifies
artifact and usage isolation.
