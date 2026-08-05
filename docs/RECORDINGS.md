# Governed recordings

## Standards And Protocols

| Standard or protocol | Recording profile |
|---|---|
| Rerun 0.35.0 gRPC and RRD | Producer-local ingestion and immutable `object-store`-optimized shards with footer manifests. |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Recording-scoped read subset over HTTP/2 and gRPC-Web. Veoveo does not expose a general Rerun catalog or mutation surface. |
| Rerun MessageProxy `rerun.sdk_comms.v1alpha1` | Recording-scoped `ReadMessages` server stream over gRPC-Web for the bounded live projection. Write and table methods are unavailable. |
| Veoveo recording ingest `2026-08-01` | Authenticated protobuf batches and distinct producer Blueprint publications preserve native Rerun store identities, order, idempotency, and IDR-aligned rollover. |
| Veoveo recording playback `v5` | `veoveo.io/recording-playback/v5` binds one producer Blueprint, one lazy archive dataset, one optional `rerun_message_proxy_grpc` live source, catalog revision, and scoped session. Console selects exactly one recording receiver at a time. |
| H.264/AVC Annex B | Decoder-reentrant `VideoStream` access units, sparse keyframe markers, and exact producer timeline indices. |
| JSON Web Token and SHA-256 | Host-limited Redap read access and immutable shard, layer-revision, and artifact identities. |

Recording ingest begins at a producer-local forwarder. Native Rerun gRPC stays
on `127.0.0.1:9876`; the forwarder journals bounded batches and sends the
authenticated protobuf protocol through the gateway. Recording Hub receives
only gateway-issued internal assertions on its ClusterIP API at port 9878.
Neither Kubernetes Services nor public ingress expose its loopback Rerun
receiver.

Recording Hub fsyncs each validated batch journal before advancing its
SurrealDB checkpoint, then materializes immutable ordered parts beneath one
cataloged writing segment. These small parts are an internal crash-recovery
format and never become archive playback URLs. The normal archive boundary is
one hour, with a 192 MiB pre-compaction safety cap below the artifact-plane
upload ceiling. A video-bearing writer that reaches either boundary waits for
the next batch whose first video sample is an H.264 IDR, then starts the new
shard with that batch. The forwarder inspects the encoded sample and closes its
pending batch before every IDR, which makes each video GoP an available durable
rollover boundary even when telemetry and camera messages share one stream.

Freeze runs one materialization pass with Rerun 0.35.0's `object-store`
optimization profile. It compacts the one-row ingest chunks into chunks capped
at 2 MiB or 65,536 sorted rows, separates thick image/video columns from thin
telemetry, rebatches video chunks on GoP boundaries, repairs keyframe metadata,
and writes the footer manifest. Archive publication fails closed if this pass
fails. The optimized, footer-indexed shard is the frozen authority; optimization
does not run again on a read request. Recording Hub also carries its compact
static-context snapshot into every shard, so codec, calibration, and other
static state do not depend on an earlier window.

The loopback-native path writes RRD segments directly and never truncates an
existing file; a restart creates an `.rN` sibling. Startup reconciles journal
checkpoints, decodes and hashes every segment, repairs crash-safe footer-less
files, and fails closed on corruption or a catalog mismatch.

The authenticated batch protocol marks a recording `ready` once its finish
request has drained. A loopback-native publisher becomes `ready` after the
configured idle grace closes its final segment. A recovered row without either
completion boundary is `interrupted`; new data resumes it as `live`.

`recording-mcp` is the governed control plane. It exposes catalog, recording,
and segment resources; prompt and completion support; resource subscriptions;
bounded temporal queries; and synchronous idempotent sealing. Sealing requires
`admin:manage`, validates each frozen segment again, creates immutable governed
artifact occurrences for every segment and a JSON manifest, stages those
occurrence identities, then changes the recording and its segments to `sealed`
while publishing the durable outbox event in the same SurrealDB transaction.
Producer-authored Blueprints are presentation metadata. Hub admits one complete
Blueprint store only through an explicitly enabled producer policy, binds it to
the recording's tenant, application, producer, Work Context, and invocation
authority, and persists immutable monotonic revisions. The current revision is
published as a separate governed RRD artifact during sealing and appears as a
separate entry in recording manifest v2. Its layout cannot become simulation,
mission, or world-data authority.

`started_at` is the first cataloged producer message, `ended_at` is the capture
boundary, and `sealed_at` records later publication. These timestamps are not
interchangeable.

The recording server owns an authenticated playback manifest and bounded live
route beside its MCP surface. The gateway applies the canonical recording
resource policy and audit path, then issues a short-lived internal assertion.
The BFF authenticates each Console request and passes the manifest through. It
does not retain playback sessions or proxy archive bytes.

Playback manifest `veoveo.io/recording-playback/v5` establishes a renewable
five-minute server session scoped to one recording and actor. It returns a
Rerun-compatible read token whose standard Redap claims limit delivery to the
installation hostname. History mode renews that session once at 80 percent of
its exact lifetime because the archive receiver consumes the Redap token. Live
mode does not schedule credential renewal. Console owns the same-origin stream
request and supplies its HttpOnly session normally. Natural stream completion
rechecks policy and resolves the next segment. The opaque session
identifier contains no bearer, catalog, or filesystem identity.

When a recording has a producer Blueprint, the playback manifest carries its
store identity, revision, digest, and length. The BFF exposes its bytes on a
recording-scoped authenticated route. Recording MCP verifies the stored digest
again and rewrites every Blueprint message, including its activation command,
to the playback application's identity. Console opens that finite Blueprint
source before the selected recording receiver, which applies one producer
layout to live, ready, and sealed playback without mixing the stores.

The installation selects the browser map provider and owns its public browser
token. A producer Blueprint selects the Rerun map background and layout within
that provider. Mapbox selection admits only Mapbox network origins in Console's
content-security policy, so OpenStreetMap cannot silently substitute. Missing,
malformed, or provider-rejected credentials create a map-scoped viewer
diagnostic; recording ingest and the 3D view remain available. Tokens stay in
the installation Secret and authenticated no-store configuration response.
They do not enter RRD, Blueprint, manifest, MCP, artifact metadata, or logs.

Completed playback opens one stable Rerun Data Protocol dataset-segment URI.
The recording UUIDv7 supplies the exact dataset id. The producer's logical
recording id supplies the segment id, and each immutable Hub shard becomes a
named layer of that segment. `recording-mcp` verifies this identity when it
registers a layer. The manifest carries a deterministic revision over the
ordered layer names, content digests, and lengths. A newly frozen shard changes
the revision without changing the archive URI.

The browser connects directly to a same-origin, recording-scoped Redap service.
That service permits only the Rerun viewer's read operations. `FindEntries`
sees the one dataset in the authorized session's isolated derived catalog;
cross-recording access, mutation, registration, table, task, and maintenance
methods remain unavailable.
Rerun reads each shard's footer manifest, then fetches chunks as the active
timeline and view require them. It does not download every archive shard when a
recording opens. The derived Rerun catalog is a bounded in-memory projection;
the durable Veoveo catalog and immutable RRD files rebuild it after restart or
eviction. There are no archive-byte proxy routes and no whole-recording RRD
concatenation endpoint.

The current writing shard is delivered through Rerun 0.35's generated
MessageProxy `ReadMessages` stream. Recording MCP converts each selected native
`LogMsg` with Rerun's LZ4 protobuf transport and streams it over gRPC-Web. It
does not rebuild an indefinitely open RRD file in the browser path.

WebViewer opens one canonical same-origin `rerun+https` proxy URI. Rerun's WASM
client posts to the fixed MessageProxy RPC path, and a Console-scoped service
worker routes that request to the selected recording by controlled browser
client identity. The route carries no bearer. The normal HttpOnly Console
session reaches the BFF and gateway policy boundary, while another tab cannot
select or close this tab's recording stream. The route is acknowledged before
WebViewer opens the receiver. Console never drives the cursor from
`time_update` events.

The Console snapshot stream announces catalog changes. A segment rollover
refreshes the manifest from that event and replaces the completed receiver;
there is no manifest or provider polling.

Live playback is a distinct governed projection. The manifest identifies the
current writing segment and declares the configured history window. The
production default sends one second of recent temporal state plus two seconds
of video preroll, followed by newly durable batches. Store information and
static chunks are retained even when they predate the temporal cutoff. Full
recording history remains on the lazy History path. Authenticated ingest
maintains a compact static-context snapshot, so a late viewer reads that
snapshot and the decoder-reentrant live tail instead of loading a minute of
data before reaching the present. Direct native writers are decoded through
the same temporal filter while the decoder follows the growing file.
Filesystem notifications advance authenticated parts by their next exact
sequence. Neither ingest rollover accounting nor live following rescans the
active segment for every batch; an idle recording performs no periodic scan.

Console opens one native Rerun MessageProxy receiver for Live mode. History remains user-scrubbable.
The bounded history stays behind the live playhead without replaying at
wall-clock speed. Console never rotates an active receiver or mutates its cursor
on a timer. Browser residency belongs to Rerun's store budget; the server-side
history bound controls reconnect bootstrap rather than forcing a viewer restart.

Rerun gRPC does not carry an SDK-flush marker. The producer forwarder groups its
chunks by monotonic source-generation span and H.264 access-unit boundaries,
then wakes the durable uploader immediately. It does not add a batch-flush
clock. Every discovery, OAuth, and ingest request has a bounded deadline, and
shutdown cancels an in-flight request before draining the durable queue.
Filesystem events publish newly acknowledged Hub parts to live receivers.
Backoff remains limited to failed durable uploads and does not pace healthy
live delivery.

Console exposes explicit Live and History modes because Rerun 0.35 cannot keep
two receivers with the same recording Store ID open safely. Live selects only
the current bounded MessageProxy stream. History selects only the lazy immutable
archive dataset. A producer Blueprint remains a distinct presentation store and
opens before either selected receiver. The canonical camera producer
emits the IDR first at each GoP timestamp, then reasserts pinhole metadata. Its
one-second GoP bounds rollover delay and supplies the declared live preroll.
Once the producer's world is ready, diagnostic image quality does not interrupt
encoding or the IDR cadence.

At rollover, the live response ends. Console opens the successor response
without rebuilding the WebViewer. An archive revision change follows the same
close-before-open rule at its stable URI, which prevents overlapping Store
identities. Mode changes close the current recording receiver before opening
the other mode. The persistent viewer retains its producer layout, selection,
and timeline state. The initial Redap token enters as the viewer fallback token;
later rotations use Rerun's credential-update API in place. They do not rebuild
the viewer, reopen the recording, or misclassify the token as a Rerun Cloud OAuth
credential.

Governed queries and bounded analysis use the same acknowledged writing data
without waiting for rollover. Recording MCP captures one ordered snapshot of
the complete ingest parts visible at request or task start. Stream replay and
Reason copy those live parts into bounded task-local storage, verify their byte
length and SHA-256 identity, and load them with prior immutable shards as one
logical Rerun store. Hub writes each part under an exact UUIDv7 staging name and
atomically links it to a previously absent canonical sequence name. A competing
writer can prove identical bytes for idempotency but cannot replace the winning
file. Readers do not admit the staging identity until the canonical name exists.
Unexpected directory entries remain an error. If Hub freezes the writing shard during capture, Recording
MCP discards that attempt, resolves the authorized catalog again, and captures
one coherent successor view. Later batches remain outside that task. This path
never reads the producer proxy or an incomplete part.

Recording UUIDv7 values and artifact UUIDv7 values are occurrence identities.
Filesystem paths are always tenant-internal implementation details and are not
returned by MCP. Classification is descriptive. Non-empty labels enforce
clearance; an `unclassified` recording with no labels is visible within its
tenant. Public or authorized artifact sharing is handled only through
`artifact-mcp` after sealing.

Runtime services authenticate to SurrealDB with database-scoped credentials.
Only the installation bootstrap migrates schema with root credentials. The
recording workload is intentionally one persistent spooler replica; SurrealDB
HA and a distributed recording filesystem are outside the current contract.

Encoded camera streams use the canonical H.264 `VideoStream` profile documented
in [`servers/stream-mcp/DESIGN.md`](../servers/stream-mcp/DESIGN.md).
Keyframes use sparse `is_keyframe=true` markers; non-keyframe samples omit the
component. This shape is required by Rerun's video cache and GoP rebatching.
Stream replay and Reason accept frozen or sealed RRD segments and task-start
snapshots of complete acknowledged ingest parts. Video readers merge authorized
sources when a requested clip crosses a source boundary. The authenticated
production path carries static context into every shard and begins rollover
shards at a keyframe, while the one-second producer GoP supplies decoder
preroll for just-arrived live ranges.

## Representative archive measurement

The object-store profile was measured against 76,094,593 bytes of UAV camera,
static context, and telemetry RRD captured by the Isaac Sim showcase. Rerun
reduced 30,307 one-row chunks to 31 chunks and produced a 28,815,516-byte
footer-indexed shard. It identified 194 H.264 GoPs and rebatched 3,875 frames
into 12 video chunks no larger than 2 MiB. The pass took 2.56 seconds and peaked
at 138,812 KiB RSS on the development host. `rerun rrd verify --check-footers
true` accepted the result. This is a materialization benchmark, not a
playback-time operation.
