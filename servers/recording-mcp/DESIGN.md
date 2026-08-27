# Recording MCP design

`recording-mcp` is the governed catalog, playback, and projection boundary for
Recording Hub data. [`docs/RECORDINGS.md`](../../docs/RECORDINGS.md) defines the
repository-wide ingest, storage, publication, activation, and operations contract.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| Model Context Protocol `2026-07-28` | JSON-RPC 2.0 over Streamable HTTP for recording discovery, layer inspection, sealing, projection control, resources, prompts, subscriptions, and notifications. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | `ui://recording/explorer.html` is the server-owned Recording Explorer. |
| JSON Schema Draft 2020-12 | Closed tool inputs, views, playback manifest, grants, projection handles, and storage diagnostics. |
| Rerun `0.36.3` RRD | Immutable Artifact-backed capture, properties, and derived layers. Dataset UUID is the Rerun application ID, and recording UUID is the Rerun recording and segment ID. |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Read-only WebViewer and Catalog SDK subset over HTTP/2 or gRPC-Web. The service does not claim complete Redap conformance. |
| Apache Arrow IPC stream | Deterministic bounded projection payload produced from exact admitted RRD layers. |
| Veoveo playback manifest v9 | `veoveo.io/recording-playback/v9` is the only accepted manifest. |
| Veoveo framed RRD stream v2 | Same-origin live channel adapter using one complete RRD per big-endian length frame. |
| OAuth service authentication and JWT | Gateway internal assertions, short-lived host-limited Redap grants, and separate Artifact-read credentials. |
| H.264 Annex B and SHA-256 | Decoder-reentrant live continuity and immutable byte identity. |

## Durable Authority

SurrealDB records durable recording datasets, recordings, immutable layer manifests,
producer Blueprints, read grants, and projection receipts. Artifact occurrences are
authoritative for every committed RRD layer and sealed producer Blueprint. Hub-local
files and the Recording MCP cache are recoverable staging or derived state.

One dataset may admit many recording segments. The producer recording key remains source
metadata. Every layer is decoded and verified against the dataset and recording Store ID
before registration. Catalog revision is deterministic over durable dataset and recording
revisions plus the ordered layer identities and digests.

`RecordingService` resolves visibility before loading layer manifests. Committed layers
require a fresh Artifact-read caller. Writing layers may use the confined Hub spool for
the live receiver only. A missing credential never falls back to an old local archive
path.

A producer Blueprint remains confined staging while its recording is live. The
idempotent seal path validates its application, Blueprint identity, message count,
length, and digest, publishes an occurrence reserved from the durable Blueprint record,
stages that occurrence before manifest publication, and removes the spool copy. Sealed
playback materializes the Blueprint from Artifact storage and never treats the removed
spool path as archive authority. Seal also removes the recording-scoped static context
after the durable state transition. An idempotent seal retry repeats that cleanup, which
prevents live-only context from accumulating after a successful seal.

## Layer Cache

`layer_cache.rs` owns Artifact-to-PVC materialization. It reserves capacity before the
download, uses a partial file, verifies length and SHA-256, checks the canonical RRD Store
ID, and atomically installs the result. Capture and properties layers bind the durable
dataset and recording UUIDs. Blueprints bind the application ID, Blueprint ID, and exact
message count. The cache key includes occurrence UUID and digest. Pinned entries cannot
be evicted. Least-recently-used unpinned entries are removed until both the managed
ceiling and physical free-space floor are safe.

Startup removes partial and unrecognized files. A complete cache deletion changes no
durable identity and loses no recording. `/readyz` fails when the cache or projection
scratch violates its storage contract. Authenticated `/admin/storage` exposes the typed
`veoveo.io/recording-storage-diagnostics/v1` snapshot.

## Governed Virtual Catalogs

`playback.rs` builds one Rerun handler per durable grant key:

```text
(tenant, dataset, policy revision, admitted recording-set digest, grant class)
```

Only the exact admitted layers are registered. The service never registers a broader
dataset and relies on response filtering for direct chunk access. Viewer grants admit one
recording. Catalog grants admit an explicit sorted set in one dataset. Projection grants
cannot reach Redap.

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
denied. Selected `re_redap_tests 0.36.3` assertions cover query filters, manifest scans,
chunk completeness, and missing recording segments. Veoveo tests own grant isolation,
scope, expiry, and direct-fetch authorization.

## Projection

`service/projection.rs` validates every request bound before obtaining a work permit or
reserving scratch. The shared `veoveo-rrd` projection module builds a Rerun query
expression, combines the admitted immutable layers, emits canonical Arrow IPC, rejects
non-finite selected numeric data, and computes result and schema digests.

The runtime has two permits and 96 MiB aggregate scratch at the reviewed maximum. A
request may select at most 64 entities, 64 components, 10,000 samples, 10,000 rows,
32 MiB, and 15 seconds. Deployment values may lower these limits. Cancellation, deadline,
validation failure, or worker failure removes partial output and releases its reservation.

Receipts persist the actor, one-recording projection grant, idempotency key, manifest
digest, query digest, result identity, state, and expiry. Reusing a key for a different
request conflicts. A ready download rechecks the file length and SHA-256 before streaming.

The Gateway and Console BFF keep authorization and routes outside the opaque App frame.
The Console host extension `veoveo/recordings/projection-stream` accepts only the exact
Recording Explorer descriptor. It transfers a `ReadableStream` through a dedicated
`MessagePort` together with expected length and digest. There is no buffering fallback.

## Playback Manifest And Live Stream

Manifest v9 contains the durable dataset ID, recording segment ID, catalog revision,
short-lived viewer grant, archive descriptor, optional live receiver, and governed
Blueprint. Archive URI is one stable Rerun dataset-segment URI. It is not a URL per
capture layer.

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
| `service.rs` | visibility, playback plans, sealing, properties publication, and catalog revision |
| `service/read.rs` | governed Artifact-backed analysis plans and task-local live-part snapshots |
| `service/projection.rs` | request validation, receipts, concurrency, scratch, and Arrow download |
| `layer_cache.rs` | verified bounded Artifact-backed RRD cache |
| `playback.rs` | durable grants, virtual Rerun handlers, scoped Redap, and manifest composition |
| `live_playback.rs` | bounded reactive Rerun message projection for writing layers |
| `live_stream.rs` | authenticated framed RRD transport |
| `bin/server.rs` | thin HTTP, gRPC-Web, readiness, diagnostics, and MCP composition |

## Validation

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
