# Map Routing

## Standards And Protocols

This design uses the Map server’s [standards profile](DESIGN.md#standards-and-protocols),
including MCP 2026-07-28, RFC 9562 identifiers and the Veoveo Frames and Work Context
contracts. Routing engines and supported network formats are identified below.

## Routing

Every route request names an immutable mobility profile version, endpoints,
departure time, objective, constraints, alternatives, and a data policy.
Endpoints may be WGS84 positions, location ids, or facility ids.

The planner resolves active releases whose source families are compatible with
the profile and whose validity contains the departure time. It captures an
operational snapshot, applies effective restrictions, and persists route
provenance. Missing coverage fails explicitly.

### Land

Human and road-vehicle profiles use the supervised Valhalla engine. The adapter
maps the controlled profile to pedestrian, bicycle, motor-scooter, motorcycle,
auto, truck, or bus costing and validates profile values against the engine's
supported limits. Valhalla produces route geometry, maneuver instructions,
distance, duration, alternatives, and land isochrones.

### Governed Networks

Off-road, rail, surface-vessel, subsurface-vessel, fixed-wing, rotorcraft, and
UAS profiles use explicit activated LineString edges for their map family. The
planner connects each exact endpoint to the nearest governed node within 10 km whose
connector satisfies the mobility envelope and avoids excluded areas. It checks ingress
and egress in their direction of travel, including distinct climb and descent limits.
An intermediate waypoint can use different nodes for arrival and departure. Selection
fails when no connector qualifies; A* must then find a path between the selected nodes.
The planner retains the connector segments in the returned geometry and costs them at
the profile's preferred, nominal, or cruise speed. Before persistence, it densifies
governed edges and exact-endpoint connectors to the profile's maximum segment length.
The exact endpoints remain unchanged, and inserted points interpolate ellipsoidal
height. The planner verifies consistent node geometry, applies avoided areas, and runs
A* for fastest or shortest objectives. Planning requires connected activated edges.
If restriction effects cannot be fully resolved, route creation returns
`planning_advisory` only when the caller permits it in the data policy.

Routing selects its release IDs and compatible map families through
`MapCatalog::routing_authority`. One SurrealQL statement follows each tenant's active
pointer to its release and source. All three records must belong to the tenant and
agree on the dataset. The release must be active and valid at departure; the source
must be enabled and share a map family with the mobility profile. SQL computes that
family intersection. A missing parent or mismatched reference cannot supply routing
coverage. This selection also serves matrices, travel models and reachable areas.

The reader checks the selected documents against the indexed IDs, versions, release
validity, active state and source family/enabled fields. It also checks pointer identity
and a positive pointer version. A selected malformed record fails planning. The internal
result contains the complete matching release and family sets, with a five-second
database deadline. Public catalog paging has its own resource contract.

Preflight retained pointer/source/release relationships before replacing Map replicas
together, after their active routing Tasks settle. Stored formats are unchanged. Preserve
rejected rows for repair; the reader never rewrites them. Rollback restores the prior
binary and its selection policy against the same data. Operators must accept that
policy difference before restoring traffic. Installed routing and reverse/forward
replacement remain required for acceptance.

### Restrictions And Validation

Map owns restriction selection in `catalog/restrictions.rs`. SQL applies the tenant
before exact lookup, completion and page limits. Restrictions are shared within the
tenant; their creator does not narrow read access. Routing, travel-model construction,
reachable-area calculation, route validation and spatial derivation select their
mobility family at the operation time. Corridor inspection selects all families.
The database excludes withdrawn records and uses the half-open interval
`valid_from <= operation_time < valid_until`, with an absent end treated as unbounded.
Queries have a five-second database deadline. An operation that selects more than
10,000 effective restrictions fails before applying any constraints. It never plans
with a truncated restriction set. Geometry intersection and dimensional validation
belong to the spatial and routing algorithms after this temporal/family selection.

Selected documents must agree with their record ID and indexed identity, kind, effect,
mobility families, validity, withdrawal reference and record version. A disagreement
fails the read. Exact resource addresses use `MapRestrictionUri` with `RestrictionId`;
collection addresses use `MapRestrictionsUri` and its typed cursor. The collection
returns `items`, `limit: 100` and nullable `next_cursor` in restriction ID order. Items
contain compact metadata and a typed resource address; exact reads provide geometry.
The summary decoder checks ID/URI agreement, validity and record version. Each
continuation repeats tenant selection. The cursor carries position and grants no access.
Its decoded fields are version 1, collection `map://restrictions` and `after`.

Upgrade Map and restriction collection consumers together after active routing and
spatial Tasks settle. Clients replace the array response with page traversal. Preflight
retained restriction IDs, cancellation references and document/index agreement; preserve
rejected records for operator repair before reopening traffic. This change does not
rewrite stored rows. Rollback restores the prior Map/consumer pair against the same data
and discards new cursors. Operators must accept the prior reader's weaker retained-data
checks before restoring traffic. Installed retained-data recovery and reverse/forward
replacement are required for acceptance.

Effective restrictions target mobility families and carry typed effects,
geometry, authority, and validity. Prohibitions become avoided areas during
planning. Route creation checks the complete primary geometry and every alternative
against the planning envelope and active restrictions before promoting status or
persisting a route. Later validation and handoff use the same spatial checks. Purely
vertical segments have a 90-degree climb or descent angle. Lateral clearance expands
the checked route corridor.
Typed dimensional, mass, speed, depth, altitude, and reserve limits resolve
against the selected profile. A missing or incompatible vertical reference
fails closed.

Routes pin base release ids, one operational snapshot id, planner version, cost
model version, restriction ids, facilities, and validation identity. Release
changes and restriction withdrawal invalidate dependent routes while preserving
the original record for review.

Map is also the sole producer of the versioned
`veoveo.ai/map-route-handoff/v1` cross-server profile. A handoff is prepared
from one persisted route only after Map rejects stale, invalidated, or
unavailable state and repeats complete mobility and restriction validation. It
contains the Map route identity and digest, exact mobility-profile identity,
execution-neutral WGS84 path, validation identity, operational snapshot,
release provenance, and restriction identities. A consuming domain may add its
own actuation constraints, but it does not resolve places, plan a replacement
path, or reinterpret Map restrictions.

The contract library exposes `MapRouteHandoffBuilder` and a checked
`MapRouteHandoff`. Construction and JSON decoding validate the supported schema,
route state, typed addresses and provenance IDs, lowercase SHA-256, unique release
and restriction references, path bounds and validation/preparation order. The path
contains 2–10,000 valid WGS84 positions without consecutive duplicates. Ground paths
may omit height; UAV requires it when admitting flight. A handoff conveys no execution
authority. Exact retained route reads reject a URI that names a different route.

### Durable Routing Operations

Single routes, route matrices, and reachable areas use the MCP Task API. Each
operation persists its result before the task reaches `completed`. The task
record carries its owner, lease, progress, terminal payload, retention pins,
and recovery request. A client can poll or subscribe, cancel active work, and
read the resulting `map://` resource without holding the initiating request
open.

Route matrices are limited to 20 origins, 20 destinations, and 400 cells.
Individual unavailable cells are typed as unavailable; the entire matrix fails
when no pair has supported coverage.

`build_travel_model` serves a different contract from `route_matrix`. It
accepts one shared ordered set of up to 128 locations and as many as 64
vehicle types. Every vehicle type binds a stable Optimization-facing ID to one
exact Map mobility-profile version. The builder resolves and persists one
governed operational snapshot per requested vehicle type, then performs one
Valhalla many-to-many request. It publishes square objective-cost and
transit-time matrices with the same location order for every type.

Duration or distance may be the objective metric. Transit time is always
published separately. The time model is static or uses one invariant local
departure across all matrix cells. Per-origin dynamic departure propagation is
outside this artifact profile because it would make one cell depend on an
unknown upstream route sequence.

The artifact records `veoveo.ai/travel-model-artifact/v1`, the exact
`map://travel-model/{travel_model_id}` identity, unavailable cell indices, and
the profile release, operational-snapshot, planner, cost-model, and matrix
algorithm provenance. The Map record retains its neutral `artifact://`
manifest URI. Optimization requires both identities and rejects a manifest
that does not attest the requested Map resource.

Map's contract feature owns `MapTravelModelUri`; Optimization imports that type without
Map's runtime or MCP integration. Travel-model producers retain the type through routing,
Artifact publication and Task results. A record's URI must contain its declared model ID,
and its neutral manifest must identify the Artifact in its metadata.

`TravelModelReads` selects exact results, collection pages and completion candidates in
SurrealDB. Indexed owner, tenant, profile and Work Context must agree with the Task owner
envelope and retained request identity. Result ownership and context must also match.
The Task must have succeeded without an error result, and its request and result must
name the same model. These predicates run before grouping, ordering and limits. Exact
reads reject duplicate identities. A malformed selected record fails the read.

`map://travel-models` returns `items`, `limit` and an optional `next_cursor`. The page
contains at most 100 records; `map://travel-models{?cursor}` continues in native Task ID
order. Every continuation repeats current-authority checks. The cursor contains version
1 and a native Task UUIDv7, encoded as lowercase hex JSON. Discovery stays fixed, and
completion returns up to 100 typed model IDs with one SQL lookahead row.

Upgrade Map and its collection consumers together after active travel-model Tasks settle.
Clients must replace the collection's array reader with page traversal. Preflight retained
Task request/result ownership, identity spellings and Artifact parents before reopening
traffic. The supported retained result field is `structuredContent`; preserve rejected
records, including any `structured_content`-only rows, for operator review. This reader
does not rewrite stored Tasks or immutable Artifacts. Rollback restores the prior Map and
consumer pair against the same data, discards new collection cursors and restores the
earlier read policy. Operator approval of that policy difference precedes traffic.
Native checks cover contract consumption and separate Store connections. Installed
retained-data recovery, collection traversal and reverse/forward replacement remain
required for installation acceptance.

Reachable areas are Valhalla isochrones for human and road profiles. All four
operations renew leases while running and resume after a server restart.
