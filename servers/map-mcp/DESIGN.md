# Map MCP Design

This document is the canonical design and operational contract for the
`map-mcp` crate.

`map-mcp` is Veoveo's Earth geography and logistics-routing domain. Agents use
one strongly typed MCP surface to find places, inspect facilities and borders,
work with coordinates, apply transport restrictions, calculate routes, build
matrices, inspect reachable areas, and author governed feature layers. Source
administration runs through the same MCP surface: scoped tools for mutations,
`map://` resources for reads, and MCP App views that hosts render
(see `mcp/apps-extension/DESIGN.md`).

## Status

Implemented in this workspace.

The implementation includes the Map domain contract, SurrealDB records,
tenant-scoped DuckDB Spatial tables, a supervised Valhalla land engine, a
governed network planner, source acquisition, release activation, MCP discovery
surfaces, administrative MCP tools, the administration MCP App view, gateway
proxying, Helm, and offline image registration.

The canonical service identity is:

```text
crate       veoveo-map-mcp
folder      servers/map-mcp
slug        map
URI scheme  map
MCP         /map/mcp
admin app   ui://map/admin.html
health      /map/healthz
```

Gateway-mounted tools use names such as `map__route`. Resource identities keep
the `map://` scheme.

## Domain Scope

Map answers where something is on Earth and whether a specific mobility
profile can travel there. It provides:

- WGS84 geography, projected CRS transformations, and ellipsoidal geodesics;
- locations, facilities, boundaries, map datasets, and effective restrictions;
- versioned human and vehicle mobility profiles;
- route feasibility, geometry, cost, provenance, matrices, and reachable areas;
- governed source acquisition and immutable release activation;
- Work Context-owned GeoJSON and JSON-FG feature authoring, revision, query,
  tombstone, restore, and publication;
- map-owned analytical and routing-engine projections.

Optimization consumes Map feasibility and route costs to compose fleet
selection, assignments, schedules, stop sequences, and multi-asset transfers.
Map embeds the hardened DuckDB runtime as a library and owns its analytical
database and SQL policy.

## Architecture

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
  |-- MCP protocol, administrative tools, and the admin app view
  |-- source catalog and release service
  |-- PROJ and GeographicLib calculations
  |-- DuckDB Spatial analytical projection
  |-- supervised loopback Valhalla process
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

## Coordinate Contract

WGS84 longitude and latitude are the canonical route exchange. Optional height
is ellipsoidal unless a contract states otherwise.

Map provides bounded two-dimensional CRS transformation through PROJ. It
rejects geocentric EPSG:4978 and vertical values instead of silently copying
or mis-transforming them. GeographicLib supplies WGS84 direct and inverse
geodesics. Geofence validation checks segment geometry, not only vertices.

The shared `mcp/contract/src/coordinates.rs` types keep WGS84 exchange
consistent across Veoveo services.

## Persistence

SurrealDB is the canonical operational catalog. It stores:

- registered sources and immutable dataset-release records;
- active release pointers and optimistic record versions;
- mobility profiles and effective restrictions;
- operational snapshots, routes, dependencies, and matrices;
- acquisition jobs and durable task state.
- authored feature layers, schema and style revisions, feature revisions and
  heads, atomic changesets, and immutable layer publications.

DuckDB Spatial is the local analytical projection. Its schema is tenant keyed
and contains active-release pointers, locations, facilities, boundaries, and
governed network edges. Spatial queries use `ST_Contains`, `ST_Intersects`, and
`ST_Distance_Sphere`. The Spatial extension is copied into the image at build
time and loaded only from its pinned local path.

The artifact plane stores immutable raw source bytes, normalized products,
routing builds, quality reports, and large task outputs. Cross-server artifact
identity remains `artifact://{artifact_id}`. Map projects those artifacts as
`map://artifact/{artifact_id}` only after applying the normal artifact policy.

## Authored Feature Layers

An authored layer is a governed operational dataset inside one Work Context. The
gateway resolves its business owner, initial grants, classification, labels,
membership, policy revision, and invocation provenance. Map stamps that authority
on the canonical layer and every changeset instead of accepting authority fields
from a tool request.

Feature geometry uses WGS84 GeoJSON coordinates. The complete canonical feature
remains a valid GeoJSON Feature and adds JSON-FG `featureType` and valid-time
members. The initial geometry set covers Point, MultiPoint, LineString,
MultiLineString, Polygon, and MultiPolygon. Validators reject non-finite or
out-of-range coordinates, malformed topology, incorrect polygon winding,
unbounded property payloads, remote JSON Schema references, and unsafe style
expressions.

SurrealDB owns the immutable truth. A direct commit contains at most 100 mutations
and 1 MiB. One transaction checks the expected layer revision and each expected
feature revision, creates immutable feature revisions, advances feature heads,
records a scoped idempotent changeset, and appends the outbox event. The changeset
stores the event sequence needed for read-your-write projection checks. A repeated
idempotency key returns the original changeset only when its request digest matches.

DuckDB Spatial is a rebuildable query projection. Its outbox consumer writes a
revision table, a current-head table, R-tree indexes, and a local contiguous
checkpoint in one transaction. Queries can select a current layer or a published
layer revision. They accept a validated WGS84 bounding box, open valid-time
interval, geometry type, opaque keyset cursor, and a bounded CQL2 JSON subset.
Property paths and literal values remain parameters. A dateline-crossing box is
split into two query polygons.

The public MCP surface includes create, update, validate, commit, query, restore,
publish, and archive tools. Layer heads, schema revisions, style revisions,
feature queries, feature heads and revisions, changesets, and publications are
URI-addressed resources. Mutable heads and indexes support MCP subscriptions and
resource-update notifications. Individual features are never expanded into the
resource list; agents traverse them through the paginated query template.

`reference`, `named_locations`, `facilities`, `boundaries`, and
`network_candidate` are authoring classifications, not routing authority. A
generic feature commit or publication never changes an active source release or
Valhalla data. Routing influence requires a separate governed validation and
release-promotion operation.

## Authoritative Data Acquisition

A map release records one governed occurrence of source bytes. Every registered
source declares authority, coverage, map families, acquisition model, location,
media types, limits, license, and credential references.

Authority is evaluated per fact and region. An official bridge-clearance source
can supersede a community road tag while the same community release continues
to supply nearby road geometry. Publisher responsibility and validity determine
precedence alongside time.

### Recommended Sources By Domain

| Domain | Practical baseline | Higher-authority additions |
|---|---|---|
| roads, paths, names, places | regional OpenStreetMap PBF | transport departments, municipalities, bridge and tunnel operators, border and customs authorities |
| rail and public transport | OSM geometry and GTFS Schedule | infrastructure managers, timetable publishers, station and terminal operators |
| borders and jurisdictions | OSM for general context | responsible cadastral, statistical, customs, maritime-limit, or civil-aviation authority |
| maritime | licensed S-57 ENC during transition | hydrographic-office S-100 products, port and navigation authorities |
| aviation | authority exchange sets | AIS or ANSP AIXM, effective AIRAC releases, FAA NASR where applicable |
| facilities | OSM discovery | port, airport, depot, warehouse, fueling, charging, and terminal operators |
| terrain and conditions | installation-selected environmental source | responsible weather, hydrology, ocean, terrain, and traffic authority |

OpenStreetMap supplies the global baseline. Operations that depend on legal
borders, clearances, navigational charts, airspace, or effective restrictions
select the responsible publisher for those facts.

### Registered Source Contract

`RegisteredSource` controls acquisition before any network or file operation.

```text
source_id
dataset_id
name
adapter_kind
authority
acquisition_model
map_families
location
credential?
publisher_key_refs
expected_media_types
maximum_download_bytes
maximum_elapsed_seconds
license
enabled
record_version
```

`SourceLocation` is a tagged enum:

- `https` contains one HTTPS endpoint and explicit redirect hosts;
- `osm_replication` records a snapshot endpoint and replication endpoint;
- `mounted_exchange_set` contains a controlled mount id and relative path.

The current acquisition worker processes snapshots. An `osm_replication`
location acquires its registered snapshot and retains the replication endpoint
as source metadata. The contract classifies sequenced deltas, effective-event
feeds, and observation streams as operational feeds governed by continuity,
update-chain, and expiry rules.

### Network And File Controls

The Rust process resolves every input from a registered source before invoking
the helper.

HTTPS acquisition enforces:

- HTTPS endpoints without embedded credentials or fragments;
- registered endpoint and redirect-host allowlists;
- public resolved addresses, including every redirect target;
- bounded redirect count, response bytes, and one absolute elapsed deadline;
- registered response media types;
- controlled bearer or `x-*` credential headers loaded from secret files;
- direct connections governed by the registered host policy.

Mounted inputs are canonicalized beneath the installation exchange root and
must be regular files. Job workspaces are unique. Paths returned by the helper
must remain inside the job output directory.

### Same-Container Acquisition Application

The Python package lives under `servers/map-mcp/data/` and is locked by
`uv.lock`. Rust writes one typed JSON command to stdin and accepts one typed
JSON result from stdout. The helper uses argument arrays without a shell,
bounds diagnostic output, applies a wall-clock limit, and terminates its whole
process group on timeout or cancellation.

The image includes:

- GDAL and `ogr2ogr` for vector normalization;
- Osmium for OSM PBF validation;
- Valhalla graph-building utilities;
- Python only for controlled source-tool orchestration;
- the pinned DuckDB Spatial extension for the Rust runtime.

The image installs every package and the pinned DuckDB Spatial extension during
its build.

### Adapter Availability

| Adapter kind | Current snapshot behavior |
|---|---|
| `open_street_map` | checks PBF references, writes named-point GeoParquet and GeoJSON Sequence, builds and archives Valhalla routing data |
| `authority_vector` | uses GDAL to write GeoParquet and WGS84 GeoJSON |
| `gtfs_schedule` | checks safe ZIP expansion and required files, optionally runs a configured validator, retains a normalized ZIP |
| `environmental` | uses the authority-vector normalization path |
| `s57_enc` and `s100` | use the pinned GDAL maritime conversion path to GeoParquet |
| `aixm` and `faa_nasr` | use the pinned GDAL aviation conversion path to GeoParquet |
| `gtfs_realtime` | represented in the contract but rejected by base-release acquisition |

The generic maritime and aviation conversions establish the intake primitive.
Operational reliance adds product-specific S-57 update-chain, S-100 product,
AIXM timeslice, or NASR validation in the corresponding adapter.

GeoJSON and GeoJSON Sequence products feed the analytical projection. Named
points become locations unless `facility_kind` is present. Polygon features
become boundaries. A LineString becomes a governed network edge when it carries
`from_node`, `to_node`, `map_family`, and `nominal_duration_s`; optional fields
include `distance_m` and `bidirectional`. Feature ids derive stable UUIDv5 Map
ids from source identity and source feature identity.

### Acquisition Jobs

The `start_acquisition` tool accepts a registered source id, a requested
WGS84 bounding box, an idempotency key, and an optional
`expected_source_digest_sha256`. When supplied, the digest is verified against
the downloaded bytes before a release is staged.

Jobs are durable catalog records with queued, running, succeeded, failed,
cancel-requested, and cancelled states. A successful job creates a staged
release, and activation remains an explicit version-guarded operation. After a
server restart, listing jobs marks interrupted work failed; the operator starts
a new idempotent acquisition.

Public failure messages identify the phase without copying helper stderr or
licensed source excerpts. Bounded diagnostics stay in server logs.

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
planner snaps endpoints within 10 km, verifies consistent node geometry,
applies avoided areas, and runs A* for fastest or shortest objectives. It
returns `planning_advisory` until the selected sources and performance models
carry domain-specific certification. Planning requires connected activated
edges, supports fastest and shortest objectives, and accepts explicit avoided
areas. The caller opts into planning-advisory output through its data policy.

### Restrictions And Validation

Effective restrictions target mobility families and carry typed effects,
geometry, authority, and validity. Prohibitions become avoided areas during
planning. Route validation checks geometry, release availability, profile
availability, quarantine state, and intersections with active prohibitions.

Routes pin base release ids, one operational snapshot id, planner version, cost
model version, restriction ids, facilities, and validation identity. Release
changes and restriction withdrawal invalidate dependent routes while preserving
the original record for review.

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

Reachable areas are Valhalla isochrones for human and road profiles. All three
operations renew leases while running and resume after a server restart.

## MCP Surface

### Tools

| Tool | Invocation | Required scope | Result |
|---|---|---|---|
| `search_locations` | direct | `map:dataset:read` | bounded named locations and optional facilities |
| `inspect_location` | direct | `map:dataset:read` | location, nearby facilities, containing boundaries, lineage, gaps |
| `transform_crs` | direct | `map:dataset:read` | bounded 2D CRS transformation |
| `geodesic_inverse` | direct | `map:dataset:read` | WGS84 distance and azimuths |
| `geodesic_direct` | direct | `map:dataset:read` | WGS84 destination |
| `validate_geofence` | direct | `map:dataset:read` | topological and segment relationship findings |
| `route` | task only | `map:route` | persisted route with pinned provenance |
| `route_matrix` | task only | `map:route_matrix` | persisted many-to-many matrix |
| `reachable_area` | task only | `map:route` | land isochrone |
| `validate_route` | direct | `map:route` | typed validation findings |
| `inspect_corridor` | direct | `map:dataset:read` | restrictions, facilities, boundaries, and gaps |
| `publish_restriction` | direct | `map:restriction:publish` | effective restriction |
| `withdraw_restriction` | direct | `map:restriction:withdraw` | ended restriction and invalidation count |

All tool results use structured content schemas. Tool and resource lists are
paginated. Task-only tools use the durable task extension.

Map uses stateful Streamable HTTP sessions and SSE responses. This transport
keeps Task API traffic, resource subscriptions, `resources/updated`, and
`resources/list_changed` notifications on the canonical MCP session.

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
```

Resource templates are:

```text
map://source/{source_id}
map://dataset/{dataset_id}
map://dataset/{dataset_id}/release/{release_id}
map://location/{location_id}
map://facility/{facility_id}
map://mobility-profile/{profile_id}/{profile_version}
map://restriction/{restriction_id}
map://route/{route_id}
map://matrix/{matrix_id}
map://artifact/{artifact_id}
```

Source resources present public source fields. Routes and matrices are owner
scoped. Dataset, geography, profile, and restriction resources are tenant
scoped.

### Prompts And Completions

Map exposes `prepare_route_request`, `review_route`, and
`prepare_logistics_matrix`. They direct the client to inspect profile versions,
provenance, status, bounds, and advisory rules.

Completion applies to resource-template arguments and returns only visible ids.
The implementation completes source, dataset, release, location, facility,
profile, restriction, route, and matrix identities from the caller's scope.

### Subscriptions And Notifications

Subscriptions cover the mutable dataset, restriction, route, and matrix
collections. The server emits resource-update and resource-list-change
notifications after relevant mutations. Subscription state is session local;
durable long-running work uses task subscriptions.

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

Administrative reads are resources: `map://sources`, `map://datasets`,
`map://acquisitions` and `map://acquisition/{acquisition_id}` (map:admin),
`map://active-releases` (map:admin), and `map://mobility-profiles`. Creation
tools use idempotency keys; source and release mutations use expected record
versions; activation also uses the expected active-pointer version.
Validation failures surface as MCP invalid-params errors and concurrency
conflicts name the changed version.

The administration app view ships as `ui://map/admin.html` from
`assets/admin-app.html`: a self-contained document listed for `map:admin`
identities, linked to every administrative tool, discovered and hosted by any
MCP Apps host. The gateway projects it under `resource_projection:
server_owned`, and the Console renders it from its generic app catalog — no
map-specific console page, BFF route, or REST router exists.

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
- Admin pages accept at most 200 records.
- Source elapsed time is within 1 second and 24 hours.
- Routing archive expansion defaults to 16 GiB and 5,000,000 entries.
- Route task leases last 120 seconds and renew every 40 seconds.
- Task records default to a 7-day TTL.

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

Important arguments include:

```text
--map-database
--duckdb-spill-dir
--spatial-extension
--duckdb-memory-limit
--duckdb-threads
--valhalla-url
--valhalla-executable
--valhalla-config
--valhalla-active-dir
--acquisition-scratch-root
--release-root
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
      datasets.rs
      geometry.rs
      ids.rs
      mobility.rs
      operations.rs
      routes.rs
      units.rs
    routes/
      graph.rs
      service.rs
      valhalla/
        adapter.rs
        client.rs
        process.rs
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
    release_products.rs
    state.rs
    uris.rs
  data/
    src/map_data/
      adapters/
      contract.py
      main.py
      subprocesses.py
    tests/
    pyproject.toml
    uv.lock
  Dockerfile
```

## Verification

The implementation is checked at several boundaries:

- Rust contract tests cover ids, quantities, geometry, mobility taxonomy,
  source validation, geodesics, graph costs, Valhalla profile limits, URI
  parsing, paging, stable feature ids, routing archive bounds, and activation;
- DuckDB runtime tests cover controlled HTTPS source policy;
- Python tests cover typed contracts, a bounded GTFS acquisition with validator
  execution, unsafe ZIP rejection, subprocess timeout, process-group
  termination, and bounded diagnostics;
- SurrealDB integration tests apply the schema to SurrealDB 3.2 and verify
  atomic release activation under record versions;
- Console TypeScript and production Vite builds validate the administrative
  projection;
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

The principal local commands are:

```text
cargo test -p veoveo-map-mcp --lib
cargo test -p veoveo-platform-store --lib
uv run --project servers/map-mcp/data --frozen python -m unittest discover -s servers/map-mcp/data/tests -v
npm --prefix apps/console/web run build
docker build -f servers/map-mcp/Dockerfile -t veoveo/map-mcp:0.1.0 .
just smoke-map-mcp
```

The risk-based suite targets representative acquisition, land routing,
governed-network routing, restriction, invalidation, Task API, and persistence
boundaries. Authority datasets and certified performance models add their own
domain acceptance cases as they enter an installation.
