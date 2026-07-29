# Governed recordings

## Standards And Protocols

| Standard or protocol | Recording profile |
|---|---|
| Rerun 0.35.0 gRPC and RRD | Producer-local ingestion and immutable `object-store`-optimized shards with footer manifests. |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Recording-scoped read subset over HTTP/2 and gRPC-Web. Veoveo does not expose a general Rerun catalog or mutation surface. |
| Veoveo recording ingest `2026-07-24` | Authenticated protobuf batches preserve native Rerun messages, order, idempotency, and IDR-aligned rollover. |
| Veoveo recording playback `v2` | `veoveo.io/recording-playback/v2` binds one lazy archive dataset, one optional bounded live source, catalog revision, and scoped session. |
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
`started_at` is the first cataloged producer message, `ended_at` is the capture
boundary, and `sealed_at` records later publication. These timestamps are not
interchangeable.

The recording server owns an authenticated playback manifest and bounded live
route beside its MCP surface. The gateway applies the canonical recording
resource policy and audit path, then issues a short-lived internal assertion.
The BFF authenticates each Console request and passes the manifest through. It
does not retain playback sessions or proxy archive bytes.

Playback manifest `veoveo.io/recording-playback/v2` establishes a renewable
five-minute server session scoped to one recording and actor. It returns a
Rerun-compatible read token whose standard Redap claims limit delivery to the
installation hostname. Active replay renews the session every minute, while
live manifest refreshes renew it every five seconds. Each renewal rechecks
recording policy. The opaque session identifier contains no bearer, catalog, or
filesystem identity.

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

The current writing shard remains one bounded HTTP RRD receiver with the same
recording and segment identity. Rerun 0.35 creates that Fetch request with
credentials omitted, so Console installs a reversible exact-route adapter while
the viewer is mounted. It changes only the canonical same-origin live request
to `same-origin` credentials. The HttpOnly Console session therefore reaches
the normal BFF and gateway policy boundary without a token in the URL, while
Redap and arbitrary HTTP sources remain untouched. The adapter inspects
unrelated Fetch inputs without reconstructing their `Request` objects because
Rerun's streaming Redap requests may already own a one-use body.

Live playback is a distinct governed projection. The manifest identifies the
current writing segment and declares the configured history window. The
production default sends 60 seconds of recent temporal data plus two seconds of
video preroll, followed by newly durable batches. Store information and static
chunks are retained even when they predate the temporal cutoff. Authenticated
ingest maintains a compact static-context snapshot, so a late viewer reads that
snapshot and recent parts instead of scanning the full active hour. Direct native
writers are decoded through the same temporal filter while the decoder follows
the growing file.

Rerun opens the lazy archive dataset and the current live HTTP response in one
viewer. Recording MCP rewrites every live message to the archive dataset and
segment identity, so camera and telemetry appear before shard freeze while
earlier history remains on the same timeline. The canonical camera producer
emits the IDR first at each GoP timestamp, then reasserts pinhole metadata. Its
one-second GoP bounds rollover delay and supplies the declared live preroll.
Once the producer's world is ready, diagnostic image quality does not interrupt
encoding or the IDR cadence.

At rollover, the live response ends. Console refreshes the stable archive URI
when its revision changes, opens the successor live response, then detaches the
prior receiver. The persistent viewer retains its layout, selection, and
timeline state. Rerun's web API accepts a generic Redap token only as the
viewer's fallback token at startup. Token rotation therefore replaces the
viewer credential context instead of misclassifying the token as a Rerun Cloud
OAuth credential; rollover within one token lifetime keeps the viewer intact.

Governed queries and bounded analysis use the same acknowledged writing data
without waiting for rollover. Recording MCP captures one ordered snapshot of
the complete ingest parts visible at request or task start. Stream replay and
Reason copy those live parts into bounded task-local storage, verify their byte
length and SHA-256 identity, and load them with prior immutable shards as one
logical Rerun store. Hub writes each part under an exact UUIDv7 staging name and
publishes it by atomic rename; readers do not admit that staging identity until
the canonical sequence-named part exists. Unexpected directory entries remain
an error. If Hub freezes the writing shard during capture, Recording
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
