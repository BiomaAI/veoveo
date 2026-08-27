# Recording Catalog Hard Cut Implementation Plan

Status: complete. Platform request `016` is activated in the disposable Bioma
installation and accepted through live and sealed Rerun playback on hardware-backed
graphics. The landed contract is the only supported recording architecture.

This plan extracts the Recording Catalog Hard Cut from
[`PLATFORM_IMPROVEMENTS_PLAN.md`](PLATFORM_IMPROVEMENTS_PLAN.md). It is deliberately
bounded. The work replaces the current per-recording, filesystem-backed playback catalog
with one durable dataset model, immutable Rerun layers in the Artifact plane, governed
virtual catalogs, and bounded Arrow projections. It does not implement the other platform
improvement phases.

## Standards And Protocols

The implementation boundary includes:

| Standard or contract | Supported profile |
|---|---|
| [Rerun 0.36](https://rerun.io/docs/changelog/changeset-0-36) | Exact stable patch selected at implementation start across Rerun Rust crates and `@rerun-io/web-viewer`; recordings are Rerun segments, layers are immutable RRD objects, and the server exposes only the read methods named below. |
| [Rerun catalog object model](https://rerun.io/docs/concepts/query-and-transform/catalog-object-model) | One Veoveo recording dataset maps to one Rerun dataset. Each Veoveo recording UUID maps to one Rerun segment ID. |
| Rerun Redap | Read-only subset required by WebViewer and Catalog SDK reads. This plan does not claim full Rerun Server conformance. |
| Apache Arrow IPC | Stream format from the Arrow release selected by the matching Rerun/DataFusion dependency graph. No JSON row response remains on the public recording query path. |
| MCP `2026-07-28` and Veoveo MCP contract | Existing hosted-server contract. Recording projection control remains MCP; bulk Arrow bytes do not pass through MCP. |
| OAuth 2.0 Authorization Server Metadata, client credentials, and `private_key_jwt` | Existing Veoveo service-authentication profile. Recording Hub publishes with its own service identity and never retains a producer bearer. |
| Veoveo Artifact plane | Immutable, tenant-scoped occurrences and content-addressed blobs. A narrow streaming publication method is added for recording layers. |
| RRD ingest v2 | Existing producer-to-Hub framed ingest protocol remains unchanged apart from canonical Store ID normalization. |
| Playback manifest v9 | Repository-owned hard-cut playback manifest introduced by this work. Manifest v8 is removed at activation. |

Immediately before the first build-input change, verify the latest stable upstream Rerun
release from Rerun's official release material. Pin one exact patch everywhere. A release
change is not a reason to expand this project into a general Rerun upgrade.

## Scope Decision

Implement request `016` as one isolated vertical change. The required behavior is:

- a durable `recording_dataset` identity shared by many recordings;
- a recording UUID that is also the Rerun segment identity;
- immutable capture, properties, and derived layers stored through the Artifact plane;
- object-backed, policy-filtered virtual catalogs served by `recording-mcp`;
- read-only Redap interoperability for WebViewer and Rerun Catalog SDK clients;
- deterministic, bounded Arrow IPC projections for hosted Apps;
- unchanged live-channel semantics under the new dataset and segment identities;
- explicit PVC, concurrency, and disk-headroom limits that prevent catalog work from
  consuming unbounded pod ephemeral storage.

The recommended activation policy is to discard existing development recording data.
The current local and showcase deployments are disposable, while an in-place migrator
would need to reinterpret producer keys, old capture-shard rows, local paths, existing
artifact occurrences, and playback tokens. That work is not required to prove the new
contract.

Before schema activation, the owner must choose one of these outcomes:

1. Approve the recommended discard and reset the SurrealDB recording data, recording
   spool, catalog cache, and associated development object data during activation.
2. Require retention. In that case, stop this implementation before activation and
   approve a separate, offline migration project with fixtures from the data that must be
   retained.

The implementation must not silently support both paths. It must not add compatibility
tables, aliases, manifest readers, or background conversion.

## Boundaries That Keep The Work Finite

This project does not include:

- exact App resource resolution or health reporting from request `014`;
- governed Artifact upload from request `017`;
- extension release, tracing, Live View packaging, GPU memory, reasoning, or deployment
  work from requests `018` through `023`;
- a separate `rerun server` deployment or a fork of Rerun;
- Redap mutation, table administration, task, maintenance, or general object-store
  protocols;
- a general workflow, distributed transaction, quota, or OAuth client framework;
- domain-specific query languages beyond registered recording projection descriptors;
- replacement of the existing RRD v2 live protocol, NVENC paths, or browser renderer;
- compatibility for playback manifest v8 or the current per-recording catalog.

The current Recording Explorer App is the only hosted App that must consume the new
projection stream. The host bridge primitive may be reusable, but this project does not
redesign the App catalog or resource authority.

## Implementation Record

The implementation follows the bounded design in this document:

- migration `0046_recording_catalog_hard_cut` introduces durable datasets, layers,
  grants, and projection receipts and removes the old recording catalog tables;
- `platform/recordings/rrd` owns Store ID normalization, properties layers, and
  deterministic bounded Arrow IPC;
- Recording Hub publishes verified capture layers through a scoped streaming Artifact
  route and reserves spool headroom before materialization;
- Recording MCP owns the verified PVC cache, durable virtual-catalog grants, selected
  read-only Redap profile, manifest v9, projections, storage readiness, and diagnostics;
- Gateway and Console carry short-lived internal Artifact authority while the Recording
  Explorer receives only a host-mediated transferable stream;
- Helm supplies separate cache storage, bounded scratch and temporary files, explicit
  ephemeral-storage resources, and fail-closed free-space floors;
- `cargo xtask doctor` validates the recording storage budget and rejects obsolete
  surfaces in normative recording contracts.

The activation uses the approved disposable-data path. Recording-based durable Reason
and Stream replay remain outside request `016` because restart-safe Artifact-read
authority requires its own design. They fail closed instead of persisting a caller bearer
or falling back to a Hub-local archive path.

## Current Baseline And Hard-Cut Deletes

The current implementation already has useful parts: bounded ingest, fsynced local
spooling, complete RRD batch validation, live rollover, Blueprint handling, recording
governance, Artifact occurrences, and a read-only Redap endpoint. Preserve those parts
unless the new identity or durability contract directly conflicts with them.

The following surfaces conflict and must be removed in the activation change:

| Current surface | Hard-cut replacement |
|---|---|
| Free-form `recording.dataset` string | Reference to a durable `recording_dataset` record |
| `segment` table and `SegmentId` for physical capture shards | `recording_layer` rows; the recording UUID itself is the Rerun segment ID |
| Producer `recording_key` as playback segment identity | Producer metadata only; never an Rerun or database identity |
| Per-recording derived Rerun dataset | One dataset identity with one or more admitted recording segments |
| Frozen local paths as catalog authority | Artifact occurrence, digest, length, and layer metadata in SurrealDB |
| `MAX_CATALOGS` per recording | Aggregate catalog, layer-cache, scratch, and concurrency budgets per pod |
| Hub `query` and `hub-query` JSON rows | Typed Arrow projection in `recording-mcp` and shared RRD query code |
| Playback manifest v8 | Playback manifest v9 only |
| History assembled by scanning local spool paths | Policy-filtered committed layer manifest fetched from the durable store |

`servers/recording-mcp/AGENTS.md`, `servers/recording-mcp/DESIGN.md`, and
`docs/RECORDINGS.md` must change with the implementation. They must not describe the old
contract after activation.

## Locked Architecture

### Durable identities and records

Add strong types for `RecordingDatasetId` and `RecordingLayerId`. Continue to use
`RecordingId`, but define its UUID as the canonical Rerun segment ID. Do not provide
string aliases for old identifiers.

The next store migration introduces these records:

| Record | Required fields and invariants |
|---|---|
| `recording_dataset` | UUIDv7 ID, tenant, unique tenant-scoped key, display label, optional default Blueprint Artifact occurrence, typed retention policy, revision, and timestamps. |
| `recording` | Existing authority, governance, lifecycle, source, and timing fields, with a required `recording_dataset` reference. Its UUID is the Rerun segment ID. |
| `recording_layer` | UUIDv7 ID, recording reference, typed kind (`capture`, `properties`, or `derived`), stable name, capture ordinal when applicable, revision, publication state, optional pre-commit staging path, committed Artifact occurrence, length, SHA-256, Rerun version, schema digest, optional time bounds, and timestamps. |
| `recording_read_grant` | Grant ID, class, tenant, dataset, admitted recording IDs, canonical admitted-set digest, actor, Work Context, policy revision, catalog revision, expiry, and timestamps. |
| `recording_projection_receipt` | Projection ID, caller idempotency key, tenant, dataset, admitted recordings, manifest digest, query digest, authority context, state, result length and digest when complete, expiry, and timestamps. |

Enforce unique `recording_layer` keys for `(recording, name)` and capture ordinals. A
committed row requires an Artifact occurrence, length, and digest and cannot return to a
mutable state. Properties and derived layers are immutable revisions; mutable tags,
retention state, sharing, policy, and audit facts remain in SurrealDB.

The migration must fail with an actionable message when old recording rows exist. The
activation runbook performs the approved data reset before applying the migration. This
guard is safer than deleting object data from a schema migration.

### RRD identity and layer construction

Normalize every accepted capture before publication:

- the Rerun application ID is derived from `RecordingDatasetId`;
- the Rerun recording ID equals `RecordingId`;
- the producer recording key remains bounded source metadata;
- the existing tenant, actor, Work Context, source, and governance metadata remains
  explicit;
- each RRD object contains one admitted Store ID and a valid RRD footer.

The shared `platform/recordings/rrd` crate owns Store ID rewriting, footer validation,
properties-layer construction, query-expression construction, and deterministic Arrow
serialization. It must expose typed operations rather than raw JSON payloads.

Capture layers use increasing ordinals within one recording. The properties layer is
written when the recording is sealed and contains the bounded immutable recording
metadata needed by Rerun clients. A derived layer records its producer, source recording,
revision, schema digest, and provenance. The producer Blueprint wins when it exists;
otherwise the dataset default Blueprint is used.

The properties layer may contain the canonical recording and dataset identities, dataset
key, playback application identity, bounded producer key, lifecycle state, start/end/seal
times, source and manifest revisions, immutable manifest digest, and non-secret declared
model or environment revisions. It must not contain principals, groups, credentials,
policy rules, private object coordinates, filesystem paths, classification inputs, or
internal failure text. Admitted mutable labels belong in the virtual segment table rather
than reusable RRD bytes.

### Streaming Artifact publication

Artifact storage is authoritative after a layer commit. Local files are staging and
recovery material only.

Add one internal, create-only streaming publication operation to
`platform/artifacts/client` and `platform/artifacts/service`. It accepts a pre-reserved
Artifact occurrence ID, expected length, expected SHA-256, typed media descriptor, and a
stream body. The service writes through bounded buffers, verifies length and digest, and
atomically records the occurrence after the blob is durable. It must not assemble the
complete RRD in memory.

Use the `RecordingLayerId` UUID as the requested Artifact occurrence UUID for this exact
one-to-one publication. The Rust wrappers remain distinct types. A retry with the same
tenant, actor class, descriptor, length, and digest returns the existing occurrence. A
different request for that UUID returns a conflict. Partial uploads never become ledger
occurrences and are cleaned within a bounded interval.

Expose this operation through a recording-specific Gateway publication route. Recording
Hub authenticates with its own `private_key_jwt` service principal for capture layers.
`recording-mcp` uses a separately scoped service principal when sealing the properties
layer. The Gateway applies the recording publication policy and exchanges either identity
for the internal Artifact audience. Producer and operator access tokens are never
retained for background publication and are never written to disk.

The freeze transaction is retry-safe:

1. Reserve the `recording_layer` UUID and row in SurrealDB.
2. Validate and normalize the staged RRD, including H.264 GOP integrity where video is
   present, into a bounded local publication file.
3. Reserve enough spool headroom for the operation before writing the normalized file.
4. Stream the normalized file through the recording publication route.
5. Verify the returned occurrence ID, length, and digest.
6. Commit the layer manifest and increment the recording and dataset catalog revisions in
   one SurrealDB transaction.
7. Publish the revision notification.
8. Delete local publication and capture files after the recovery window.

A crash at any boundary resumes from the durable row. Recovery checks the exact
occurrence identity and digest; it does not list buckets or infer completion from a local
filename. This is a local retry state machine, not a distributed workflow engine.

### Object-backed read plans and cache

The open-source `re_server` integration used by this repository registers local files; it
does not make the Veoveo Artifact plane a Rerun catalog. Keep Artifact/object storage as
the source of truth and give `recording-mcp` a bounded derived file cache.

For each admitted layer, `recording-mcp` streams the Artifact occurrence to a partial file
on its catalog-cache PVC, verifies the committed length, digest, footer, and Store ID, then
atomically renames it into the cache. Cache keys include the occurrence ID and digest.
Virtual catalogs pin files they use. Eviction removes unreferenced catalogs before their
layer files and follows least-recently-used order.

The Gateway supplies a short-lived Artifact-read credential only on the internal viewer
manifest, Catalog SDK grant, and projection redemption paths. `recording-mcp` uses it to
populate missing cache entries and never persists or returns it. The credential header is
redacted from logs and stripped before every response.

At startup, delete partial files and expired projection scratch, load durable unexpired
grants, and rebuild catalog handlers lazily. Deleting the entire cache PVC must not lose a
recording or change its catalog identity.

### Governed virtual catalogs

A virtual catalog key is:

```text
(tenant, dataset_id, policy_revision, admitted_recording_set_digest, grant_class)
```

The catalog handler registers only the immutable layers belonging to that admitted set.
It never registers a broader dataset and tries to filter responses afterward. This makes
direct chunk fetches subject to the same authority as metadata listing.

Support exactly three grant classes:

| Grant class | Purpose | Data path |
|---|---|---|
| `viewer_segment` | Console playback of one recording | Read-only Redap plus the existing framed live channel |
| `catalog_dataset` | Rerun Catalog SDK access to an explicitly admitted recording set | Read-only Redap |
| `app_projection` | One bounded Arrow projection | Projection redemption only; no Redap token |

The Gateway evaluates tenant, owner, sharing, Work Context, policy revision, and requested
recordings before creating a grant. The Redap bearer subject maps to the durable grant ID,
not to a recording filename. A replica can reload an unexpired grant after restart.

Implement only the Redap methods exercised by the matching WebViewer, Catalog SDK, and
the selected subset of official `re_redap_tests`. Every other method returns the
protocol-correct unsupported response. Write and mutation methods are never registered.
Document the exact supported method list in `servers/recording-mcp/DESIGN.md` after the
compatibility spike establishes it.

The provisional read profile that the spike must confirm is:

- version and server identity;
- exact entry discovery inside the virtual catalog;
- dataset entry, schema, manifest schema, and segment table schema;
- dataset manifest and segment table scans;
- admitted RRD manifest and asset reads;
- `QueryDataset` latest-at and range chunk selection;
- `FetchChunks` constrained to chunk identities issued for the same grant;
- bounded event watch for immutable catalog revision changes.

Remove an item if neither supported client uses it. Adding a method requires an explicit
consumer and authorization test. Dataset, segment, table, registration, task, and
maintenance mutation remain denied.

### Deterministic Arrow projection

MCP handles projection control and returns a non-secret projection handle. The App frame
passes that handle to the Console host. The host redeems it through an authenticated,
same-origin BFF route, and the Gateway forwards the request with a short-lived internal
recording identity and Artifact-read credential. The model and App frame never receive a
bearer token, object URL, local path, or Redap credential.

The request type names the dataset, admitted recording set, entity selectors, component
selectors, timeline, range, latest-at or sampling mode, row limit, byte limit, and
deadline. All fields are explicit. Omitted bounds are rejected rather than interpreted as
an unbounded query.

Apply these initial server maxima:

| Limit | Initial maximum |
|---|---:|
| Entity selectors | 64 |
| Component selectors | 64 |
| Rows | 10,000 |
| Serialized Arrow result | 32 MiB |
| Wall-clock execution | 15 seconds |
| Concurrent projections per `recording-mcp` pod | 2 |
| Aggregate projection scratch per pod | 96 MiB |

Deployment configuration may lower these values. Raising them requires measured evidence
and a separate review, not an automatic formula.

Build ordered Arrow record batches through Rerun query expressions. Materialize the IPC
stream to bounded scratch on the catalog-cache PVC before response headers. Verify the
result length, schema, selected numeric constraints, and digest, then stream the file.
Cancellation, timeout, disconnect, or failed validation deletes scratch and releases the
concurrency permit. A matching caller idempotency key and request digest returns the same
receipt; a mismatched digest conflicts.

Result metadata names the recording set, dataset, immutable manifest revision and digest,
query revision and digest, timeline, units, coordinate-frame references, sample grid,
omitted-sample count, Arrow schema digest, byte length, and payload digest. It contains no
cookie, bearer, Redap token, object URL, or filesystem path.

Add one typed host-mediated stream operation to the existing App bridge. The Console host
performs the same-origin authenticated fetch and transfers a `ReadableStream` over a
dedicated `MessagePort`. It must not construct an `ArrayBuffer`, expose the BFF URL, or
fall back to buffering when transferable streams are unavailable. The Recording Explorer
App fails closed with a clear unsupported-browser state in that case.

### Live playback and manifest v9

Keep RRD v2 channel framing, generation rollover, reconnect, static context, bounded
history, and Blueprint behavior. Change only the identity rewrite: application ID is the
dataset UUID and recording ID is the recording UUID.

Playback manifest v9 contains the dataset ID, recording segment ID, catalog revision,
archive Redap descriptor, optional live receiver descriptor, and governed Blueprint.
Delete v8 structs, schemas, examples, tests, and readers in the activation commit. Do not
negotiate manifest versions.

## Disk And Pod Safety Contract

Catalog and projection work must not consume unbounded node ephemeral storage.

- Add a dedicated `recording-catalog-cache` PVC mounted read-write only by
  `recording-mcp`. Do not reuse the Hub spool PVC or an unbounded `/tmp`.
- Begin with a 10 GiB cache PVC, an 8 GiB managed cache ceiling, and 1 GiB minimum free
  headroom. The remaining capacity covers filesystem and recovery variance.
- Place projection scratch under the same managed PVC and enforce its separate 96 MiB
  aggregate ceiling.
- Give incidental `emptyDir` mounts an explicit `sizeLimit`. Add realistic ephemeral
  storage requests and limits for both Hub and `recording-mcp`.
- Before normalization, publication, or cache download, reserve the worst-case local
  bytes. Reject before writing when the reservation would cross the managed ceiling or
  minimum headroom.
- Export typed counters for spool bytes, committed cache bytes, pinned cache bytes,
  scratch bytes, reservations, evictions, and headroom rejections. This project adds the
  metrics required to operate its limits; it does not build the tracing UI from request
  `019`.

Local and showcase values may choose a smaller PVC only when their managed ceiling and
smoke fixtures are lowered with it. A software or API-only check cannot replace the
required hardware browser evidence for the WebViewer path.

## Implementation Work Packages

Each package ends in a coherent commit with its affected tests green. Do not start the
next package by committing a red test report.

### 1. Contract lock and dependency spike

- Confirm discard versus retention. Stop on a retention decision.
- Verify and pin the latest stable Rerun patch across Cargo and npm inputs.
- Add the matching `re_redap_tests` development dependency and identify the smallest
  official read-only method set needed by WebViewer and Catalog SDK clients.
- Prove that verified Artifact downloads can be registered through the existing local
  file resolver without a Rerun fork.
- Lock the public types, method list, manifest v9 shape, limits, and error semantics in
  the relevant design documents.

Exit gate: one dataset containing two recording segments can be assembled from local
fixture files, read by the selected clients, and rejected by unsupported write methods.

### 2. Durable model and hard-cut store API

- Add the guarded migration after the current migration head.
- Replace `SegmentId` and `SegmentRecord` with dataset and layer domain types.
- Split `platform/store/src/recordings.rs` by responsibility while changing it; keep the
  public store facade focused.
- Add transactional dataset revision, layer commit, grant, receipt, expiry, and cleanup
  operations.
- Update fixtures and every caller in one hard cut. No deprecated accessors remain.

Exit gate: SurrealDB integration tests prove uniqueness, immutable commit transitions,
revision atomicity, grant expiry, receipt idempotency, and the nonempty-old-data guard.

### 3. Canonical RRD layers

- Move Store ID normalization and validation into `platform/recordings/rrd`.
- Add typed properties and derived-layer builders.
- Update forwarder/Hub boundaries so the producer key cannot become an internal ID.
- Preserve current batch limits, video sample boundaries, and live rollover behavior.
- Delete Hub JSON query code and the `hub-query` binary once projection coverage exists.

Exit gate: fixtures with mismatched Store IDs are normalized deterministically, malformed
or multi-store files fail closed, and published layer bytes are stable across repeated
runs.

### 4. Streaming publication and Hub recovery

- Implement the bounded Artifact streaming operation and recording Gateway route.
- Add the Hub service principal and scoped publication policy.
- Replace in-memory archive publication with file-to-stream publication.
- Implement the durable freeze state machine, crash recovery, local cleanup, and disk
  reservation checks.
- Publish the properties layer through the same path when a recording seals.

Exit gate: a crash matrix at every numbered freeze boundary converges on one occurrence
and one committed layer, never publishes corrupt bytes, and leaves no permanent local
path in the catalog.

### 5. Layer cache, virtual catalogs, and grants

- Split the current large `playback.rs` into focused catalog, grant, Redap, and layer-cache
  modules. Keep HTTP wiring in the binary entrypoint.
- Implement verified Artifact-to-PVC materialization and reference-aware LRU eviction.
- Build dataset-scoped virtual catalogs from exact admitted recording sets.
- Add durable viewer and Catalog SDK grant routes through the Gateway.
- Integrate the selected official Redap tests and delete the per-recording catalog.

Exit gate: direct metadata and chunk requests cannot escape the admitted set, a cache
wipe and pod restart rebuild the same catalog, and expired grants fail on every replica.

### 6. Arrow projection and App stream

- Add typed projection request and receipt contracts.
- Implement query expressions and deterministic Arrow IPC in the shared RRD crate.
- Add projection control to Recording MCP and authenticated redemption through Gateway
  and Console BFF.
- Add the bounded host-mediated `ReadableStream` bridge operation and update only the
  Recording Explorer App.
- Remove public JSON query types and endpoints.

Exit gate: equal manifest and query inputs produce equal Arrow bytes, all limits fail
before response headers, cancellation leaves no scratch, and neither frame nor model can
observe a secret or data URL.

### 7. Manifest v9, deployment, and activation

- Switch archive and live playback to dataset and recording UUID identities.
- Delete playback manifest v8 and all old catalog/configuration surfaces.
- Add the cache PVC, disk resources, limits, startup cleanup, readiness checks, and
  metrics to Helm schemas and profiles.
- Extend the typed Rust smoke harness with the end-to-end scenarios below.
- Update all normative documents and `docs/CODEMAP.md` to the landed architecture.
- Execute the approved data reset, deploy the single new version, and run acceptance.

Exit gate: no old name or protocol shape remains in source, schema, tests, Helm values,
examples, generated schemas, or documentation.

## File Routing

Use these ownership paths. New modules should follow the responsibility split instead of
expanding existing large files.

| Concern | Primary paths |
|---|---|
| Store schema and strong types | `platform/store/migrations/`, `platform/store/src/models.rs`, `platform/store/src/recordings/` |
| RRD normalization and Arrow | `platform/recordings/rrd/src/` |
| Freeze, staging, and recovery | `platform/recordings/hub/src/archive.rs`, `catalog.rs`, `ingest.rs`, `spool.rs`, `config.rs` |
| Streaming Artifact write | `platform/artifacts/client/`, `platform/artifacts/service/` |
| Publication, grant, and projection routes | `platform/gateway/src/bin/gateway/` |
| Virtual catalog and Redap | `servers/recording-mcp/src/catalog.rs`, `grants.rs`, `redap.rs`, `layer_cache.rs` |
| Projection control | `servers/recording-mcp/src/projection.rs`, `contract.rs`, `service.rs` |
| Playback composition | `servers/recording-mcp/src/bin/server/`, `live_playback.rs`, `live_stream.rs` |
| Same-origin streaming | `apps/console/bff/src/recording_playback.rs`, `apps/console/web/src/apps/`, Recording Explorer view/components |
| Deploy contract | `deploy/helm/veoveo/values.yaml`, `values.schema.json`, `templates/recording.yaml`, profile overrides |
| Acceptance | `testing/smoke/src/bin/smoke/scenarios/recording_ingest.rs`, `testing/browser-smoke/`, component tests |

Names in this table describe the intended split. Adjust a filename when the existing
module boundary gives it a clearer home, then update `docs/CODEMAP.md` in the same commit.

## Verification And Evidence

### Component tests

The minimum automated matrix covers:

- store dataset/layer uniqueness, immutable transitions, revision races, expiry, and
  receipt idempotency;
- Artifact chunked input, bounded buffering, exact retry, digest mismatch, length
  mismatch, interrupted upload, and partial cleanup;
- Hub Store ID normalization, publication crash recovery, disk reservation failure before
  write, local cleanup, and service-token redaction;
- virtual catalog isolation at dataset listing, segment listing, schema, query, and direct
  chunk fetch methods;
- supported official Redap tests and explicit unsupported responses for every excluded
  method;
- Arrow selector validation, ordering, deterministic IPC bytes, row/byte/time/concurrency
  limits, cancellation, and scratch cleanup;
- manifest v9 archive/live composition, generation rollover, Blueprint priority, and v8
  absence;
- BFF streaming without buffering and App bridge tests proving no bearer, URL, local path,
  or `ArrayBuffer` crosses into the frame.

### Deployment smoke

All smoke orchestration, lifecycle, assertions, retries, cleanup, and evidence parsing
remain in Rust. Extend the existing recording scenario to:

1. Create one dataset and ingest two recordings into it.
2. Force more than one capture layer and seal both recordings.
3. Verify Artifact occurrences and the absence of authoritative local paths.
4. Open a viewer grant for one recording and prove the second is absent, including a
   direct chunk request.
5. Open a Catalog SDK grant for both recordings and read both through the matching Rust
   client.
6. Run a minimal Python Catalog SDK fixture under Rust harness control; the Rust harness
   owns assertions and parses its bounded result.
7. Render archived and concurrent live data in a headed browser after proving a
   hardware-backed WebGPU or WebGL context.
8. Redeem an App projection, verify Arrow content and bounded streaming, then cancel a
   second projection and prove scratch cleanup.
9. Delete the derived catalog cache, restart `recording-mcp`, and prove the same grants
   rebuild from Artifact storage.
10. Confirm cache and spool headroom, zero leaked scratch, no `DiskPressure`, and no
    evicted recording pods.

### Evidence commands

Run affected commands through the repository evidence recorder before each build-input
commit. The exact package set may be narrowed per commit, but the final activation must
include at least:

```sh
cargo xtask test-report run --name recording-store -- cargo test -p veoveo-platform-store recordings
cargo xtask test-report run --name recording-rrd -- cargo test -p veoveo-rrd
cargo xtask test-report run --name artifact-plane -- cargo test -p veoveo-artifact-service
cargo xtask test-report run --name recording-hub -- cargo test -p veoveo-recording-hub
cargo xtask test-report run --name recording-mcp -- cargo test -p veoveo-recording-mcp
cargo xtask test-report run --name console-bff -- cargo test -p veoveo-console-bff
cargo xtask test-report run --name console-web -- npm --prefix apps/console/web test
cargo xtask test-report run --name console-web-build -- npm --prefix apps/console/web run build
cargo xtask test-report show
```

Confirm package names with Cargo metadata when each package begins; do not copy a stale
name into test evidence. Run the existing typed deployment and browser smoke entrypoints
after the digest-locked images are installed. Commit only green, current
`testing/local-test-report.json` entries for build-input changes. Documentation-only
planning commits do not invalidate build evidence.

## Activation And Recovery

Before activation:

- record explicit approval of the discard path;
- take a complete pre-cut snapshot if any installation data might matter;
- stop recording producers and drain accepted Hub batches;
- reset the approved development recording, spool, cache, and object data;
- apply the guarded schema migration;
- deploy Gateway, Artifact service, Hub, `recording-mcp`, Console, and Helm contract from
  one compatible image set;
- run store, ingest, Redap, SDK, projection, live, browser-GPU, restart, and disk-pressure
  acceptance before reopening producers.

Before schema activation, rollback is an ordinary code rollback. After activation, the
supported recovery is either roll forward to a corrected hard-cut build or restore the
complete pre-cut installation snapshot and old build together. A mixed v8/v9 deployment
is not supported.

## Completion Record

The disposable-data cut was activated on 2026-08-27. Runtime closure is recorded at
Git revision `ad54be9ad39ae0a1cdb535c4c027eec7eb8c11d5`; the implementation began with
`b5f5a006` and remained a hard cut throughout activation.

| Contract fact | Accepted result |
|---|---|
| Durable dataset | `01a0417d-c5bc-7731-8b10-b768f5754b76` contains independently governed recording segments. |
| Sealed segment | `01a0417d-c5c4-7ba2-938d-65c218672137` has seven capture layers plus one properties layer, all committed as Artifact occurrences. |
| Live replacement | `01a041bb-5fe9-7963-b8ba-1fb12e37cf9c` continued ingest under simulator pod generation `9c89c130-4175-48df-92f0-dfb5c0ed54de` while the prior generation remained sealed. |
| Manifest and Blueprint | Playback uses `veoveo.io/recording-playback/v9`. The sealed Blueprint is Artifact occurrence `01a0417d-c5e0-7870-9982-cde82d8c98c2`; no manifest v8 reader or local archive fallback remains. |
| Bounded Arrow projection | Projection `01a041b9-827b-7392-ac49-be2143ff6482` returned four rows and 476,680 bytes. Its payload SHA-256 is `f46fff7ab78a5929d5749fa2bceb5fa92280b0ea4c4f3d81f9dda6e571ee4e6b`; its Arrow schema SHA-256 is `1b4199cff35abf3326969cd0bcba809da8bc44aefcfba64e37d343b86aba0500`. Public redemption matched the declared length, media type, and digest. |
| Cache-loss recovery | All nine derived cache files were deleted, Recording MCP restarted, and sealed playback reconstructed 883,683,240 bytes from Artifact occurrences. The Hub Blueprint staging copy and sealed recording static context were absent afterward. |

Headed live evidence is at
`output/acceptance/uav-recording-browser/746d291f915df0385923f197ec47f36ef0f46c26/01a041ab-e8f8-7843-a829-c0912f848699/evidence.json`.
It observed 120 seconds of advancing live Rerun state and a changed aerial camera frame.
The cache-loss archive replay is at
`output/acceptance/uav-recording-archive-browser/3c6d6c97104b242839a9426c704701472b8c5f7d/01a04218-598d-7883-a77f-d6aea77688ca/evidence.json`.
Final deployed archive evidence is at
`output/acceptance/uav-recording-archive-browser/ad54be9ad39ae0a1cdb535c4c027eec7eb8c11d5/01a0422f-d5da-7380-9a6f-42cb7b967baf/evidence.json`.

The final archive capture used headed Chrome 151. WebGPU exposed SwiftShader and was
rejected as hardware evidence. WebGL reported NVIDIA GeForce RTX 4090 through ANGLE and
rendered the fleet, detailed leader camera, OpenStreetMap, and latest
`simulation_time` frame. Redap returned every required successful read path with zero
failed playback requests. The image contained 470 quantized colors; the camera pane
contained 153, which guards against blank or uniform playback.

The final 40 GiB release preflight found 479 GiB free and projected 73 GiB of margin
above the retained filesystem reserve. The post-build zero-growth check found 478 GiB
free and 111 GiB above that reserve. The Kubernetes node reported `Ready=True` and
`DiskPressure=False`. Recording, Reason, Stream, Console, and simulator workloads were
Ready with no current failed or evicted pod. Recording cache readiness returned HTTP
200, projection scratch was empty, and the rebuilt layer cache retained exactly nine
files.

## Completion Criteria

This project is complete only when:

- one durable dataset contains multiple independently governed recording segments;
- every committed archive layer is reconstructible from its Artifact occurrence and
  manifest without a Hub-local path;
- policy filtering holds for all supported Redap read methods, including direct chunk
  fetches;
- matching Rerun WebViewer and Catalog SDK clients read the supported profile;
- the Recording Explorer receives deterministic Arrow through the host-mediated stream
  without a secret, URL, or whole-result browser buffer;
- live playback preserves current reconnect and rollover behavior under the new identity;
- restart and complete catalog-cache loss rebuild from durable state;
- configured disk, scratch, response, time, and concurrency limits fail closed and the
  deployment smoke shows no recording pod eviction;
- manifest v8, old segment identities, filesystem catalog authority, Hub JSON query code,
  and compatibility shims are absent;
- normative designs, operations documentation, Helm schemas, tests, and the code map all
  describe the same landed contract.
