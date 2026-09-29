# Recording MCP design

`recording-mcp` is the governed catalog, playback, and projection boundary for
Recording Hub data. [`docs/RECORDINGS.md`](../../docs/RECORDINGS.md) defines the
repository-wide ingest, storage, publication, activation, and operations contract.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| Model Context Protocol `2026-07-28` | JSON-RPC 2.0 over Streamable HTTP for recording discovery, layer inspection, sealing, projection control, resources, prompts, subscriptions, and notifications. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | `ui://recording/explorer.html` is the server-owned Recording Explorer. |
| RFC 3986, RFC 6570 and RFC 9562 | Shared URI components and discovery templates; recording routes require lowercase hyphenated RFC UUIDv7 identities. |
| Recording catalog cursor version 1 | Collection-bound JSON encoded as lowercase hexadecimal, limited to 2048 input bytes, with a timestamp and typed recording identity. |
| JSON Schema Draft 2020-12 | Closed tool inputs, views, playback manifest, grants, projection handles, and storage diagnostics. |
| Rerun `0.38.1` RRD | Immutable Artifact-backed capture, properties, and derived layers. Dataset UUID is the Rerun application ID, and recording UUID is the Rerun recording and segment ID. |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Read-only WebViewer and Catalog SDK subset over HTTP/2 or gRPC-Web. The service does not claim complete Redap conformance. |
| Apache Arrow IPC stream | Deterministic bounded projection payload produced from exact admitted RRD layers. |
| Veoveo playback manifest v9 | `veoveo.ai/recording-playback/v9` is the only accepted manifest. |
| Veoveo framed RRD stream v2 | Same-origin live channel adapter using one complete RRD per big-endian length frame. |
| OAuth service authentication and JWT | Gateway internal assertions, short-lived host-limited Redap grants, and separate Artifact-read credentials. |
| H.264 Annex B and SHA-256 | Decoder-reentrant live continuity and immutable byte identity. |

## Library Features

The library's `contract` feature exposes the types owned by
[`platform/recordings/contract`](../../platform/recordings/contract/DESIGN.md).
Consumers enable it with default features disabled. That domain crate sits below Hub
and this server because both publish Recording identities while the server uses Hub's
Blueprint, live-message and publication implementations. The library exposes one model
through `contract.rs` and the same discovery factories through `uris.rs`.

The contract includes recording views, playback manifests, catalog grants and Arrow
projection requests and handles. It excludes MCP, Store, asynchronous runtimes and
Rerun implementations. The gateway's Recording adapter imports this library's contract
feature and uses its Recording ID admission and URI builder for policy targets.
MCP core defines no Recording models and depends on neither domain crate nor server.

`runtime` enables Store access, the reader and caches, sealing, projection execution
and live playback. `mcp` adds discovery and HTTP adapter dependencies and is enabled
by default. `redap` includes that profile and adds the Rerun catalog service used by
the binary. `redap-conformance` adds Rerun's read-profile checks. Native test fixtures
are attached to their runtime feature, so contract tests do not activate Rerun SDKs.

The resource adapter dispatches `RecordingResource` in `bin/server/resources.rs`;
sealing, prompts and subscriptions use the same Recording identity admission. The
service converts admitted catalog positions to Store types immediately before SQL.
Public catalog, playback, layer and projection models keep distinct owner types for
Recording, dataset, layer, read-grant and projection IDs. JSON admission requires
canonical RFC UUIDv7 values; Store conversion requires native UUID keys and the
declared table. Catalog selection construction checks the 1–500 input limit and
sorts and deduplicates the Recording IDs before gateway authorization and Store calls.
Redap token subjects use the same grant ID admission. The optional reusable-grant
header accepts one canonical ID; malformed or duplicate values fail before catalog work.
Authorization, dataset membership and operational limits belong to runtime owners.

Store grant requests validate the catalog revision and normalize the selected Recording
IDs. Viewer and projection grants admit one Recording; catalog grants admit up to 500.
Grant creation checks dataset tenancy and every selected Recording's current dataset,
tenant and labels in the same transaction as the write. The caller scope carries typed
tenant, actor, Work Context, policy revision and label clearance and is shared with
projection admission. It comes from gateway admission, not from request JSON.

A reusable-grant hint must match that scope, the request's class, dataset, complete
selection, admitted-set digest and catalog revision, and must be unexpired. SQL applies
those predicates and current parent visibility before decoding a row. A mismatched hint
may result in a freshly admitted grant; it grants no authority itself. Redap redemption
first verifies the signed token's issuer, host and read permission, then SQL selects an
unexpired viewer or catalog grant. Projection grants cannot enter the Redap catalog.
Redap uses the short-lived grant's recorded authority; its bearer supplies no fresh
actor label or Work Context assertion.

Manifest assembly uses the domain's checked builder. Console decodes the same sealed
model, while the server validates the selected grant and prepares only its admitted
catalog. The domain's origin and URI builders use URL host, port, path and query setters,
including IPv6. Catalog response construction checks the typed entry URI against the
dataset, admits a sorted unique selection and exposes typed UTC expiry. Playback
construction checks the archive URI's dataset and Recording identities against the
manifest. Runtime qualification compares both address families with Rerun 0.38.1.
Remaining address-field admission is adoption work
in the [foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

`service/views.rs` converts SQL-admitted rows into checked catalog, layer and manifest
models. Artifact record references require the expected table and native RFC UUIDv7
key; malformed references return errors without panicking. Digests, layer names and
lifecycle facts pass the domain builders before public output. Seal admission checks
committed metadata before advancing state, and constructs the checked result before
completing the seal. A sealed retry validates its output before removing local static
context. The domain imports Artifact types through the lightweight owner contract.

Projection coordinate metadata carries Frames-owned revision-scoped URIs. The domain
admits at most 64 distinct references and preserves their order in request identity
and result comparison. These are supplied metadata; projection neither resolves their
worlds nor transforms coordinates. The Frames contract sits below both runtimes to
avoid the Frames-to-RRD-to-Recording dependency cycle.

## MCP Setup And Sealing Permission

`mcp_setup::RecordingContract` implements the shared `McpServerContract` trait with
Recording's resource and scope types. Startup forces its checked setup before Store,
cache or Redap initialization. Discovery serves the six admitted resources and four
parsed templates from that setup. Reference, local and catalog-fixture registrations declare revision 3 and
static discovery. Subscription admission accepts the catalog root, recording metadata
and layer addresses; it excludes documents, catalog pages and unrelated Task handles.

The shared Recording contract owns `RecordingScope::Seal`, serialized as
`recording:seal`. The service checks that typed permission before reading or mutating
Recording state, then applies the ordinary SQL visibility and lifecycle checks.
Gateway rules additionally require the administrator profile, `operator:use` and
`admin:manage`, with their existing role or service-principal constraints. Console's
configured scope request and the administrator clients' allowlists include the domain
permission. Possessing a parsed scope value establishes no grant.

The coordinated foundations installation uses this permission as a hard cut. Its
server, gateway configuration and clients deploy together; callers request a fresh
token containing `recording:seal`. The service accepts no `admin:manage` alias.

The binary initializes dependencies and starts the listener. `bin/server/mcp.rs` owns
MCP handlers; `bin/server/http.rs` wires authenticated HTTP and Redap routes and owns
playback, projection, readiness and storage diagnostics. The shared service enforces
sealing permission for every adapter.

## Durable Authority

SurrealDB records durable recording datasets, recordings, immutable layer manifests,
producer Blueprints, read grants, and projection receipts. Artifact occurrences are
authoritative for every committed RRD layer and sealed producer Blueprint. Hub-local
files and the Recording MCP cache are recoverable staging or derived state.

One dataset may admit many recording segments. The producer recording key remains source
metadata. Every layer is decoded and verified against the dataset and recording Store ID
before registration. Catalog revision is deterministic over durable dataset and recording
revisions plus the ordered layer identities and digests.

`RecordingService` and the shared analysis reader resolve visibility in Store SQL.
The predicate requires the actor's tenant and every recording label in the actor's
clearance. Ownership and Work Context do not further restrict this recording read
profile. The Store applies the same predicate to direct reads, pages, and completions.
Services load layer manifests only after the recording passes that predicate. Committed layers
require a fresh Artifact-read caller. Writing layers may use the confined Hub spool for
the live receiver only. A missing credential never falls back to an old local archive
path.

MCP resource discovery lists stable roots and documents. Templates describe individual
recordings, layers, and `recording://catalog{?cursor}`. Recording writes invalidate
resource contents and do not advertise or emit discovery-list changes.

The catalog root returns `{items, limit, next_cursor}` with at most 100 recording
views. SQL applies tenant, labels, and cursor predicates before fetching 101 rows.
Pages sort by immutable `started_at` descending, then recording UUID descending. The
versioned opaque cursor carries that pair and the collection identity. Invalid
versions, UUIDs, shapes, and query parameters fail with invalid params. Each page
rechecks the caller's current clearance; a cursor grants no access. Pages are live
reads rather than a snapshot. A new recording ahead of the cursor appears on refresh.
Clients subscribe to the catalog root for page invalidation.

Catalog views obtain layer counts through SQL aggregates. Completion matches recording
UUID or producer key case-insensitively in SQL, orders UUIDs, and fetches at most 101
values. It returns 100 suggestions with `hasMore` and omits the total when more exist.
The shared workbench requests one page at a time through its Previous and Next controls.
This resource envelope is part of the coordinated foundations installation upgrade;
clients must deploy together, with no array-response adapter.

A producer Blueprint remains confined staging while its recording is live. The
idempotent seal path validates its application, Blueprint identity, message count,
length, and digest, publishes an occurrence reserved from the durable Blueprint record,
stages that occurrence before manifest publication, and removes the spool copy. Sealed
playback materializes the Blueprint from Artifact storage and never treats the removed
spool path as archive authority. Seal also removes the recording-scoped static context
after the durable state transition. An idempotent seal retry repeats that cleanup, which
prevents live-only context from accumulating after a successful seal.

## Layer Cache

The [shared reader cache](../../platform/recordings/reader/DESIGN.md) owns Artifact-to-PVC materialization. It reserves capacity before the
download, uses a partial file, verifies length and SHA-256, checks the canonical RRD Store
ID, and atomically installs the result. Capture and properties layers bind the durable
dataset and recording UUIDs. Blueprints bind the application ID, Blueprint ID, and exact
message count. The cache key includes occurrence UUID and digest. Pinned entries cannot
be evicted. Least-recently-used unpinned entries are removed until both the managed
ceiling and physical free-space floor are safe.

Virtual catalogs release their cache pins after five minutes without authorized access,
equal to the maximum Redap token lifetime. Playback and projection entrypoints prune idle
catalogs before they materialize another layer.

Startup removes partial and unrecognized files. A complete cache deletion changes no
durable identity and loses no recording. `/readyz` fails when the cache or projection
scratch violates its storage contract. Authenticated `/admin/storage` exposes the typed
`veoveo.ai/recording-storage-diagnostics/v1` snapshot.

## Governed Virtual Catalogs

`playback.rs` builds one Rerun handler per durable grant key:

```text
(tenant, dataset, policy revision, admitted recording-set digest, grant class)
```

Only the exact admitted layers are registered. The service never registers a broader
dataset and relies on response filtering for direct chunk access. Viewer grants admit one
recording. Catalog grants admit an explicit sorted set in one dataset. Projection grants
cannot reach Redap.

An active viewer does not construct a virtual archive catalog. Its manifest names the
live receiver and Blueprint, while committed history remains authoritative in Artifact
storage. Once the recording leaves `live`, the viewer materializes the complete archive.
Catalog SDK and projection requests select complete committed materialization explicitly.

The scoped Redap service implements these read methods:

- `Version`, `WhoAmI`, and exact `FindEntries`
- `ReadDatasetEntry`
- dataset, dataset-manifest, and recording-segment-table schema reads
- dataset manifest and recording segment table scans
- RRD manifest and segment asset reads
- `QueryDataset` and `FetchChunks`
- bounded `WatchEvents` and the client bandwidth probe

The handler returns permission denied for entry, dataset, table, registration, task,
maintenance, and streaming mutations. `WriteChunks` and `WriteTable` are explicitly
denied. Rerun 0.38's `WhoAmI` response advertises an explicit empty capability set,
because this read-only endpoint cannot register `file://` sources. Selected
`re_redap_tests 0.38.1` assertions cover query filters, manifest scans,
chunk completeness, and missing recording segments. Veoveo tests own grant isolation,
scope, expiry, and direct-fetch authorization.

## Projection

The Recording domain's builders admit projection bounds, unique selectors, ordered
sampling and metadata before producing an immutable request. JSON decoding uses those
same builders. `service/projection.rs` applies the configured deadline and prepares a
Rerun query through `ArrowProjectionQuery::new` before Artifact materialization, Store
receipts or scratch reservation. The shared RRD module owns the upstream grammar and
rejects selectors that resolve to duplicate entities. The service acquires its concurrency
permit before materializing source layers. RRD combines the admitted immutable
layers, enforces row, sample and byte limits, emits Arrow IPC, rejects non-finite selected
numeric data, and computes result and schema digests.

Result construction uses the Recording domain's checked handle builder. It compares
parents, query metadata and output bounds with the request. Result reuse additionally
matches query digest, catalog revision and expiry with the SQL-admitted receipt and
verifies file length and SHA-256. Digests stay typed until their wire or database
conversion; output byte lengths use a nonzero type.

`service/projection/scratch.rs` owns file accounting and recovery. Startup retains
unexpired, structurally admitted metadata with a matching projection filename and
verified payload. It removes partial, corrupt, expired and orphan files. Metadata reads
and writes have a 512 KiB ceiling, which covers the maximum sample grid and metadata
fields. Reservations include that metadata allowance; committed and recovered pairs
charge the actual combined Arrow and metadata length against the scratch quota. New
reservations reclaim expired completed pairs under the accounting lock and preserve
active partial files. Expiry cleanup needs no process restart or background poller.

The runtime has two permits and 96 MiB aggregate scratch at the reviewed maximum. A
request may select at most 64 entities, 64 components, 10,000 samples, 10,000 rows,
32 MiB, and 15 seconds. Deployment values may lower these limits. Cancellation, deadline,
validation failure, or worker failure removes partial output and releases its reservation.

Receipts persist the actor, one-recording projection grant, idempotency key, manifest
digest, query digest, result identity, state, and expiry. The typed Store request requires
dataset and Recording IDs, two SHA-256 digests, and an admitted idempotency key. A key
belongs to one tenant and actor; while its receipt exists, reuse requires the original
Work Context, policy revision, dataset, Recording, and input digests. SQL reports a
conflict before decoding a mismatched receipt.

Reservation and every state transition apply the caller's typed scope inside the same
transaction as the write. SQL checks current Recording labels, its dataset, and the
matching unexpired App projection grant, including actor, context, policy, dataset,
recording set and catalog revision. Reservation takes its catalog revision from the
admitted grant and requires the receipt to expire no later than that grant. Transitions
enforce the predecessor state; repeating a terminal transition requires the same result
length and digest or the same failure reason. Conflicting concurrent transitions cannot
both commit. Store retries a reported transaction conflict at most seven times and
resolves a concurrent unique-key insertion through admitted lookup. A transport error
returns to the caller without redispatching the mutation.

Download admission applies the same scope and relationships to a ready, unexpired
receipt for exactly the requested Recording. Denied rows never reach Rust decoding or
scratch access. An admitted download rechecks the file length and SHA-256 before streaming.

The Gateway and Console BFF keep authorization and routes outside the opaque App frame.
The Console host extension `veoveo/recordings/projection-stream` accepts only the exact
Recording Explorer descriptor. It transfers a `ReadableStream` through a dedicated
`MessagePort` together with expected length and digest. There is no buffering fallback.

## Playback Manifest And Live Stream

Manifest v9 contains the durable dataset ID, recording segment ID, catalog revision,
short-lived viewer grant, optional archive descriptor, optional live receiver, and
governed Blueprint. An active recording exposes the live receiver without prewarming its
committed archive. A recording that has left `live` exposes the archive. Archive URI is
one stable Rerun dataset-segment URI. It is not a URL per capture layer.

Live playback retains one WebViewer `LogChannel`. The first transport supplies bounded
static and temporal bootstrap state. A reconnect on the same channel starts at the
current durable head. Filesystem notifications advance complete ingest parts and writing
layers without polling. All messages are rewritten to the same Store ID used by archive
playback.

The live adapter removes sparse H.264 keyframe columns after compaction because Rerun
derives sync samples from the access-unit bytes and requires dense sample chunks. The
durable capture bytes are not modified by this browser adapter.

## Module Ownership

| Path | Responsibility |
|---|---|
| `contract.rs` | recording, layer, seal, manifest v9, and manifest-occurrence views |
| `contract.rs`, `uris.rs` | public access to the shared Recording domain contract and its resource factories |
| `service.rs` | playback plans, sealing, properties publication, and catalog revision |
| `service/index.rs`, `index.rs` | SQL-authorized catalog assembly, completions, direct reads, and versioned resource cursors |
| `platform/recordings/reader` | shared governed analysis plans, task-local live-part snapshots and bounded cache |
| `service/projection.rs` | RRD query preparation, receipt transitions, result construction and Arrow download |
| `service/projection/scratch.rs` | concurrency, storage reservations, bounded metadata, integrity checks and restart recovery |
| `blueprint_cache.rs` | server-owned Blueprint validation for the shared RRD cache |
| `playback.rs` | durable grants, virtual Rerun handlers, scoped Redap, and manifest composition |
| `live_playback.rs` | bounded reactive Rerun message projection for writing layers |
| `live_stream.rs` | authenticated framed RRD transport |
| `bin/server.rs` | thin HTTP, gRPC-Web, readiness, diagnostics, and MCP composition |

## Validation

The native `tests/catalog_queries.rs` fixture proves pagination beyond 500 records,
timestamp tie handling, label and tenant rejection before limits, current-clearance
rechecks, bounded SQL completion, and layer counts. It requires the pinned disposable
SurrealDB image and no GPU. Shared workbench tests provide browser behavioral evidence
for page navigation and notification refresh.

Focused component evidence includes deterministic RRD normalization and Arrow bytes,
cache corruption and eviction behavior, scratch cleanup, manifest v9 rejection of other
schemas, durable grant transactions, and selected official Redap assertions. Console
tests prove that no bearer, URL, local path, RRD bytes, or whole `ArrayBuffer` crosses the
projection bridge.

The deployment acceptance uses the typed Rust smoke harness. Archive and live viewer
acceptance must run in a headed browser after the harness proves hardware WebGPU or
WebGL. It rejects SwiftShader, llvmpipe, software adapters, and software rasterizer
warnings. The operator interacts with the embedded Rerun viewer and visually inspects the
captured image before the result qualifies.

## Replica Resource Observation

Each replica opens one shared group of projected Store LIVE queries for datasets, recordings, layers and Blueprints.
Committed changes invalidate only each listener's accepted resource contents, including
the recording catalog resource. They do not emit resource-list changes. Writes coalesce
over 100 milliseconds. The catalog URI accepts subscriptions from authenticated callers;
exact recording and layer URIs require visibility of that recording at admission.
Source reconnection invalidates readers after a delivery gap; reads retain normal
current authority. Idle sources emit no periodic resource-change notifications. This observes durable state and cannot dispatch work.
