# Map MCP Design

This document is the canonical design and operational contract for the
`map-mcp` crate.

`map-mcp` is Veoveo's Earth geography and logistics-routing domain. Agents use
one strongly typed MCP surface to find places, inspect facilities and borders,
work with coordinates, apply transport restrictions, calculate routes, build
matrices, publish cuOpt-ready travel models, inspect reachable areas, and
author governed feature layers. Source administration runs through the same
MCP surface: scoped tools for mutations, `map://` resources for reads, and one
permission-aware MCP App that renders immutable compositions
(see `mcp/apps-extension/DESIGN.md`).

Map MCP is also a reusable capability for other MCP servers. Consumers use the
canonical `map://` resources and Map tools through the gateway, or declare a
typed App dependency when their own App needs Map data. They do not connect to
Map storage, private HTTP routes, or renderer internals. The installation's
profile, policy, scopes, tenant, labels, and Work Context remain authoritative
for every consumer.

## Status

Implemented in this workspace.

The implementation includes the Map domain contract, SurrealDB records,
tenant-scoped DuckDB Spatial tables, a supervised Valhalla land engine, a
governed network planner, source acquisition, release activation, MCP discovery
surfaces, administrative MCP tools, the Map Explorer MCP App, gateway
proxying, Helm, offline image registration, governed spatial and raster
derivations, and the immutable travel-model handoff to Optimization MCP.

The canonical service identity is:

```text
crate       veoveo-map-mcp
folder      servers/map-mcp
slug        map
URI scheme  map
MCP         /map/mcp
workspace   ui://map/workspace.html
health      /map/healthz
```

Gateway-mounted tools use names such as `map__route`. Resource identities keep
the `map://` scheme.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with tools, resources and templates, prompts, completions, subscriptions, notifications, and typed structured content. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; acquisition, routing, import, export, publication, and vector-product operations use durable task semantics where declared. |
| [MCP Apps SEP-1865](../../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; `ui://map/workspace.html` uses the sandboxed host bridge and canonical Map tools and resources. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | MCP schemas and immutable authored-layer property contracts. Layer schemas reject remote references. |
| [Veoveo resource components](../../platform/types/DESIGN.md#concrete-resource-components) and URI Template RFC 6570 | Authoring metadata pages use the shared URL parser and builder with typed IDs and cursors. Discovery declares form-style parent and cursor query parameters. Other resource families are tracked for migration in the foundations plan. |
| WGS 84 and EPSG identifiers | Longitude, latitude, and ellipsoidal height are the geographic exchange. PROJ handles bounded projected-CRS conversion; EPSG:4978 and vertical transformations are outside that 2D operation. |
| SurrealDB 3.3.0 | Internal catalog queries, transactions, LIVE/change-feed delivery, and [JSON decoding](https://surrealdb.com/docs/reference/query-language/functions/database-functions/encoding#encodingjsondecode) for selection against complete route documents. |
| Map travel-model resource profile | Map-owned component builders, RFC-variant UUIDv5/v7 IDs in lowercase hyphenated spelling, 100-item SQL pages and version 1 hex-encoded JSON cursors over native UUIDv7 Task positions. Exact and collection templates follow RFC 6570. |
| Map mobility-profile resource profile | Map-owned component builders and canonical RFC-variant UUIDv5/v7 IDs. Profile versions are integers in 1..=9223372036854775807. Collections use 100-item pages and version 1 hex-encoded JSON cursors bound to `map://mobility-profiles` and an ID/version position. Exact and collection templates follow RFC 6570. |
| Map source resource profile | Map-owned component builders, canonical RFC-variant UUIDv5/v7 source IDs, checked public summaries, 100-item pages and version 1 hex-encoded JSON cursors bound to `map://sources`. Exact and collection templates follow RFC 6570. |
| Map restriction resource profile | Map-owned component builders, canonical RFC-variant UUIDv5/v7 IDs, 100-item pages and version 1 hex-encoded JSON cursors bound to `map://restrictions`. Exact and collection templates follow RFC 6570. |
| Map product resource profile | Typed dataset release, source feature, raster, raster derivation, spatial derivation and route addresses. IDs require RFC-variant UUIDv5/v7 in lowercase hyphenated spelling; parents are typed components and discovery uses the same RFC 6570 templates. |
| DuckDB 1.5.6 and DuckDB Spatial | Map selects `geometry_always_xy = true`, constructs longitude/latitude as `POINT_2D`, and uses one materialized spherical-distance score per candidate. |
| [GeoJSON RFC 7946](https://www.rfc-editor.org/rfc/rfc7946.html), OGC JSON-FG 1.0, and [GeoJSON Text Sequences RFC 8142](https://www.rfc-editor.org/rfc/rfc8142.html) | Canonical feature geometry, semantic feature types, valid time, bulk import, and immutable export. |
| [OGC GeoPackage 1.4](https://www.geopackage.org/spec140/) | Bounded vector-table inspection, selected-table import, and one-table export. Raster tiles, related tables, and non-linear or measured geometry are outside this profile. GDAL 3.13.3 performs full conformance validation and controlled conversion. |
| OGC CQL2 1.0 | Bounded Basic CQL2-JSON predicates over top-level authored properties. Arbitrary CQL2 and spatial predicates are not claimed. |
| GeoParquet 1.0.0 | WKB primary geometry and verified `geo` metadata for immutable analytical products. |
| OGC Cloud Optimized GeoTIFF 1.0 and GeoTIFF 1.1 | Environmental sources normalize to immutable COG products with explicit CRS, affine transform, extent, resolution, bands, units, nodata values, value interpretation, checksum, license, and attribution. |
| Veoveo spatial derivation profile `map-spatial-local-equirectangular-wgs84-v1` | Advisory operations use a WGS84 local equirectangular plane with the exact 6,371,008.8 m mean Earth radius. Each operation is bounded to two degrees on either axis and records its origin and algorithm revision. This is a repository-owned profile, not a projected-CRS standard. |
| Mapbox Vector Tile 2.1, MapLibre Style 8, MapLibre GL JS 6.6.0, and the OpenFreeMap hosted style profile | Deterministic bounded XYZ tile bundles, safe literal presentation styles, a keyless vector basemap, and a self-contained WebGL2 composition viewer. |
| OSM PBF, GTFS Schedule, S-57/S-100, AIXM, and FAA NASR exchange sets | Registered acquisition adapters accept only their documented snapshot profiles. Product-specific operational validation remains explicit. |
| HTTPS and mounted exchange sets | Registered sources control hosts, redirects, media types, credentials, byte limits, elapsed time, and filesystem roots before an adapter runs. |
| Valhalla HTTP/JSON | A supervised loopback-only routing-engine protocol. The travel-model adapter uses one concise many-to-many request per requested vehicle type. It is an internal projection, never a public Map API. |
| `veoveo.ai/travel-model-artifact/v1` | Repository-owned immutable exchange from Map to Optimization. It carries shared location order, per-vehicle-type cost and transit-time matrices, unavailable cells, and exact Map resource attestation. |

The workspace pins `geo` 0.32.0 because SurrealDB 3.2 uses the same release
line and requires `i_overlay <4.1`. `geo` 0.33.1 requires `i_overlay >=4.5`,
which Cargo cannot resolve in this workspace. The selected release contains
the signed buffer operation used by the spatial profile.

## Domain Scope

Map answers where something is on Earth and whether a specific mobility
profile can travel there. It provides:

- WGS84 geography, projected CRS transformations, and ellipsoidal geodesics;
- locations, facilities, boundaries, map datasets, and effective restrictions;
- versioned human and vehicle mobility profiles;
- route feasibility, geometry, cost, provenance, matrices, and reachable areas;
- immutable heterogeneous travel models for cuOpt routing;
- advisory spatial geometry and complete-route mobility validation;
- governed source acquisition and immutable release activation;
- Work Context-owned GeoJSON and JSON-FG feature authoring, revision, query,
  tombstone, restore, and publication;
- map-owned analytical and routing-engine projections.

Optimization consumes attested Map travel models to compose fleet selection,
assignments, schedules, stop sequences, and multi-asset transfers.
Map embeds the hardened DuckDB runtime as a library and owns its analytical
database and SQL policy.

## Architecture

### Public Types And Authorization

`contract/product_uri.rs` owns the six exact Map product address families consumed by
View. Builders require each segment's domain ID, and parsers admit the complete route
through the foundational URI library. Dataset release and source feature addresses
retain their typed parent identities. Reads use those identities when calling the
domain readers; SQL owns tenant, caller and parent selection. Derivation and route
producers use the same builders as discovery. Their broader result DTO relationship
checks remain tracked in the foundations plan.

The server's public contract owns `MapScope`, its closed authorization vocabulary.
Handlers and Tasks require that enum and share one grant check. Configurable
administrative admission accepts a validated `ScopeName` and defaults to `MapScope::Admin`.
Unrelated server scopes in the caller's grants remain valid. `MapAccessContext`
carries database identity and is separate from this vocabulary.

`MapMetadataRequest` implements the foundational `ResourceAddress` trait for authored
layer, publication, product, and composition collections. Variants carry their specific
ID types and optional parent selection. The shared URI library parses and encodes query
components. `MapMetadataCursor` validates its version-1 envelope and typed position,
then checks the collection and parent again when resumed. Serialization emits a hex
string. A cursor grants no access; every page applies current database visibility.

Artifact references in releases, raster products and derivations, publications,
layer products, compositions, and travel-model outputs use the Artifact owner's
[`ArtifactUri`](../../platform/artifacts/contract/DESIGN.md#wire-and-construction).
Release and raster validators require the neutral plane variant. Publication and
product Store inputs preserve that type until driver serialization. Stored JSON keeps
the same string fields; the Artifact owner's compatibility profile defines admission
and retained-data handling. These types confer no access to bytes or parent records.
Retained Map documents must also pass their domain validators before an upgrade;
these enforce the neutral-only rule beyond the shared Artifact address profile.

The library exposes `contract`, `runtime`, and `mcp` features. Cross-server consumers
use `default-features = false, features = ["contract"]`. This builds the public model,
including CRS, datum, and ellipsoid IDs, without MCP, database, GPU, network client,
or async runtime dependencies. Geometry validation uses the workspace's geo 0.32.0
profile with default features disabled. Chrono supplies date/time values without its
clock feature; UUID supplies the existing generated and stable ID profiles.
The runtime owns engines and persistence. The MCP adapter owns App HTML and hosted
protocol wiring; the default `mcp` feature includes the runtime.

Remaining resource families and typed Store query keys are work in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

The MCP adapter separates resource reads in `src/mcp/resources.rs` from descriptors
and templates in `src/mcp/discovery.rs`. The authoring metadata adapter delegates
parsing to the public contract and executes the selected catalog query.

### Hosted Process

```text
agent
  |
  | MCP
  v
mcp-gateway
  |
  | signed internal identity
  v
map-mcp container
  |-- MCP protocol, feature authoring, durable tasks, and MCP App views
  |-- source catalog and release service
  |-- PROJ and GeographicLib calculations
  |-- DuckDB Spatial analytical projection
  |-- supervised loopback Valhalla process
  |-- immutable Optimization travel-model builder
  |-- governed network planner
  |-- Python acquisition application
  |-- GDAL and Osmium source utilities
  |-- SurrealDB platform store
  `-- shared artifact plane
```

The Rust server is PID 1. Valhalla listens only on loopback and is supervised by
the server. The Python acquisition application is invoked as a bounded child
process. These components ship in one image and share one persistent Map
volume.

## Canonical Map Families

The contract has seven map families.

| Map family | Meaning | Runtime use |
|---|---|---|
| `road_street` | motor-road network | Valhalla road routing |
| `active_mobility` | walking, hiking, cycling, and accessibility paths | Valhalla human routing |
| `rail_transit` | governed rail network | explicit network edges |
| `off_road_terrain` | traversable terrain and off-road corridors | explicit network edges |
| `maritime` | surface and subsurface corridors | explicit network edges |
| `aviation` | air corridors and operational routes | explicit network edges |
| `intermodal` | terminals and transfer relationships | facility and transfer metadata |

Shared layers include names, facilities, administrative borders, hazards,
restrictions, elevation, bathymetry, weather, tides, currents, and traffic.
They constrain one or more families rather than becoming separate route
engines.

Intermodal is a first-class compatibility and transfer family. A single
`route` request still uses one mobility profile. Multi-asset transfer selection
is assembled from Map legs by Optimization.

## Mobility Profiles

The canonical `MobilityProfile` has one human family and eight vehicle
families. Versioned profile instances carry actual dimensions, performance,
energy, permissions, validity, and operational constraints.

| Family | Initial controlled modes or classes |
|---|---|
| Human | walk, run, hike, manual mobility aid, powered mobility aid |
| Road vehicle | bicycle, powered two-wheeler, passenger car, light commercial, rigid truck, articulated truck, bus or coach, emergency service |
| Off-road vehicle | wheeled, tracked, ATV or UTV, heavy equipment, uncrewed ground vehicle |
| Rail vehicle | light rail or metro, passenger train, freight train, maintenance train |
| Surface vessel | small craft, cargo, tanker, passenger ferry, tug or workboat, fishing or service, uncrewed surface vessel |
| Subsurface vessel | submarine, autonomous underwater vehicle, remotely operated vehicle, underwater glider |
| Fixed wing | light, regional transport, heavy cargo, amphibious |
| Rotorcraft | helicopter, heavy-lift helicopter, tiltrotor |
| UAS | multirotor, fixed wing, hybrid VTOL |

This gives 9 profile families and 43 initial controlled class or movement
values. The number of profile instances is unbounded. A deployment can create
separate versions for its people, cars, trucks, ships, aircraft, accessibility
needs, and mission rules without changing the enum.

Profile fields remain specific to the domain. Examples include axle loads and
hazardous cargo for road vehicles, ground pressure and water depth for off-road
vehicles, gauge and electrification for rail, draft and under-keel clearance
for vessels, and runway, ceiling, reserve, navigation, and airspace permissions
for aircraft.

Every family also carries one `MobilityPlanningEnvelope`. It sets the minimum
speed, optional minimum turn radius, climb and descent bounds, lateral and
vertical clearance, optional ceiling and range, route-point and segment limits,
allowed terrain classes, and allowed restriction kinds. A family-specific
physical range or aircraft service ceiling remains authoritative when it is
more restrictive than the common envelope.

### Mobility Catalog Reads

Map owns profile reads in `catalog/mobility.rs`. Exact reads bind both profile ID and
version. SQL applies tenant selection before limits; catalog pages order by profile ID
and numeric version. Creator identity and metadata labels are attribution, not additional
read restrictions. Catalogs include historical versions outside their validity interval;
planning operations check validity at the requested departure time.

`MobilityProfileVersion` admits a positive value within the Store's signed integer range.
Map uses it in profile metadata, route and matrix requests, spatial derivations, travel-model
requests and provenance. Database-driver bindings and wire serialization perform the
numeric conversion. `MapMobilityProfileUri` requires both typed ID and version, and
Map's route handoff carries that address. UAV grants and mission plans import these
types from the contract-only library. The flight client keeps profile IDs and versions
typed until request serialization. Public numeric fields and valid URI spellings are
unchanged.

`MapMobilityProfilesUri` carries an optional `MapMobilityProfileCursor`. Its decoded
fields are `version: 1`, `collection: "map://mobility-profiles"`, `after_id` and
`after_version`. A page returns complete validated profiles in `items`, `limit: 100`
and nullable `next_cursor`. Continuations apply current tenant selection and do not hold
a snapshot across requests. The decoder requires sorted unique ID/version pairs and a
continuation matching the last item of a full page. Map Explorer follows all pages
before replacing its view and preserves the previous view when a continuation fails.

The reader validates each selected profile's domain rules and its agreement with physical
record identity, indexed ID, version, family, name and validity. Completion matches in SQL
before grouping and its 101-result lookahead limit; version completion binds an optional
typed parent and orders numerically. These queries have a five-second database deadline.

Upgrade Map and its catalog clients together after acquisition, routing and spatial Tasks
settle. Clients replace array decoding with page traversal. Preflight retained profile IDs
and versions, including references in routes, matrices, spatial derivations, travel models,
route handoffs and installation bootstrap. Require lowercase hyphenated RFC-variant
UUIDv5/v7 IDs, admitted numeric versions and agreement between documents and indexed fields.
Preserve rejected records and immutable artifacts for operator repair; readers do not
rewrite them. Rollback restores the prior server/client pair against the same records and
discards new cursors. Operators must accept the prior reader's weaker admission checks.
Installed paging and reverse/forward replacement are required qualification.

## Coordinate Contract

WGS84 longitude and latitude are the canonical route exchange. Optional height
is ellipsoidal unless a contract states otherwise. `Wgs84Position` rejects undeclared
JSON fields; preflight retained coordinate payloads before upgrading consumers. UAV
requires height for executable waypoints and owns its additional admission checks.

Map provides bounded two-dimensional CRS transformation through PROJ. It
rejects geocentric EPSG:4978 and vertical values instead of silently copying
or mis-transforming them. GeographicLib supplies WGS84 direct and inverse
geodesics. Geofence validation checks segment geometry, not only vertices.

`src/contract/geodetic_ids.rs` owns `CrsId`, `DatumId`, and `EllipsoidId`. They preserve
the public 1–128 byte ASCII name profile, including colons in authority codes. Frames
and RRD import these through Map's contract feature. Frames owns its distinct WGS84
position payload with explicit degree and height fields; Map's geographic position
payload keeps its existing longitude, latitude, and optional height representation.

## Persistence

SurrealDB is the canonical operational catalog. It stores:

- registered sources and immutable dataset-release records;
- active release pointers and optimistic record versions;
- mobility profiles and effective restrictions;
- operational snapshots, routes, dependencies, and matrices;
- acquisition jobs, durable task state, and immutable raster and spatial derivations;
- authored feature layers, schema and style revisions, feature revisions and
  heads, atomic changesets, and immutable layer publications;
- immutable publication products, map composition heads, and composition
  revisions.

DuckDB Spatial is the local analytical projection. The service opens one
configured database instance for its lifetime and clones connections inside
that instance for concurrent work. Read paths begin explicit read-only
transactions. Task exports receive only a validated direct child of the
installation-owned task root, while the database and spill directory remain
outside that file surface. Its schema is tenant keyed
and contains active-release pointers, locations, facilities, boundaries,
governed network edges, and authored feature revision and head projections.
SurrealDB stores immutable raster and spatial derivations by tenant, Work Context,
kind, and derivation ID. Each Map replica reads that shared record directly.
Spatial queries use `ST_Contains`, `ST_Intersects`, and `ST_Distance_Sphere`.
The Spatial extension is copied into the image at build time and loaded only
from its pinned local path. Map selects the shared runtime's closed
`GeoJsonLongitudeLatitude` axis policy before configuration is locked. Startup and
health read `current_setting('geometry_always_xy')` and require `true`.

Selective geometry reads use the existing DuckDB Spatial R-tree indexes on
boundaries, immutable source features, authored revisions, and authored heads.
Map admits only schema 11 and eagerly binds and verifies all four indexes before
serving requests. DuckDB 1.5.6 and its matching Spatial extension replay committed
current-format WAL after an unclean shutdown. The process-exit regression checks
mixed geometries, index contents, spatial selection, uncommitted rollback and a
second reopen after recovery. Historical schema markers fail with an explicit
projection-rebuild error; startup does not convert or delete them. Operators drain
Map before changing its engine, preserve a snapshot of the database and WAL together,
and restore that pair with its matching image if rollback is required.
The query shape first obtains geometry-only candidates from the indexed base
table. Tenant, Work Context, release, layer, revision, and exact spatial
predicates remain on the authoritative outer query. This separation prevents
non-spatial selectivity estimates from hiding the R-tree from DuckDB's planner.
Dateline-crossing boxes use two candidate branches joined by `UNION`, because
an `OR` between spatial predicates does not produce two R-tree scans.

`src/authoring/query/performance.rs` is the executable performance contract for
feature-layer viewport reads. It loads 10,000, 100,000, and 1,000,000 indexed
features under the production 1 GiB memory and four-thread settings. Every
scale must expose `RTREE_INDEX_SCAN` in the production query plan and match an
independent numeric point oracle. The same gate covers dateline branches,
publication revision selection, moved features, delete and reinsert index
maintenance, R-tree leaf cardinality, and database size. On the test host,
selective reads must remain below two seconds cold and 250 ms warm p95. Indexed
fixture loading must sustain 5,000 rows per second and the million-feature
database must remain below 2 GiB. These generous regression ceilings are local
acceptance budgets, not service latency claims.

Release-product projection is attempt scoped. Each preparation receives a
private UUIDv7 attempt and writes complete source features in transactions of
at most 256 features or 32 MiB of canonical source-feature data. Stable logical
ids remain unchanged across releases. Stored rows add the tenant, immutable
release, attempt, and contiguous ordinal needed to keep simultaneous releases
and interrupted retries distinct.

The completion ledger is the visibility boundary. Its final transaction checks
the row count, distinct ordinal count, ordinal range, and logical-id uniqueness
for every high-volume release table. It also checks the raster count. Only the
winning attempt becomes readable or activatable. An interrupted attempt may
remain on disk, but its rows cannot enter tools, resources, routing, or spatial
queries. A release retains at most eight attempts before preparation stops with
an instruction to rebuild this derived projection. The supported deployment
uses one Map replica and one release writer.

Source tags stay in the immutable feature JSON. Equality predicates match only
JSON strings, while existence predicates include a present JSON null. JSON
Pointer escaping protects tag keys containing `/` or `~`. This avoids the
write amplification of an exploded tag table without weakening release and
attempt isolation.

Schema version 9 is a hard cut. Map refuses to open an older analytical schema
or managed tables without a valid marker. During upgrade, preserve SurrealDB,
the artifact plane, and retained release products, then rebuild only the local
DuckDB projection and replay the retained products before activation. No source
reacquisition or compatibility migration is part of this contract.

The artifact plane stores immutable raw source bytes, normalized products,
routing builds, quality reports, and large task outputs. Cross-server artifact
identity remains `artifact://{artifact_id}`. Map projects those artifacts as
`map://artifact/{artifact_id}` only after applying the normal artifact policy.

## Authored Feature Layers

The [authoring design](AUTHORING.md) defines feature schemas, transactional revisions,
spatial projections, editing and publication.

## Authoritative Data Acquisition

The [acquisition design](ACQUISITION.md) defines registered sources, source catalog
reads, acquisition applications, network controls and adapter availability.

## Release Versioning And Activation

`DatasetRelease` contains:

```text
release_id
dataset_id
source_id
version_label
source_digest_sha256
coverage
acquired_at
valid_from
valid_until?
schema_version
normalization_pipeline_version
routing_build_version?
license
raw_artifact_uri
normalized_artifact_uris
quality_report_uri
supersedes_release_id?
state
record_version
```

The current version label is `sha256:{digest}`. The full digest proves byte
identity. A release id identifies the governed occurrence, its validity,
license, normalized products, and policy context. Release states are `staged`,
`active`, `retired`, and `quarantined`.

Routing archives are safely expanded once into the retained release directory.
Archive traversal, links, excessive entry count, and excessive expanded bytes
are rejected. Activation and rollback reuse the retained cached products.

Activation follows this sequence:

1. Validate and ingest retained release products into tenant-scoped DuckDB
   rows that are not yet selected.
2. Atomically update the SurrealDB release state and active dataset pointer
   under expected release and pointer versions.
3. Atomically switch the Valhalla active-directory symlink on Unix and update
   the DuckDB active pointer.
4. Restart the supervised Valhalla process when the release has routing data.
5. Retire the previous release and invalidate routes that depend on it.

The SurrealDB state and pointer share one database transaction and establish the
canonical active release. DuckDB and filesystem projections reconcile after
that catalog commit. A projection failure returns an error while preserving the
canonical release, and calling `activate` again with current record versions
performs an idempotent reconciliation. The admin app exposes this as `Reconcile`.

Map deploys as one replica with one persistent `ReadWriteOnce` volume. The
activation mutex serializes local product switches inside that process.

Licenses travel with each release. The contract records attribution,
redistribution, derivative, offline-bundle, and expiry policy.

## Routing

The [routing design](ROUTING.md) defines land routing, governed networks, restrictions,
validation and durable routing operations.

## Spatial And Terrain Derivations

`derive_spatial_geometry` performs one bounded operation and persists its
result before returning. The supported operations are:

- line resampling;
- deterministic nearest-neighbor point ordering and closed tours;
- polygon boundaries and inward or outward standoff perimeters;
- line corridors and parallel lanes;
- racetracks with explicit turn direction;
- relay or station points;
- alternating coverage tracks clipped to polygon interiors and holes;
- connected components with an optional metric tolerance;
- lead-in and ingress geometry;
- complete-route validation.

Each request names one immutable mobility-profile version and may pin immutable
Map releases. It declares effective time and terrain classes. Output records
include typed findings, intersected restriction identities, the exact
projection origin, algorithm revision, request digest, geometry digest,
principal, and Work Context.

Inputs contain at most 10,000 coordinates. Connected-component requests contain
at most 512 geometries, and parallel-lane requests contain at most 128 lanes.
The server rejects an output above 50,000 coordinates. The local projection
rejects polar and dateline-spanning work instead of silently reducing
accuracy.

Terrain sampling remains a durable raster derivation. `corridor_maximum`
resamples a WGS84 line at a declared spacing, evaluates up to 64 cross-track
positions, and records no more than 10,000 samples. Its JSON artifact includes
every sampled position and value together with deterministic minimum and
maximum records. The source raster, band, source transform, checksums,
algorithm revision, caller, and Work Context use the same provenance contract
as every other raster derivation.

## MCP Surface

### Tools

| Tool | Invocation | Required scope | Result |
|---|---|---|---|
| `search_locations` | direct | `map:dataset:read` | bounded named locations and optional facilities |
| `list_active_dataset_releases` | direct | `map:dataset:read` | bounded active immutable release identities, digests, and pointer revisions |
| `query_source_features` | direct | `map:dataset:read` | deterministic page from one immutable complete source release |
| `inspect_location` | direct | `map:dataset:read` | location, nearby facilities, containing boundaries, lineage, gaps |
| `inspect_position` | direct | `map:dataset:read` | position, distance-ordered nearby places, containing boundaries, active releases, gaps |
| `transform_crs` | direct | `map:dataset:read` | bounded 2D CRS transformation |
| `geodesic_inverse` | direct | `map:dataset:read` | WGS84 distance and azimuths |
| `geodesic_direct` | direct | `map:dataset:read` | WGS84 destination |
| `validate_geofence` | direct | `map:dataset:read` | topological and segment relationship findings |
| `route` | task only | `map:route` | persisted route with pinned provenance |
| `route_matrix` | task only | `map:route_matrix` | persisted many-to-many matrix |
| `build_travel_model` | task only | `map:route_matrix` | immutable heterogeneous cuOpt cost and transit-time matrices |
| `reachable_area` | task only | `map:route` | land isochrone |
| `validate_route` | direct | `map:route` | typed validation findings |
| `prepare_route_handoff` | direct | `map:route` | current validated `veoveo.ai/map-route-handoff/v1` projection for a consuming domain |
| `inspect_corridor` | direct | `map:dataset:read` | restrictions, facilities, boundaries, and gaps |
| `publish_restriction` | direct | `map:restriction:publish` | effective restriction |
| `withdraw_restriction` | direct | `map:restriction:withdraw` | ended restriction and invalidation count |
| `create_feature_layer` | direct | `map:feature:write` | empty governed layer with schema and style revisions |
| `update_feature_layer` | direct | `map:feature:write` | optimistic metadata, schema, or style revision |
| `validate_feature_changes` | direct | `map:feature:write` | validation and concurrency findings without a write |
| `commit_feature_changes` | direct | `map:feature:write` | atomic changeset and updated feature revisions |
| `restore_feature` | direct | `map:feature:write` | new live revision of a tombstoned feature |
| `query_features` | direct | `map:feature:read` | current or publication-pinned feature page |
| `publish_feature_layer` | direct | `map:feature:publish` | immutable layer publication |
| `archive_feature_layer` | direct | `map:feature:admin` | archived layer head with history retained |
| `create_map_composition` | direct | `map:feature:write` | governed composition and first revision |
| `update_map_composition` | direct | `map:feature:write` | optimistic immutable composition revision |
| `archive_map_composition` | direct | `map:feature:admin` | archived composition head with history retained |
| `import_feature_layer` | task only | `map:feature:write` | atomic import changeset from an authorized artifact |
| `inspect_geopackage` | task only | `map:feature:read` | validated bounded vector-table manifest for an authorized artifact |
| `export_feature_layer` | task only | `map:feature:publish` | immutable GeoJSON sequence, GeoParquet 1.0, or GeoPackage 1.4 product |
| `build_vector_tiles` | task only | `map:feature:publish` | immutable MVT 2.1 bundle and MapLibre style |
| `derive_raster` | task only | `map:raster:derive` | governed raster sample, terrain-corridor maximum, window, mask, contour, polygon, skeleton, or line artifact |
| `derive_spatial_geometry` | direct | `map:dataset:read`, `map:spatial:derive` | persisted advisory geometry and complete mobility findings |

All tool results use structured content schemas. Tool and resource lists are
paginated. Task-only tools use the durable task extension.

The listed scope is necessary but not sufficient for a Work Context-owned
object. Reads require effective read access, edits require write access, and
publication, product creation, or archival requires effective admin access.

Map uses stateless Streamable HTTP. Ordinary responses are JSON, while
`subscriptions/listen` owns the request-scoped SSE stream carrying task,
resource, and resource-list notifications.

### Resources

Root resources are:

```text
map://sources
map://datasets
map://locations
map://facilities
map://mobility-profiles
map://restrictions
map://routes
map://matrices
map://travel-models
map://feature-layers
map://publications
map://layer-products
map://compositions
map://rasters
map://raster-derivations
map://spatial-derivations
```

Resource templates are:

```text
map://source/{source_id}
map://sources{?cursor}
map://datasets{?cursor}
map://dataset/{dataset_id}{?cursor}
map://dataset/{dataset_id}/release/{release_id}
map://source-feature/{release_id}/{source_feature_id}
map://raster/{raster_id}
map://raster-derivation/{derivation_id}
map://spatial-derivation/{derivation_id}
map://location/{location_id}
map://facility/{facility_id}
map://mobility-profile/{profile_id}/{profile_version}
map://mobility-profiles{?cursor}
map://restriction/{restriction_id}
map://restrictions{?cursor}
map://routes{?cursor}
map://matrices{?cursor}
map://acquisitions{?cursor}
map://acquisition/{acquisition_id}
map://route/{route_id}
map://matrix/{matrix_id}
map://travel-model/{travel_model_id}
map://travel-models{?cursor}
map://artifact/{artifact_id}
map://feature-layers{?cursor}
map://publications{?layer_id,cursor}
map://layer-products{?publication_id,cursor}
map://compositions{?cursor}
map://feature-layer/{layer_id}
map://feature-layer/{layer_id}/schema/{schema_version}
map://feature-layer/{layer_id}/style/{style_version}
map://feature-layer/{layer_id}/features{?publication_id,bbox,datetime,geometry_type,filter,limit,cursor,minimum_commit_sequence}
map://feature-layer/{layer_id}/feature/{feature_id}
map://feature-layer/{layer_id}/feature/{feature_id}/revision/{feature_revision}
map://feature-layer/{layer_id}/changeset/{changeset_id}
map://feature-layer/{layer_id}/publication/{publication_id}
map://feature-layer/{layer_id}/publication/{publication_id}/product/{product_id}
map://composition/{composition_id}
map://composition/{composition_id}/revision/{composition_revision}
```

Source resources present public source fields. Routes and matrices are owner
scoped. Travel models are filtered by principal, gateway profile, tenant,
labels, and Work Context from their durable task owner. Dataset, geography,
profile, and restriction resources are tenant scoped. Authored layers,
publications, products, and compositions are Work Context scoped and filtered
by the caller's data labels. Store applies those predicates in SQL before returning
layer and composition records. Publication and product queries select visible parent
layers in SQL, including direct product reads. Raster derivation resources are confined to their
creating Work Context, while the immutable source raster remains tenant
scoped. Spatial derivations are also confined to their creating Work Context.

`map://datasets` and `map://dataset/{dataset_id}` return release pages with
`items`, `limit: 100`, and `next_cursor`. Items are release documents in ascending
release-ID order. The root includes all tenant-visible datasets. A dataset URI
selects its parent in SQL before the keyset predicate and limit. A version-1
hex-encoded cursor binds the release collection and optional dataset ID; it does
not confer access. Each page applies the current tenant again. These reads observe
current committed rows, without a snapshot across page requests. An empty first
page for a dataset returns not-found; an exhausted continuation returns an empty
page. An exact release URI binds both dataset and release IDs in the database.
A layer-product URI likewise binds its layer, publication, and product IDs in SQL
alongside current layer visibility.

Authoring metadata collections use the same 100-item envelope, ordered by immutable
domain ID. Layer and composition pages exclude archived rows. Publication and product
pages select current parent-layer visibility and may read immutable products of an
archived layer. Their optional `layer_id` and `publication_id` filters apply in SQL
before keyset selection and limits. Cursors bind those filters and reapply current
tenant, Work Context, and label access on every request. These pages observe current
committed rows rather than a snapshot across requests. The App follows all pages
before publishing a refreshed collection, preserving its previous view on failure.

Route, matrix, and acquisition indexes return the same page envelope with up to
100 items ordered by immutable domain ID. Their cursors bind the collection and
are valid only at version 1. Store applies tenant and owner predicates before
keyset selection and limits, including for direct reads. Route and matrix items
contain status or profile metadata and a `resource_uri` for the complete document;
index queries omit route geometry and matrix cells. Matrix reads and completion
select only rows containing a matrix document. Acquisition pages contain job
records, and acquisition updates enforce ownership in the same SQL write as their
revision check.

Map owns the coordinated collection-page transition. Installations drain Map and
replace its binary and packaged Map Explorer together. Clients must consume the
page envelope and follow `next_cursor`; grouped dataset objects and bare arrays
for releases, routes, matrices, acquisition jobs, or authoring metadata are unsupported. Route and
matrix consumers read each summary's `resource_uri` when they need the payload.
Rollback restores the previous binary and App together; this response change does
not convert persisted records. Qualification covers multiple pages, foreign
tenants and owners, mismatched parents, and rejected cursors. No mixed-version
response adapter is supported during this installation upgrade.

Raster and spatial derivation indexes return up to 100 summaries with resource
links, a `limit`, and an optional `next_cursor`. Page templates accept the version-1
hex-encoded cursor; its kind must match the requested collection. A page request
always applies the current tenant and Work Context in SQL before the cursor and
limit. Direct resources return the complete immutable document. Completion applies
the search term in SQL and selects at most 101 IDs to determine `hasMore`.

Both indexes accept resource subscriptions. Raster index admission requires
`map:dataset:read`; spatial index admission additionally requires
`map:spatial:derive`, matching their resource reads.

### Catalog Maintenance Queries

Acquisition recovery selects only the owner's queued, running, or cancel-requested
jobs without a registered worker, in batches of 100. Admission holds the worker
inventory lock while creating a job and registering its worker. Recovery holds the
same lock throughout selection and settlement, so a newly admitted job cannot enter
recovery between those steps. This mechanism uses the declared single Map writer.

Release and restriction administration selects dependent tenant routes across
owners. SQL decodes dependency arrays from each complete route document before
its 100-row limit. Map chooses those arrays because separately written dependency
rows can be incomplete after interruption. The predicate uses database JSON
decoding; it is not an indexed dependency lookup. Each update commits the route's
invalidated status and complete document together and counts only a new transition.
Native qualification removes a dependency row and adds an incorrect one, then proves
that selection still follows the route document.

### Prompts And Completions

Map exposes `prepare_route_request`, `review_route`,
`prepare_logistics_matrix`, `prepare_optimization_travel_model`, and
`author_feature_layer`. The travel-model prompt carries stable shared IDs,
exact profile versions, units, limits, and the Map resource plus artifact
manifest into the Optimization routing tools. The authoring prompt directs
agents through resource inspection, validation, optimistic commit,
publication, and task-based bulk transfer without granting routing authority.

Completion applies to resource-template arguments and returns only visible ids.
The implementation completes source, dataset, release, location, facility,
profile, restriction, route, matrix, travel-model, layer, publication, product,
and composition identities from the caller's scope. Store and DuckDB apply scope,
search text, and deduplication before a 101-ID query limit. MCP returns the first
100 IDs and `hasMore`; it omits `total` when more matches exist. Dataset, profile,
layer, publication, and composition arguments supplied in completion context
constrain the SQL query. Schema, style, and composition versions come from their
stored revision tables under current parent visibility. Geography completion reads
only committed projections of active releases. Travel-model completion selects
caller-owned Tasks in the current Work Context without loading their result documents.

### Subscriptions And Notifications

Subscriptions cover the mutable dataset, restriction, route, travel-model,
feature-layer, publication-product, and composition surfaces. A Store LIVE observer
invalidates each replica's resource hub for catalog, derivation, authoring, and Task
writes. The shared observer uses change-feed recovery after a disconnected watch and
requests a current read after startup or a history gap. Each listener receives only
updates for its admitted resource URIs. Authored-feature reads reconcile the local
DuckDB projection through the committed Map changeset head before answering.

Discovery advertises fixed roots and templates. Content writes send resource updates
without changing that inventory. Subscription state belongs to each listen request;
long-running work uses task subscriptions. Shared derivation reads and notifications
do not change the single-writer deployment profile for release products and Valhalla.

## Installation Bootstrap

Map consumes the platform's generic server-bootstrap contract
(`veoveo_mcp_contract::ServerBootstrapDocument`): a `server: map` envelope with
a `tenant_key` and a Map-owned payload of `sources` and `mobility_profiles`.
The deployment mounts the document at `/etc/veoveo/bootstrap/catalog.json` and
passes `--bootstrap-catalog`; the Helm chart renders it generically from
`serverBootstrap.map-mcp` without naming Map in core templates. Application is
create-only and idempotent: existing sources and mobility-profile versions are
skipped. The payload rejects unknown fields and mistargeted envelopes fail
closed. `map-mcp bootstrap-validate <path>` validates a document without
booting the server. Bootstrap never downloads, validates, or activates a
release; those remain governed operations by an authorized caller.

## Administration Over MCP

Administration crosses the same MCP boundary as every other operation
(`mcp/apps-extension/DESIGN.md` owns the contract). Mutations are
`map:admin`-scoped tools implemented in `administration.rs` and exposed from
`mcp.rs`:

| Tool | Purpose |
|---|---|
| `register_source` | register one governed source (idempotent on identical re-registration) |
| `replace_source` | replace source configuration under an expected record version |
| `disable_source` | disable future acquisition under an expected record version |
| `start_acquisition` | start a snapshot acquisition with an idempotency key |
| `cancel_acquisition` | request cancellation of a running job |
| `activate_release` | activate staged data or reconcile the active projection |
| `rollback_release` | activate a retained release |
| `quarantine_release` | quarantine an inactive release |
| `register_mobility_profile` | register an immutable profile version |

Dataset readers use `map://sources`, `map://datasets`,
`map://active-releases`, and `map://mobility-profiles`. Administrative job
reads use `map://acquisitions` and `map://acquisition/{acquisition_id}`.
Creation tools use idempotency keys; source and release mutations use expected record
versions; activation also uses the expected active-pointer version.
Validation failures surface as MCP invalid-params errors and concurrency
conflicts name the changed version.

Map Explorer traverses dataset release and acquisition pages before publishing a refreshed view.
A refresh permits 100 pages and 60 seconds per collection, rejects malformed or
repeated cursors, and keeps the previous view when any selected resource fails.
The client keeps at most four host resource reads in flight.

The Map workspace ships as `ui://map/workspace.html` from
`assets/workspace-app.html`. It is listed when the caller has
`map:dataset:read`, `map:feature:read`, or `map:admin`, while
`map://workspace` tells the App which sections and controls to present. Every
operation remains scope-gated by its canonical resource or tool handler. The gateway projects the App under
`resource_projection: server_owned`, and the Console renders it from its generic
catalog; no map-specific Console page, BFF route, or REST router exists.

The image packages the document at `/opt/veoveo/map/assets/workspace-app.html`.
Startup loads one bounded immutable `AppHtml` snapshot before connecting services.
For a local process, pass `serve --workspace-app servers/map-mcp/assets/workspace-app.html`
alongside its normal configuration. Regenerate the HTML and restart that process
to preview an edit. Image builds keep the App generator and HTML in the assembly
context, allowing an unchanged Rust artifact to be reused.

## Isolation And Security

Every SurrealDB catalog read includes the tenant id. Owner-scoped routes,
matrices, acquisition jobs, and artifacts also check the principal. DuckDB
tables include `tenant_key` in their primary keys, and every active-release
lookup is tenant constrained.

The public server validates the Host authority and a gateway-signed internal
token. Tool handlers enforce domain scopes again after gateway policy;
administrative tools and resources require `map:admin`. Secret references are
bounded identifiers; MCP resources expose those references.

Health reports DuckDB Spatial verification and both the supervised Valhalla
process and its loopback health. A failed routing process makes the Map health
endpoint unavailable.

## Operational Limits And Failure Semantics

- Location search returns at most 100 results.
- Resource list materialization reads at most 10,000 active locations and
  10,000 facilities before MCP pagination.
- A route accepts 32 waypoints and 3 alternatives.
- Graph endpoints must snap within 10 km.
- Matrices accept at most 400 cells.
- Travel models accept at most 128 shared locations, 64 vehicle types, and
  1,048,576 total matrix cells.
- Admin pages accept at most 200 records.
- Source elapsed time is within 1 second and 24 hours.
- Routing archive expansion defaults to 16 GiB and 5,000,000 entries.
- Route task leases last 120 seconds and renew every 40 seconds.
- Task records default to a 7-day TTL.
- Direct authored changesets contain at most 100 mutations and 1 MiB of encoded
  mutation data.
- Bulk imports contain at most 10,000 features and remain one atomic commit.
- Bulk input and output artifacts default to a 256 MiB byte limit.
- Feature queries return at most 1,000 records per page. CQL2 filters contain at
  most 64 nodes and nest at most 16 levels.
- A feature contains at most 50,000 coordinates, 256 properties, and 256 KiB of
  encoded properties.
- A style contains at most 32 rules. A composition contains at most 64 layers.
- A vector product request contains at most 512 distinct XYZ tiles through zoom
  22.
- A raster sample accepts at most 10,000 positions. A window contains at most
  16,777,216 output pixels, class masks contain at most 256 class values, and
  a full-raster derivation reads at most 4,194,304 source pixels.
- A spatial input contains at most 10,000 coordinates and produces at most
  50,000. Terrain-corridor sampling produces at most 10,000 positions across
  no more than 64 cross-track offsets.
- Raster helpers inherit the 256 MiB artifact limit and terminate their process
  group after the configured five-minute deadline or task cancellation.
- The authored-feature R-tree performance gate uses 10,000, 100,000, and
  1,000,000 row fixtures, a two-second cold ceiling, a 250 ms warm p95 ceiling,
  a 5,000 indexed-row/s floor, and a 2 GiB database-size ceiling.
- The source-feature R-tree performance gate applies the same scales and
  latency, ingest-rate, and storage ceilings to the exact viewport query used
  by the workspace. It asserts the physical R-tree plan at every scale, checks
  results against a full-scan point oracle, verifies two index scans across the
  antimeridian, and inspects one million index leaves.

Unavailable coverage, invalid profile versions, disallowed advisory status,
unsupported objectives, source digest mismatch, unsafe archives, and
optimistic-concurrency conflicts all fail explicitly while preserving the
original question.

## Deployment

The container image is built from `servers/map-mcp/Dockerfile`. The Helm chart
mounts one `map-data` volume and the optional source exchange read-only, exposes
only port 8799 inside the cluster, and deploys one replica with a 100 GiB
`ReadWriteOnce` claim. The offline image lock contains
`veoveo/map-mcp:0.1.0` and its Dockerfile.

An installation selects its existing, controlled source PVC and optional
credential Secret through `domainServiceSourceMounts.map-mcp.exchangeClaim`
and `domainServiceSourceMounts.map-mcp.secretName`. The chart mounts these
objects read-only; it never copies source bytes or turns acquisition into a
Helm side effect. `domainServiceResources.map-mcp` independently sets the Map
pod's acquisition and indexing envelope without inflating every hosted server.

Important arguments include:

```text
--map-database
--duckdb-spill-dir
--spatial-extension
--duckdb-memory-limit
--duckdb-threads
--workspace-basemap-light-style-url
--workspace-basemap-dark-style-url
--valhalla-url
--valhalla-executable
--valhalla-config
--valhalla-active-dir
--acquisition-scratch-root
--release-root
--authoring-task-root
--raster-helper-module
--raster-operation-timeout-seconds
--source-mount-root
--source-secret-root
--max-artifact-bytes
--max-routing-expanded-bytes
```

The image build pins the DuckDB C API and architecture-specific Spatial
extension, verifies its SHA-256 digest, compiles the Rust server, and copies
native map utilities from pinned images or packages. The runtime user is uid
10001. System packages are fixed during the image build.

## Dependencies

The server uses existing workspace crates for MCP, tasks, the platform store,
artifacts, gateway identity, and DuckDB hardening. Domain dependencies include:

| Crate | Use |
|---|---|
| `duckdb` | embedded analytical database |
| `proj` | projected CRS transformations |
| `geographiclib-rs` | WGS84 geodesics |
| `geo` and `geojson` | topology and controlled interchange |
| `petgraph` | governed network A* |
| `reqwest` | loopback Valhalla and controlled source acquisition |
| `tar` and `flate2` | bounded routing-build extraction |
| `sha2` | content and cache digests |
| `nix` | child process-group supervision |

Valhalla remains a supervised native process because its routing model and
tile build are already mature. The Rust contract isolates that choice from MCP
clients.

## Module Layout

```text
servers/map-mcp/
  src/
    acquisition/
      helper.rs
      service.rs
    admin/
      error.rs
      handlers.rs
    contract/
      admin.rs
      compositions.rs
      datasets.rs
      features.rs
      geometry.rs
      ids.rs
      mobility.rs
      operations.rs
      routes.rs
      source_products.rs
      spatial.rs
      travel_models.rs
      transfers.rs
      units.rs
    authoring/
      presentations.rs
      projection.rs
      query.rs
      service.rs
      transfers.rs
      validation.rs
    routes/
      graph.rs
      service.rs
      valhalla/
        adapter.rs
        client.rs
        process.rs
    spatial/
      derive.rs
      projection.rs
      validation.rs
    server/
      auth.rs
      config.rs
      host.rs
      tasks.rs
    analytics.rs
    artifacts.rs
    catalog.rs
    geodesy.rs
    geography.rs
    mcp.rs
    prompts.rs
    raster.rs
    release_products.rs
    state.rs
    uris.rs
  assets/
    workspace-app.html
  app/
    build.mjs
    package.json
    workspace.js
    workspace.template.html
  data/
    src/map_data/
      adapters/
      contract.py
      main.py
      raster_ops.py
      subprocesses.py
      terrain.py
    tests/
    pyproject.toml
    uv.lock
  Dockerfile
```

## Verification

The implementation is checked at several boundaries:

- Rust contract tests cover ids, quantities, geometry, mobility taxonomy,
  source validation, geodesics, graph costs, Valhalla profile limits, URI
  parsing, paging, stable feature ids, routing archive bounds, travel-model
  bounds, and activation;
- DuckDB runtime tests cover controlled HTTPS source policy, closed Spatial axis
  selection, effective-setting verification, and a pinned extension-backed meter
  baseline;
- Python tests cover typed contracts, a bounded GTFS acquisition with validator
  execution, unsafe ZIP rejection, subprocess timeout, process-group
  termination, and bounded diagnostics;
- SurrealDB integration tests apply the schema to SurrealDB 3.2 and verify
  atomic release activation under record versions;
- Console TypeScript and production Vite builds validate the administrative
  projection;
- Map workspace contract tests verify one permission-aware App resource, the
  validated same-origin light and dark basemap contract and CSP declaration, exact MCP bridge operations,
  immutable publication and active-release queries, guided GeoPackage tasks,
  subscription wiring, the embedded MapLibre pin, and fail-closed hardware
  WebGL2 checks;
- the Rust Map workspace browser smoke serves the exact generated App under the
  Console's opaque-origin sandbox and an exact local MapLibre Style CSP. Headed Chrome
  must prove an NVIDIA WebGL adapter before the App completes bounded
  publication-pinned and active-release viewport queries, switches from light
  to dark basemap without moving the camera or losing overlays, synchronizes
  map and table selection, retains the map during governed-data inspection,
  and emits screenshot evidence;
- the container build verifies the pinned Spatial extension and packages GDAL,
  Osmium, Valhalla, and the Python application;
- the Rust Map smoke launches that image with a real SurrealDB 3.2 catalog and
  artifact service. It acquires and activates authority, OSM, and governed
  network fixtures, rejects a bad source digest before staging, and exercises
  named-location, facility, boundary, and corridor queries;
- the same smoke invokes road and maritime routing through the MCP Task API. It
  checks task creation and completion, executes a real Valhalla road route,
  executes a governed graph route, validates persistence, applies restriction
  risk, withdraws the restriction, and reads the invalidated dependent route;
- the broader smoke and conformance suites validate gateway, control-plane,
  offline, task, and MCP behavior.
- the cross-server compatibility test serializes the Map travel-model artifact
  and deserializes it directly as the Optimization contract without a
  translation shim.

The principal local commands are:

```text
cargo test -p veoveo-map-mcp --lib
cargo test -p veoveo-platform-store --lib
uv run --project servers/map-mcp/data --frozen python -m unittest discover -s servers/map-mcp/data/tests -v
npm --prefix servers/map-mcp/app ci
npm --prefix servers/map-mcp/app run build
npm --prefix apps/console/web run build
cargo xtask image build --target map-mcp
cargo xtask smoke map-mcp
cargo xtask smoke map-workspace-browser-verify
```

The risk-based suite targets representative acquisition, land routing,
governed-network routing, restriction, invalidation, Task API, and persistence
boundaries. Authority datasets and certified performance models add their own
domain acceptance cases as they enter an installation.
