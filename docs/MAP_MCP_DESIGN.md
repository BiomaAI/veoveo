# Map MCP Design

`map-mcp` is Veoveo's Earth geography and logistics-routing domain. Agents use
one strongly typed MCP surface to find places, inspect facilities and borders,
work with coordinates, apply transport restrictions, calculate routes, build
matrices, and inspect reachable areas. Source administration runs through REST
on the same server and through the Console projection of that API.

Map returns data. It does not render an image, run a browser, or expose a public
tile endpoint. A renderer such as Mapbox MCP can consume Map results later
without becoming the source of route truth.

## Status

Implemented in this workspace.

The implementation includes the Map domain contract, SurrealDB records,
tenant-scoped DuckDB Spatial tables, a supervised Valhalla land engine, a
governed network planner, source acquisition, release activation, MCP discovery
surfaces, administrative REST, gateway proxying, Console workflows, Compose,
Helm, and offline image registration.

The canonical service identity is:

```text
crate       veoveo-map-mcp
folder      servers/map-mcp
slug        map
URI scheme  map
MCP         /map/mcp
admin REST  /map/admin
health      /map/healthz
```

Gateway-mounted tools use names such as `map__route`. Resource identities keep
the `map://` scheme.

## Responsibility

The three spatial domains answer different questions.

```text
frames-mcp
  Where is this pose relative to another frame at this time?

map-mcp
  Where is this on Earth, and can this mobility profile travel there?

optimization-mcp
  Which asset, assignment, schedule, or stop sequence should be selected?
```

Map owns:

- WGS84 geography, projected CRS transformations, and ellipsoidal geodesics;
- locations, facilities, boundaries, map datasets, and effective restrictions;
- versioned human and vehicle mobility profiles;
- route feasibility, geometry, cost, provenance, matrices, and reachable areas;
- governed source acquisition and immutable release activation;
- map-owned analytical and routing-engine projections.

Frames owns ECEF, ENU, NED, FRD, body, sensor, simulation, and time-indexed
frame graphs. A caller converts robot-local data through Frames before sending
an Earth-referenced request to Map. Map never hides that cross-domain operation
behind an internal service call.

Optimization consumes Map route costs and feasibility. It owns fleet selection,
task assignment, stop ordering, concurrency, and transfer choice. Map exposes
intermodal facilities and compatible map families; Optimization composes the
actual multi-asset plan.

DuckDB MCP remains a general owner-scoped SQL service. Map links the same
hardened DuckDB runtime as a library and owns its database and SQL. Map does not
call public DuckDB tools or expose arbitrary SQL.

## Non-Goals

- Map is not a map-image renderer or browser application.
- Map is not an XYZ, WMTS, WMS, or vector-tile HTTP server.
- Map is not an ArcGIS editing, layout, or enterprise-catalog replacement.
- Map does not own relative robotics transforms.
- Map does not select vehicles or command actuators.
- Planning-advisory maritime, aviation, rail, and off-road results are not
  certified navigation products.
- Missing data never becomes an invented straight-line route or a fabricated
  clearance.

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
  |-- MCP protocol and administrative REST
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
volume; no helper container is required.

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

Frames converts WGS84 to ECEF and local ENU or NED frames. The shared
`mcp/contract/src/coordinates.rs` types keep these domains interoperable; that
file is not a separate server.

## Persistence

SurrealDB is the canonical operational catalog. It stores:

- registered sources and immutable dataset-release records;
- active release pointers and optimistic record versions;
- mobility profiles and effective restrictions;
- operational snapshots, routes, dependencies, and matrices;
- acquisition jobs and durable task state.

DuckDB Spatial is the local analytical projection. Its schema is tenant keyed
and contains active-release pointers, locations, facilities, boundaries, and
governed network edges. Spatial queries use `ST_Contains`, `ST_Intersects`, and
`ST_Distance_Sphere`. The Spatial extension is copied into the image at build
time and loaded only from its pinned local path.

The artifact plane stores immutable raw source bytes, normalized products,
routing builds, quality reports, and large task outputs. Cross-server artifact
identity remains `artifact://{artifact_id}`. Map projects those artifacts as
`map://artifact/{artifact_id}` only after applying the normal artifact policy.

## Authoritative Data Acquisition

A map release is a governed occurrence of source bytes, not a tile archive.
Every registered source declares authority, coverage, map families, acquisition
model, location, media types, limits, license, and credential references.

Authority is evaluated per fact and region. An official bridge-clearance source
can supersede a community road tag while the same community release continues
to supply nearby road geometry. A newer timestamp alone does not establish
greater authority.

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

OpenStreetMap is an excellent global baseline, not a legal authority. Legal
borders, clearances, navigational charts, airspace, and effective restrictions
must use the responsible publisher where the operation depends on them.

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

The current acquisition worker accepts `snapshot` records only. An
`osm_replication` location therefore acquires its registered snapshot and
retains the replication endpoint as source metadata; it does not apply diffs.
Sequenced deltas, effective-event feeds, and observation streams require their
dedicated ingestion paths. GTFS Realtime is rejected as a base release.

This distinction is intentional. Snapshot handling is implemented and tested.
Delta continuity, update-chain rules, and feed expiry must not be implied by a
generic downloader.

### Network And File Controls

The Rust process resolves the registered source. The helper never receives an
arbitrary URL from an agent or browser.

HTTPS acquisition enforces:

- HTTPS only, no URL credentials, and no fragments;
- registered endpoint and redirect-host allowlists;
- public resolved addresses, including every redirect target;
- bounded redirect count, response bytes, and one absolute elapsed deadline;
- registered response media types;
- controlled bearer or `x-*` credential headers loaded from secret files;
- no proxy variables inherited from the surrounding installation.

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

No package or DuckDB extension is installed when the container starts or when
an acquisition runs.

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

The generic maritime and aviation conversions are intake primitives, not a
claim of full S-57 update-chain, S-100 product, AIXM timeslice, or NASR product
semantics. Product-specific validation belongs in the corresponding adapter
before operational reliance.

GeoJSON and GeoJSON Sequence products feed the analytical projection. Named
points become locations unless `facility_kind` is present. Polygon features
become boundaries. A LineString becomes a governed network edge when it carries
`from_node`, `to_node`, `map_family`, and `nominal_duration_s`; optional fields
include `distance_m` and `bidirectional`. Feature ids derive stable UUIDv5 Map
ids from source identity and source feature identity.

### Acquisition Jobs

`POST /map/admin/acquisitions` accepts a registered source id, a requested
WGS84 bounding box, an idempotency key, and an optional
`expected_source_digest_sha256`. When supplied, the digest is verified against
the downloaded bytes before a release is staged.

Jobs are durable catalog records with queued, running, succeeded, failed,
cancel-requested, and cancelled states. A successful job creates a staged
release. It never activates data implicitly. After a server restart, listing
jobs marks interrupted work failed; the operator starts a new idempotent
acquisition.

Public failure messages identify the phase without copying helper stderr or
licensed source excerpts. Bounded diagnostics stay in server logs. The current
runtime does not publish a diagnostics artifact, so `diagnostics_uri` remains
absent.

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
are rejected. Activation reuses those cached products; rollback does not rebuild
the graph or regenerate a complete output.

Activation follows this sequence:

1. Validate and ingest retained release products into tenant-scoped DuckDB
   rows that are not yet selected.
2. Atomically update the SurrealDB release state and active dataset pointer
   under expected release and pointer versions.
3. Atomically switch the Valhalla active-directory symlink on Unix and update
   the DuckDB active pointer.
4. Restart the supervised Valhalla process when the release has routing data.
5. Retire the previous release and invalidate routes that depend on it.

The SurrealDB state and pointer share one database transaction. Local DuckDB
and filesystem projections cannot share that transaction. A failure after the
catalog commit returns an error and leaves the canonical active release intact.
Calling `activate` again with the active release and current record versions
performs an idempotent local reconciliation; the Console exposes this as
`Reconcile`.

Map deploys as one replica with one persistent `ReadWriteOnce` volume. The
activation mutex serializes local product switches inside that process. A
future multi-replica deployment requires an explicit projection-distribution
design rather than sharing mutable local state by accident.

Licenses travel with each release. The contract records attribution,
redistribution, derivative, offline-bundle, and expiry policy. Offline packaging
registers the Map image and tooling; it does not silently bundle licensed map
content.

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
auto, truck, or bus costing and rejects unsupported limits rather than
clamping. Valhalla produces route geometry, maneuver instructions, distance,
duration, alternatives, and land isochrones.

### Governed Networks

Off-road, rail, surface-vessel, subsurface-vessel, fixed-wing, rotorcraft, and
UAS profiles use explicit activated LineString edges for their map family. The
planner snaps endpoints within 10 km, verifies consistent node geometry,
applies avoided areas, and runs A* for fastest or shortest objectives. It
returns `planning_advisory` because source-specific certification and complete
vehicle physics are outside the generic graph adapter.

The governed graph never invents a connection between disconnected nodes.
Alternative routes and required-area constraints are not available on this
adapter. A caller must explicitly allow planning-advisory output.

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
paginated. A direct call to a task-only tool returns an explicit instruction to
use task invocation.

Map uses stateful Streamable HTTP sessions and SSE responses. This transport
keeps Task API traffic, resource subscriptions, `resources/updated`, and
`resources/list_changed` notifications on the canonical MCP session instead of
collapsing them into a one-response JSON channel.

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

Source resources omit credentials. Routes and matrices are owner scoped.
Dataset, geography, profile, and restriction resources are tenant scoped.

### Prompts And Completions

Map exposes `prepare_route_request`, `review_route`, and
`prepare_logistics_matrix`. They direct the client to inspect profile versions,
provenance, status, bounds, and advisory rules.

Completion applies to resource-template arguments and returns only visible ids.
The implementation completes source, dataset, release, location, facility,
profile, restriction, route, and matrix identities from the caller's scope.

### Subscriptions And Notifications

Mutable collection resources for datasets, restrictions, routes, and matrices
are subscribable. Immutable instance resources are not. The server emits
resource-update and resource-list-change notifications after relevant
mutations. Subscription state is session local; durable long-running work uses
task subscriptions.

## Administrative REST

Administrative acquisition is not an MCP tool. The same Axum process exposes a
typed REST tree protected by the gateway-signed identity and `map:admin`.

| Method and path | Purpose |
|---|---|
| `GET /map/admin/sources` | list sources with `enabled` and `adapter_kind` filters |
| `GET /map/admin/sources/{source_id}` | read one source |
| `POST /map/admin/sources` | register one source |
| `PUT /map/admin/sources/{source_id}` | replace source configuration under expected version |
| `POST /map/admin/sources/{source_id}/disable` | disable future acquisition |
| `GET /map/admin/acquisitions` | list jobs with source and status filters |
| `GET /map/admin/acquisitions/{acquisition_id}` | read one job |
| `POST /map/admin/acquisitions` | start a snapshot acquisition |
| `POST /map/admin/acquisitions/{acquisition_id}/cancel` | request cancellation |
| `GET /map/admin/releases` | list releases with dataset, source, and state filters |
| `GET /map/admin/releases/{release_id}` | read one release |
| `GET /map/admin/active-releases` | read active pointers and their record versions |
| `POST /map/admin/releases/{release_id}/activate` | activate staged data or reconcile the active projection |
| `POST /map/admin/releases/{release_id}/rollback` | activate a retained release |
| `POST /map/admin/releases/{release_id}/quarantine` | quarantine an inactive release |
| `GET /map/admin/mobility-profiles` | list profiles, optionally by family |
| `GET /map/admin/mobility-profiles/{profile_id}/versions/{version}` | read an immutable profile version |
| `POST /map/admin/mobility-profiles` | register an immutable profile version |

Lists use opaque `map-admin-v1` cursors, default to 50 records, and cap a page at
200. Creation requests use idempotency keys. Source and release mutations use
expected record versions. Activation also uses the expected active-pointer
version. Semantic validation can return `400`; JSON extraction can return
`422`; concurrency conflicts return `409`.

The gateway proxies only the catalog-resolved generic path:

```text
/admin/{profile}/servers/map/...
```

It accepts bounded GET, HEAD, POST, and PUT requests, replaces internal identity
headers, signs the authenticated principal, and writes the generic gateway
audit outcome. It never accepts an upstream URL from the caller.

The Console BFF exposes the same operations beneath
`/console/api/map/{path}`. The React page lists sources, acquisitions, releases,
active pointers, and mobility profiles. It can register raw typed source and
profile documents, start and cancel acquisitions, activate, reconcile,
rollback, and quarantine releases. Browser code never receives the gateway
bearer or invokes the helper directly.

## Isolation And Security

Every SurrealDB catalog read includes the tenant id. Owner-scoped routes,
matrices, acquisition jobs, and artifacts also check the principal. DuckDB
tables include `tenant_key` in their primary keys, and every active-release
lookup is tenant constrained.

The public server validates the Host authority and a gateway-signed internal
token. Tool handlers enforce domain scopes again after gateway policy. The
administrative router has its own scope gate. Secret references are bounded
identifiers, and secret material never appears in MCP resources or Console
payloads.

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
optimistic-concurrency conflicts all fail explicitly. No fallback changes the
question being answered.

## Deployment

Compose builds `servers/map-mcp/Dockerfile`, mounts one `map-data` volume, mounts
the optional source exchange read-only, and exposes only port 8799. The Helm
chart deploys one replica with a 100 GiB `ReadWriteOnce` claim. The offline
image lock contains `veoveo/map-mcp:0.1.0` and its Dockerfile.

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
10001. System packages are never installed at startup.

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

This suite is risk-based. It does not enumerate every mobility class against
every map family, contact public data providers, or duplicate the complete
task-runtime recovery and cancellation matrix inside the Map smoke. Those
combinations remain deliberate integration and acceptance-test work as real
authority datasets and certified performance models are introduced.

## Deliberate Follow-On Work

The following work is not presented as implemented:

- OSM replication and other sequenced-delta application;
- durable operational feed ingestion for GTFS Realtime, traffic, weather,
  tides, currents, NOTAMs, and navigational warnings;
- product-specific S-57 update, S-100, AIXM, NASR, and environmental validators;
- certified vessel, aircraft, rail, and terrain performance models;
- automatic multi-profile intermodal journey construction;
- a separate renderer or tile service when agent image reasoning is required;
- multi-replica distribution of local DuckDB and Valhalla projections.

These additions extend the typed release and route model. They do not require
another mapping domain or raw database queries for agents.
