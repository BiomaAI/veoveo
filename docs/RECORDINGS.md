# Governed recordings

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
shard with that batch.

Freeze runs one materialization pass with Rerun 0.34.1's `object-store`
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

The recording server also owns authenticated HTTP playback routes beside its MCP
surface. The gateway applies the same recording resource policy and audit path,
then issues a short-lived internal assertion. An authenticated manifest request
lets the Console BFF mint a five-minute opaque playback ticket scoped to one
recording; the ticket contains no bearer or filesystem identity. Completed
playback opens one authorized immutable shard URL directly. The BFF and gateway
preserve byte-range and conditional-read headers, while `recording-mcp` serves
the file without decoding it. The manifest lists every shard with its ordinal,
wall-clock bounds, length, and digest. Console selects one window at a time and
lets the operator move to the previous or next window. There is no whole-recording
RRD concatenation endpoint.

Live playback is a distinct governed projection. The manifest identifies the
current writing segment and declares the configured history window. The
production default sends 60 seconds of recent temporal data plus two seconds of
video preroll, followed by newly durable batches. Store information and static
chunks are retained even when they predate the temporal cutoff. Authenticated
ingest maintains a compact static-context snapshot, so a late viewer reads that
snapshot and recent parts instead of scanning the full active hour. Direct native
writers are decoded through the same temporal filter while the decoder follows
the growing file.

Rerun opens the live response with HTTP following enabled, so camera and
telemetry appear before shard freeze. An encoded camera producer repeats codec
and pinhole metadata with every keyframe. The canonical producer emits an IDR
at least once per second, which bounds rollover delay and supplies the declared
live preroll. A rollover closes the old response and gives the next writing
shard a new catalog identity. Console refreshes the live manifest every five
seconds and restarts the viewer only after that identity changes.

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
in [`servers/perception-mcp/DESIGN.md`](../servers/perception-mcp/DESIGN.md).
Keyframes use sparse `is_keyframe=true` markers; non-keyframe samples omit the
component. This shape is required by Rerun's video cache and GoP rebatching.
Frozen or sealed RRD segments are the only Perception source. Video readers
merge authorized shards only when a requested clip crosses an archive window.
The authenticated production path carries static context into every shard and
begins rollover shards at a keyframe, which keeps normal archive-window decoder
initialization local to that shard.

## Representative archive measurement

The object-store profile was measured against 76,094,593 bytes of UAV camera,
static context, and telemetry RRD captured by the Isaac Sim showcase. Rerun
reduced 30,307 one-row chunks to 31 chunks and produced a 28,815,516-byte
footer-indexed shard. It identified 194 H.264 GoPs and rebatched 3,875 frames
into 12 video chunks no larger than 2 MiB. The pass took 2.56 seconds and peaked
at 138,812 KiB RSS on the development host. `rerun rrd verify --check-footers
true` accepted the result. This is a materialization benchmark, not a
playback-time operation.
