# Map Feature Authoring

## Standards And Protocols

This design uses the Map server’s [standards profile](DESIGN.md#standards-and-protocols):
MCP 2026-07-28, OGC feature and tile formats, RFC 9562 identifiers and Veoveo Work
Context access. The supported source adapters and format subsets are specified below.

## Authored Feature Layers

An authored layer is a governed operational dataset inside one Work Context. The
gateway resolves its business owner, initial grants, classification, labels,
membership, policy revision, and invocation provenance. Map stamps that authority
on the canonical layer and every changeset instead of accepting authority fields
from a tool request.

Feature geometry uses WGS84 GeoJSON coordinates. The complete canonical feature
remains a valid GeoJSON Feature and adds JSON-FG `featureType`, the required
JSON-FG conformance declarations, and valid-time intervals whose open bound is
`..`. Point, MultiPoint, LineString, MultiLineString, Polygon, and MultiPolygon
are implemented. Validators reject non-finite or out-of-range coordinates,
malformed topology, incorrect polygon winding, unbounded property payloads,
remote JSON Schema references, and unsafe style expressions.

### Object Model

The model separates mutable working state from immutable history and delivery
artifacts.

| Object | Mutability | Purpose |
|---|---|---|
| feature layer | optimistic mutable head | Work Context authority, content class, title, current schema/style, and current layer revision |
| schema revision | immutable | JSON Schema 2020-12 property contract used by a range of layer revisions |
| style revision | immutable | bounded literal styling rules used by a range of layer revisions |
| feature revision | immutable | one complete canonical feature or tombstone at one layer revision |
| feature head | optimistic mutable pointer | current revision and deletion state for one feature |
| changeset | immutable and idempotent | one atomic commit, request digest, actor, authority, and projection event sequence |
| publication | immutable | layer, schema, style, and layer-revision pin for stable reads |
| layer product | immutable | exported artifact identity, format, digest, byte count, and publication provenance |
| composition revision | immutable | ordered publication pins, visibility, opacity, and initial camera view |
| composition head | optimistic mutable pointer | current composition revision and archival state |
| durable task | mutable lifecycle record | owner-scoped bulk import, export, or vector-tile execution and recovery |

Layer content classes are `reference`, `named_locations`, `facilities`,
`boundaries`, and `network_candidate`. They express user intent. They do not
grant authority to alter the routing data plane.

### Storage Authority And Projection

SurrealDB owns the immutable truth. A direct commit contains at most 100
mutations and 1 MiB. One transaction checks the expected layer revision and
each expected feature revision, creates feature revisions, advances heads,
records the scoped idempotent changeset, and allocates its next sequence from the
Map projection head. A repeated idempotency key returns the original changeset only
when its request digest matches.

A synchronous database event advances the Map head in the same transaction. An
allocated sequence at or below the committed head is rejected; concurrent writes
conflict atomically. The sequence belongs to authored Map commits, so unrelated domain
writes cannot contend on this ordering key. Read-your-write requests carry that
committed sequence.

DuckDB Spatial is a rebuildable query projection. Recovery captures the committed
Map head once, then reads immutable changesets through that bound using the
`commit_sequence` index. Each page contains at most 1,000 Map commits. The projector
checks each changeset's complete feature revision inventory and writes the revision
table, current-head table, R-tree indexes and local checkpoint in one DuckDB transaction.
Restart resumes that checkpoint. Requests above the committed Map bound fail.

SurrealDB `ASYNC` events cannot own this projection because it writes a separate DuckDB
volume. The Map service owns the external transaction and its checkpoint. The Map-head
event must also run synchronously: advancing it after commit would permit a reader to
capture a head that omits an accepted changeset. Native changefeeds record Map mutations
for other consumers; immutable changesets supply the complete rebuild log.

Queries can select a current layer or a published
layer revision. They accept a validated WGS84 bounding box, open valid-time
interval, geometry type, opaque keyset cursor, and a bounded Basic CQL2-JSON subset.
Property paths and literal values remain parameters. A dateline-crossing box is
split into two query polygons.

Map eagerly binds and inspects every persisted R-tree during startup before accepting
projection writes. Projection writes then use ordinary `INSERT` statements after
deterministic replay checks. They do not use DuckDB conflict-merge insertion against
R-tree tables, because lazy index binding can replay non-flat Spatial vectors when one
transaction mixes point, line, and polygon geometries. Duplicate projected identities
fail the transaction. The pinned Spatial regression test reopens the database, commits
all three geometry families together, inspects both authored R-tree leaf sets, and
executes an indexed intersection before acceptance.

The artifact plane stores bulk input bytes and immutable output products. The
task root stages a verified import under its parsed task identity. It survives
process restart and is deleted after a terminal task state. Export tasks receive
one task-bound write capability at submission. Durable task requests persist
the verified internal identity and capability, never the caller bearer or an
artifact download URL.

### Standards Profile

The authoring contract pins dated standards. A later upstream revision does not
silently change an existing layer or product contract.

| Standard | Implemented profile |
|---|---|
| [GeoJSON, RFC 7946](https://www.rfc-editor.org/rfc/rfc7946.html) | WGS84 input geometry, FeatureCollection import, feature output |
| [OGC JSON-FG 1.0, OGC 21-045r1](https://docs.ogc.org/is/21-045r1/21-045r1.html) | Core and Feature Types and Schemas conformance declarations, `featureType`, valid-time interval |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/json-schema-core) | local property schemas; remote references are rejected |
| [OGC CQL2 1.0, OGC 21-065r2](https://docs.ogc.org/is/21-065r2/21-065r2.html) | bounded Basic CQL2-JSON equality, ordering, null, boolean, and logical predicates over top-level properties |
| [GeoJSON Text Sequences, RFC 8142](https://www.rfc-editor.org/rfc/rfc8142.html) | record-separator and LF-framed import/export |
| [OGC GeoPackage 1.4](https://www.geopackage.org/spec140/) | full validation and bounded vector-table inspection through pinned GDAL 3.13.3; explicit table and metadata-column mappings on import; one two-dimensional WGS84 vector table with an R-tree on export |
| [GeoParquet 1.0.0](https://github.com/opengeospatial/geoparquet/blob/v1.0.0/format-specs/geoparquet.md) | WKB primary geometry and GeoParquet metadata emitted by pinned DuckDB Spatial |
| [Mapbox Vector Tile Specification 2.1](https://github.com/mapbox/vector-tile-spec/tree/master/2.1) | requested XYZ tiles with canonical feature identity retained as an attribute |
| [MapLibre Style Specification 8](https://maplibre.org/maplibre-style-spec/) and MapLibre GL JS 6.6.0 | vector source plus safe literal point, line, polygon, label, opacity, and zoom projections; self-contained WebGL2 workspace viewing |
| [OpenFreeMap](https://openfreemap.org/quick_start/) hosted MapLibre Style profile | Credential-free MapLibre Style 8 URLs supply vector geographic context for both Console themes. The defaults are OpenFreeMap Positron for light mode and OpenFreeMap Dark for dark mode. Map MCP validates both URLs, requires one exact HTTPS origin, and declares that origin in MCP App CSP metadata. The supported profile requires both style documents, sprites, glyphs, TileJSON, and tiles to remain on that origin; an installation may replace the pair with controlled or self-hosted styles that preserve this boundary. This is a basemap presentation profile, not complete conformance to an external tile-service API. |

The image pins [DuckDB Spatial](https://duckdb.org/docs/stable/core_extensions/spatial/overview)
to DuckDB 1.5.6. Export verification rejects a generated Parquet file unless
its `geo` metadata declares version `1.0.0`, names `geometry` as the primary
column, and identifies its encoding as `WKB`. GeoParquet 2.0 is not claimed.
A future 2.0 path requires an encoder and verifier that both implement its
Parquet geometry logical type.

### Editing, Transfer, And Publication

The public MCP surface includes create, update, validate, commit, query, restore,
publish, and archive tools. Layer heads, schema revisions, style revisions by
layer version or stable style identity,
feature queries, feature heads and revisions, changesets, and publications are
URI-addressed resources. Mutable heads and indexes support MCP subscriptions and
resource-update notifications. Individual features are never expanded into the
resource list; agents traverse them through the paginated query template.

An import task accepts one authorized GeoJSON FeatureCollection, GeoJSON text
sequence, or GeoPackage artifact. It stages and hashes the bounded input before
task creation, maps external string or numeric identifiers to stable typed feature
ids, and commits at most 10,000 features in one SurrealDB transaction. GeoPackage
import selects one feature table and explicitly maps identity, semantic type,
title, and valid-time columns. The pinned GDAL adapter validates the complete
package, rejects unsupported dimensions and geometry types, converts the declared
CRS to two-dimensional OGC:CRS84, and emits the same canonical RFC 8142 boundary
used by native bulk import. The request supplies a default semantic type when the
source has none. JSON-FG records that declare `featureType` must declare the
supported conformance classes.

`inspect_geopackage` validates an authorized artifact before import and returns a
bounded manifest of feature tables, fields, declared CRS identifiers, extensions,
feature counts, WGS84 extents where safely available, and R-tree declarations.
The manifest does not grant access to the underlying artifact and never selects a
table implicitly.

An immutable publication is the only input to export and presentation. Export
tasks produce GeoJSON text sequence, GeoParquet 1.0, or GeoPackage 1.4. A
GeoPackage product contains one explicitly named WGS84 feature table and its
standard R-tree spatial index. Canonical feature metadata uses reserved
`veoveo_*` fields. Top-level properties use `property:` columns with OGR scalar
or JSON subtypes, which preserves their types across Map export and re-import.
A vector task accepts at
most 512 distinct XYZ coordinates through zoom 22 and emits a deterministic tar
bundle containing MVT 2.1 tiles, a manifest, and a MapLibre Style 8 document.
Every product retains the publication, layer revision, digest, size, format,
artifact identity, creator, and Work Context authority.

A map composition orders at most 64 authored layers and pins one immutable
publication for each layer. Every change appends a composition revision with a
bounded WGS84 view and literal opacity and visibility settings. The mutable head,
immutable revisions, root indexes, completions, and update subscriptions are MCP
resources. A composition is the stable handoff to map presentation clients.

The workspace app at `ui://map/workspace.html` uses the MCP Apps bridge. It
reads `map://workspace` first and exposes only the dataset, administration,
feature-read, feature-write, and publication controls admitted for the caller.
The map remains visible while the user discovers layers, previews records,
inspects a feature, authors geometry, imports an artifact, acquires a source, or
saves a composition. Authoring and administration live in contextual drawers
instead of replacing the map with unrelated forms.

The left catalog presents authored layers and active governed source releases
through one visibility model. Authored layers render their current heads by
default; selecting a saved composition switches them to its exact publication
and style-revision pins. Active source releases render through
`query_source_features`. The bottom preview is a bounded table synchronized
with the visible map, and the right inspector follows the selected catalog item
or feature. A map or table selection highlights the same feature in both
places. Raw JSON is an advanced diagnostic view, never the primary workflow.

MapLibre GL JS 6.6.0 is bundled into the self-contained App with a classic
worker emitted from the same pinned source. The opaque-origin sandbox admits
the worker through its `connect-src data:` and `worker-src blob:` boundaries.
Map MCP supplies one validated basemap descriptor with credential-free light
and dark MapLibre Style URLs and declares their shared exact HTTPS resource
origin to the host. The workspace follows the initial host theme and reacts to
host-context changes without losing its camera, governed overlays, selection,
or bounded preview. The style profile keeps both documents, sprites, glyphs,
TileJSON, and tiles on that origin, while governed feature bytes continue to
cross only the MCP bridge. Basemap failure leaves governed layers usable and
reports the degraded context locally without taking down the workspace.

Each enabled authored layer issues R-tree-backed, viewport-bounded
`query_features` calls in pages of 1,000 and stops at 5,000 features per layer
and viewport. Each enabled active release issues DuckDB Spatial R-tree-backed
`query_source_features` calls in pages of 500 and stops at 5,000 features per
release and viewport. Generation cancellation prevents stale responses from
painting after a camera or visibility change. The visible cap is reported
instead of silently dropping the condition. The UI reports a successful
refresh only after MapLibre reaches an idle paint with returned geometry
visible. Resource subscriptions use MCP `notifications/resources/updated` and
cover admitted layers, publications, compositions, datasets, active releases and
mobility profiles. Source registration and acquisition indexes support explicit reads;
they do not expose update subscriptions. An active-release change also refreshes its
source and dataset metadata. The App registers before reading its first snapshot. Notifications within
80 ms coalesce into a refresh of the changed indexes with at most four concurrent
reads. The App publishes a snapshot only when its requested reads succeed; a failed
refresh preserves the previous view and offers Retry.

The hardware WebGL2 check runs before host requests. Ordinary bridge requests time
out after 15 seconds; a subscription deadline ends on the MCP acknowledgment and
does not limit a healthy stream. Teardown rejects pending requests. Startup Retry
repeats the handshake, permission read and map initialization inside the same frame.
Mutations are never replayed automatically after a missing response.

The Add data workflow distinguishes three actions. Create layer uses ordinary
fields with a permissive JSON Schema default. Add feature uses map drawing,
title, semantic type, and a property editor. Import artifact accepts an
authorized artifact id and explicit GeoJSON FeatureCollection, RFC 8142, or
GeoPackage settings; GeoPackage inspection runs first and the user selects one
reported feature table. Task-only imports use the MCP Tasks lifecycle inside
the App. Source acquisition chooses a governed source and draws its WGS84
extent on the persistent map. Canonical source, schema, profile, and request
JSON remain available under an Advanced disclosure for operators who need the
complete typed contract.

The map fails closed unless WebGL2 creation succeeds with the major-performance
caveat check, the debug renderer extension identifies the adapter, and the
renderer fingerprint is not a known software path. Browser acceptance must
prove the same condition in a headed browser. The App is a two-dimensional
feature and source-release workspace. It does not provide raster presentation,
3D geometry authoring, collaborative geometry editing, or server-side
rendering.

### Deliberate Boundaries

`reference`, `named_locations`, `facilities`, `boundaries`, and
`network_candidate` are authoring classifications, not routing authority. A
generic feature commit or publication never changes an active source release or
Valhalla data. Routing influence requires a separate governed validation and
release-promotion operation.

The implementation does not provide arbitrary CQL2, spatial CQL2 predicates,
GeoParquet 2.0, mutable raster authoring, 3D authoring, an OGC API Features
HTTP service, collaborative locks or CRDTs, or automatic promotion of a
`network_candidate`.
These are new contracts, not compatibility details, and must be added through
their owning components.

Artifact write capabilities expire after 24 hours by artifact-plane policy. An
output task that cannot redeem its submission-time capability within that
window fails closed on recovery. It never refreshes authority through a stored
caller bearer.

The next implementation phase should add a validated network-candidate promotion
task, a true GeoParquet 2.0 encoder, streaming artifact ingest for datasets above
the transactional import bound, and schema migration tasks. Property flattening and packaged
tile pyramids belong in the vector product path when client requirements justify
their storage cost.
