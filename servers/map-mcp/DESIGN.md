# Map MCP Design

`map-mcp` owns Veoveo's Earth geography and logistics-routing contract. Source
administration and feature authoring share its typed MCP tools, resources and
permission-aware Map Explorer App.

Consumers use Map tools and `map://` resources through the gateway, or declare
typed App dependencies. Storage, private HTTP routes and renderer internals are
not integration surfaces. Installation profiles, policy, scopes, tenant, labels
and Work Context authorize every consumer.

## Service Identity

```text
crate       veoveo-map-mcp
folder      servers/map-mcp
slug        map
URI scheme  map
MCP         /map/mcp
workspace   ui://map/workspace.html
health      /map/healthz
```

Gateway tools use names such as `map__route`; resources keep `map://`.

## Standards And Protocols

Map owns the v2 travel-model artifact wire type. Optimization decodes that owner document, checks its selected Map resource attestation and matrix relations, then converts it into its existing internal travel model. Inline and prepared Optimization documents keep their declared representation.

Map-controlled JSON fields use camelCase; controlled vocabulary values use snake_case. GeoJSON geometry names, CQL2 operators, provider documents and open property maps use their upstream profiles. Native database columns keep their declared field names. The coordinated installation cut requires drained Tasks and fresh Map state; receivers reject obsolete wire keys, cursor envelopes and schema revisions. Source-feature query continuations bind the v3 request domain to the admitted camelCase request. Source-feature, raster and spatial product documents and the three private Python helper protocols use schema revision 2.

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with tools, resources and templates, prompts, completions, subscriptions, notifications, and typed structured content. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; acquisition, routing, import, export, publication, and vector-product operations use durable task semantics where declared. |
| [MCP Apps SEP-1865](../../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; `ui://map/workspace.html` uses the sandboxed host bridge and canonical Map tools and resources. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | MCP schemas and immutable authored-layer property contracts. Layer schemas reject remote references. |
| [`ai.veoveo/knowledge-source`](../../mcp/knowledge-extension/DESIGN.md) | Six optional collections expose bounded JSON summaries, source SHA-256 digests, typed scopes, observations and conditional reads. Summary addresses append `/knowledge` to the full source address. |
| [Veoveo resource components](../../platform/types/DESIGN.md#concrete-resource-components) and URI Template RFC 6570 | The host composes Map-owned address parsers for direct resources, products, pages, filtered features and knowledge. Builders encode typed IDs and version-1 catalog cursors. Discovery declares form-style parent and cursor query parameters. |
| WGS 84 and EPSG identifiers | Longitude, latitude, and ellipsoidal height are the geographic exchange. PROJ handles bounded projected-CRS conversion; EPSG:4978 and vertical transformations are outside that 2D operation. |
| SurrealDB 3.3.0 | Internal catalog queries, transactions, LIVE/change-feed delivery, and [JSON decoding](https://surrealdb.com/docs/reference/query-language/functions/database-functions/encoding#encodingjsondecode) for selection against complete route documents. |
| Map travel-model resource profile | Map-owned component builders, RFC-variant UUIDv5/v7 IDs in lowercase hyphenated spelling, 100-item SQL pages and version 2 hex-encoded JSON cursors over native UUIDv7 Task positions. Exact and collection templates follow RFC 6570. |
| Map mobility-profile resource profile | Map-owned component builders and canonical RFC-variant UUIDv5/v7 IDs. Profile versions are integers in 1..=9223372036854775807. Collections use 100-item pages and version 2 hex-encoded JSON cursors bound to `map://mobility-profiles` and an ID/version position. Exact and collection templates follow RFC 6570. |
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
| `veoveo.ai/travel-model-artifact/v2` | Repository-owned immutable exchange from Map to Optimization. It carries shared location order, per-vehicle-type cost and transit-time matrices, unavailable cells, and exact Map resource attestation. |

The workspace pins `geo` 0.32.0 because SurrealDB 3.3 uses the same release
line and requires `i_overlay <4.1`. `geo` 0.33.1 requires `i_overlay >=4.5`,
which Cargo cannot resolve in this workspace. The selected release contains
the signed buffer operation used by the spatial profile.

The Map bootstrap payload admits `sources` and `mobilityProfiles` through the
same current public source/profile decoders used by tools. Bootstrap validation
completes before identity creation or catalog writes. Installation values render
these members unchanged into the shared server-bootstrap document.

Selected active-release SQL rows have a private native projection with snake_case
column names. The catalog constructs the public camelCase pointer from those
admitted native fields and checks it against the selected release document.
Raster helper admission closes every known operation, position, corridor and
bounds object before creating output directories or opening GDAL. Quality reports
and their checks share one strict helper model at writing and reading; successful
normalization requires its admitted acquisition identity and passing checks.

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

### Knowledge Collections

Map publishes `map.layers`, `map.features`, `map.publications`, `map.locations`,
`map.facilities` and `map.releases`. Each collection enumerates up to 100 summary
links through `map://knowledge/{collection}{?cursor}`. Map owns the collection enum,
member addresses and collection-bound cursors in `contract/knowledge.rs`; consumers
can use those types with the public `contract` feature. The `knowledge` feature adds
collection descriptors without the server runtime. Link titles contain at most
128 bytes after JSON escaping, keeping 100-item pages within the response budget.

Summary resources append `/knowledge` to the full resource address. Each JSON document
links to that source and records the SHA-256 digest of its complete serialized body.
The digest changes when geometry or omitted properties change. Feature summaries carry
the bounding box and up to 32 property excerpts, with 128-byte names and values,
truncation flags and an omitted-property count. Full geometry stays in the source
resource. The summary types keep every member below 64 KiB without reducing Map's
50,000-coordinate feature limit.

The source revision binds the summary text, access descriptor, stored modification
time and modifying principal. A change to provenance requires a full read even when
the summary text stays unchanged.

Layers, features and publications require `map:feature:read`. Their SQL selects the
current tenant, selected Work Context, classification and label clearance before decoding
or pagination. The access descriptor combines classification with data labels; changing
only parent access changes feature and publication observation revisions.
A feature or publication read selects its body and current parent layer in one database
statement. The observation records `selected-work-context-members`: owners and grant
holders still need membership in the selected context. Layer changes revise child access
observations. Feature attribution names the actor recorded on its current revision;
publication attribution names its publisher. A layer omits `modifiedBy` because it
records its creator, rather than the actor responsible for every later update.

Locations, facilities and releases require `map:dataset:read` and share data within
the tenant. Location and facility SQL selects completed projections in active releases.
When active releases repeat an ID, the lowest release ID supplies its resource, page
entry and search hit. SQL resolves that choice before applying the query and limit.
`search_locations` returns the extension's result shape and one summary resource link
per hit, with a combined limit of 100 locations and facilities.

Authoring and release summaries use Store-backed subscriptions. Locations and facilities
declare 300-second revalidation because projection visibility can follow the catalog's
activation signal. Their source records contain validity dates, but do not record a
modification timestamp; observations leave that timestamp absent. Other observations use
stored modification times. Reads authorize before evaluating conditional validators.

Native tests cover observations, paging, active-release selection and denied malformed
records. The [consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md) tracks installed
knowledge conformance and mutation/restart qualification.

`tests/gateway_source_conformance.rs` changes layer and feature titles, publishes two
disposable layers across a Map Deployment restart, and activates then rolls back a
release in a separate non-routing dataset. The shared checker verifies update
notifications for members and roots, creation notifications for collections, committed
state after restart, and publication enumeration, full reads and conditional reads.
Search tests use an authenticated reader without dataset-read authority. Cleanup
restores the release pointer and archives owned layers. Fixture admission, credentials
and commands follow [the installed harness contract](../../testing/installed/DESIGN.md).

### Public Types And Authorization

[Map resource contracts](RESOURCES.md) define the owner types, URI builders, checked
startup and discovery, scope admission, public library features and metadata cursors.
Domain readers apply current authorization and parent relationships in SQL.

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

## Query Files

Map stores SurrealQL in `src/persistence/queries/` and `src/queries/`; native fixture
statements live in `tests/queries/` or their colocated source query family. Catalog
pages, travel-model reads and authored knowledge reads select complete static files
for each supported selection. Rust binds identities, clearance, cursor positions and
limits, and decodes the existing statement results. DuckDB analytical SQL follows the
separate spatial execution profile described below.

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

Route creation and invalidation derive declared release, restriction and facility ID
arrays from the typed `RoutePlan` in the same write. Dependency pages use those
arrays and verify them against the selected document before consumption.
Travel-model Tasks atomically contribute `map_travel_model_task` creation and terminal
rows. The lookup declares retained actor, profile, tenant, clearance and Work Context
facts. Task selection, whole-input equality and lifecycle/result agreement precede
read and completion limits; one typed decoder verifies selected product identities.
`TravelModelInputRecord` and `TravelModelResultRecord` keep immutable original JSON
with its admitted request or travel-model interpretation. Native decoding repeats
owner admission and rejects native database values inside JSON. Encoding preserves
the original value, including omitted defaults, admitted nulls and MCP extensions.
SQL compares complete receipts without inspecting their open extension material.

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

Map admits only analytical schema 12 and eagerly binds and verifies DuckDB Spatial
R-tree indexes on boundaries, immutable source features, authored revisions and
authored heads before serving. DuckDB 1.5.6 with matching Spatial replays committed current-format WAL after
unclean shutdown. Process-exit tests cover mixed geometries, index contents, spatial
selection, uncommitted rollback and a second recovery reopen. Historical markers
produce an explicit projection-rebuild error; startup neither converts nor deletes
them. Engine upgrades require a Map drain and a snapshot of database and WAL together.
Rollback restores that pair with its matching image.

Spatial queries obtain geometry-only candidates from the indexed base table, then
apply tenant, Work Context, release, layer, revision and exact spatial predicates in
the outer query. This preserves R-tree planning despite non-spatial selectivity
estimates. Dateline boxes use two branches joined by `UNION`; spatial `OR` predicates
do not produce two R-tree scans.

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

Release preparation uses a private UUIDv7 attempt. Transactions write at most 256
complete source features or 32 MiB of canonical feature data. Stable logical IDs span
releases; rows also carry tenant, immutable release, attempt and contiguous ordinal.

The completion-ledger transaction checks row count, distinct ordinals, ordinal range
and logical-ID uniqueness for each high-volume table, plus raster count. Only its
winning attempt becomes readable or activatable. Interrupted rows may persist but
cannot enter tools, resources, routing or spatial queries. Eight retained attempts
per release stop further preparation with a projection-rebuild diagnostic. Deployment
supports one Map replica and one release writer.

Source tags stay in the immutable feature JSON. Equality predicates match only
JSON strings, while existence predicates include a present JSON null. JSON
Pointer escaping protects tag keys containing `/` or `~`. This avoids the
write amplification of an exploded tag table without weakening release and
attempt isolation.

Unmarked managed tables also fail analytical-schema admission. Projection rebuilds
preserve SurrealDB, artifacts and retained release products, then replay those products
before activation. The upgrade requires neither source reacquisition nor a
compatibility migration.

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
| `search_locations` | direct | `map:dataset:read` | summary resource links for named locations and optional facilities |
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
| `prepare_route_handoff` | direct | `map:route` | current validated `veoveo.ai/map-route-handoff/v2` projection for a consuming domain |
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

After contribution binding, the shared recovery observer applies the initial retained
Task report before HTTP starts. SQL change notifications and lease deadlines revisit
live leases within that finite startup set. Existing operation, request and claim
guards admit resumable work. A failed claim skips local scheduling only when current
SQL proves physical absence, terminal settlement, or a matching Task held by another
worker's unexpired lease. An unproven conflict or observation error stops HTTP,
cancels resource observation, and reaches the owned Valhalla shutdown.

The listed scope is necessary but not sufficient for a Work Context-owned
object. Reads require effective read access, edits require write access, and
publication, product creation, or archival requires effective admin access.

Map uses stateless Streamable HTTP. Ordinary responses are JSON, while
`subscriptions/listen` owns the request-scoped SSE stream carrying task,
resource, and resource-list notifications.

### Resources

The [resource surface](RESOURCES.md#resource-surface) defines Map's root addresses,
resource templates, visibility rules, page and cursor semantics, URI relationships,
client upgrade policy and subscription scopes. Resource readers apply current SQL
visibility and parent selection before keyset predicates and limits; typed owner
addresses and cursors preserve those relationships through dispatch.

### Catalog Maintenance Queries

`list_active_dataset_releases` selects tenant-owned pointers and their active releases
in one SQL statement. Source and dataset filters precede the limit of 1–100 results;
one additional row determines `truncated`. The release must belong to the pointer's
tenant and dataset. Results order by dataset ID, and selected documents must agree
with their indexed identities, versions, validity and digest. Internal reconciliation
continues to read the complete pointer inventory.

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
and composition identities from the caller's scope. Map SQL and DuckDB apply scope,
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

Workers enter domain work only after the initial Task checkpoint returns Running under the executing live lease. A terminal or rejected checkpoint stops dispatch.

Resume worker updates use TaskRuntime settlement with genuine failures preserved. Final success dispatch carries the local stop token and only a committed success triggers the travel-model notification. Cancellation changes Task delivery; it does not undo prior feature imports, Artifact publication or usage. Import recovery retains its stored Resume class. After current Write membership and parent-policy admission, a retry reads the retained changeset and feature revisions before preparing mutations against the original layer revision. The receipt read repeats tenant, Work Context, labels, classification and active-parent guards in one Store snapshot. The request digest must match; no write is replayed. Returned changeset and features are the committed records; projection state describes current reconciliation. New commits keep their revision and idempotency fences.

## Installation Bootstrap

Map consumes the platform's generic server-bootstrap contract
(`veoveo_mcp_contract::ServerBootstrapDocument`): a `server: map` envelope with
a `tenantKey` and a Map-owned payload of `sources` and `mobilityProfiles`.
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
tables include `tenantKey` in their primary keys, and every active-release
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

The acquisition helper uses Pydantic `2.13.5` for its private request and result
objects. Both decode entrypoints require the current schema revision and admit
only model-declared wire keys, including mixed-key refusal. Existing path and
acquisition-ID checks run before filesystem effects. `data/uv.lock` fixes the Python dependency
graph. The image installs its hash-verified packages in `/opt/veoveo/map-python`,
which the configured system Python reads through `PYTHONPATH`.

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

Rust contracts live in `src/contract`; acquisition, authoring, routing and spatial
services keep their own modules. `data/src/map_data` owns the Python helpers, and
`app` builds the browser asset. The [code map](../../docs/CODEMAP.md) indexes these
modules, their query owners and their qualification paths.

## Verification

[Input cases](testdata/controlled-inputs.json) qualify travel-time, spatial, raster, mobility and mutation branches in [hosted tests](src/mcp/tool_input_tests.rs) and the [consumer](../../testing/fixtures/server-contract-consumer/DESIGN.md).

Rust/Python [protocol checks](../../testing/python/DESIGN.md) qualify
`testdata/private-protocol.schema.json`; owner checks cover filesystem confinement,
release labels and digests.

- Contract tests cover IDs, quantities, geometry, mobility taxonomy, sources,
  geodesics, graph costs, Valhalla limits, URIs, paging, stable feature IDs,
  routing archives, travel-model bounds and activation.
- DuckDB tests cover controlled HTTPS, closed Spatial axis selection, effective
  settings and a pinned extension-backed meter baseline.
- Python tests cover typed contracts, bounded GTFS acquisition and validation,
  unsafe ZIP rejection, subprocess deadlines, process-group termination and
  bounded diagnostics.
- SurrealDB 3.3 tests install the schema and verify atomic, version-checked release
  activation. Console TypeScript and production Vite builds check administrative
  projection.
- App contract tests cover one permission-aware App resource, same-origin light/dark
  basemaps and CSP, MCP bridge operations, immutable-publication and active-release
  queries, guided GeoPackage tasks, subscriptions, the MapLibre pin and hardware
  WebGL2 rejection.
- Browser smoke serves the generated App in Console's opaque-origin sandbox with
  the local MapLibre Style CSP. Headed Chrome proves NVIDIA WebGL before testing
  bounded publication-pinned and active-release viewport queries, light/dark changes
  that preserve camera and overlays, synchronized map/table selection, data inspection
  with the map visible, and screenshot capture.
- Image builds verify pinned Spatial and package GDAL, Osmium, Valhalla and Python.
  Rust Map smoke runs that image against real SurrealDB 3.3 and Artifact services.
  It acquires and activates authority, OSM and governed-network fixtures, rejects
  bad source digests before staging, and queries named locations, facilities,
  boundaries and corridors.
- The same smoke checks MCP Task creation and completion for real Valhalla road
  and governed-graph maritime routes, persistence, restriction risk, withdrawal
  and dependent-route invalidation.
- Broader smoke and conformance suites cover gateway, control-plane, offline,
  Task and MCP behavior. The cross-server test serializes Map travel models and
  deserializes them directly into Optimization's contract without translation.

The principal local commands are:

```text
cargo test -p veoveo-map-mcp --lib
cargo test -p veoveo-platform-store --lib
uv run --project servers/map-mcp/data --locked --extra test python -m pytest servers/map-mcp/data/tests
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

## HTTP Probes

The shared host serves `/map/healthz` and `/map/readyz` with its
normal Host validation. Liveness checks the spatial engine, supervised routing process and routing HTTP
endpoint. Readiness requires those checks and a query against the selected platform
database to succeed within five seconds. Database loss affects readiness without
turning a recoverable connection outage into a liveness-driven restart.

## Identity Declaration Mechanics

Map ID declarations use `Id` with Map-owned prefixes, UUID admission and stable-key namespace. Owners that accept UUID aliases preserve their input spelling; canonical owners require RFC spelling. UUIDv5 stable-key generation and UUIDv7 fresh generation share the owner admission path. Coordinate names and travel keys keep their separate lexical validators and schema profiles.

## Value Admission

Quantity types share the Map finite, nonnegative validator, preserve negative zero,
and expose copied const getters. `Degrees` has no 360-degree cap. `Ratio` has its
separate inclusive zero-to-one admission; errors retain the quantity name, value
and rule.

SourceSummary stores unchanged summary fields through `Checked` with Map-owned parent and metadata relationships. Restriction summaries keep their explicit URI-derived identity adapter.

Immutable route, source-feature, raster, derivation, travel, feature, publication,
layer-product and composition products store their admitted fields in `Checked`.
Construction and deserialization apply the same owner relationships. Route and
spatial URI identities match their roots; travel manifests name the published
Artifact. Composition revisions name their composition, and feature layer schema
and style revisions name their layer. Producers consume a value and admit a new
product when changing immutable fields.

Source, release, restriction, acquisition and feature-layer lifecycle models allow
progress mutation. Their constructors, decoders and serializers apply common value
checks; serialization cannot publish an invalid mutation. Layer revision zero is
the initial empty layer, while schema and composition revisions begin at one.
Restrictions require a typed limit for the Limit effect. Other effects admit an
optional limit. Present vertical bounds are finite and ordered, including equal
or negative heights; omitted bounds are unbounded.

Repository APIs carry owner IDs and typed dependency or derivation variants to
driver binding. The layer-product driver maps public `geo_json_seq` and `geo_parquet` to stored
`geojson_seq` and `geoparquet` values; the public spelling and column values are independent. Readers compare selected physical identities, parent indexes and
retained documents after SQL visibility selection. Layer and composition hydration
checks duplicated owner, classification and data-label values against selected
policy columns. Corrupt selected bodies fail; denied and foreign rows stay outside
decoding. Exact release sets preserve each
operation's lifecycle policy and avoid decoding unrelated bodies. Activation uses
the requested dataset pointer and checks the source/release dataset before preparing
products. The complete pointer inventory checks every referenced release without
applying a public page limit.

Feature related and evidence references use the generic `ResourceUri` profile,
with eight existing domain schemes, 1024 bytes per item and 64 items per list.
References preserve escaped text, ports, queries and fragments. Admission requires
a lowercase scheme and an absolute hierarchical URI; raw spaces or Unicode,
malformed escapes, templates and opaque spellings fail. Concrete route parsers
and their separately qualified ID aliases retain their own profiles. Open provider
properties and source metadata keep their existing value checks.

## Cursor Admission

Source, restriction, mobility and travel cursors retain canonical hexadecimal JSON under owner codecs. Metadata continuation preserves admitted text and keeps selection agreement in its explicit resume adapter. Knowledge cursors retain on-demand member encoding. Anonymous feature and source-query cursors keep their existing base64url payloads; query digest and ordering checks remain in their query owners. Page wire checks precede fixed-limit domain projections.

## Persistence Module Declaration

The `runtime` feature admits retained Task results through RMCP's `CallToolResult`
type and enables that existing dependency. The `mcp` feature separately enables the
server entrypoint. Contract and schema consumers exclude RMCP and analytical runtimes.

The independent `schema` feature exports `schema::module_setup(execution)` for the
`map` optional module. It activates `veoveo-modules` with default features disabled and the foundational
vocabulary, Serde and schema dependencies used by owner table declarations. MCP,
Store and asynchronous runtime dependencies require their own features. Default
runtime behavior is unchanged. The declaration claims `map_*`.
It requires Tasks, including earlier kernel lanes through transitive requirements.

The version-zero lane installs the current Map schema from
`servers/map-mcp/src/schema/migrations/0000_current.surql` and the typed travel-model
Task lookup lane body beside it. The composition root supplies
its checked execution image and command. Gateway composition prepares runtime
credentials and executes selected owner lanes through `module-migrate`. The
runtime opens the shared authenticated Store connection without applying schema.
Installed image and Job qualification is tracked separately in the active contract plan.

The Map lane initializes `map_projection_state:authored_features` with
`last_sequence = 0`. The changeset event advances this head on committed feature
edits. Replaying an installed lane preserves the existing head and its sequence.

Client-facing geographic or temporal integrations imply no optional-module dependency.
Kernel queries require their owners' APIs.

Map projection recovery resolves retained tenant/context keys through
`tenant_context_keys_v1`. Disabled authority keeps its immutable journal metadata
available. An absent or mismatched relationship fails typed hydration of the indexed
commit page and stops checkpoint advancement; recovery never skips such a commit.

## Persistence Observation

The `schema` feature exposes closed `MapObservationTable` names. Runtime consumers
compose their checked `ObservationTable` descriptors with kernel tables. Descriptor
admission validates identifiers, not installed schema or read authority. Owner tables
declare the installed SQL's 30-day changefeed retention. LIVE invalidation and
changefeed recovery preserve reconciliation and checkpoint behavior. Public DTO
contract features do not activate observation sources.

## Task Completion Products

Task product output uses `MapTaskProduct<T>`. The owner type derives the canonical
address for retained routes, matrices, travel models, raster derivations, imported
changesets and published layer products. Decoding checks that address against the
metadata. Each result has one product link; source and secondary Artifact references
stay in structured output. Reachable areas return inline polygons and a calculation
ID. GeoPackage inspection returns its typed source Artifact reference and manifest.
Neither inline operation creates an addressable result resource.

## Browser Contract Admission

The App imports its generated `app/generated` types and JSON Schema bundle from
`contract::app_schema`. The bundle selects the same DTOs that tools and resource
readers serialize. Browser adapters validate a complete result before publishing
it to state, then check its requested parent identities. Collection pages keep the
owner's cursor representation and existing stale-response checks. Open provider
maps keep their admitted contents.

The shared MCP Apps browser package bundles the maintained SDK protocol schemas
and CSP-safe CfWorker JSON Schema validator into the App. It has no domain route
registry. This server owns the route and tool selection in `app/contracts.js`.
The package build produces the self-contained HTML asset at its existing path,
with a 2 MiB limit. Schema URLs describe formats and never fetch executable code.
Behavioral contract tests qualify rejection before rendering or decoding; they
make no hardware or visual acceptance claim.

The App build uses parse5 8.0.1 to inspect browser-decoded HTML attributes and CSS load sites. This development dependency handles unquoted attributes and character references without rewriting packaged bytes; schema identifiers and validator URL metadata do not trigger remote asset rejection.

The source-feature query contract owns its cursor-independent selection digest.
`QuerySourceFeaturesRequest::query_digest_sha256` hashes current serialized request
bytes with the `veoveo.ai/map/source-feature-query/v3` domain and NUL separator,
after removing the continuation. The public result and private cursor keep this
identity as `Sha256Digest`; their declared wire adapter emits bare lowercase hex.
