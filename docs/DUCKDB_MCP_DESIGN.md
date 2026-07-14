# DuckDB MCP Design

`duckdb-mcp` is Veoveo's hosted analytical SQL domain. It gives an authenticated
principal mutable DuckDB databases, unrestricted analytical SQL inside those
databases, governed data ingress, immutable exports, and DuckDB Spatial without
granting SQL ambient file, network, extension, or configuration authority.

The central design choice is that arbitrary SQL is the capability. The server
does not replace DuckDB with a narrow query builder. Isolation is enforced around
the engine through derived database paths, locked connections, resource limits,
governed source materialization, typed MCP contracts, durable tasks, and the
shared artifact plane.

DuckDB is not platform coordination state and is not the canonical live common
operating picture. SurrealDB owns platform and durable task state. The artifact
plane owns immutable bytes and sharing. A future map server owns tiles, styles,
views, and rendered maps. DuckDB supplies analytical tables and derived spatial
products to those domains.

## Status

Implemented in this workspace. The current server provides:

- the `veoveo-duckdb-mcp` crate under `servers/duckdb-mcp`
- the shared hardened runtime under `platform/runtimes/duckdb`
- the shared `DuckDbSource` contract in `veoveo_mcp_contract::duckdb`
- `query`, `execute`, `ingest`, and `export` tools
- owner-scoped mutable database files
- direct and durable final-extension execution
- database, artifact, and task-usage resources
- shared artifact-plane input and output
- governed allowlisted HTTPS source materialization
- pinned DuckDB Spatial loading and startup verification
- Compose, Helm, gateway catalog, profile, and policy integration
- Rust unit, integration, conformance, and multi-process smoke coverage

The crate, folder, hosted server slug, and URI scheme are:

```text
crate       veoveo-duckdb-mcp
folder      servers/duckdb-mcp
slug        duckdb
URI scheme  duckdb
```

The internal hosted endpoint is:

```text
/duckdb/mcp
```

The gateway exposes local tools under its mounted namespace:

```text
duckdb__query
duckdb__execute
duckdb__ingest
duckdb__export
```

## Goals

- Preserve DuckDB's expressive analytical SQL surface for agents.
- Isolate mutable database files by authenticated owner and gateway profile.
- Make reads, mutations, ingest, export, task behavior, and recovery explicit.
- Keep file and network access outside caller-controlled SQL.
- Bound memory, threads, spill paths, execution time, inline rows, and bytes.
- Move large and cross-server data through the shared artifact plane.
- Make long-running operations durable without replaying uncertain mutations.
- Ship DuckDB Spatial as a required server capability.
- Support geometry, spatial joins, CRS transforms, R-tree indexes, and vector
  tile computation inside governed databases.
- Return typed structured tool content and canonical resource links.
- Keep the server's public contract independent of the Rust DuckDB binding and
  internal storage paths.

## Non-Goals

- DuckDB is not the SurrealDB platform store.
- DuckDB databases are not shared mutable tenant databases.
- The server does not provide a client-facing REST SQL API.
- The server does not expose filesystem paths, S3 addresses, or presigned URLs
  as canonical identities.
- SQL cannot install extensions, fetch URLs, attach arbitrary files, or change
  locked engine settings.
- The typed ingest contract does not expose arbitrary DuckDB file functions.
- The server does not silently enable additional DuckDB extensions.
- The server is not a feature service, tile server, geocoder, router, or map
  renderer.
- It does not reproduce ArcGIS project, editing, topology, print-layout, or
  catalog administration surfaces.
- It does not make a mutable database visible through artifact grants. Sharing
  uses an immutable export followed by artifact-plane authorization.
- It does not promise automatic recovery for a timed-out mutation.

## Fit With Veoveo

Clients normally reach DuckDB through an authenticated gateway profile.

```text
MCP client
  |
  | MCP over streamable HTTP
  v
mcp-gateway profile (/mcp/{profile})
  |
  | gateway-signed internal identity
  v
duckdb-mcp (/duckdb/mcp)
  |-- owner DuckDB files
  |-- SurrealDB task and usage state
  |-- artifact-service
  `-- governed HTTPS source client
```

Resource URIs remain owned by the server when tools are projected through the
gateway. Tool names gain the `duckdb__` namespace; resource identities do not.

```text
duckdb://dbs
duckdb://db/{db_id}
duckdb://artifact/{artifact_id}
duckdb://usage
duckdb://usage/task/{task_id}
```

The server binds one Axum listener. Its HTTP surface contains protocol and
health routes only.

```text
/duckdb/mcp       internal MCP over streamable HTTP
/duckdb/healthz   operational health
```

The MCP transport is stateless. Durable task continuity comes from the shared
task runtime rather than an HTTP session.

## MCP Capabilities

The core server surface provides:

- tools
- resources
- resource templates

The final task extension adds:

- task-capable tool invocation
- task discovery and retrieval
- task cancellation and input update
- SSE task subscriptions
- task status notifications
- durable result retrieval across MCP sessions

Prompts, completions, and resource subscriptions are not part of the current
server. SQL and database schema discovery already have direct domain surfaces.
They should be added only when a concrete agent workflow benefits from them.

## Canonical Domain Model

Controlled request and result shapes use server-owned Rust structs and enums.
Raw JSON appears only where DuckDB values or extensible read options cannot be
modeled more narrowly.

### Database Identity

`DuckDbDatabaseId` is an owner-local logical name. It accepts one through 64
ASCII lowercase letters, digits, or underscores, beginning with a letter.

```text
robot_metrics
cop_snapshot_2026
mission_7
```

The id is not a global identity. Its canonical resource identity includes the
calling owner through authorization, while its visible URI stays concise:

```text
duckdb://db/robot_metrics
```

The physical file path is never accepted from a client. The server derives an
owner directory from a SHA-256 digest over:

- principal issuer
- principal subject
- canonical principal id
- tenant or installation scope
- gateway profile

Different profiles receive different mutable database workspaces even for the
same principal. Data labels remain authorization and artifact-classification
state; they are not part of the physical filename.

### Shared Data Source

`veoveo_mcp_contract::duckdb::DuckDbSource` is shared by hosted servers that
need the same governed tabular input vocabulary.

```text
InlineCsv
Uri
Uris
Artifact
```

Supported typed source formats are:

```text
auto
csv
parquet
json
ndjson
```

`DuckDbReadOptions` has controlled fields for header detection, delimiter, and
timestamp format. Its extension map accepts option names made from ASCII letters,
digits, and underscores. Values may be booleans, numbers, strings, or arrays of
those values. Objects and null are rejected.

The contract describes data. It does not authorize access. `duckdb-mcp` resolves
an artifact under the live caller's plane identity or fetches an HTTPS source
under the server's allowlist before DuckDB sees a request-local file.

### Export Model

Query and export output formats are controlled separately from input formats.

```text
parquet
csv
duck_db
```

`duck_db` means one immutable snapshot of the complete database file. It is not
valid for a table or query-result export.

## Tool Model

| Tool | Invocation | Mutation | Purpose |
|---|---|---:|---|
| `query` | direct or task | no | Run one read-only SQL statement with inline or artifact output. |
| `execute` | direct or task | yes | Run DDL or DML against one owned mutable database. |
| `ingest` | task required | yes | Materialize a governed source and load it into a table. |
| `export` | task required | no, except checkpoint | Export a table, query result, or full database snapshot to the artifact plane. |

The gateway may present task support to clients through its final-task
projection. Invocation behavior remains canonical: `ingest` and `export` reject
ordinary direct execution, while `query` and `execute` support both paths.

Every successful result contains typed structured content. Human-readable text
and resource links are additional content blocks over the same result.

## `query`

`query` accepts:

```text
db
sql
attach[]?
row_limit?
timeout_ms?
output
```

The server opens the target database read-only. Each attached database is
resolved from the same owner workspace and attached read-only under its database
id. Duplicate names and attachment of the target database are rejected.

The SQL must contain exactly one statement. The validator understands quoted
strings, quoted identifiers, line comments, nested block comments, and a single
trailing terminator. DuckDB remains the parser and execution authority.

Inline output returns:

```text
columns[]
rows[]
row_count
truncated
artifact = null
```

`row_limit` can lower the server's inline row ceiling but cannot raise it. The
byte ceiling is checked before a value is materialized. An interactive query
stops reading once either limit is reached and marks the response truncated.
Callers that need the complete set should request artifact output.

Artifact output accepts `parquet` or `csv`. The server wraps the validated query
in a DuckDB `COPY`, permits only one fresh exchange directory, uploads the result
to the shared plane, removes the exchange directory, and returns one
`duckdb://artifact/{artifact_id}` link. Query output does not accept `duck_db`.

### Query example

```json
{
  "db": "mission_geo",
  "sql": "SELECT feature_id FROM features WHERE ST_Intersects(geom, ST_MakeEnvelope(-90.1, 13.6, -89.9, 13.8))",
  "output": { "mode": "inline" }
}
```

An artifact result uses:

```json
{
  "db": "mission_geo",
  "sql": "SELECT * FROM features",
  "output": { "mode": "artifact", "format": "parquet" }
}
```

## `execute`

`execute` accepts:

```text
db
sql
create_if_missing
timeout_ms?
```

The operation opens one writable database connection. `create_if_missing` must
be true when the owner-local database file does not exist. The parent directory
is derived and created by the server.

`execute` deliberately accepts multiple statements. It is the administrative
and mutation surface for schema creation, indexes, views, macros, inserts,
updates, deletes, and other DuckDB behavior permitted inside the sandbox. A
single in-process mutex serializes writers for each database file. Readers do
not take that mutex.

Single-statement execution reports DuckDB's changed-row count. Batch execution
reports its statement count and sets `rows_changed` to zero because DuckDB does
not return a reliable per-statement aggregate for the batch.

The result includes:

```text
db
statements
rows_changed
db_created
```

### Spatial schema example

```json
{
  "db": "mission_geo",
  "create_if_missing": true,
  "sql": "CREATE TABLE features(feature_id VARCHAR, geom GEOMETRY); CREATE INDEX features_geom_rtree ON features USING RTREE (geom);"
}
```

The tool is intentionally destructive. Gateway policy decides which profiles
may call it. The MCP annotations mark it non-idempotent and potentially
destructive.

## `ingest`

`ingest` accepts:

```text
db
table
source
mode
create_db_if_missing
```

The table name is quoted as a DuckDB identifier after empty-name rejection.
Ingest modes are:

| Mode | Behavior |
|---|---|
| `create` | `CREATE TABLE ... AS`; fail when the table already exists. |
| `append` | `INSERT INTO ... SELECT`; require a compatible existing table. |
| `replace` | `CREATE OR REPLACE TABLE ... AS`. |

The service creates a fresh exchange directory and materializes the source
there. It then builds one typed `read_csv`, `read_parquet`, `read_json`, or
`read_ndjson` expression. Caller data never becomes an arbitrary SQL fragment.

Inline CSV is written directly under the source byte cap. Artifact input is
resolved through the shared artifact service under the caller's current grants,
tenant, and label clearance. The canonical cross-server input identity is:

```text
artifact://{artifact_id}
```

URI input must use HTTPS and an exact configured hostname. An empty source-host
allowlist disables remote URI sources. The governed client:

- rejects embedded credentials
- resolves the hostname before connecting
- rejects private, loopback, link-local, documentation, multicast, and other
  reserved addresses
- pins the validated address while TLS authenticates the original host
- disables automatic redirect following
- revalidates every redirect
- enforces connection, total-time, redirect-count, and streaming byte limits

DuckDB receives only the local request path. SQL network access remains disabled.
The exchange directory is removed after success or failure.

The result includes the database id, table name, ingested row count, and whether
the database file was created.

## `export`

`export` accepts one selection:

```text
table
sql
database
```

Table and SQL selections support Parquet and CSV. SQL selection uses the same
single-statement validation and standalone DuckDB preparation as query artifact
output. It is then embedded into a bounded `COPY` operation inside a fresh
exchange directory.

Database selection requires `duck_db`. The server takes the database writer
lock, runs `CHECKPOINT`, reads the complete file, and uploads one immutable
snapshot. The snapshot is the canonical way to move a full database through the
artifact plane.

The result contains:

```text
db
rows_exported
artifact
```

The artifact content block is a resource link. Export metadata records the
operation, database, selection, task id, and row count where applicable.

## DuckDB Spatial

Spatial is a required capability of the hosted DuckDB image. The Docker build
downloads the official extension that matches DuckDB `1.5.4` for AMD64 or
ARM64, verifies a pinned SHA-256 digest, installs the decompressed binary at a
read-only canonical path, and copies it into the runtime image.

```text
/usr/local/lib/duckdb/extensions/spatial.duckdb_extension
```

Every server connection loads that exact absolute file before external access
and configuration changes are disabled. The service never runs `INSTALL` at
runtime. Startup opens an in-memory hardened connection and verifies
`ST_Point`/`ST_AsText`; a missing or incompatible extension prevents the server
from listening.

The installed extension enables DuckDB geometry types and functions, including:

- geometry construction and serialization
- predicates, measurements, buffers, intersections, and unions
- CRS transforms backed by the Spatial extension's packaged PROJ data
- R-tree spatial indexes
- GeoJSON, WKB, and SVG geometry serialization
- `ST_TileEnvelope`, `ST_AsMVTGeom`, and `ST_AsMVT` vector-tile computation

Availability of a function follows the pinned DuckDB Spatial version. Agents
should query the database with SQL rather than depend on a second Veoveo
function catalog.

Spatial does not widen data access. `ST_Read` and other file-oriented functions
cannot escape the connection sandbox. The current typed ingest formats remain
CSV, Parquet, JSON, and NDJSON. A new geospatial input format requires an
explicit contract and governed materialization path.

DuckDB can compute an MVT BLOB, but `duckdb-mcp` does not assign tile identities
or expose an XYZ tile route. A map domain may consume a governed snapshot or
derived layer and use these functions to build tiles. Tile caching, styles,
camera views, rendered images, and live layer invalidation remain outside this
server.

### Vector tile query shape

A map-owned DuckDB table should materialize or index its render geometry in
Web Mercator. The tile envelope returned by `ST_TileEnvelope` is EPSG:3857, so
the indexed geometry and envelope must use the same CRS.

The core query shape is:

```sql
WITH bounds AS (
    SELECT ST_TileEnvelope(z, x, y) AS geom
),
tile_rows AS (
    SELECT
        feature.feature_id AS id,
        feature.kind,
        ST_AsMVTGeom(
            feature.geom_3857,
            ST_Extent(bounds.geom),
            4096,
            256,
            true
        ) AS geom
    FROM features AS feature, bounds
    WHERE ST_Intersects(feature.geom_3857, bounds.geom)
)
SELECT ST_AsMVT(
    {'id': id, 'kind': kind, 'geom': geom},
    'features',
    4096,
    'geom',
    'id'
) AS tile
FROM tile_rows;
```

The map service replaces `z`, `x`, and `y` with validated integer tile
coordinates. The result is one MVT BLOB suitable for an internal XYZ response
with a vector-tile MIME type.

The public `query` tool is not the efficient per-tile transport. Its BLOB result
is encoded into structured MCP content and it has no HTTP cache semantics. A
future map service should use the shared hardened runtime over a map-owned
derived database, or define one typed binary tile operation. It must not open a
principal's private DuckDB workspace by path.

On-demand tile cache keys should include the immutable layer version plus
`z/x/y`. Immutable regional or mission layers may be materialized into PMTiles
or MBTiles instead. Raster output still requires a renderer that combines these
vector tiles with a style, glyphs, sprites, and any basemap sources.

## Engine Sandbox

The shared `veoveo-duckdb-runtime` applies the same engine boundary to every
connection opened by this server.

Initialization occurs in a fixed order:

1. Open the derived database file in native read-only or read-write mode.
2. Attach service-selected owner databases read-only.
3. Set memory, thread, and spill limits.
4. Disable community extensions, automatic installation, and automatic loading.
5. Load only service-selected trusted extensions by canonical absolute path.
6. Allow one request exchange directory when the operation needs file IO.
7. Disable external access and lock configuration.
8. Execute caller SQL.

Caller SQL cannot re-enable external access after the lock. It cannot attach a
path, load another extension, install an extension, change the spill directory,
or widen the request-local directory.

The spill directory is always disjoint from an exchange directory. It contains
DuckDB temporary data only. Governed source bytes never enter the spill path.

Query connections use DuckDB's native read-only access mode. This protects the
database more reliably than a SQL keyword blacklist while preserving CTEs,
windows, macros, comments, and the rest of DuckDB's query language.

### Resource limits

Current defaults are:

| Limit | Default |
|---|---:|
| memory per connection | `512MB` |
| DuckDB threads per connection | `2` |
| inline result rows | `1,000` |
| inline materialized bytes | `1 MiB` |
| source bytes | `256 MiB` |
| artifact bytes | `512 MiB` |
| operation timeout | `30,000 ms` |
| maximum requested timeout | `120,000 ms` |

The memory and thread limits are per connection, not global admission control.
Deployment memory limits and workload concurrency must account for simultaneous
readers and task workers.

### Timeout behavior

Blocking DuckDB work runs in `spawn_blocking`. Each worker registers a DuckDB
interrupt handle before executing SQL. At the deadline, the service interrupts
the connection and waits for the worker to stop before releasing a database
write lock.

A read timeout returns `interrupted`. A mutating timeout returns
`interrupted_indeterminate` because the server cannot prove whether a commit
occurred. It does not retry the mutation.

## Database Ownership and Sharing

Mutable database files are private to one principal, tenant scope, and gateway
profile. Database resource listing scans only that derived workspace. Schema
reads and tool calls resolve the same derived path again rather than trusting a
path stored in an MCP request.

The server has no mutable-database grant ledger. Cross-principal collaboration
uses this flow:

```text
owner export
  -> duckdb://artifact/{artifact_id}
  -> artifact-plane user/group grant
  -> recipient artifact://{artifact_id} ingest
  -> recipient-owned mutable database
```

This produces an auditable immutable handoff. It avoids concurrent mutation of
one embedded database by principals with different policy contexts.

`attach` is not a sharing mechanism. It resolves additional databases from the
caller's own profile-scoped workspace and attaches them read-only.

## Canonical Resources

Resources are the stable nouns behind discovery and output links.

```text
duckdb://dbs
```

Lists databases visible in the current owner workspace. Each entry carries its
database id, resource URI, and whether it belongs to the current principal.

```text
duckdb://db/{db_id}
```

Returns a JSON schema summary from `information_schema.columns`. Tables include
ordered column names and DuckDB type names. An existing empty database returns
an empty table list.

```text
duckdb://usage
```

Lists task usage resources visible to the caller.

```text
duckdb://usage/task/{task_id}
```

Returns a `UsageReport` built from the shared platform usage ledger. Each
operation records actual row quantity under a model id such as `duckdb/query` or
`duckdb/ingest`. DuckDB work currently has no monetary amount or currency.

```text
duckdb://artifact/{artifact_id}
```

Presents immutable bytes produced by DuckDB. The artifact id is an occurrence
id owned by the shared plane. `resources/read` authorizes through that plane and
returns a base64 MCP blob with the recorded MIME type.

Artifacts are not enumerated from `resources/list`. DuckDB maintains no private
artifact index. Tools and tasks return resource links, while the shared artifact
plane remains the byte and grant authority.

Resource and template lists use cursor pagination with a page size of 100.

## Shared Artifact Plane

`duckdb-mcp` does not own an object-store bucket, artifact table, or byte route.
It forwards the verified caller identity to `artifact-service`.

Canonical identities have two forms:

```text
artifact://{artifact_id}          neutral cross-server identity
duckdb://artifact/{artifact_id}   DuckDB presentation identity
```

The artifact service stamps tenant and owner from the verified identity, records
the owner grant, applies label clearance, encrypts under tenant scope, and stores
the bytes. DuckDB attaches the task owner's data labels as artifact compliance
metadata.

Direct query artifact output writes under the live caller. Task-based query and
export reserve a bounded write capability while the live caller is present. The
current capability permits one artifact, inherits the server artifact byte cap,
expires after 24 hours, and is redeemed with an operation-specific idempotency
key.

Task recovery can therefore complete an authorized read/export without minting
a background principal or persisting a bearer token.

## Durable Tasks and Recovery

The shared task runtime stores DuckDB task requests, owners, leases, progress,
results, cancellation state, retention pins, and usage in SurrealDB. Task ids
are UUIDv7 values. A task is visible only when principal, profile, tenant, and
data-label checks match the durable owner record.

Current task timing is:

| Setting | Value |
|---|---:|
| task TTL | 7 days |
| suggested poll interval | 3 seconds |
| worker lease | 120 seconds |
| lease heartbeat | 40 seconds |

The final task extension supports creation, get, update, cancellation, and SSE
subscriptions. A later MCP session with a fresh gateway token can continue a
task created by the same durable principal and profile.

Recovery classes follow side-effect semantics:

| Operation | Recovery class | Reason |
|---|---|---|
| `query` | `Resume` | Read-only and safe to repeat. |
| `export` | `Resume` | Read-only derivation with idempotent artifact redemption. |
| `execute` | `InterruptedIndeterminate` | A mutation may have committed. |
| `ingest` | `InterruptedIndeterminate` | A table mutation may have committed. |

Only query and export are scheduled again during recovery. Execute and ingest
never gain a polling or replay fallback.

## Persistence

The server composes three storage domains:

| Data | Authority |
|---|---|
| mutable analytical databases | DuckDB files in the server workspace |
| tasks, owners, leases, usage | SurrealDB platform store |
| immutable exports and query artifacts | shared artifact plane |

Compose mounts the database directory from the `duckdb_workspaces` volume and
places exchange and spill directories on container tmpfs. Helm deploys one
replica with a `ReadWriteOnce` workspace PVC. The chart does not claim database
high availability.

The singleton is part of the correctness boundary. The per-database write mutex
is process-local, and DuckDB is embedded rather than a network database. Adding
replicas against separate volumes would create divergent owner workspaces.

Operators back up mutable workspaces through installation storage policy or by
creating explicit `duck_db` snapshots. Artifact backup does not replace backup
of databases that have never been exported.

## Authentication, Policy, and Audit

The gateway is the public OAuth/OIDC and policy boundary. The hosted server also
requires the gateway's Ed25519-signed internal assertion on MCP and task
extension calls. The assertion is audienced to the `duckdb` server.

The server reconstructs ownership only from the verified identity. It captures
the forwarded bearer for calls to the artifact service and never logs it.

Request host validation uses the typed public deployment plus explicit allowed
authorities. Unknown or malformed authorities are rejected before MCP routing.

The canonical gateway manifest exposes:

```text
tools               query, execute, ingest, export
resources           duckdb://...
resource templates  enabled
tasks               enabled
notifications       enabled
prompts              disabled
completions          disabled
resource subscriptions disabled
```

Normal use requires `operator:use`. Profile exposure and policy remain explicit
for every tool, resource scheme, task action, artifact read, and usage read.

The gateway records external authorization and audit evidence. The domain
server enforces owner checks again for databases, tasks, usage, and artifacts.

## Container and Deployment Boundary

The production image runs as the non-root `veoveo` user with UID `10001`.
DuckDB Spatial is read-only inside the image. The mutable workspace is mounted
under `/var/lib/veoveo/duckdb`.

Compose applies:

- a read-only root filesystem
- all Linux capabilities dropped
- `no-new-privileges`
- a process limit
- a two-GiB container memory limit
- loopback-only host publication

The server requires database-scoped SurrealDB credentials. Installation root
credentials are never accepted by its configuration parser.

### Configuration

| Argument or environment | Default or requirement |
|---|---|
| `--port` | `8791` |
| `PUBLIC_BASE_URL` | required public deployment origin |
| `--database-dir` | `databases` |
| `--exchange-dir` | `exchange` |
| `--spill-dir` | `spill` |
| `--spatial-extension` | canonical `/usr/local/lib/...` path |
| `--artifact-service-url` | `http://artifact-service:8790` |
| `--allowed-host` | repeatable additional host authority |
| `--allow-loopback-hosts` | false |
| `--allow-source-host` | repeatable; empty denies remote sources |
| `--max-source-bytes` | `268435456` |
| `--max-artifact-bytes` | `536870912` |
| `--engine-memory-limit` | `512MB` |
| `--engine-threads` | `2` |
| `--max-inline-rows` | `1000` |
| `--max-inline-bytes` | `1048576` |
| `--default-timeout-ms` | `30000` |
| `--max-timeout-ms` | `120000` |
| `VEOVEO_SURREAL_*` | required database-scoped runtime connection |
| `VEOVEO_INTERNAL_TRUST_JWKS` | required gateway verification keys |

The health endpoint reports process availability. Startup failure is the
readiness signal for invalid deployment configuration or an unavailable Spatial
extension.

## Output Encoding

Inline rows are row-major JSON values accompanied by column names and DuckDB
type names. SQL null, booleans, numeric values, strings, dates, times, and
timestamps receive controlled JSON conversions. Binary values are base64 text.
Values that do not have a narrower controlled conversion use DuckDB's owned
value representation before JSON serialization.

Output size accounting happens before large blobs are copied into a result.
This makes the byte limit a materialization boundary rather than a check after
allocation.

Artifact metadata returned from a tool omits direct download URLs. Bytes remain
behind the artifact-plane and gateway download authorization paths.

## Server Layout

The server follows the repository's domain-server module boundary.

```text
mcp/contract/src/duckdb.rs
  shared DuckDbSource and safe SQL-fragment construction

platform/runtimes/duckdb/
  src/engine.rs
  src/source.rs

servers/duckdb-mcp/
  Cargo.toml
  Dockerfile
  src/
    lib.rs
    contract.rs
    engine.rs
    artifacts.rs
    state.rs
    uris.rs
    bin/
      server.rs
      server/
        app_state.rs
        config.rs
        host.rs
        internal_auth.rs
        outputs.rs
        ownership.rs
        sql_ops.rs
        task_extension.rs
```

`server.rs` owns MCP protocol composition and the task worker loop. SQL
operations, ownership, output assembly, authentication, configuration, and task
extension handling remain focused modules. New map, catalog, or feature-service
behavior must not accumulate in this binary.

## Testing Strategy

Runtime tests cover:

- arbitrary analytical SQL inside the sandbox
- native read-only mutation rejection
- external access, configuration, attachment, install, and load denial
- trusted-extension path validation and missing-extension failure
- real Spatial geometry predicates, R-tree creation, and MVT generation when
  the pinned binary is supplied
- request and spill directory separation
- inline row and byte caps
- single-statement lexical validation
- governed source path, size, host, redirect, and IP checks

Server tests cover:

- database id validation
- query output wire shapes
- URI parsing
- artifact-plane repository behavior
- task recovery classification
- timeout interruption and indeterminate mutation reporting
- multi-statement DDL/DML
- embedded query injection rejection
- identity-derived profile workspaces

The Rust multi-process smoke harness covers gateway projection of DuckDB tools,
task-capable execution, durable task completion, and result continuity across
MCP sessions with a fresh token for the same principal. Deployment checks cover
the image, shared library packaging, edge-route denial, persistent singleton
shape, and container configuration.

Spatial release validation should always include the real pinned extension.
Mocks cannot prove extension ABI compatibility, geometry execution, or R-tree
availability.

## Deliberate Limits and Follow-On Work

The current server has deliberate boundaries:

- Database resources expose schemas, not row browsing or a database catalog
  administration model.
- Mutable databases have no cross-owner grant surface.
- Typed ingest does not yet include FlatGeobuf, GeoPackage, Shapefile, COG, or
  arbitrary GDAL-backed `ST_Read` sources.
- Spatial column semantics do not yet carry shared coordinate frame and CRS
  metadata in the database resource.
- MVT computation exists in SQL, but no tile archive export or tile resource is
  defined.
- There is no global query admission queue beyond container limits and
  per-database writer serialization.
- There are no resource subscriptions, prompts, or completions.
- A whole database cannot be deleted through a separate administrative tool.
  SQL may remove its contents, while physical workspace lifecycle stays an
  operator concern.

Likely follow-on work should preserve the current boundaries:

- add coordinate/CRS metadata to database and artifact summaries
- add governed geospatial source variants where a real workflow requires them
- add query-plan or concurrency policy after measuring contention
- add immutable tile-archive export only through a typed map or GIS contract
- expose new discovery surfaces as resources, templates, prompts, or
  completions rather than adding lookup tools by default

A future `map-mcp` should consume immutable analytical products or explicitly
governed live layers. It should not read another principal's DuckDB file, use a
filesystem path as a layer identity, or turn DuckDB into a second unaudited HTTP
data plane.
