# Coordinates MCP MVP Design

This document describes the first hosted Veoveo MCP server for coordinate
systems, robot frames, CRS projection, geodesics, and basic geofence
validation. It also defines the shared coordinate contract that other Veoveo MCP
servers and services should use when they accept, persist, transform, or return
locations, frames, poses, trajectories, geofences, or transform provenance.

The intended users are AI agents controlling autonomous robots, UAVs, simulated
actors, and other digital entities.

The first implementation should be small in surface area, not small in
capability. It should give agents one authoritative place to ask "what frame is
this in?", "how do I transform it?", "is this mission geometry valid?", and
"what assumptions did that transform use?"

The platform design choice is that coordinate semantics are not local to one
server. The types in this design become the official Veoveo coordinate and frame
types for the whole stack. The `coordinates-mcp` server owns execution of
coordinate operations; the shared contract owns the shapes.

## Status

Design proposal only. No implementation exists in this workspace yet.

The target crate name is `veoveo-coordinates-mcp`, with a concise folder name:

```text
crates/coordinates-mcp
```

The hosted server slug and URI scheme should both be `coordinates`.

```text
/coordinates/mcp
coordinates://...
```

Through the gateway, local tool names are exposed under the mounted server
namespace, for example:

```text
coordinates__convert_frame
coordinates__transform_crs
```

## Goals

- Establish official coordinate, frame, pose, trajectory, geofence, and
  operation-provenance types for all Veoveo MCP servers and services.
- Provide a strongly typed MCP surface for coordinate and frame transforms used
  by autonomous agents.
- Make frame, unit, axis order, origin, datum, operation provenance, and
  approximation status explicit in every transform result.
- Support common robot and UAV frames: WGS84, ECEF, ENU, NED, FRD, and
  simulation/world frames with explicit axis conventions.
- Support real CRS projection through PROJ, including EPSG, UTM, Web Mercator,
  datum operations, and area-of-use diagnostics.
- Provide precise geodesic distance, bearing, direct destination, and
  ellipsoidal area/perimeter operations.
- Validate basic geofences and mission geometries before agents pass them to
  robot-specific execution systems.
- Use MCP resources, templates, prompts, completions, tasks, and notifications
  where they fit the domain instead of flattening discovery and state into
  tools.
- Keep the public API as Veoveo-owned typed structs and enums, not raw `proj`,
  `geo`, or `sguaba` types.
- Make heavy dependencies acceptable when they improve correctness,
  reproducibility, or performance.

## Non-Goals

- No high-frequency robot control loop. MCP is for planning, validation,
  mission transforms, geofence checks, and agent decision support, not 100 Hz
  actuator or attitude control.
- No client-facing REST API for coordinate operations.
- No hidden compatibility aliases such as `EDU`. The canonical local tangent
  plane names are `ENU` and `NED`.
- No silent approximation fallback. Missing grids, unsupported datum
  operations, invalid frame origins, or out-of-area CRS transforms should fail
  unless the request explicitly allows an approximation.
- No exposing provider or library-specific data structures as the MCP contract.
- No terrain, raster, shapefile, GeoPackage, or large GIS file IO in the MVP.
- No global spatial indexing in the MVP.
- No robust GEOS topology dependency in the MVP unless `geo` proves
  insufficient for the first geofence workflows.
- No per-server coordinate models. Other Veoveo servers should not invent
  ad hoc `lat`, `lon`, `x`, `y`, `z`, or frame JSON blobs when the shared
  coordinate contract has a controlled type for the same concept.

## Runtime Stance

This server is not optimized around a slim Docker image. It is optimized around
correctness, deterministic behavior, and throughput for autonomous systems.

The MVP should be willing to ship a powerful geospatial runtime:

- native PROJ and the PROJ database
- packaged projection/grid data required by supported transforms
- enough runtime libraries to support deterministic CRS behavior
- CPU and memory settings appropriate for batch transforms

Network grid downloads should be disabled by default. If a transform requires a
grid that is not packaged, the server should return a typed error explaining the
missing grid and the affected operation. A caller may explicitly request an
approximate fallback only where the product chooses to support one.

Native dependencies are not a smell here. Uncontrolled dependency overlap is the
thing to avoid. The MCP contract should stay small and typed while the runtime
can be capable.

## Shared Contract Ownership

Coordinate and frame types belong in the shared contract crate:

```text
crates/mcp-contract/src/coordinates.rs
veoveo_mcp_contract::coordinates
```

That module should contain only contract types and schema helpers:

- typed ids
- coordinate values
- frame definitions
- axis conventions
- poses
- trajectories
- geofence geometry
- operation provenance
- resource-reference types

It should not depend on `proj`, `sguaba`, `geo`, `geographiclib-rs`, `gdal`, or
other execution engines. Those crates belong behind server implementations.
This keeps all Veoveo services aligned on one public contract without forcing
every service to compile every geospatial engine.

The split is:

```text
veoveo-mcp-contract::coordinates
  official schemas, ids, refs, typed coordinate/frame data

coordinates-mcp
  authoritative execution: frame conversion, CRS transforms, geodesics,
  geofence validation, operation provenance resources

other Veoveo servers
  consume and emit the shared types, link to coordinates:// resources, and
  record coordinate operation refs when they depend on transformed data
```

Hard cut rule: when a Veoveo server needs controlled coordinate data, it should
adopt the shared coordinate type or add a new shared type. It should not add a
parallel local coordinate shape for compatibility with older internal callers.

## MVP Dependency Stack

Use the following as the initial implementation stack, with versions kept in the
workspace dependency table when the crate is added:

```toml
sguaba = { version = "0.10.3", features = ["serde"] }
proj = { version = "0.31.0", default-features = true }
geographiclib-rs = "0.2.7"
geo-types = { version = "0.7.19", features = ["serde"] }
geo = { version = "0.33.1", features = ["serde"] }
rayon = "1.12.0"
```

The server also uses the normal workspace dependencies for MCP, HTTP, typing,
serialization, telemetry, and auth:

```text
anyhow
axum
clap
dotenvy
rmcp
schemars
serde
serde_json
tokio
tower-http
tracing
uuid
veoveo-mcp-contract
```

### Dependency Roles

`sguaba` is the safety layer for named coordinate systems, body frames,
orientations, and rigid transforms. It should back internal frame math, but its
types should not be exposed directly in MCP schemas.

`proj` is the primary CRS and projection engine. It is the source of truth for
EPSG/CRS transforms, UTM, Web Mercator, datum operations, and area-of-use
diagnostics.

`geographiclib-rs` handles direct and inverse geodesic operations on ellipsoids,
including accurate distance, bearings, and polygon area/perimeter.

`geo-types` provides geometry boundary types for points, line strings,
polygons, and collections.

`geo` provides the first-pass geometry algorithms for point-in-polygon,
intersection, containment, and mission/geofence validation.

`rayon` supports CPU-parallel batch transforms and bulk geofence checks.

### Deferred Dependencies

Do not add these in the MVP unless a first implementation cannot meet its
contract without them:

- `gdal`: raster/vector file IO, GeoTIFF, DEMs, shapefiles, GeoPackage, COGs.
- `geos`: robust topology engine for complex invalid polygons or precision
  pathologies.
- `h3o`: global hex indexing, fleet zones, tiling, and broad-area cache keys.
- `rstar`: spatial indexing for large obstacle/geofence sets.
- `nav-types`: overlaps with `sguaba`; only add if a specific gap justifies it.
- `map_3d`: useful for direct pymap3d-style conversion, but not needed until
  `sguaba` or PROJ paths prove awkward for batch ENU/NED work.
- `geodesy`, `proj4rs`, `proj-core`: candidates for later fallback,
  cross-check, pure-Rust, or WASM profiles. PROJ is the primary MVP engine.

## Fit With Veoveo

The coordinates server follows the hosted-server pattern used by the existing
domain MCP servers:

```text
MCP client
  |
  | MCP over streamable HTTP
  v
mcp-gateway profile (/mcp/{profile})
  |-- media-mcp
  |-- timeseries-mcp
  |-- optimization-mcp
  |-- coordinates-mcp
```

The direct hosted server endpoint is internal:

```text
/coordinates/mcp
```

The gateway exposes it through configured profiles. Resource URIs remain owned
by the coordinates server:

```text
coordinates://frame/{frame_id}
coordinates://crs/{authority}/{code}
coordinates://operation/{operation_id}
coordinates://usage/task/{task_id}
```

## MCP Capabilities

The MVP should advertise:

- tools
- resources
- resource templates
- prompts
- completions
- tasks
- notifications

Resource subscriptions are not required in the MVP unless mutable frame
registries or live robot state become part of the server contract. If added
later, subscriptions should apply to explicit resources such as
`coordinates://frame/{frame_id}` or `coordinates://operation/{operation_id}`,
not hidden server state.

## Canonical Domain Model

MCP schemas should use Veoveo-owned types from
`veoveo_mcp_contract::coordinates`. Raw JSON is not appropriate for controlled
coordinate data.

The names below are design names for shared contract types. The Rust
implementation may use concise names where the module context is clear, but the
schema meaning should stay stable across servers.

### Typed Ids

```text
FrameId
CrsId
DatumId
EllipsoidId
CoordinateOperationId
TrajectoryId
GeofenceId
```

Use typed ids instead of plain strings anywhere the value controls frame,
coordinate, or transform semantics.

### Coordinate Kinds

```text
Wgs84Point
  latitude_deg
  longitude_deg
  altitude_m
  datum
  epoch

EcefPoint
  x_m
  y_m
  z_m
  datum
  epoch

EnuOffset
  east_m
  north_m
  up_m
  origin_frame_id

NedOffset
  north_m
  east_m
  down_m
  origin_frame_id

FrdVector
  forward_m
  right_m
  down_m
  body_frame_id

ProjectedPoint
  crs
  x
  y
  z?
  units
```

`latitude_deg` and `longitude_deg` are degrees at the MCP boundary. Internal
engines may use radians, but the public API must not make agents guess.

### Frame Kinds

```text
EarthFixedFrame
LocalTangentFrame
BodyFrame
SimulationWorldFrame
ProjectedCrsFrame
```

Every local frame carries:

- `frame_id`
- `kind`
- `axis_convention`
- `origin`
- `datum`
- `epoch`
- optional `parent_frame_id`
- optional orientation relative to parent
- `created_at`
- owner/audit metadata

For digital entities and simulation worlds, the frame must also declare:

- handedness: left-handed or right-handed
- up axis
- forward axis
- unit scale
- parent transform

### Pose, Trajectory, and Geofence Kinds

These types are included in the shared contract because optimization,
timeseries, simulation, Rerun artifacts, and future robot servers all need the
same semantics.

```text
Orientation3
  representation
  frame_id
  quaternion?
  yaw_pitch_roll_deg?

Pose3
  frame_id
  position
  orientation
  covariance?

TrajectoryPoint
  time?
  pose
  velocity?

Trajectory3
  trajectory_id?
  frame_id
  points
  interpolation?

GeofenceGeometry
  geofence_id?
  frame_id
  rule
  geometry
```

The first MVP can keep trajectory and geofence geometry small. The important
part is that every pose, trajectory, and geofence declares a frame, and that
frame is a shared contract type.

### Operation Provenance

Every transform result should include a compact provenance object:

```text
operation_id
engine
source_frame
target_frame
crs_operation
area_of_use
accuracy_m?
used_grid_ids
approximation
warnings
```

This object is how agents know whether a result is safe to pass onward.

Servers that consume transformed coordinates should store a lightweight
operation reference:

```text
CoordinateOperationRef
  operation_id
  operation_uri
  source_frame
  target_frame
  created_at
```

The full provenance remains available from
`coordinates://operation/{operation_id}`.

## Tool Model

The MVP should expose a small direct tool set plus task support for bulk work.

| Tool | Invocation | Purpose |
|---|---|---|
| `convert_frame` | direct or task | Convert points/vectors between WGS84, ECEF, ENU, NED, FRD, and known simulation frames. |
| `transform_crs` | direct or task | Transform points or geometries between CRS identifiers using PROJ. |
| `derive_local_frame` | direct | Create or preview a local ENU/NED frame from a WGS84 origin and axis convention. |
| `geodesic_inverse` | direct | Compute ellipsoidal distance and forward/reverse bearings between WGS84 points. |
| `geodesic_direct` | direct | Compute destination point from WGS84 origin, azimuth, and distance. |
| `validate_geofence` | direct or task | Validate point/path/polygon mission geometry against a geofence. |

Small inputs may run synchronously. Batch inputs should be task-capable so
clients can request progress, cancellation, and durable result retrieval. The
server may require task invocation above a documented input-size threshold.

## Tool Details

### `convert_frame`

Converts coordinates between controlled frame types. The request includes:

- source frame
- target frame
- points or vectors
- optional origin or frame id
- desired output units
- explicit approximation policy

The response includes:

- converted points/vectors
- operation provenance
- warnings

This is the main safety-critical tool. It should reject ambiguous input such as
`x/y/z` without a frame id, local offsets without an origin, and body-frame
vectors without orientation.

### `transform_crs`

Transforms points or geometries between CRS identifiers. The request includes:

- `from_crs`, for example `EPSG:4326`
- `to_crs`, for example `EPSG:3857`
- point or geometry inputs
- optional area of interest
- approximation policy

The response includes:

- transformed points or geometries
- selected PROJ operation
- area-of-use diagnostics
- grid usage
- accuracy estimate when available

Axis order should be canonicalized in the MCP contract. Use explicit field
names like `longitude_deg` and `latitude_deg` for geographic input instead of
accepting unlabeled coordinate arrays.

### `derive_local_frame`

Creates or previews a local tangent frame. The request includes:

- origin WGS84 point
- frame kind: `ENU` or `NED`
- optional frame id
- optional parent frame
- optional epoch

The result returns a frame resource link and the frame definition. If the tool
is called in preview mode, it returns the definition without persisting it.

### `geodesic_inverse`

Computes distance and bearings between two WGS84 points on an ellipsoid.

The response includes:

- distance meters
- initial bearing degrees
- final bearing degrees
- ellipsoid
- algorithm engine

### `geodesic_direct`

Computes a destination from:

- WGS84 origin
- azimuth degrees
- distance meters
- ellipsoid

The response includes:

- destination WGS84 point
- reverse/final bearing
- operation provenance

### `validate_geofence`

Validates mission geometry against a geofence. The MVP should support:

- point inside/outside
- path intersects forbidden polygon
- path remains inside allowed polygon
- polygon validity checks
- CRS/frame consistency checks before geometry operations

The response includes:

- `valid`
- violations
- nearest offending segment or point when available
- transformed working CRS/frame
- operation provenance

Complex invalid topology cases may be deferred until `geos` is added.

## Canonical Resources

Resources are the stable nouns. Tools should return resource links when they
create reusable identities.

```text
coordinates://frames
```

List visible frames for the principal.

```text
coordinates://frame/{frame_id}
```

Typed frame definition, owner, origin, parent, axis convention, datum, epoch,
and creation metadata.

```text
coordinates://crs/{authority}/{code}
```

CRS metadata: authority, code, name, axis metadata, unit metadata, area of use,
and projection/proj diagnostics when available.

```text
coordinates://operation/{operation_id}
```

Recorded transform operation provenance for a reusable or task-produced result.

```text
coordinates://artifact/{sha256}
```

Immutable artifact bytes for bulk transform outputs, validation reports, or
GeoJSON/WKT exports produced by a task.

```text
coordinates://usage/task/{task_id}
```

Usage and execution metrics for task-based bulk operations.

## Cross-Server Adoption

The coordinates contract is platform-level. These examples are not separate
implementations; they are adoption points for the same shared types.

### Optimization MCP

`optimization-mcp` should use shared coordinate types for mission state,
assignment inputs, selected options, plan outputs, trajectory references,
geofence references, transform provenance, and Rerun worldline frame metadata.

The first optimization benefit should be validation and provenance, not a larger
solver:

- `PlanningAgent`, `PlanningTask`, and `PlanningOption` may carry optional
  shared `Pose3`, `FrameId`, `Trajectory3`, `GeofenceGeometry`, or
  `CoordinateOperationRef` fields.
- `PlanOutput` should preserve coordinate operation refs that influenced the
  selected options.
- Spatial costs, travel times, and geofence pass/fail facts should be produced
  by `coordinates-mcp` or precomputed data, then consumed by optimization as
  typed scores, resources, constraints, or validation refs.
- The solver should not own CRS projection, datum transforms, or frame
  conversion logic.

### Timeseries MCP

Timeseries artifacts that represent physical or simulated measurements should
use shared frame ids and coordinate types in metadata, Rerun entity paths, and
artifact summaries. A forecast that predicts position should identify the frame
and operation provenance behind the series.

### DuckDB MCP

DuckDB remains a SQL engine, but table mappings and exported artifact metadata
can reference shared coordinate schemas. When a table contains latitude,
longitude, projected coordinates, poses, or trajectories, the metadata should
say which shared type and frame the columns represent.

### Media and 3D Assets

Generated 3D assets, scene layouts, textures projected onto terrain, or visual
annotations should use shared frame metadata when they represent real or
simulated spatial objects. Media generation itself does not become a coordinate
engine.

### Gateway, Policy, and Audit

Gateway policy and audit should treat `coordinates://...` resource identities as
first-class evidence. Other servers should include coordinate operation refs in
task payloads and artifact metadata so audit trails can connect planning,
simulation, media, and timeseries outputs back to the coordinate assumptions
they used.

## Resource Templates

The MVP should publish templates for:

```text
coordinates://frame/{frame_id}
coordinates://crs/{authority}/{code}
coordinates://operation/{operation_id}
coordinates://artifact/{sha256}
coordinates://usage/task/{task_id}
```

## Completions

Completions should make the server agent-friendly without adding search tools:

- common CRS identifiers, especially `EPSG:4326`, `EPSG:4978`, `EPSG:3857`,
  and UTM zones
- known frame ids visible to the principal
- frame kinds: `WGS84`, `ECEF`, `ENU`, `NED`, `FRD`, `SimulationWorld`
- ellipsoid ids

## Prompts

The MVP should include prompts that help agents use the server safely:

| Prompt | Purpose |
|---|---|
| `coordinates-frame-audit` | Review a mission or tool request for frame/unit/datum assumptions. |
| `coordinates-local-frame-select` | Choose ENU or NED and origin strategy for a robot workflow. |
| `coordinates-geofence-review` | Inspect geofence and mission geometry before execution. |
| `coordinates-transform-explain` | Explain a transform chain and its risks in operator-readable language. |

## Error Model

Errors should be typed and actionable. Important cases:

- unknown frame id
- unknown or unsupported CRS
- ambiguous axis order
- missing local-frame origin
- missing body-frame orientation
- datum/grid unavailable
- CRS operation outside area of use
- approximation not allowed
- invalid latitude/longitude/altitude
- invalid polygon or self-intersection
- geometry/frame mismatch
- batch too large for direct invocation; task required

Do not coerce invalid coordinates or quietly swap axes.

## Testing Strategy

Unit tests should cover:

- WGS84/ECEF/local frame round trips
- ENU/NED sign and axis conventions
- FRD/body-frame orientation handling
- EPSG:4326 to EPSG:3857 and back
- UTM zone transforms
- geodesic direct/inverse known examples
- geofence containment and path intersection
- typed error cases for ambiguous or invalid input

Golden tests should include known coordinate fixtures and tolerances stated in
meters or degrees.

Smoke tests should be Rust smoke harnesses, not Justfile shell scripts. They
should start the hosted MCP server, connect with the conformance client, list
resources/templates/prompts/completions/tools, call the direct tools, and run at
least one task-based batch transform.

## Implementation Shape

Keep the binary entrypoint thin:

```text
crates/mcp-contract/src/coordinates.rs
crates/coordinates-mcp/src/bin/server.rs
crates/coordinates-mcp/src/lib.rs
crates/coordinates-mcp/src/types.rs
crates/coordinates-mcp/src/frames.rs
crates/coordinates-mcp/src/crs.rs
crates/coordinates-mcp/src/geodesic.rs
crates/coordinates-mcp/src/geofence.rs
crates/coordinates-mcp/src/tasks.rs
crates/coordinates-mcp/src/resources.rs
crates/coordinates-mcp/src/prompts.rs
crates/coordinates-mcp/src/completions.rs
crates/coordinates-mcp/src/uris.rs
```

Split earlier if responsibilities compound. Do not grow one mixed file that
contains routes, auth, frame math, CRS projection, tasks, prompts, resources,
and tests.

## Future Extensions

After the MVP proves the contract, likely additions are:

- Adoption of the shared coordinate contract across existing hosted servers.
- `gdal` for DEM, GeoTIFF, COG, shapefile, and GeoPackage workflows.
- `geos` for robust topology and repair of complex mission polygons.
- `rstar` for obstacle and geofence indexing at scale.
- `h3o` for broad-area partitioning, cell coverage, and fleet-zone cache keys.
- Rerun artifact output for transform chains, trajectories, geofence failures,
  and operator visual review.
- Live frame resources or subscriptions for robot state integration.
- Compatibility helper tools for clients that cannot surface resources,
  completions, prompts, or tasks, implemented only as projections over the
  canonical protocol surfaces.
