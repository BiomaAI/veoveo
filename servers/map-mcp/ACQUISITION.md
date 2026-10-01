# Map Source Acquisition

## Standards And Protocols

This design uses the Map server’s [standards profile](DESIGN.md#standards-and-protocols):
MCP 2026-07-28, OGC feature and tile formats, RFC 9562 identifiers and Veoveo Work
Context access. The supported source adapters and format subsets are specified below.

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

### Source Catalog Reads

Map owns source reads in `catalog/sources.rs`. The database applies tenant selection
before exact lookup, completion and page limits. Sources are shared within a tenant;
the creator is attribution, and disabled sources remain visible for administration.
The reader checks source and dataset identity, physical record identity, indexed name,
adapter, authority, map families, enablement and version against the validated document.
Store timestamps record persistence activity separately from document timestamps.
Queries have a five-second database deadline.

`MapSourceUri` accepts a typed `MapSourceId`; `MapSourcesUri` accepts the typed collection
cursor. `map://sources` returns `items`, `limit: 100` and nullable `next_cursor`, ordered
by source ID. A version 1 hex-encoded JSON cursor binds `map://sources` and the last
returned source ID. Each request reapplies current tenant access; pages do not hold a
snapshot across requests. Completion matches in SQL before grouping and its 101-row
lookahead limit. Selected malformed documents fail exact reads and pages explicitly.

`SourceSummary` exposes the same public metadata on exact resources and catalog pages.
It excludes acquisition locations, credentials, publisher key references, media-type
controls and acquisition limits. Its checked decoder validates name, license, families,
version and timestamps. Map Explorer follows every source page before replacing its
view; a continuation failure preserves the prior view.

Deploy Map and its page consumers together after draining acquisition and routing Tasks.
The foundations rollout uses a fresh disposable installation and the current typed
source format. Installed acceptance qualifies page traversal, current authority and
current-format restart recovery. The reader rejects selected malformed documents;
it supplies no historical-data conversion or weaker fallback reader.

### Network And File Controls

The Rust process resolves every input from a registered source before invoking
the helper.

`HttpsEndpoint` wraps the foundational `HttpsUrl`; admission rejects noncanonical
spelling and preserves accepted path and query bytes. Acquisition passes that typed
value to the [shared download runtime](../../platform/runtimes/duckdb/DESIGN.md).
The runtime applies the following controls independently of URL syntax:

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
| `open_street_map` | checks PBF references, writes every point, line, multiline, multipolygon, and relation layer plus GeoParquet, then builds and archives Valhalla routing data |
| `authority_vector` | uses GDAL to write GeoParquet and WGS84 GeoJSON |
| `gtfs_schedule` | checks safe ZIP expansion and required files, optionally runs a configured validator, retains a normalized ZIP |
| `environmental` | writes a COG with Zstandard compression and overviews, then records complete typed raster metadata |
| `s57_enc` and `s100` | use the pinned GDAL maritime conversion path to GeoParquet |
| `aixm` and `faa_nasr` | use the pinned GDAL aviation conversion path to GeoParquet |
| `gtfs_realtime` | represented in the contract but rejected by base-release acquisition |

The generic maritime and aviation conversions establish the intake primitive.
Operational reliance adds product-specific S-57 update-chain, S-100 product,
AIXM timeslice, or NASR validation in the corresponding adapter.

GeoJSON and GeoJSON Sequence products feed the analytical projection. Every
normalized point, line, polygon, and relation becomes a complete immutable
source feature before specialized projections run. The governed OSM profile
emits the source element version and every source tag as JSON. A relation
GeometryCollection becomes one feature per bounded leaf geometry. Those
features retain the common relation identity and a deterministic geometry
path.

Each feature retains normalized tags, original names and references, source
element identity and version, source and release digests, geometry digest,
operating-area memberships, license, and attribution. Feature ids derive
stable UUIDv5 Map ids from source identity, element kind, source element
identity, and geometry path. A line-delimited product also includes its stable
product and record position when the source supplies no element identity.

`inspect_position` resolves one WGS84 observation against the active governed
projection in one bounded query per entity class. It returns distance-ordered
named locations and facilities, containing boundaries, the active release
identities used by the projection, and explicit gaps when the governed data
does not label the surrounding area. Callers do not need to enumerate releases
or search raw source features to answer where an observed position is.

`query_source_features` always names one immutable release. It supports source
and element identity, exact tag equality, tag existence, normalized text,
representation, bounding box, intersection, containment, distance, and nearest
predicates. Results use a deterministic identity order, or distance then
identity for distance queries. Spherical distance casts the feature centroid to
`POINT_2D`, constructs query positions with `ST_Point2D(longitude, latitude)`, and
materializes `distance_m` once for limits, cursor comparison, projection, and order.
An opaque cursor binds to the `veoveo.ai/map/source-feature-query/v2` digest domain.
The decoder requires distance state to match the selected order and rejects prior
cursor domains.

Environmental acquisition publishes the COG and its metadata sidecar as
separate immutable artifacts. Release activation indexes a `RasterProduct`
whose artifact identity, checksum, source release, CRS, affine transform,
dimensions, extent, resolution, bands, units, nodata values, interpretation,
license, and attribution remain available through Map resources.

`derive_raster` is a durable task for bounded sampling, windows, class masks,
contours, polygonization, skeletonization, and line derivation. A controlled
GDAL helper reads only the already-authorized staged source and writes into
the task directory. The task publishes one governed artifact and records the
source checksum, CRS and affine transform, every operation parameter, the
exact `gdal-3.13.3-veoveo-raster-v1` algorithm revision, output digest,
output CRS and affine transform where applicable, principal, and Work
Context. Sampling and raster products preserve their declared CRS. GeoJSON
derivations are transformed to WGS84 before publication.

The narrower projections remain derived conveniences. Named points become
locations unless `facility_kind` is present. Polygon features become
boundaries. A LineString becomes a governed network edge when it carries
`from_node`, `to_node`, `map_family`, and `nominal_duration_s`; optional fields
include `distance_m` and `bidirectional`.

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
