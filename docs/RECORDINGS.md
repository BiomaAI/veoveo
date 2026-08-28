# Governed recordings

Veoveo stores recordings as durable Rerun datasets. A dataset may contain many
recordings, and each recording is one independently governed Rerun segment. Immutable
RRD layers live in the Artifact plane. SurrealDB holds identity, policy, lifecycle, and
the manifest that binds those bytes.

## Standards And Protocols

| Standard or protocol | Recording profile |
|---|---|
| Rerun `0.36.3` RRD | Canonical recording bytes. Every committed layer uses the dataset UUID as its Rerun application ID and the recording UUID as its Rerun recording and segment ID. |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Governed read-only profile for WebViewer and Catalog SDK reads. Veoveo does not claim mutation, table administration, task, maintenance, or complete Redap conformance. |
| Apache Arrow IPC stream | Deterministic bounded App projections over admitted RRD layers. Arrow is a response format, not durable recording authority. |
| MCP `2026-07-28` | Recording discovery, seal control, and projection creation. Bulk Redap, RRD, and Arrow bytes stay outside MCP content blocks. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The Recording Explorer is the sole App admitted to the recording projection stream extension. |
| Veoveo recording ingest `2026-08-06` | Authenticated protobuf batches and Blueprint publications from a producer-local forwarder to Recording Hub. |
| Veoveo framed RRD stream v2 | Same-origin live transport. Each frame is a four-byte big-endian length followed by one complete RRD payload. |
| Veoveo playback manifest v9 | `veoveo.io/recording-playback/v9` binds the durable dataset, recording segment, catalog revision, governed archive grant, optional live receiver, and Blueprint. No version negotiation exists. |
| OAuth metadata, client credentials, and `private_key_jwt` | Recording Hub and Recording MCP publish with separate service identities. Producer and browser bearers are not retained for background work. |
| H.264/AVC Annex B | Decoder-reentrant capture rollover keeps SPS, PPS, and IDR boundaries. Archive and live playback preserve the original timeline indices. |
| SHA-256 | Immutable layer, manifest, schema, cache, and Arrow-result identity. |

## Canonical Object Model

```text
tenant
└── recording_dataset <UUIDv7, tenant-local key>
    ├── optional default Blueprint
    ├── recording <UUIDv7 = Rerun segment ID>
    │   ├── capture-00000000000000000000 <RRD layer>
    │   ├── capture-00000000000000000001 <RRD layer>
    │   ├── properties <RRD layer>
    │   └── derived-<kind>-<revision> <RRD layer>
    └── recording <another Rerun segment>
```

`recording_dataset`, `recording`, and `recording_layer` are durable records with
strong UUIDv7 types. The producer recording key remains bounded source metadata. It is
never a database identity, a dataset identity, or a Rerun segment identity.

A capture layer has one increasing ordinal within its recording. A committed layer has
one Artifact occurrence, byte length, SHA-256 digest, Rerun version, and schema digest.
Commit is irreversible. Properties and derived output use new immutable revisions
instead of updates in place.

The properties layer carries only reusable, non-secret facts. It may include canonical
dataset and recording identities, the producer key, lifecycle timestamps, source and
manifest revisions, and declared model or environment revisions. Principals, groups,
credentials, policy rules, object coordinates, filesystem paths, classification inputs,
and internal errors never enter those bytes.

## Ingest And Publication

The producer-local forwarder journals bounded native Rerun batches before sending the
authenticated ingest protocol. Recording Hub fsyncs each accepted journal entry and
projects complete ordered parts for the active capture layer. Rollover is driven by the
configured byte or age boundary. Video-bearing capture waits for a decoder-reentrant
access unit before opening the next layer.

The UAV producer also bounds the logical Rerun recording. Every process start mints a
fresh UUIDv4 recording ID, and the running process replaces it before its conservative
encoded-payload budget exceeds 4 GiB or its wall age reaches four hours. A pod UID is not
a recording identity because a container restart would reuse it while resetting the
in-memory budget. The payload budget counts each native H.264 access unit exactly and
reserves an envelope for every telemetry and control event. Starting the replacement
emits the Blueprint and static world context again. The forwarder then finishes the
superseded recording through its ordinary source-generation transition. Capture-layer
rollover and logical-recording rotation are separate controls: the former keeps
publication work small, while the latter keeps one playback segment within the managed
cache envelope.

Publication is one retry-safe sequence:

1. Reserve the `recording_layer` record and UUID.
2. Materialize and normalize the RRD into a staged file.
3. Verify one producer store, then rewrite it to the durable dataset and recording IDs.
4. Reserve spool headroom before writing the worst-case journal and normalized files.
5. Stream the file through the recording-specific Gateway route into the Artifact plane.
6. Verify occurrence UUID, length, and digest.
7. Commit the layer and both catalog revisions in one SurrealDB transaction.
8. Remove local materialization after the bounded recovery window.

The layer UUID is also the requested Artifact occurrence UUID at this one-to-one
boundary. A byte-identical retry returns the committed occurrence. A different request
for that UUID conflicts. Interrupted streams do not create ledger occurrences.

Hub recovery resumes from the durable layer state and exact occurrence identity. It
does not list a bucket or infer publication from a filename. Local paths are staging
coordinates only. Artifact storage becomes authoritative when the layer commit lands.

Sealing emits the deterministic properties layer through the same publication path. A
producer Blueprint is validated, published under the occurrence UUID reserved by its
durable catalog record, and removed from Hub staging before the v9 manifest occurrence
is published. A retry resumes from the staged occurrence. A producer Blueprint wins
when present. The dataset default is otherwise eligible. After the durable seal commits,
Recording MCP removes the recording-scoped static context used to seed live capture
layers. An idempotent seal request retries both Blueprint and static-context cleanup.

## Governed Catalogs

Recording MCP materializes admitted Artifact occurrences into a verified bounded cache.
A download first enters a partial file, then passes length, digest, and RRD identity
checks before atomic rename. Archive layers bind the dataset and recording IDs.
Blueprints bind the producer application ID, Blueprint ID, and message count. Cache keys
bind the occurrence and digest. Active virtual catalogs and Blueprint responses pin
their files, and least-recently-used eviction removes only unpinned entries. A virtual
catalog releases its pins after five minutes without authorized access, which matches the
maximum Redap token lifetime. Recording MCP prunes those idle catalogs before reserving
space for another materialization.

A virtual catalog key contains tenant, dataset ID, policy revision, the digest of the
admitted recording set, and grant class. The Rerun handler receives only those layers.
Authorization does not depend on filtering a broader catalog response after a direct
chunk request has already arrived.

A viewer manifest selects one recording data plane. While the recording is `live`, it
contains the governed live receiver and does not materialize committed history into the
archive cache. After the recording leaves `live`, the viewer manifest materializes and
names the complete immutable archive. Catalog SDK grants and Arrow projections continue
to request committed layers explicitly, including for an active recording.

| Grant class | Consumer | Authority |
|---|---|---|
| `viewer_segment` | Console WebViewer | one recording segment plus its admitted dataset context |
| `catalog_dataset` | Rerun Catalog SDK | an explicit set of recordings in one dataset |
| `app_projection` | Recording Explorer | one projection redemption, with no Redap access |

Grants persist in SurrealDB with actor, Work Context, policy revision, catalog revision,
admitted-set digest, and expiry. A replica can reconstruct an unexpired grant after
restart. Redap tokens are host-limited and map to the grant ID. They are never placed in
MCP tool output or exposed to an App frame.

The selected Redap read profile contains `Version`, `WhoAmI`, `FindEntries`,
`ReadDatasetEntry`, dataset and manifest schema reads, recording segment table schema and
scan, dataset manifest scan, RRD manifest and segment asset reads, `QueryDataset`,
`FetchChunks`, bounded event watch, and the bandwidth probe used by the pinned client.
Mutation and administrative methods return permission denied. Selected assertion-based
tests from `re_redap_tests 0.36.3` cover query filters, manifest scans, chunk completeness,
and missing-segment behavior. The scoped grant and cross-recording rules are Veoveo
authorization tests, not a claim of complete upstream conformance.

## Arrow Projection And App Delivery

`recording-project` creates one closed projection request. It names the dataset,
recording, entity paths, component IDs, timeline, range or sample grid, sparse-fill
policy, idempotency key, units, coordinate-frame references, and every applicable
bound. Omitted bounds are invalid.

The service queries the exact admitted RRD layers and writes canonical Arrow IPC to
managed scratch before response headers. Equal manifest and query inputs produce equal
bytes. The server checks row count, serialized length, selected numeric finiteness,
schema digest, and payload digest. Cancellation or timeout removes partial and final
scratch for the failed receipt.

| Limit | Maximum |
|---|---:|
| Entity selectors | 64 |
| Component selectors | 64 |
| Samples | 10,000 |
| Rows | 10,000 |
| Result bytes | 32 MiB |
| Deadline | 15 seconds |
| Concurrent projections per pod | 2 |
| Aggregate projection scratch per pod | 96 MiB |

Console redeems the handle through an authenticated same-origin BFF route. The host
sends the Recording Explorer a dedicated `MessagePort`, then transfers only a
`ReadableStream`, expected length, and SHA-256 digest. The frame never sees a bearer,
route URL, object URL, local path, RRD source, or whole-result `ArrayBuffer`. Browsers
without transferable streams fail closed.

## Playback And Live Continuity

Playback manifest v9 identifies the durable dataset and recording segment directly.
Its archive URI is the stable Rerun dataset-segment URI for the grant. The optional live
receiver names the active writing layer, but all outgoing Rerun messages use the same
dataset and recording Store ID as archive playback.

Console opens one `LogChannel` for the selected live recording. A bounded bootstrap
includes static context and recent rows, then filesystem notifications advance complete
durable parts. Reconnect on the existing channel starts at the durable head. It never
replays the bootstrap. Rollover stays event-driven, preserves Blueprint state, and does
not poll a manifest or provider.

Rerun `0.36.3` derives H.264 sync samples from access-unit bytes. The live adapter removes
sparse `VideoStream:is_keyframe` columns after compaction because the viewer requires a
dense sample column. Archive normalization validates canonical keyframe identity from
the same encoded bytes.

Recording-based Reason and Stream replay are outside this catalog activation. Those
durable tasks cannot reuse a submitted caller bearer after restart. They must remain
fail-closed until their own design provides a fresh Artifact-read capability. Live Stream
sessions continue to consume admitted live ingress directly.

## Storage And Pod Safety

The default Recording MCP cache PVC is 10 GiB. Managed layer files may consume 8 GiB,
projection scratch may consume 96 MiB, and the service preserves at least 1 GiB of
filesystem headroom. Startup removes partial cache and projection files. Readiness fails
when either managed ceiling or free-space floor is violated.

Hub reserves the worst-case journal and normalized-layer footprint before new work. Its
readiness endpoint checks the spool free-space floor. Both containers have explicit
ephemeral-storage requests and limits, and their incidental `/tmp` mounts have bounded
`emptyDir.sizeLimit` values.

Startup reconciles authenticated mutable capture layers after replaying durable batch
journals. A `writing` layer with no parts is an empty resumable reservation. A `writing`
or `staged` layer with recovery parts is frozen and published idempotently before ingest
opens. A staged layer without recovery parts, multiple mutable capture layers, a failed
capture layer, or a mutable ordinal preceding a later layer stops startup or the next
ingest operation with the exact layer identity. Hub never skips that state to open a
later ordinal.

Authenticated `/admin/storage` returns typed layer-cache and projection counters. Hub
diagnostics expose spool reservations, available bytes, configured floor, and headroom
rejections. Operators should treat a headroom rejection as backpressure, not as a reason
to lower the floor during a rollout.

## Change Checklist

Run this checklist for every recording schema, protocol, cache, or deployment change:

- Decide retention before schema activation. The supported development cut discards old
  recording rows, spool bytes, cache bytes, and associated disposable object data.
- Suspend the owning GitOps reconciliation before scaling Hub and producer Deployments
  to zero. Confirm the desired and current replica counts remain zero; a manual scale
  alone can be corrected while the database reset is still running.
- Keep the producer's logical recording rotation at or below 4 GiB and four hours. Run
  `cargo xtask doctor` after changing the producer, chart, cache, or projection budgets.
- Run the focused host-safe policy test from `showcase/uav-sim/runtime` with
  `PYTHONPATH=. python -m unittest tests/test_recording_segments.py`. The complete UAV
  runtime contract suite uses dependencies supplied by the Isaac simulation image. Copy
  the complete 3 MiB `showcase/uav-sim/runtime` tree into a writable image path before
  running it; the suite inspects its adjacent source, asset, Dockerfile, and patch fixtures.
- Update the Store migration, Rust strong types, Hub, Recording MCP, Gateway, Console,
  smoke fixtures, Helm schema, examples, and both recording design documents together.
- Search active contracts for obsolete manifest versions, query tool names, Hub query
  binaries, segment-table Rust types, and filesystem playback authority.
- Run the live SurrealDB catalog transaction test. Multi-statement transaction responses
  include statement slots, and a wrong response index can survive compile-only checks.
- Delete approved development rows by explicit IDs in bounded batches. Do not wrap
  interactive `surreal sql` input in a hand-written `BEGIN`/`COMMIT` pair; use one
  independently atomic bounded statement or a checked non-interactive script and verify
  the count after every batch.
- Install the selected Rustls crypto provider at every binary and focused-test entrypoint
  before constructing a Reqwest client. A compile pass cannot detect a missing process-global
  provider, and the resulting panic is initialization failure rather than transport evidence.
- Continue timeline values in restart fixtures when the test intends append semantics.
  Reusing the same sequence coordinates exercises Rerun conflict resolution and can hide
  an otherwise durable earlier layer from the projected result.
- Run deterministic RRD, Artifact streaming, layer cache, projection cancellation,
  selected Redap, BFF, bridge, Helm lint, and Helm render checks through
  `cargo xtask test-report`.
- Reopen a long-running live recording after committed layers have filled most of the
  managed archive cache. Require the manifest and Blueprint requests to succeed without
  an archive materialization, prove the live channel remains open, and visually inspect
  an advancing Rerun camera pane in headed hardware-backed Chrome.
- Run `cargo xtask doctor`. It rejects incoherent cache, scratch, headroom, concurrency,
  deadline, and spool budgets and scans normative recording contracts for old surfaces.
- Run `cargo xtask release preflight` with the expected build growth and Kubernetes node.
  Do not start the image build when the retained host reserve would be crossed.
- Treat a local-path PVC size as a scheduling request, not a filesystem quota. Compare
  Hub storage diagnostics and actual mounted usage with node free space before activation;
  the kubelet cannot protect its image filesystem from an unbounded backing directory.
  Set `spoolMinimumFreeBytes` at or above the installation's retained node reserve.
  Do not use a larger request as a substitute, and do not attempt in-place growth when
  the selected StorageClass does not advertise volume expansion.
- Delete a large pre-cut SurrealDB row set through bounded record-ID batches. An
  unbounded delete on a changefeed table can retain the complete transaction in database
  memory, and disconnecting the CLI does not guarantee server-side cancellation.
- Parse every mounted publisher PEM before rollout. A Secret object with the expected key
  name but a zero-byte value passes Kubernetes shape checks and fails both publishers at
  startup with `InvalidKeyFormat`.
- Obtain a recording-seal admin token from the admin profile's protected-resource
  metadata. The deployed profile requires `operator:use admin:manage time:read`; a
  narrower hand-written scope list fails discovery before the tool call.
- Seal only after every capture layer is committed. When a producer Blueprint exists,
  require its Artifact URI in the seal result and require archive playback to survive
  removal of both the Hub staging copy and the complete Recording MCP cache.
- After rollout, confirm `Ready=True`, `DiskPressure=False`, no new `Evicted` recording
  pods, zero leaked scratch, and healthy storage diagnostics.
- Before browser automation, prove the headed browser has hardware WebGPU or WebGL and
  reject SwiftShader, llvmpipe, or a software rasterizer. Interact with the embedded Rerun
  viewer and inspect the captured image, not only API output.

## Activation And Recovery

Development activation discards pre-cut recording data. Stop producers, drain accepted
batches, and suspend their owning GitOps reconciliation. Remove the approved recording
rows and disposable recording object data, clear the spool and catalog cache, then apply
migration `0046_recording_catalog_hard_cut` and deploy Gateway, Artifact service, Hub,
Recording MCP, Console, and their Helm contract as one compatible release. Resume
reconciliation only after the compatible desired state is available.

Suspension prevents a new reconciliation; it does not cancel a Helm action that is already
running. Confirm that the root Kustomization and affected HelmReleases are idle before the
reset. When an action is already active, keep the resources suspended and let its bounded
upgrade or rollback settle before resuming from the root Kustomization.

Before the migration, rollback is an ordinary code rollback. After activation, recovery
is roll-forward or restoration of the complete old database, object data, and workload
set together. A mixed playback-manifest deployment is unsupported.

The final acceptance uses the typed Rust recording and browser smoke entrypoints. The
browser run must be headed, hardware-backed, and visibly exercise archived or live data
inside the embedded Rerun viewer. API checks alone do not qualify playback.
