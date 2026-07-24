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

Frames owns complete spatial-frame worlds and bounded coordinate conversion.
Map MCP owns Earth geography, projected coordinate reference systems,
geodesics, geofences, and routing. High-rate transforms remain in live streams
or governed recordings.

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
| `convert_frame` | direct | Bounded typed conversion with durable provenance. |
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

`batch_transform` uses the final Veoveo task extension. Direct calls are
rejected. Large results go through the artifact plane.

## Resources

```text
frames://worlds
frames://world/{world_id}
frames://world/{world_id}/revision/{revision_id}
frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}
frames://operation/{operation_id}
frames://artifact/{artifact_id}
frames://usage
frames://usage/task/{task_id}
```

The world resource carries mutable head metadata. Revision and frame resources
are immutable. Operations, artifacts, tasks, and usage retain owner, tenant,
profile, and data-label isolation.

World identifiers support completion. Resource lists are paginated. The server
emits resource-update notifications when mutable state changes.

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

## Persistence

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

## Module layout

```text
servers/frames-mcp/src/
  contract.rs             tool and result types
  engine.rs               coordinate conversion
  world.rs                tree validation, hashing, and transform resolution
  state.rs                durable world, revision, and operation access
  artifacts.rs            artifact-plane integration
  uris.rs                 canonical identities
  bin/server.rs           transport and MCP composition
  bin/server/
    app_state.rs
    config.rs
    host.rs
    internal_auth.rs
    outputs.rs
    ownership.rs
    prompts.rs
    task_extension.rs
```

## Verification

Rust tests cover world validation, canonical hashing, persistence, WGS84/ECEF
math, arbitrary static tree conversion, schemas, ownership, and tasks. The Rust
smoke creates an empty world, publishes a multi-frame tree, reads its immutable
resources, converts through the tree, runs a durable batch, and verifies
artifact and usage isolation.
